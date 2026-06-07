//! Highlight span computation using tree-sitter queries.
//!
//! Given a parsed `tree_sitter::Tree`, a source string, and a compiled
//! `tree_sitter::Query`, this module produces an ordered `Vec<HighlightSpan>`
//! mapping source ranges to semantic scope names.

use std::sync::Arc;

use dashmap::DashMap;
use smol_str::SmolStr;
use streaming_iterator::StreamingIterator as _;

use crabide_core::{
    error::{crabideError, Result},
    types::{Language, Position, Range},
};

use crate::grammar::GrammarEntry;

// ── HighlightSpan ─────────────────────────────────────────────────────────────

/// A single highlighted region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    /// The source range this span covers.
    pub range: Range,
    /// Tree-sitter capture name without the `@`, e.g. `"keyword"`, `"string"`.
    pub scope: SmolStr,
}

impl HighlightSpan {
    pub fn new(range: Range, scope: impl Into<SmolStr>) -> Self {
        Self {
            range,
            scope: scope.into(),
        }
    }
}

// ── Scope → VS Code token scope mapping ──────────────────────────────────────

/// Convert a tree-sitter capture name (without `@`) to a VS Code TextMate
/// scope prefix for theme lookup.
pub fn scope_to_vscode(scope: &str) -> &'static str {
    // We match on prefixes so "keyword.function" → "keyword", etc.
    let base = scope.split('.').next().unwrap_or(scope);
    match base {
        "comment" => "comment",
        "string" => "string",
        "number" => "constant.numeric",
        "boolean" => "constant.language",
        "constant" => "constant",
        "keyword" => "keyword",
        "operator" => "keyword.operator",
        "function" => "entity.name.function",
        "type" => "entity.name.type",
        "variable" => "variable",
        "namespace" => "entity.name.namespace",
        "attribute" => "entity.other.attribute-name",
        "punctuation" => "punctuation",
        "label" => "entity.name.label",
        "markup" => "markup",
        "tag" => "entity.name.tag",
        "property" => "support.type.property-name",
        _ => "source",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_to_vscode_comment() {
        assert_eq!(scope_to_vscode("comment"), "comment");
        assert_eq!(scope_to_vscode("comment.line"), "comment");
        assert_eq!(scope_to_vscode("comment.block"), "comment");
    }

    #[test]
    fn scope_to_vscode_string() {
        assert_eq!(scope_to_vscode("string"), "string");
        assert_eq!(scope_to_vscode("string.special"), "string");
    }

    #[test]
    fn scope_to_vscode_number() {
        assert_eq!(scope_to_vscode("number"), "constant.numeric");
        assert_eq!(scope_to_vscode("number.float"), "constant.numeric");
    }

    #[test]
    fn scope_to_vscode_boolean() {
        assert_eq!(scope_to_vscode("boolean"), "constant.language");
    }

    #[test]
    fn scope_to_vscode_constant() {
        assert_eq!(scope_to_vscode("constant"), "constant");
        assert_eq!(scope_to_vscode("constant.builtin"), "constant");
    }

    #[test]
    fn scope_to_vscode_keyword() {
        assert_eq!(scope_to_vscode("keyword"), "keyword");
        assert_eq!(scope_to_vscode("keyword.control"), "keyword");
        assert_eq!(scope_to_vscode("keyword.operator"), "keyword");
    }

    #[test]
    fn scope_to_vscode_operator() {
        assert_eq!(scope_to_vscode("operator"), "keyword.operator");
    }

    #[test]
    fn scope_to_vscode_function() {
        assert_eq!(scope_to_vscode("function"), "entity.name.function");
        assert_eq!(scope_to_vscode("function.call"), "entity.name.function");
        assert_eq!(scope_to_vscode("function.method"), "entity.name.function");
    }

    #[test]
    fn scope_to_vscode_type() {
        assert_eq!(scope_to_vscode("type"), "entity.name.type");
        assert_eq!(scope_to_vscode("type.builtin"), "entity.name.type");
    }

    #[test]
    fn scope_to_vscode_variable() {
        assert_eq!(scope_to_vscode("variable"), "variable");
        assert_eq!(scope_to_vscode("variable.builtin"), "variable");
        assert_eq!(scope_to_vscode("variable.member"), "variable");
    }

    #[test]
    fn scope_to_vscode_namespace() {
        assert_eq!(scope_to_vscode("namespace"), "entity.name.namespace");
    }

    #[test]
    fn scope_to_vscode_attribute() {
        assert_eq!(scope_to_vscode("attribute"), "entity.other.attribute-name");
    }

    #[test]
    fn scope_to_vscode_punctuation() {
        assert_eq!(scope_to_vscode("punctuation"), "punctuation");
        assert_eq!(scope_to_vscode("punctuation.bracket"), "punctuation");
        assert_eq!(scope_to_vscode("punctuation.delimiter"), "punctuation");
    }

    #[test]
    fn scope_to_vscode_label() {
        assert_eq!(scope_to_vscode("label"), "entity.name.label");
    }

    #[test]
    fn scope_to_vscode_markup() {
        assert_eq!(scope_to_vscode("markup"), "markup");
        assert_eq!(scope_to_vscode("markup.bold"), "markup");
        assert_eq!(scope_to_vscode("markup.raw.inline"), "markup");
    }

    #[test]
    fn scope_to_vscode_tag() {
        assert_eq!(scope_to_vscode("tag"), "entity.name.tag");
    }

    #[test]
    fn scope_to_vscode_property() {
        assert_eq!(scope_to_vscode("property"), "support.type.property-name");
    }

    #[test]
    fn scope_to_vscode_unknown_falls_back_to_source() {
        assert_eq!(scope_to_vscode("unknown"), "source");
        assert_eq!(scope_to_vscode(""), "source");
    }

    #[test]
    fn scope_to_vscode_empty_scope() {
        assert_eq!(scope_to_vscode(""), "source");
    }
}

