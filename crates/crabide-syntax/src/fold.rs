//! Folding range extraction from tree-sitter parse trees.
//!
//! Walks the syntax tree and emits a `FoldingRange` for every multi-line node
//! whose type matches the fold-point heuristics. This is language-agnostic;
//! any braced/bracketed block, comment block, or import group spanning more
//! than one line becomes a fold candidate.

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

// ── Extraction ────────────────────────────────────────────────────────────────

/// Extract all folding ranges from a parsed tree.
///
/// `source` is the full document text (needed for line counting checks).
pub fn extract_folding_ranges(tree: &tree_sitter::Tree, _language: &Language) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();
    collect_folds(tree.root_node(), &mut ranges);
    // Deduplicate and sort
    ranges.sort_by_key(|r| (r.start_line, r.end_line));
    ranges.dedup_by_key(|r| (r.start_line, r.end_line));
    ranges
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
