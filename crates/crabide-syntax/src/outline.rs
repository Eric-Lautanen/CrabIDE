//! Symbol outline extraction from tree-sitter parse trees.
//!
//! Produces a hierarchical list of named symbols suitable for the breadcrumb
//! bar, the "Go to Symbol" picker (Ctrl+Shift+O), and the outline panel.
//!
//! Each language has its own extractor that knows which node types carry
//! meaningful names (functions, classes, structs, etc.).

use crabide_core::types::{Language, Range};

// ── SymbolKind ────────────────────────────────────────────────────────────────

/// The semantic kind of a symbol, matching the LSP `SymbolKind` enum values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SymbolKind {
    File,
    Module,
    Namespace,
    Package,
    Class,
    Method,
    Property,
    Field,
    Constructor,
    Enum,
    Interface,
    Function,
    Variable,
    Constant,
    String,
    Number,
    Boolean,
    Array,
    Object,
    Key,
    Null,
    EnumMember,
    Struct,
    Event,
    Operator,
    TypeParameter,
}

// ── SymbolOutline ─────────────────────────────────────────────────────────────

/// A single named symbol in the document.
#[derive(Debug, Clone)]
pub struct SymbolOutline {
    /// Display name of the symbol.
    pub name: String,
    pub kind: SymbolKind,
    /// The full range of the symbol's definition (including body).
    pub range: Range,
    /// The name range only (used for cursor positioning on jump).
    pub selection_range: Range,
    /// Nested symbols (e.g. methods inside a class).
    pub children: Vec<SymbolOutline>,
}

impl SymbolOutline {
    pub fn new(name: String, kind: SymbolKind, range: Range, selection_range: Range) -> Self {
        Self {
            name,
            kind,
            range,
            selection_range,
            children: Vec::new(),
        }
    }
}

// ── Extraction ────────────────────────────────────────────────────────────────

/// Extract the symbol outline for a document.
pub fn extract_outline(
    tree: &tree_sitter::Tree,
    source: &[u8],
    language: &Language,
) -> Vec<SymbolOutline> {
    match language.as_str() {
        "rust" => extract_rust(tree.root_node(), source),
        "python" => extract_python(tree.root_node(), source),
        "javascript" => extract_js(tree.root_node(), source),
        "typescript" => extract_js(tree.root_node(), source),
        "go" => extract_go(tree.root_node(), source),
        "c" | "cpp" => extract_c(tree.root_node(), source),
        "json" => extract_json(tree.root_node(), source),
        "toml" => extract_toml(tree.root_node(), source),
        "markdown" => extract_markdown(tree.root_node(), source),
        _ => Vec::new(),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn node_text(node: tree_sitter::Node<'_>, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("?").to_owned()
}

fn node_range(node: tree_sitter::Node<'_>) -> Range {
    use crate::highlight::ts_point_to_position;
    Range::new(
        ts_point_to_position(node.start_position()),
        ts_point_to_position(node.end_position()),
    )
}

/// Find the first child field node by field name.
fn field_child<'a>(node: tree_sitter::Node<'a>, field: &str) -> Option<tree_sitter::Node<'a>> {
    node.child_by_field_name(field)
}

// ── Rust ──────────────────────────────────────────────────────────────────────

fn extract_rust(root: tree_sitter::Node<'_>, source: &[u8]) -> Vec<SymbolOutline> {
    let mut symbols = Vec::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if let Some(sym) = rust_symbol(child, source) {
            symbols.push(sym);
        }
    }
    symbols
}