// ── HighlightEngine ───────────────────────────────────────────────────────────

/// Caches compiled `tree_sitter::Query` objects per language and runs
/// highlight queries against parsed trees.
pub struct HighlightEngine {
    /// Compiled highlight queries, keyed by language.
    /// `None` means the query failed to compile (logged + skipped).
    query_cache: DashMap<Language, Option<Arc<tree_sitter::Query>>>,
}

impl HighlightEngine {
    pub fn new() -> Self {
        Self {
            query_cache: DashMap::new(),
        }
    }

    /// Get (or compile and cache) the highlight query for `language`.
    /// Returns `None` if no query is available or the query failed to compile.
    fn get_query(
        &self,
        language: &Language,
        entry: &GrammarEntry,
    ) -> Option<Arc<tree_sitter::Query>> {
        // Fast path: already cached.
        if let Some(cached) = self.query_cache.get(language) {
            return cached.clone();
        }

        // Compile query.
        let query_src = entry.highlights_query.as_ref();
        if query_src.is_empty() {
            self.query_cache.insert(language.clone(), None);
            return None;
        }

        let result = tree_sitter::Query::new(&entry.language, query_src);
        let compiled = match result {
            Ok(q) => {
                log::debug!("Compiled highlight query for {}", language);
                Some(Arc::new(q))
            }
            Err(e) => {
                log::warn!("Highlight query compile error for {}: {:?}", language, e);
                None
            }
        };
        self.query_cache.insert(language.clone(), compiled.clone());
        compiled
    }

    /// Compute highlight spans for `source` using the given parsed `tree`
    /// and the grammar entry for its language.
    ///
    /// Returns spans sorted by start position.
    pub fn compute_highlights(
        &self,
        language: &Language,
        entry: &GrammarEntry,
        source: &str,
        tree: &tree_sitter::Tree,
    ) -> Vec<HighlightSpan> {
        let query = match self.get_query(language, entry) {
            Some(q) => q,
            None => return Vec::new(),
        };

        let source_bytes = source.as_bytes();
        let root = tree.root_node();
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut spans = Vec::new();

        let capture_names = query.capture_names();

        let mut matches_iter = cursor.matches(query.as_ref(), root, source_bytes);
        while let Some(mat) = matches_iter.next() {
            for capture in mat.captures {
                let node = capture.node;
                let name = &capture_names[capture.index as usize];

                // Skip captures that span 0 characters.
                if node.start_byte() == node.end_byte() {
                    continue;
                }

                let start = ts_point_to_position(node.start_position());
                let end = ts_point_to_position(node.end_position());

                spans.push(HighlightSpan::new(Range::new(start, end), *name));
            }
        }

        // Sort spans by start position so the UI can render them in order.
        spans.sort_by_key(|s| s.range.start);

        spans
    }
}

impl Default for HighlightEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Convert a tree-sitter `Point` (row/column in bytes) to a crabide `Position`
/// (line/character in chars). We treat column as UTF-8 byte offset for now;
/// full UTF-16 conversion happens in the UI layer if needed.
#[inline]
pub fn ts_point_to_position(p: tree_sitter::Point) -> Position {
    Position::new(p.row as u32, p.column as u32)
}

/// Compile a one-off highlight query for a language entry. Used by tests and
/// tools outside the engine.
pub fn compile_query(entry: &GrammarEntry) -> Result<tree_sitter::Query> {
    tree_sitter::Query::new(&entry.language, &entry.highlights_query).map_err(|e| {
        crabideError::QueryError {
            language: "unknown".into(),
            message: format!("{e:?}"),
        }
    })
}
