//! Folding range extraction from tree-sitter parse trees and custom
//! `#region` / `#endregion` markers.
//!
//! Walks the syntax tree and emits a `FoldingRange` for every multi-line node
//! whose type matches the fold-point heuristics. This is language-agnostic;
//! any braced/bracketed block, comment block, or import group spanning more
//! than one line becomes a fold candidate.
//!
//! Additionally, comments matching `#region` / `#endregion` patterns are
//! recognised and converted into folding ranges. Supported comment styles:
//! - `// #region` / `// #endregion` (Rust, C, JS, Go, …)
//! - `# #region` / `# #endregion` (Python, Ruby, YAML, …)
//! - `-- #region` / `-- #endregion` (SQL, Haskell, …)
//! - `; #region` / `; #endregion` (Lisp, Clojure, …)

use crabide_core::types::Language;

// ── Types ─────────────────────────────────────────────────────────────────────

/// The semantic kind of a folding range (matches LSP `FoldingRangeKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldKind {
    /// A general code block (function body, struct, etc.).
    Region,
    /// A line or block comment block.
    Comment,
    /// A consecutive group of import / use statements.
    Imports,
}

/// A collapsible region in a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldingRange {
    /// First line of the fold (0-based, inclusive).
    pub start_line: u32,
    /// Last line of the fold (0-based, inclusive). Always > `start_line`.
    pub end_line: u32,
    pub kind: FoldKind,
}

/// Comment prefix patterns for `#region` / `#endregion` markers, ordered by
/// how they appear in source. The `//` prefix is listed first since it is the
/// most common across C-family languages.
const REGION_COMMENT_PREFIXES: &[&str] = &["//", "#", "--", ";"];

/// The `#region` marker keyword (case-insensitive first token after prefix).
const REGION_START: &str = "#region";
/// The `#endregion` marker keyword (case-insensitive first token after prefix).
const REGION_END: &str = "#endregion";

// ── Extraction ────────────────────────────────────────────────────────────────

/// Extract all folding ranges from a parsed tree, including custom `#region` /
/// `#endregion` markers found in `source`.
///
/// `source` is the full document text (needed for region-marker scanning).
pub fn extract_folding_ranges(
    tree: &tree_sitter::Tree,
    source: &str,
    _language: &Language,
) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();
    collect_folds(tree.root_node(), &mut ranges);
    ranges.extend(extract_region_folds(source));
    // Deduplicate and sort
    ranges.sort_by_key(|r| (r.start_line, r.end_line));
    ranges.dedup_by_key(|r| (r.start_line, r.end_line));
    ranges
}

/// Scan `source` for `#region` / `#endregion` markers and return the matching
/// fold ranges. Lines are 0-based.
fn extract_region_folds(source: &str) -> Vec<FoldingRange> {
    let lines: Vec<&str> = source.lines().collect();
    let mut regions: Vec<FoldingRange> = Vec::new();
    // Stack of start-line numbers for nested regions.
    let mut stack: Vec<u32> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let line_num = i as u32;
        let trimmed = line.trim();

        // Skip empty lines.
        if trimmed.is_empty() {
            continue;
        }

        // Determine the marker (if any) on this line.
        let marker = match region_marker(trimmed) {
            Some(m) => m,
            None => continue,
        };

        match marker {
            RegionMarker::Start => {
                stack.push(line_num);
            }
            RegionMarker::End => {
                if let Some(start) = stack.pop() {
                    // Only emit when end > start (multi-line region).
                    if line_num > start {
                        regions.push(FoldingRange {
                            start_line: start,
                            end_line: line_num,
                            kind: FoldKind::Region,
                        });
                    }
                }
                // If the stack is empty, unmatched `#endregion` is silently
                // ignored (some editors treat it as folding to end-of-file).
            }
        }
    }

    // Close any unclosed regions at end-of-file.
    // Treat the last line of the document as the end.
    if let Some(last_line) = lines.last().map(|_| (lines.len() - 1) as u32) {
        while let Some(start) = stack.pop() {
            if last_line > start {
                regions.push(FoldingRange {
                    start_line: start,
                    end_line: last_line,
                    kind: FoldKind::Region,
                });
            }
        }
    }

    regions
}