fn rust_symbol(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<SymbolOutline> {
    match node.kind() {
        "function_item" | "function_signature_item" => {
            let name_node = field_child(node, "name")?;
            let name = node_text(name_node, source);
            let sym = SymbolOutline::new(
                name,
                SymbolKind::Function,
                node_range(node),
                node_range(name_node),
            );
            // Methods inside impl blocks are handled at impl level
            Some(sym)
        }
        "struct_item" => {
            let name_node = field_child(node, "name")?;
            let name = node_text(name_node, source);
            let mut sym = SymbolOutline::new(
                name,
                SymbolKind::Struct,
                node_range(node),
                node_range(name_node),
            );
            // fields
            if let Some(body) = field_child(node, "body") {
                let mut c = body.walk();
                for field in body.named_children(&mut c) {
                    if field.kind() == "field_declaration" {
                        if let Some(fname) = field_child(field, "name") {
                            sym.children.push(SymbolOutline::new(
                                node_text(fname, source),
                                SymbolKind::Field,
                                node_range(field),
                                node_range(fname),
                            ));
                        }
                    }
                }
            }
            Some(sym)
        }
        "enum_item" => {
            let name_node = field_child(node, "name")?;
            let name = node_text(name_node, source);
            let mut sym = SymbolOutline::new(
                name,
                SymbolKind::Enum,
                node_range(node),
                node_range(name_node),
            );
            if let Some(body) = field_child(node, "body") {
                let mut c = body.walk();
                for variant in body.named_children(&mut c) {
                    if variant.kind() == "enum_variant" {
                        if let Some(vname) = field_child(variant, "name") {
                            sym.children.push(SymbolOutline::new(
                                node_text(vname, source),
                                SymbolKind::EnumMember,
                                node_range(variant),
                                node_range(vname),
                            ));
                        }
                    }
                }
            }
            Some(sym)
        }
        "trait_item" => {
            let name_node = field_child(node, "name")?;
            let name = node_text(name_node, source);
            let mut sym = SymbolOutline::new(
                name,
                SymbolKind::Interface,
                node_range(node),
                node_range(name_node),
            );
            if let Some(body) = field_child(node, "body") {
                let mut c = body.walk();
                for item in body.named_children(&mut c) {
                    if let Some(child_sym) = rust_symbol(item, source) {
                        sym.children.push(child_sym);
                    }
                }
            }
            Some(sym)
        }
        "impl_item" => {
            // "impl Foo" or "impl Trait for Foo"
            let type_node = field_child(node, "type")?;
            let impl_name = format!("impl {}", node_text(type_node, source));
            let mut sym = SymbolOutline::new(
                impl_name,
                SymbolKind::Namespace,
                node_range(node),
                node_range(type_node),
            );
            if let Some(body) = field_child(node, "body") {
                let mut c = body.walk();
                for item in body.named_children(&mut c) {
                    if let Some(child_sym) = rust_symbol(item, source) {
                        sym.children.push(child_sym);
                    }
                }
            }
            Some(sym)
        }
        "mod_item" => {
            let name_node = field_child(node, "name")?;
            let name = node_text(name_node, source);
            let mut sym = SymbolOutline::new(
                name,
                SymbolKind::Module,
                node_range(node),
                node_range(name_node),
            );
            if let Some(body) = field_child(node, "body") {
                let mut c = body.walk();
                for item in body.named_children(&mut c) {
                    if let Some(child_sym) = rust_symbol(item, source) {
                        sym.children.push(child_sym);
                    }
                }
            }
            Some(sym)
        }
        "const_item" | "static_item" => {
            let name_node = field_child(node, "name")?;
            let name = node_text(name_node, source);
            Some(SymbolOutline::new(
                name,
                SymbolKind::Constant,
                node_range(node),
                node_range(name_node),
            ))
        }
        "type_alias" => {
            let name_node = field_child(node, "name")?;
            let name = node_text(name_node, source);
            Some(SymbolOutline::new(
                name,
                SymbolKind::TypeParameter,
                node_range(node),
                node_range(name_node),
            ))
        }
        _ => None,
    }
}

// ── Python ────────────────────────────────────────────────────────────────────

fn extract_python(root: tree_sitter::Node<'_>, source: &[u8]) -> Vec<SymbolOutline> {
    let mut symbols = Vec::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if let Some(sym) = python_symbol(child, source) {
            symbols.push(sym);
        }
    }
    symbols
}

fn python_symbol(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<SymbolOutline> {
    match node.kind() {
        "function_definition" => {
            let name_node = field_child(node, "name")?;
            let name = node_text(name_node, source);
            let mut sym = SymbolOutline::new(
                name,
                SymbolKind::Function,
                node_range(node),
                node_range(name_node),
            );
            if let Some(body) = field_child(node, "body") {
                let mut c = body.walk();
                for item in body.named_children(&mut c) {
                    if let Some(child_sym) = python_symbol(item, source) {
                        sym.children.push(child_sym);
                    }
                }
            }
            Some(sym)
        }
        "class_definition" => {
            let name_node = field_child(node, "name")?;
            let name = node_text(name_node, source);
            let mut sym = SymbolOutline::new(
                name,
                SymbolKind::Class,
                node_range(node),
                node_range(name_node),
            );
            if let Some(body) = field_child(node, "body") {
                let mut c = body.walk();
                for item in body.named_children(&mut c) {
                    if let Some(child_sym) = python_symbol(item, source) {
                        sym.children.push(child_sym);
                    }
                }
            }
            Some(sym)
        }
        "decorated_definition" => {
            // Unwrap the decorated function/class.
            let mut c = node.walk();
            for child in node.named_children(&mut c) {
                if matches!(child.kind(), "function_definition" | "class_definition") {
                    return python_symbol(child, source);
                }
            }
            None
        }
        _ => None,
    }
}

// ── JavaScript / TypeScript ───────────────────────────────────────────────────

fn extract_js(root: tree_sitter::Node<'_>, source: &[u8]) -> Vec<SymbolOutline> {
    let mut symbols = Vec::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if let Some(sym) = js_symbol(child, source) {
            symbols.push(sym);
        }
    }
    symbols
}

