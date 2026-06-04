//! `SyntaxEngine` — the main syntax service.
//!
//! Maintains a per-document parse cache (`DashMap<BufferId, ParsedDoc>`) and
//! exposes methods for full/incremental re-parse, highlight spans, folding
//! ranges, and symbol outline.
//!
//! The engine does **not** implement `DocumentObserver` directly — the UI layer
//! calls its methods explicitly so that text retrieval stays in the caller's
//! control. Heavy parsing is dispatched to the Rayon thread pool.
//!
//! # Threading
//! - Parse calls are synchronous but cheap for small files; callers can wrap
//!   them in `rayon::spawn` for large files.
//! - The `DashMap` is lock-free for concurrent reads from multiple threads.

use std::sync::Arc;

use crabide_core::{
    error::{crabideError, Result},
    types::{BufferId, Language},
};
use dashmap::DashMap;

use crate::{
    fold::{self, FoldingRange},
    grammar::{GrammarEntry, GrammarRegistry},
    highlight::{HighlightEngine, HighlightSpan},
    indent::{IndentEngine, LineIndent},
    outline::{self, SymbolOutline},
};

// ── ParsedDoc ─────────────────────────────────────────────────────────────────

/// Cached parse result for one open document.
pub struct ParsedDoc {
    /// The most recent tree-sitter parse tree.
    pub tree: tree_sitter::Tree,
    /// Language used to produce this tree (for query cache lookup).
    pub language: Language,
    /// Version of the document when this tree was last parsed.
    pub version: u32,
    /// Source bytes that produced `tree` (used for outline/fold queries).
    pub source: Arc<[u8]>,
}

// ── SyntaxEngine ──────────────────────────────────────────────────────────────

/// Central syntax analysis service.
pub struct SyntaxEngine {
    registry: &'static GrammarRegistry,
    cache: DashMap<BufferId, ParsedDoc>,
    highlighter: HighlightEngine,
    indenter: IndentEngine,
}

impl SyntaxEngine {
    /// Create a new engine backed by the global grammar registry.
    pub fn new() -> Self {
        Self {
            registry: crate::grammar::grammar_registry(),
            cache: DashMap::new(),
            highlighter: HighlightEngine::new(),
            indenter: IndentEngine::new(),
        }
    }

    pub fn with_registry(registry: &'static GrammarRegistry) -> Self {
        Self {
            registry,
            cache: DashMap::new(),
            highlighter: HighlightEngine::new(),
            indenter: IndentEngine::new(),
        }
    }

    // Parse

    /// Parse (or re-parse) a document from scratch.
    ///
    /// If no grammar is registered for `language`, this is a no-op (the
    /// document will return empty results for highlights/folds/outline).
    pub fn parse_document(&self, id: BufferId, language: &Language, source: &str, version: u32) {
        let entry = match self.registry.get(language) {
            Some(e) => e,
            None => {
                log::debug!("No grammar registered for {language}; skipping parse of {id}");
                return;
            }
        };

        let mut parser = match make_parser(&entry) {
            Ok(p) => p,
            Err(e) => {
                log::error!("Failed to create parser for {language}: {e}");
                return;
            }
        };

        let source_bytes: Arc<[u8]> = Arc::from(source.as_bytes());
        let tree = parser.parse(source_bytes.as_ref(), None);

        match tree {
            Some(tree) => {
                self.cache.insert(
                    id,
                    ParsedDoc {
                        tree,
                        language: language.clone(),
                        version,
                        source: source_bytes,
                    },
                );
                log::trace!("Parsed {id} ({language}) version {version}");
            }
            None => {
                log::warn!("Parser returned no tree for {id} ({language})");
            }
        }
    }