/// The kind of region marker found on a line.
enum RegionMarker {
    Start,
    End,
}

/// Check whether `trimmed_line` starts with a recognised comment prefix followed
/// by a `#region` / `#endregion` marker.
fn region_marker(trimmed_line: &str) -> Option<RegionMarker> {
    for prefix in REGION_COMMENT_PREFIXES {
        if let Some(after_prefix) = trimmed_line.strip_prefix(prefix) {
            let after_prefix = after_prefix.trim_start();
            // Case-insensitive comparison for the marker keyword.
            if after_prefix.to_ascii_lowercase().starts_with(REGION_START) {
                return Some(RegionMarker::Start);
            }
            if after_prefix.to_ascii_lowercase().starts_with(REGION_END) {
                return Some(RegionMarker::End);
            }
        }
    }
    None
}

fn collect_folds(node: tree_sitter::Node<'_>, out: &mut Vec<FoldingRange>) {
    let start_row = node.start_position().row as u32;
    let end_row = node.end_position().row as u32;

    if end_row > start_row {
        // Emit a fold for nodes that are multi-line and structurally meaningful.
        if let Some(kind) = fold_kind_for(node.kind()) {
            out.push(FoldingRange {
                start_line: start_row,
                end_line: end_row,
                kind,
            });
        }
    }

    // Recurse into children.
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            collect_folds(cursor.node(), out);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Map a tree-sitter node kind to a `FoldKind`, or `None` if this node type
/// should not generate a fold.
fn fold_kind_for(kind: &str) -> Option<FoldKind> {
    match kind {
        // Generic block / body nodes (language-agnostic braced blocks)
        "block"
        | "declaration_list"
        | "field_declaration_list"
        | "enum_variant_list"
        | "use_list"
        | "impl_item"
        | "trait_item"
        | "function_item"
        | "mod_item"
        | "function_definition"
        | "class_body"
        | "class_definition"
        | "method_definition"
        | "function_declaration"
        | "arrow_function"
        | "object"
        | "object_type"
        | "interface_body"
        | "enum_body"
        | "switch_body"
        | "try_statement"
        | "catch_clause"
        | "for_statement"
        | "while_statement"
        | "if_statement"
        | "match_expression"
        | "match_arm"
        | "tuple_struct_pattern"
        | "struct_item"
        | "struct_expression"
        | "struct_literal_expression"
        | "array"
        | "array_expression"
        | "parenthesized_expression"
        | "arguments"
        | "parameters"
        | "type_parameters"
        // Go
        | "func_literal"
        | "composite_literal"
        | "select_statement"
        // C / C++
        | "compound_statement"
        | "struct_specifier"
        | "class_specifier"
        | "namespace_definition"
        | "translation_unit" => Some(FoldKind::Region),

        // Comments
        "block_comment"
        | "line_comment"
        | "doc_comment"
        | "multiline_comment" => Some(FoldKind::Comment),

        // Import groups
        "use_declaration"
        | "import_statement"
        | "import_from_statement"
        | "import_group" => Some(FoldKind::Imports),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_kind_for_block() {
        assert_eq!(fold_kind_for("block"), Some(FoldKind::Region));
        assert_eq!(fold_kind_for("declaration_list"), Some(FoldKind::Region));
        assert_eq!(fold_kind_for("function_item"), Some(FoldKind::Region));
        assert_eq!(fold_kind_for("class_definition"), Some(FoldKind::Region));
        assert_eq!(fold_kind_for("if_statement"), Some(FoldKind::Region));
        assert_eq!(fold_kind_for("for_statement"), Some(FoldKind::Region));
    }

    #[test]
    fn fold_kind_for_comment() {
        assert_eq!(fold_kind_for("block_comment"), Some(FoldKind::Comment));
        assert_eq!(fold_kind_for("line_comment"), Some(FoldKind::Comment));
        assert_eq!(fold_kind_for("doc_comment"), Some(FoldKind::Comment));
        assert_eq!(fold_kind_for("multiline_comment"), Some(FoldKind::Comment));
    }

    #[test]
    fn fold_kind_for_imports() {
        assert_eq!(fold_kind_for("use_declaration"), Some(FoldKind::Imports));
        assert_eq!(fold_kind_for("import_statement"), Some(FoldKind::Imports));
        assert_eq!(fold_kind_for("import_group"), Some(FoldKind::Imports));
    }

    #[test]
    fn fold_kind_for_unknown_returns_none() {
        assert_eq!(fold_kind_for("identifier"), None);
        assert_eq!(fold_kind_for("number_literal"), None);
        assert_eq!(fold_kind_for(""), None);
    }

    #[test]
    fn region_marker_start() {
        assert!(region_marker("// #region").is_some_and(|m| matches!(m, RegionMarker::Start)));
        assert!(region_marker("# #region").is_some_and(|m| matches!(m, RegionMarker::Start)));
        assert!(region_marker("-- #region").is_some_and(|m| matches!(m, RegionMarker::Start)));
        assert!(region_marker("; #region").is_some_and(|m| matches!(m, RegionMarker::Start)));
    }

    #[test]
    fn region_marker_end() {
        assert!(region_marker("// #endregion").is_some_and(|m| matches!(m, RegionMarker::End)));
        assert!(region_marker("# #endregion").is_some_and(|m| matches!(m, RegionMarker::End)));
        assert!(region_marker("-- #endregion").is_some_and(|m| matches!(m, RegionMarker::End)));
    }

    #[test]
    fn region_marker_case_insensitive() {
        assert!(region_marker("// #REGION").is_some_and(|m| matches!(m, RegionMarker::Start)));
        assert!(region_marker("// #endregion").is_some_and(|m| matches!(m, RegionMarker::End)));
    }

    #[test]
    fn region_marker_no_prefix_returns_none() {
        assert!(region_marker("#region").is_none());
        assert!(region_marker("foo // #region").is_none());
    }

    #[test]
    fn region_marker_no_marker_returns_none() {
        assert!(region_marker("// some random comment").is_none());
        assert!(region_marker("").is_none());
    }

    #[test]
    fn extract_region_folds_empty_source() {
        let folds = extract_region_folds("");
        assert!(folds.is_empty());
    }

    #[test]
    fn extract_region_folds_single_region() {
        let source = "// #region\ninside\n// #endregion";
        let folds = extract_region_folds(source);
        assert_eq!(folds.len(), 1);
        assert_eq!(folds[0].start_line, 0);
        assert_eq!(folds[0].end_line, 2);
        assert_eq!(folds[0].kind, FoldKind::Region);
    }

    #[test]
    fn extract_region_folds_nested_regions() {
        let source = "// #region outer\nouter content\n// #region inner\ninner content\n// #endregion\n// #endregion";
        let folds = extract_region_folds(source);
        assert_eq!(folds.len(), 2);
        // Inner region
        assert_eq!(folds[0].start_line, 2);
        assert_eq!(folds[0].end_line, 4);
        // Outer region
        assert_eq!(folds[1].start_line, 0);
        assert_eq!(folds[1].end_line, 5);
    }

    #[test]
    fn extract_region_folds_unclosed_region_closes_at_eof() {
        let source = "// #region\nsome content\nno end marker";
        let folds = extract_region_folds(source);
        assert_eq!(folds.len(), 1);
        assert_eq!(folds[0].start_line, 0);
        assert_eq!(folds[0].end_line, 2);
    }

    #[test]
    fn extract_region_folds_multiple_prefix_styles() {
        let source = "# #region (python style)\ncontent\n# #endregion\n; #region (lisp style)\nmore\n; #endregion";
        let folds = extract_region_folds(source);
        assert_eq!(folds.len(), 2);
    }

    #[test]
    fn extract_region_folds_ignores_non_region_comments() {
        let source = "// just a comment\n// another comment";
        let folds = extract_region_folds(source);
        assert!(folds.is_empty());
    }

    #[test]
    fn fold_kind_for_c_and_cpp_nodes() {
        assert_eq!(fold_kind_for("compound_statement"), Some(FoldKind::Region));
        assert_eq!(fold_kind_for("struct_specifier"), Some(FoldKind::Region));
        assert_eq!(
            fold_kind_for("namespace_definition"),
            Some(FoldKind::Region)
        );
        assert_eq!(fold_kind_for("translation_unit"), Some(FoldKind::Region));
    }
}