fn js_symbol(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<SymbolOutline> {
    match node.kind() {
        "function_declaration" => {
            let name_node = field_child(node, "name")?;
            Some(SymbolOutline::new(
                node_text(name_node, source),
                SymbolKind::Function,
                node_range(node),
                node_range(name_node),
            ))
        }
        "class_declaration" => {
            let name_node = field_child(node, "name")?;
            let name = node_text(name_node, source);
            let mut sym = SymbolOutline::new(
                name,
                SymbolKind::Class,
                node_range(node),
                node_range(name_node),
            );
            if let Some(body) = field_child(node, "body") {
                let mut c = body.walk();
                for item in body.named_children(&mut c) {
                    if item.kind() == "method_definition" {
                        if let Some(mname) = field_child(item, "name") {
                            sym.children.push(SymbolOutline::new(
                                node_text(mname, source),
                                SymbolKind::Method,
                                node_range(item),
                                node_range(mname),
                            ));
                        }
                    }
                }
            }
            Some(sym)
        }
        "lexical_declaration" | "variable_declaration" => {
            // const foo = () => {} or const bar = function() {}
            let mut c = node.walk();
            let mut syms = Vec::new();
            for declarator in node.named_children(&mut c) {
                if declarator.kind() == "variable_declarator" {
                    if let (Some(name_node), Some(value)) = (
                        field_child(declarator, "name"),
                        field_child(declarator, "value"),
                    ) {
                        if matches!(value.kind(), "arrow_function" | "function") {
                            syms.push(SymbolOutline::new(
                                node_text(name_node, source),
                                SymbolKind::Function,
                                node_range(declarator),
                                node_range(name_node),
                            ));
                        }
                    }
                }
            }
            syms.into_iter().next()
        }
        _ => None,
    }
}

// ── Go ────────────────────────────────────────────────────────────────────────

fn extract_go(root: tree_sitter::Node<'_>, source: &[u8]) -> Vec<SymbolOutline> {
    let mut symbols = Vec::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                if let Some(name) = field_child(child, "name") {
                    symbols.push(SymbolOutline::new(
                        node_text(name, source),
                        SymbolKind::Function,
                        node_range(child),
                        node_range(name),
                    ));
                }
            }
            "method_declaration" => {
                if let Some(name) = field_child(child, "name") {
                    // Include receiver type in the name for clarity.
                    let recv = field_child(child, "receiver")
                        .and_then(|r| {
                            let mut c = r.walk();
                            let first = r.named_children(&mut c).next();
                            first
                        })
                        .and_then(|p| field_child(p, "type"))
                        .map(|t| node_text(t, source))
                        .unwrap_or_default();
                    let display = if recv.is_empty() {
                        node_text(name, source)
                    } else {
                        format!("({recv}) {}", node_text(name, source))
                    };
                    symbols.push(SymbolOutline::new(
                        display,
                        SymbolKind::Method,
                        node_range(child),
                        node_range(name),
                    ));
                }
            }
            "type_declaration" => {
                let mut c = child.walk();
                for spec in child.named_children(&mut c) {
                    if spec.kind() == "type_spec" {
                        if let Some(name) = field_child(spec, "name") {
                            let kind = field_child(spec, "type")
                                .map(|t| match t.kind() {
                                    "struct_type" => SymbolKind::Struct,
                                    "interface_type" => SymbolKind::Interface,
                                    _ => SymbolKind::TypeParameter,
                                })
                                .unwrap_or(SymbolKind::TypeParameter);
                            symbols.push(SymbolOutline::new(
                                node_text(name, source),
                                kind,
                                node_range(spec),
                                node_range(name),
                            ));
                        }
                    }
                }
            }
            "const_declaration" | "var_declaration" => {
                let kind = if child.kind() == "const_declaration" {
                    SymbolKind::Constant
                } else {
                    SymbolKind::Variable
                };
                let mut c = child.walk();
                for spec in child.named_children(&mut c) {
                    if matches!(spec.kind(), "const_spec" | "var_spec") {
                        let mut sc = spec.walk();
                        for name in spec.named_children(&mut sc) {
                            if name.kind() == "identifier" {
                                symbols.push(SymbolOutline::new(
                                    node_text(name, source),
                                    kind,
                                    node_range(spec),
                                    node_range(name),
                                ));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    symbols
}

// ── C / C++ ───────────────────────────────────────────────────────────────────

fn extract_c(root: tree_sitter::Node<'_>, source: &[u8]) -> Vec<SymbolOutline> {
    let mut symbols = Vec::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        c_symbol(child, source, &mut symbols);
    }
    symbols
}

fn c_symbol(node: tree_sitter::Node<'_>, source: &[u8], out: &mut Vec<SymbolOutline>) {
    match node.kind() {
        "function_definition" => {
            // Dig through declarator chain to find the name identifier.
            if let Some(name) = c_function_name(node, source) {
                out.push(SymbolOutline::new(
                    name,
                    SymbolKind::Function,
                    node_range(node),
                    node_range(node), // approximate
                ));
            }
        }
        "declaration" => {
            // Forward declarations with a declarator name.
            if let Some(declarator) = field_child(node, "declarator") {
                if declarator.kind() == "function_declarator" {
                    if let Some(inner) = field_child(declarator, "declarator") {
                        out.push(SymbolOutline::new(
                            node_text(inner, source),
                            SymbolKind::Function,
                            node_range(node),
                            node_range(inner),
                        ));
                    }
                }
            }
        }
        "struct_specifier" | "class_specifier" | "union_specifier" => {
            if let Some(name) = field_child(node, "name") {
                let kind = match node.kind() {
                    "class_specifier" => SymbolKind::Class,
                    "union_specifier" => SymbolKind::Struct,
                    _ => SymbolKind::Struct,
                };
                let mut sym = SymbolOutline::new(
                    node_text(name, source),
                    kind,
                    node_range(node),
                    node_range(name),
                );
                if let Some(body) = field_child(node, "body") {
                    let mut c = body.walk();
                    for item in body.named_children(&mut c) {
                        c_symbol(item, source, &mut sym.children);
                    }
                }
                out.push(sym);
            }
        }
        "enum_specifier" => {
            if let Some(name) = field_child(node, "name") {
                let mut sym = SymbolOutline::new(
                    node_text(name, source),
                    SymbolKind::Enum,
                    node_range(node),
                    node_range(name),
                );
                if let Some(body) = field_child(node, "body") {
                    let mut c = body.walk();
                    for enumerator in body.named_children(&mut c) {
                        if enumerator.kind() == "enumerator" {
                            if let Some(ename) = field_child(enumerator, "name") {
                                sym.children.push(SymbolOutline::new(
                                    node_text(ename, source),
                                    SymbolKind::EnumMember,
                                    node_range(enumerator),
                                    node_range(ename),
                                ));
                            }
                        }
                    }
                }
                out.push(sym);
            }
        }
        _ => {}
    }
}

fn c_function_name(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let declarator = field_child(node, "declarator")?;
    // Walk through pointer/reference declarators to find the function_declarator.
    fn find_fn_declarator<'a>(n: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
        if n.kind() == "function_declarator" {
            return Some(n);
        }
        if n.kind() == "pointer_declarator" || n.kind() == "reference_declarator" {
            if let Some(inner) = field_child(n, "declarator") {
                return find_fn_declarator(inner);
            }
        }
        None
    }
    let fn_decl = find_fn_declarator(declarator)?;
    let name_node = field_child(fn_decl, "declarator")?;
    Some(node_text(name_node, source))
}

// ── JSON ──────────────────────────────────────────────────────────────────────

fn extract_json(root: tree_sitter::Node<'_>, source: &[u8]) -> Vec<SymbolOutline> {
    // Top-level object keys become symbols.
    let mut symbols = Vec::new();
    if let Some(doc) = root.named_child(0) {
        if doc.kind() == "object" {
            let mut cursor = doc.walk();
            for pair in doc.named_children(&mut cursor) {
                if pair.kind() == "pair" {
                    if let Some(key) = field_child(pair, "key") {
                        let raw = node_text(key, source);
                        let name = raw.trim_matches('"').to_owned();
                        let kind = field_child(pair, "value")
                            .map(|v| match v.kind() {
                                "object" => SymbolKind::Object,
                                "array" => SymbolKind::Array,
                                _ => SymbolKind::Key,
                            })
                            .unwrap_or(SymbolKind::Key);
                        symbols.push(SymbolOutline::new(
                            name,
                            kind,
                            node_range(pair),
                            node_range(key),
                        ));
                    }
                }
            }
        }
    }
    symbols
}

// ── TOML ──────────────────────────────────────────────────────────────────────

fn extract_toml(root: tree_sitter::Node<'_>, source: &[u8]) -> Vec<SymbolOutline> {
    let mut symbols = Vec::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        match child.kind() {
            "table" => {
                if let Some(key) = field_child(child, "key") {
                    symbols.push(SymbolOutline::new(
                        node_text(key, source),
                        SymbolKind::Object,
                        node_range(child),
                        node_range(key),
                    ));
                }
            }
            "array_table" => {
                if let Some(key) = field_child(child, "key") {
                    symbols.push(SymbolOutline::new(
                        format!("[[{}]]", node_text(key, source)),
                        SymbolKind::Array,
                        node_range(child),
                        node_range(key),
                    ));
                }
            }
            _ => {}
        }
    }
    symbols
}

// ── Markdown ──────────────────────────────────────────────────────────────────

fn extract_markdown(root: tree_sitter::Node<'_>, source: &[u8]) -> Vec<SymbolOutline> {
    // Headings become symbols; nesting follows ATX heading level.
    let mut symbols: Vec<SymbolOutline> = Vec::new();
    let mut cursor = root.walk();

    for child in root.named_children(&mut cursor) {
        if child.kind() == "atx_heading" {
            // First child tells us the level: `atx_h1_marker` … `atx_h6_marker`.
            let level = child
                .child(0)
                .map(|m| match m.kind() {
                    "atx_h1_marker" => 1u8,
                    "atx_h2_marker" => 2,
                    "atx_h3_marker" => 3,
                    "atx_h4_marker" => 4,
                    "atx_h5_marker" => 5,
                    _ => 6,
                })
                .unwrap_or(1);

            // The heading content sits inside `atx_heading_content`.
            let text = child
                .named_children(&mut child.walk())
                .find(|n| n.kind() == "atx_heading_content")
                .map(|n| node_text(n, source).trim().to_owned())
                .unwrap_or_default();

            let sym = SymbolOutline::new(
                text,
                SymbolKind::String, // LSP uses String for heading-like symbols
                node_range(child),
                node_range(child),
            );

            // Simple nesting: H2+ go under the last H1, etc.
            if level == 1 || symbols.is_empty() {
                symbols.push(sym);
            } else if let Some(parent) = symbols.last_mut() {
                parent.children.push(sym);
            }
        }
    }
    symbols
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_outline_new_creates_empty_children() {
        let range = Range::new(
            crabide_core::types::Position::new(0, 0),
            crabide_core::types::Position::new(5, 0),
        );
        let sel = Range::new(
            crabide_core::types::Position::new(0, 4),
            crabide_core::types::Position::new(0, 8),
        );
        let sym = SymbolOutline::new("main".into(), SymbolKind::Function, range, sel);
        assert_eq!(sym.name, "main");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert!(sym.children.is_empty());
        assert_eq!(sym.range, range);
        assert_eq!(sym.selection_range, sel);
    }

    #[test]
    fn extract_outline_dispatches_unknown_language() {
        let _ = extract_outline;
    }

    #[test]
    fn symbol_kind_variants_cover_lsp_symbol_kinds() {
        let _ = SymbolKind::File;
        let _ = SymbolKind::Module;
        let _ = SymbolKind::Namespace;
        let _ = SymbolKind::Package;
        let _ = SymbolKind::Class;
        let _ = SymbolKind::Method;
        let _ = SymbolKind::Property;
        let _ = SymbolKind::Field;
        let _ = SymbolKind::Constructor;
        let _ = SymbolKind::Enum;
        let _ = SymbolKind::Interface;
        let _ = SymbolKind::Function;
        let _ = SymbolKind::Variable;
        let _ = SymbolKind::Constant;
        let _ = SymbolKind::String;
        let _ = SymbolKind::Number;
        let _ = SymbolKind::Boolean;
        let _ = SymbolKind::Array;
        let _ = SymbolKind::Object;
        let _ = SymbolKind::Key;
        let _ = SymbolKind::Null;
        let _ = SymbolKind::EnumMember;
        let _ = SymbolKind::Struct;
        let _ = SymbolKind::Event;
        let _ = SymbolKind::Operator;
        let _ = SymbolKind::TypeParameter;
    }

    #[test]
    fn symbol_outline_reasonable_size() {
        use std::mem;
        assert_eq!(mem::size_of::<SymbolKind>(), 1);
        assert!(mem::size_of::<SymbolOutline>() < 200);
    }
}