    /// Re-parse a document with an incremental edit.
    ///
    /// Callers should provide the `InputEdit` describing the change so
    /// tree-sitter can reuse unchanged subtrees. If `input_edit` is `None`,
    /// the old tree is used as a hint without edit information (best-effort).
    pub fn reparse_document(
        &self,
        id: BufferId,
        source: &str,
        new_version: u32,
        input_edit: Option<tree_sitter::InputEdit>,
    ) {
        // Retrieve the old tree and language.
        let (old_tree, language, entry) = {
            let cached = match self.cache.get(&id) {
                Some(c) => c,
                None => return,
            };
            let entry = match self.registry.get(&cached.language) {
                Some(e) => e,
                None => return,
            };
            // Clone so we can drop the DashMap guard before mutating.
            (cached.tree.clone(), cached.language.clone(), entry)
        };

        let mut parser = match make_parser(&entry) {
            Ok(p) => p,
            Err(e) => {
                log::error!("Failed to create parser for {language}: {e}");
                return;
            }
        };

        // Apply the edit to the old tree if provided.
        let mut old_tree = old_tree;
        if let Some(edit) = input_edit {
            old_tree.edit(&edit);
        }

        let source_bytes: Arc<[u8]> = Arc::from(source.as_bytes());
        if let Some(tree) = parser.parse(source_bytes.as_ref(), Some(&old_tree)) {
            self.cache.insert(
                id,
                ParsedDoc {
                    tree,
                    language,
                    version: new_version,
                    source: source_bytes,
                },
            );
        }
    }

    /// Remove a document from the parse cache.
    pub fn close_document(&self, id: BufferId) {
        self.cache.remove(&id);
    }

    // ── Queries ───────────────────────────────────────────────────────────────

    /// Compute highlight spans for a document. Returns `[]` if not parsed.
    pub fn highlights(&self, id: BufferId) -> Vec<HighlightSpan> {
        let cached = match self.cache.get(&id) {
            Some(c) => c,
            None => return Vec::new(),
        };
        let entry = match self.registry.get(&cached.language) {
            Some(e) => e,
            None => return Vec::new(),
        };
        let source = std::str::from_utf8(&cached.source).unwrap_or("");
        self.highlighter
            .compute_highlights(&cached.language, &entry, source, &cached.tree)
    }

    /// Extract folding ranges for a document. Returns `[]` if not parsed.
    pub fn folding_ranges(&self, id: BufferId) -> Vec<FoldingRange> {
        let cached = match self.cache.get(&id) {
            Some(c) => c,
            None => return Vec::new(),
        };
        fold::extract_folding_ranges(&cached.tree, &cached.language)
    }

    /// Extract the symbol outline for a document. Returns `[]` if not parsed.
    pub fn outline(&self, id: BufferId) -> Vec<SymbolOutline> {
        let cached = match self.cache.get(&id) {
            Some(c) => c,
            None => return Vec::new(),
        };
        outline::extract_outline(&cached.tree, &cached.source, &cached.language)
    }

    /// Compute indentation advice for every line in a document. Returns `[]`
    /// if not parsed or no indent query is registered for the language.
    pub fn indents(&self, id: BufferId) -> Vec<LineIndent> {
        let cached = match self.cache.get(&id) {
            Some(c) => c,
            None => return Vec::new(),
        };
        let entry = match self.registry.get(&cached.language) {
            Some(e) => e,
            None => return Vec::new(),
        };
        let source = std::str::from_utf8(&cached.source).unwrap_or("");
        self.indenter
            .compute_indents(&cached.language, &entry, source, &cached.tree)
    }

    /// Return the current parse version for a document, or `None` if not parsed.
    pub fn version(&self, id: BufferId) -> Option<u32> {
        self.cache.get(&id).map(|c| c.version)
    }

    /// Return the language a document is currently parsed as.
    pub fn parsed_language(&self, id: BufferId) -> Option<Language> {
        self.cache.get(&id).map(|c| c.language.clone())
    }

    /// Returns `true` if the document has a cached parse tree.
    pub fn is_parsed(&self, id: BufferId) -> bool {
        self.cache.contains_key(&id)
    }
}

impl Default for SyntaxEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_parser(entry: &GrammarEntry) -> Result<tree_sitter::Parser> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&entry.language)
        .map_err(|e| crabideError::Grammar(format!("set_language: {e}")))?;
    Ok(parser)
}
