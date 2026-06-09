//! Highlight span computation using tree-sitter queries.
//!
//! Given a parsed `tree_sitter::Tree`, a source string, and a compiled
//! `tree_sitter::Query`, this module produces an ordered `Vec<HighlightSpan>`
//! mapping source ranges to semantic scope names.
//!
//! # Injection language support
//!
//! When a grammar has an `injections_query`, the highlight engine can detect
//! embedded language regions (e.g. JavaScript inside `<script>` tags in HTML,
//! Rust code blocks in Markdown) and recursively highlight those regions with
//! the injected language's grammar. The injection query uses captures named
//! `@injection.content` (the region to re-highlight) and `@injection.language`
//! (a string literal identifying the target language, e.g. `"javascript"`).

use std::sync::Arc;

use dashmap::DashMap;
use smol_str::SmolStr;
use streaming_iterator::StreamingIterator as _;

use crabide_core::{
    error::{Result, crabideError},
    types::{Language, Position, Range},
};

use crate::grammar::{GrammarEntry, GrammarRegistry};

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

// ── HighlightEngine ───────────────────────────────────────────────────────────

/// Caches compiled `tree_sitter::Query` objects per language and runs
/// highlight queries against parsed trees. Supports injection queries
/// for embedded language highlighting.
pub struct HighlightEngine {
    /// Compiled highlight queries, keyed by language.
    /// `None` means the query failed to compile (logged + skipped).
    query_cache: DashMap<Language, Option<Arc<tree_sitter::Query>>>,
    /// Compiled injection queries, keyed by language.
    injection_cache: DashMap<Language, Option<Arc<tree_sitter::Query>>>,
}

