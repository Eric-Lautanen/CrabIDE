//! Scope-aware highlighting using tree-sitter locals queries.
//!
//! The locals query defines scope boundaries (`@scope`), local definitions
//! (`@definition.*`), and references (`@reference.*`). When combined with the
//! highlights query, this allows distinguishing local variables from global
//! ones, parameters from regular variables, etc.
//!
//! The approach follows the Helix / Neovim model:
//! 1. Run the locals query to build a scope tree with definitions.
//! 2. For each `@reference` capture, look up the definition in the nearest
//!    enclosing scope to determine the variable's kind (parameter, local,
//!    etc.).
//! 3. Refine highlight spans: a bare `@variable` that resolves to a local
//!    definition becomes `@variable.local`, a parameter becomes
//!    `@variable.parameter`, etc.

use std::sync::Arc;

use dashmap::DashMap;
use streaming_iterator::StreamingIterator as _;

use crabide_core::types::Language;

use crate::grammar::GrammarEntry;

/// A local definition captured by the locals query.
#[derive(Debug, Clone)]
struct LocalDef {
    /// Byte range of the definition name.
    start_byte: usize,
    end_byte: usize,
    /// The kind of definition: "function", "parameter", "var", "type", etc.
    kind: String,
}

/// A scope boundary captured by `@scope`.
#[derive(Debug, Clone)]
struct Scope {
    /// Byte range of the scope.
    start_byte: usize,
    end_byte: usize,
    /// Definitions declared in this scope.
    defs: Vec<LocalDef>,
}

/// A scope boundary captured by `@scope`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedScope {
    /// The reference resolves to a parameter definition.
    Parameter,
    /// The reference resolves to a local variable definition.
    Local,
    /// The reference resolves to a function/method definition.
    Function,
    /// The reference resolves to a type definition.
    Type,
    /// The reference could not be resolved (global or unknown).
    Unresolved,
}

/// Caches compiled locals queries per language and runs scope resolution.
pub struct LocalsEngine {
    query_cache: DashMap<Language, Option<Arc<tree_sitter::Query>>>,
}

impl LocalsEngine {
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

        let query_src = entry.locals_query.as_ref();
        if query_src.is_empty() {
            self.query_cache.insert(language.clone(), None);
            return None;
        }

        let compiled = match tree_sitter::Query::new(&entry.language, query_src) {
            Ok(q) => {
                log::debug!("Compiled locals query for {language}");
                Some(Arc::new(q))
            }
            Err(e) => {
                log::warn!("Locals query compile error for {language}: {e:?}");
                None
            }
        };

