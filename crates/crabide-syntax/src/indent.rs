//! Indentation computation using tree-sitter indent queries.
//!
//! Given a parsed `tree_sitter::Tree` and an indents query (`.scm`), this
//! module computes the indent level for every line in the document. The query
//! uses capture names following the Helix / Neovim convention:
//!
//! - `@indent`      — this node's children are indented one level
//! - `@outdent`     — this line should be outdented (e.g. closing `}`)
//! - `@indent.always` — always indent, even if the node is on a single line
//! - `@outdent.always` — always outdent
//!
//! The result is a `Vec<LineIndent>` with one entry per line, suitable for
//! driving the editor's auto-indent logic on Enter key press.

use std::sync::Arc;

use dashmap::DashMap;
use streaming_iterator::StreamingIterator as _;

use crabide_core::types::Language;

use crate::grammar::GrammarEntry;

/// Indentation advice for a single line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineIndent {
    /// The 0-based line number this advice applies to.
    pub line: u32,
    /// Net indent level (number of indent increments minus outdent increments).
    /// A value of 0 means "same as previous line's indent".
    pub indent_level: u32,
    /// Whether this line should be outdented relative to the indent level
    /// (e.g. a closing brace `}` that sits at the same level as the opening).
    pub outdent: bool,
}

impl LineIndent {
    pub fn new(line: u32, indent_level: u32, outdent: bool) -> Self {
        Self {
            line,
            indent_level,
            outdent,
        }
    }
}

/// Indent capture kinds extracted from the query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IndentCapture {
    /// `@indent` — children are indented one level.
    Indent,
    /// `@indent.always` — always indent, even on single-line nodes.
    IndentAlways,
    /// `@outdent` — this line should be outdented.
    Outdent,
    /// `@outdent.always` — always outdent.
    OutdentAlways,
}

/// Caches compiled indent queries per language.
pub struct IndentEngine {
    query_cache: DashMap<Language, Option<Arc<tree_sitter::Query>>>,
}

impl IndentEngine {
    pub fn new() -> Self {
        Self {
            query_cache: DashMap::new(),
        }
    }

    fn get_query(
        &self,
        language: &Language,
        entry: &GrammarEntry,
    ) -> Option<Arc<tree_sitter::Query>> {
        if let Some(cached) = self.query_cache.get(language) {
            return cached.clone();
        }

        let query_src = entry.indents_query.as_ref();
        if query_src.is_empty() {
            self.query_cache.insert(language.clone(), None);
            return None;
        }

        let compiled = match tree_sitter::Query::new(&entry.language, query_src) {
            Ok(q) => {
                log::debug!("Compiled indent query for {}", language);
                Some(Arc::new(q))
            }
            Err(e) => {
                log::warn!("Indent query compile error for {}: {:?}", language, e);
                None
            }
        };

        self.query_cache.insert(language.clone(), compiled.clone());
        compiled
    }

    /// Compute indentation advice for every line in the document.
    ///
    /// Returns a sorted `Vec<LineIndent>` with one entry per line that has
    /// non-trivial indent advice. Lines not present in the result should
    /// inherit the indent of the previous line.
    pub fn compute_indents(
        &self,
        language: &Language,
        entry: &GrammarEntry,
        source: &str,
        tree: &tree_sitter::Tree,
    ) -> Vec<LineIndent> {
        let query = match self.get_query(language, entry) {
            Some(q) => q,
            None => return Vec::new(),
        };

        let source_bytes = source.as_bytes();
        let root = tree.root_node();
        let mut cursor = tree_sitter::QueryCursor::new();
        let capture_names = query.capture_names();

        // Track indent/outdent balance per line.
        // We use a simple approach: for each capture, adjust the indent level
        // for the line where the node starts (for @indent) or ends (for @outdent).
        let line_count = source.lines().count().max(1);
        let mut indent_delta: Vec<i32> = vec![0; line_count];
        let mut outdent_lines: Vec<bool> = vec![false; line_count];

        let mut matches_iter = cursor.matches(query.as_ref(), root, source_bytes);
        while let Some(mat) = matches_iter.next() {
            for capture in mat.captures {
                let node = capture.node;
                let name = &capture_names[capture.index as usize];

                let indent_cap = parse_indent_capture(name);
                match indent_cap {
                    Some(IndentCapture::Indent) | Some(IndentCapture::IndentAlways) => {
                        let start_line = node.start_position().row;
                        if start_line < line_count {
                            // The line *after* this node's start line gets indented.
                            let target = start_line + 1;
                            if target < line_count {
                                indent_delta[target] += 1;
                            }
                        }
                    }
                    Some(IndentCapture::Outdent) | Some(IndentCapture::OutdentAlways) => {
                        let end_line = node.end_position().row;
                        if end_line < line_count {
                            indent_delta[end_line] -= 1;
                            outdent_lines[end_line] = true;
                        }
                    }
                    None => {}
                }
            }
        }

        // Walk forward accumulating indent levels.
        let mut results = Vec::new();
        let mut current_level: u32 = 0;

        for (line, &delta) in indent_delta.iter().enumerate() {
            // Apply delta (clamped to non-negative).
            if delta > 0 {
                current_level = current_level.saturating_add(delta as u32);
            } else {
                current_level = current_level.saturating_sub((-delta) as u32);
            }

            let outdent = outdent_lines[line];
            results.push(LineIndent::new(line as u32, current_level, outdent));
        }

        results
    }
}

impl Default for IndentEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a capture name like `indent`, `indent.always`, `outdent`,
/// `outdent.always` into an `IndentCapture`.
fn parse_indent_capture(name: &str) -> Option<IndentCapture> {
    match name {
        "indent" => Some(IndentCapture::Indent),
        "indent.always" => Some(IndentCapture::IndentAlways),
        "outdent" => Some(IndentCapture::Outdent),
        "outdent.always" => Some(IndentCapture::OutdentAlways),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_indent_captures() {
        assert_eq!(parse_indent_capture("indent"), Some(IndentCapture::Indent));
        assert_eq!(
            parse_indent_capture("indent.always"),
            Some(IndentCapture::IndentAlways)
        );
        assert_eq!(
            parse_indent_capture("outdent"),
            Some(IndentCapture::Outdent)
        );
        assert_eq!(
            parse_indent_capture("outdent.always"),
            Some(IndentCapture::OutdentAlways)
        );
        assert_eq!(parse_indent_capture("keyword"), None);
    }
}
