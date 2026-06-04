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