        self.query_cache.insert(language.clone(), compiled.clone());
        compiled
    }

    /// Resolve the scope kind for a reference at the given byte range.
    ///
    /// Walks the scope tree from innermost to outermost, looking for a
    /// definition whose byte range matches the reference's name.
    pub fn resolve_reference(
        &self,
        language: &Language,
        entry: &GrammarEntry,
        source: &str,
        tree: &tree_sitter::Tree,
        ref_start_byte: usize,
        ref_end_byte: usize,
    ) -> ResolvedScope {
        let Some(query) = self.get_query(language, entry) else {
            return ResolvedScope::Unresolved;
        };

        let source_bytes = source.as_bytes();
        let root = tree.root_node();
        let mut cursor = tree_sitter::QueryCursor::new();
        let capture_names = query.capture_names();

        let mut scopes: Vec<Scope> = Vec::new();

        let mut matches_iter = cursor.matches(query.as_ref(), root, source_bytes);
        while let Some(mat) = matches_iter.next() {
            for capture in mat.captures {
                let node = capture.node;
                let name = &capture_names[capture.index as usize];

                if *name == "scope" {
                    scopes.push(Scope {
                        start_byte: node.start_byte(),
                        end_byte: node.end_byte(),
                        defs: Vec::new(),
                    });
                } else if let Some(kind) = name.strip_prefix("definition.") {
                    let def = LocalDef {
                        start_byte: node.start_byte(),
                        end_byte: node.end_byte(),
                        kind: kind.to_owned(),
                    };
                    if let Some(scope) = scopes.iter_mut().rev().find(|s| {
                        node.start_byte() >= s.start_byte && node.end_byte() <= s.end_byte
                    }) {
                        scope.defs.push(def);
                    }
                }
            }
        }

        // Sort scopes by start_byte descending (innermost first) for lookup.
        scopes.sort_by_key(|b| std::cmp::Reverse(b.start_byte));

        // Find the definition matching the reference's text.
        let ref_text = &source[ref_start_byte..ref_end_byte];

        for scope in &scopes {
            if ref_start_byte >= scope.start_byte && ref_end_byte <= scope.end_byte {
                for def in &scope.defs {
                    if def.start_byte == ref_start_byte && def.end_byte == ref_end_byte {
                        return def_kind_to_resolved(&def.kind);
                    }
                    let def_text = &source[def.start_byte..def.end_byte];
                    if def_text == ref_text {
                        return def_kind_to_resolved(&def.kind);
                    }
                }
            }
        }

        ResolvedScope::Unresolved
    }

    /// Compute all scope-aware highlight refinements for a document.
    ///
    /// Returns a map from (start_byte, end_byte) to the refined scope name
    /// suffix. The highlight engine can use this to upgrade bare `@variable`
    /// captures to `@variable.parameter`, `@variable.local`, etc.
    pub fn compute_local_scopes(
        &self,
        language: &Language,
        entry: &GrammarEntry,
        source: &str,
        tree: &tree_sitter::Tree,
    ) -> Vec<LocalScopeInfo> {
        let Some(query) = self.get_query(language, entry) else {
            return Vec::new();
        };

        let source_bytes = source.as_bytes();
        let root = tree.root_node();
        let mut cursor = tree_sitter::QueryCursor::new();
        let capture_names = query.capture_names();

        let mut scopes: Vec<Scope> = Vec::new();
        let mut results: Vec<LocalScopeInfo> = Vec::new();

        let mut matches_iter = cursor.matches(query.as_ref(), root, source_bytes);
        while let Some(mat) = matches_iter.next() {
            for capture in mat.captures {
                let node = capture.node;
                let name = &capture_names[capture.index as usize];

                if *name == "scope" {
                    scopes.push(Scope {
                        start_byte: node.start_byte(),
                        end_byte: node.end_byte(),
                        defs: Vec::new(),
                    });
                } else if let Some(kind) = name.strip_prefix("definition.") {
                    let def = LocalDef {
                        start_byte: node.start_byte(),
                        end_byte: node.end_byte(),
                        kind: kind.to_owned(),
                    };
                    if let Some(scope) = scopes.iter_mut().rev().find(|s| {
                        node.start_byte() >= s.start_byte && node.end_byte() <= s.end_byte
                    }) {
                        scope.defs.push(def);
                    }
                    results.push(LocalScopeInfo {
                        start_byte: node.start_byte(),
                        end_byte: node.end_byte(),
                        resolved: def_kind_to_resolved(kind),
                    });
                } else if let Some(kind) = name.strip_prefix("reference.") {
                    results.push(LocalScopeInfo {
                        start_byte: node.start_byte(),
                        end_byte: node.end_byte(),
                        resolved: ref_kind_to_resolved(kind),
                    });
                }
            }
        }

        // For references, try to resolve them against the scope tree.
        scopes.sort_by_key(|b| std::cmp::Reverse(b.start_byte));
        for info in &mut results {
            if info.resolved == ResolvedScope::Unresolved {
                let text = &source[info.start_byte..info.end_byte];
                for scope in &scopes {
                    if info.start_byte >= scope.start_byte && info.end_byte <= scope.end_byte {
                        for def in &scope.defs {
                            let def_text = &source[def.start_byte..def.end_byte];
                            if def_text == text {
                                info.resolved = def_kind_to_resolved(&def.kind);
                                break;
                            }
                        }
                        if info.resolved != ResolvedScope::Unresolved {
                            break;
                        }
                    }
                }
            }
        }

        results
    }
}

impl Default for LocalsEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Scope resolution info for a single capture.
#[derive(Debug, Clone)]
pub struct LocalScopeInfo {
    /// Start byte offset of the captured node.
    pub start_byte: usize,
    /// End byte offset of the captured node.
    pub end_byte: usize,
    /// The resolved scope kind.
    pub resolved: ResolvedScope,
}

fn def_kind_to_resolved(kind: &str) -> ResolvedScope {
    match kind {
        "function" | "method" | "macro" => ResolvedScope::Function,
        "type" | "struct" | "enum" | "interface" | "trait" => ResolvedScope::Type,
        "param" | "parameter" => ResolvedScope::Parameter,
        "var" | "variable" | "const" | "constant" => ResolvedScope::Local,
        _ => ResolvedScope::Local,
    }
}

fn ref_kind_to_resolved(kind: &str) -> ResolvedScope {
    match kind {
        "call" => ResolvedScope::Function,
        "type" => ResolvedScope::Type,
        _ => ResolvedScope::Unresolved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_scope_from_def_kind() {
        assert_eq!(def_kind_to_resolved("function"), ResolvedScope::Function);
        assert_eq!(def_kind_to_resolved("method"), ResolvedScope::Function);
        assert_eq!(def_kind_to_resolved("param"), ResolvedScope::Parameter);
        assert_eq!(def_kind_to_resolved("parameter"), ResolvedScope::Parameter);
        assert_eq!(def_kind_to_resolved("var"), ResolvedScope::Local);
        assert_eq!(def_kind_to_resolved("type"), ResolvedScope::Type);
        assert_eq!(def_kind_to_resolved("struct"), ResolvedScope::Type);
    }

    #[test]
    fn resolved_scope_from_ref_kind() {
        assert_eq!(ref_kind_to_resolved("call"), ResolvedScope::Function);
        assert_eq!(ref_kind_to_resolved("type"), ResolvedScope::Type);
        assert_eq!(ref_kind_to_resolved(""), ResolvedScope::Unresolved);
    }
}