impl HighlightEngine {
    pub fn new() -> Self {
        Self {
            query_cache: DashMap::new(),
            injection_cache: DashMap::new(),
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
    /// Returns spans sorted by start position. If the grammar has an
    /// injection query, embedded language regions are recursively highlighted
    /// with the injected language's grammar.
    pub fn compute_highlights(
        &self,
        language: &Language,
        entry: &GrammarEntry,
        source: &str,
        tree: &tree_sitter::Tree,
    ) -> Vec<HighlightSpan> {
        let Some(query) = self.get_query(language, entry) else {
            return Vec::new();
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

    /// Compute highlight spans with injection language support.
    ///
    /// After running the primary highlight query, this method checks for
    /// injection captures (`@injection.content` + `@injection.language`) in
    /// the grammar's injection query. For each injection, it looks up the
    /// injected language's grammar in the registry and recursively highlights
    /// the injected region. The injected spans replace the primary spans in
    /// the injected region.
    pub fn compute_highlights_with_injections(
        &self,
        language: &Language,
        entry: &GrammarEntry,
        source: &str,
        tree: &tree_sitter::Tree,
        registry: &GrammarRegistry,
    ) -> Vec<HighlightSpan> {
        // First, compute the primary language highlights.
        let mut spans = self.compute_highlights(language, entry, source, tree);

        // If no injection query, return as-is.
        let Some(injection_query) = self.get_injection_query(language, entry) else {
            return spans;
        };

        let source_bytes = source.as_bytes();
        let root = tree.root_node();
        let mut cursor = tree_sitter::QueryCursor::new();
        let capture_names = injection_query.capture_names();

        // Collect injection regions: (content_range, language_name).
        let mut injections: Vec<(Range, String)> = Vec::new();

        let mut matches_iter = cursor.matches(injection_query.as_ref(), root, source_bytes);
        while let Some(mat) = matches_iter.next() {
            let mut content_node: Option<tree_sitter::Node> = None;
            let mut lang_node: Option<tree_sitter::Node> = None;

            for capture in mat.captures {
                let name = &capture_names[capture.index as usize];
                match &**name {
                    "injection.content" => content_node = Some(capture.node),
                    "injection.language" => lang_node = Some(capture.node),
                    _ => {}
                }
            }

            if let (Some(content), Some(lang)) = (content_node, lang_node) {
                // Extract the language name from the literal node text.
                let lang_text = &source[lang.start_byte()..lang.end_byte()];
                // Strip quotes if present (e.g. `"javascript"` → `javascript`).
                let lang_name = lang_text
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .unwrap_or(lang_text);

                let start = ts_point_to_position(content.start_position());
                let end = ts_point_to_position(content.end_position());
                injections.push((Range::new(start, end), lang_name.to_string()));
            }
        }

        // For each injection, parse the content with the injected language
        // and compute its highlights.
        for (inject_range, lang_name) in &injections {
            let inject_lang = Language::new(lang_name);
            let inject_entry = match registry.get(&inject_lang) {
                Some(e) => e,
                None => continue,
            };

            // Extract the injected source text.
            let start_byte = byte_offset_for_position(source, &inject_range.start);
            let end_byte = byte_offset_for_position(source, &inject_range.end);
            let inject_source = &source[start_byte..end_byte];

            // Parse the injected content.
            let mut parser = tree_sitter::Parser::new();
            if parser.set_language(&inject_entry.language).is_err() {
                continue;
            }

            let inject_tree = match parser.parse(inject_source, None) {
                Some(t) => t,
                None => continue,
            };

            // Compute highlights for the injected content.
            let inject_spans =
                self.compute_highlights(&inject_lang, &inject_entry, inject_source, &inject_tree);

            // Offset the injected spans to the parent document coordinates.
            let line_offset = inject_range.start.line;
            let col_offset = if line_offset == 0 {
                inject_range.start.character as usize
            } else {
                0
            };

            for span in inject_spans {
                let offset_start = Position::new(
                    span.range.start.line + line_offset,
                    if span.range.start.line == 0 {
                        span.range.start.character + col_offset as u32
                    } else {
                        span.range.start.character
                    },
                );
                let offset_end = Position::new(
                    span.range.end.line + line_offset,
                    if span.range.end.line == 0 {
                        span.range.end.character + col_offset as u32
                    } else {
                        span.range.end.character
                    },
                );
                spans.push(HighlightSpan::new(
                    Range::new(offset_start, offset_end),
                    span.scope,
                ));
            }
        }

        // Remove primary spans that fall inside injection regions (they would
        // be incorrect since the injected language should take precedence).
        if !injections.is_empty() {
            spans.retain(|span| {
                !injections
                    .iter()
                    .any(|(inject_range, _)| range_contains(inject_range, &span.range))
            });
        }

        // Re-sort after adding injected spans and removing overlapping ones.
        spans.sort_by_key(|s| s.range.start);
        spans
    }

    /// Get (or compile and cache) the injection query for `language`.
    /// Returns `None` if no injection query is available or it failed to compile.
    fn get_injection_query(
        &self,
        language: &Language,
        entry: &GrammarEntry,
    ) -> Option<Arc<tree_sitter::Query>> {
        if let Some(cached) = self.injection_cache.get(language) {
            return cached.clone();
        }

        let query_src = entry.injections_query.as_ref();
        if query_src.is_empty() {
            self.injection_cache.insert(language.clone(), None);
            return None;
        }

        let result = tree_sitter::Query::new(&entry.language, query_src);
        let compiled = match result {
            Ok(q) => {
                log::debug!("Compiled injection query for {}", language);
                Some(Arc::new(q))
            }
            Err(e) => {
                log::warn!("Injection query compile error for {}: {:?}", language, e);
                None
            }
        };
        self.injection_cache
            .insert(language.clone(), compiled.clone());
        compiled
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

/// Convert a `Position` to a byte offset in the source string.
fn byte_offset_for_position(source: &str, pos: &Position) -> usize {
    let mut offset = 0;
    for (i, line) in source.lines().enumerate() {
        if i as u32 == pos.line {
            // Find the byte offset of the character column within this line.
            let char_offsets: Vec<usize> = line.char_indices().map(|(i, _)| i).collect();
            let col = pos.character as usize;
            offset += if col < char_offsets.len() {
                char_offsets[col]
            } else {
                line.len()
            };
            break;
        }
        offset += line.len() + 1; // +1 for '\n'
    }
    offset
}

/// Check if `inner` range is fully contained within `outer` range.
fn range_contains(outer: &Range, inner: &Range) -> bool {
    inner.start >= outer.start && inner.end <= outer.end
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
