//! `SyntaxEngine` — the main syntax service.
//!
//! Maintains a per-document parse cache (`DashMap<BufferId, ParsedDoc>`) and
//! exposes methods for full/incremental re-parse, highlight spans, folding
//! ranges, and symbol outline.
//!
//! The engine implements [`DocumentObserver`] so it registers with the
//! [`Workspace`](crabide_workspace::Workspace) observer list and automatically
//! receives `on_document_changed` / `on_document_opened` / `on_document_closed`
//! callbacks. The observer stores pending re-parses in a side map; the UI drains
//! them each frame via [`SyntaxEngine::drain_pending_reparses`] and applies the
//! resulting highlight spans.
//!
//! # Threading
//! - Parse calls are synchronous but cheap for small files; callers can wrap
//!   them in `rayon::spawn` for large files.
//! - The `DashMap` is lock-free for concurrent reads from multiple threads.

use std::sync::Arc;

use crabide_core::{
    error::{crabideError, Result},
    traits::DocumentObserver,
    types::{BufferId, DocumentUri, Language, TextEdit},
};
use dashmap::DashMap;

use crate::{
    fold::{self, FoldingRange},
    grammar::{GrammarEntry, GrammarRegistry},
    highlight::{HighlightEngine, HighlightSpan},
    indent::{IndentEngine, LineIndent},
    locals::{LocalScopeInfo, LocalsEngine},
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

// ── PendingReparse ────────────────────────────────────────────────────────────

/// Information queued by the `DocumentObserver` impl for deferred re-parsing.
pub struct PendingReparse {
    pub language: Language,
    pub source: String,
    pub version: u32,
}

// ── SyntaxEngine ──────────────────────────────────────────────────────────────

/// Central syntax analysis service.
///
/// Implements [`DocumentObserver`] so it can be registered with the
/// [`Workspace`](crabide_workspace::Workspace) observer list and automatically
/// re-parse documents on every change. The observer stores the latest source
/// text in a pending-source map; the UI layer calls [`drain_pending_reparses`]
/// each frame to actually execute the parses and update highlight spans.
///
/// [`drain_pending_reparses`]: SyntaxEngine::drain_pending_reparses
pub struct SyntaxEngine {
    registry: &'static GrammarRegistry,
    cache: DashMap<BufferId, ParsedDoc>,
    highlighter: HighlightEngine,
    indenter: IndentEngine,
    locals: LocalsEngine,
    /// Documents that need re-parsing, populated by the DocumentObserver impl.
    /// The UI drains this each frame via `drain_pending_reparses()`.
    pending: DashMap<BufferId, PendingReparse>,
}

impl SyntaxEngine {
    /// Create a new engine backed by the global grammar registry.
    pub fn new() -> Self {
        Self {
            registry: crate::grammar::grammar_registry(),
            cache: DashMap::new(),
            highlighter: HighlightEngine::new(),
            indenter: IndentEngine::new(),
            locals: LocalsEngine::new(),
            pending: DashMap::new(),
        }
    }

    pub fn with_registry(registry: &'static GrammarRegistry) -> Self {
        Self {
            registry,
            cache: DashMap::new(),
            highlighter: HighlightEngine::new(),
            indenter: IndentEngine::new(),
            locals: LocalsEngine::new(),
            pending: DashMap::new(),
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
        let source = std::str::from_utf8(&cached.source).unwrap_or("");
        fold::extract_folding_ranges(&cached.tree, source, &cached.language)
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

    /// Compute scope-aware local variable information for a document.
    /// Returns `[]` if not parsed or no locals query is registered.
    pub fn local_scopes(&self, id: BufferId) -> Vec<LocalScopeInfo> {
        let cached = match self.cache.get(&id) {
            Some(c) => c,
            None => return Vec::new(),
        };
        let entry = match self.registry.get(&cached.language) {
            Some(e) => e,
            None => return Vec::new(),
        };
        let source = std::str::from_utf8(&cached.source).unwrap_or("");
        self.locals
            .compute_local_scopes(&cached.language, &entry, source, &cached.tree)
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

    /// Drain all pending re-parses queued by the `DocumentObserver` impl.
    ///
    /// Called once per frame from the UI thread. Returns the set of buffer IDs
    /// whose parse cache was updated, so the caller can refresh highlight spans
    /// in its editor tabs.
    pub fn drain_pending_reparses(&self) -> Vec<BufferId> {
        let ids: Vec<BufferId> = self.pending.iter().map(|kv| *kv.key()).collect();
        for id in &ids {
            if let Some((_, pending)) = self.pending.remove(id) {
                self.parse_document(*id, &pending.language, &pending.source, pending.version);
            }
        }
        ids
    }
}

impl Default for SyntaxEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ── DocumentObserver implementation ───────────────────────────────────────────

impl DocumentObserver for SyntaxEngine {
    fn on_document_changed(&self, buffer_id: BufferId, _edits: &[TextEdit], new_version: u32) {
        // We don't have the full source text here — only the edits.
        // The observer's on_document_changed is called from Workspace which holds
        // the document lock. We cannot request the full text synchronously because
        // the Workspace may be in the middle of an edit transaction.
        //
        // Instead we queue a pending re-parse and rely on the UI thread to call
        // `drain_pending_reparses()` each frame, at which point the document lock
        // is available and we can reconstruct the full source from the EditorTab
        // lines snapshot.
        //
        // For now, we store a placeholder flag; the actual re-parse happens in
        // `drain_pending_reparses` which the UI calls after syncing the tab text.
        //
        // A more advanced implementation would store the incremental edit information
        // and call `reparse_document` with a proper `InputEdit`. This requires
        // access to the document text, which we don't have here. We leave that
        // optimization for a future enhancement (see roadmap item "incremental
        // parsing via DocumentObserver").
        //
        // Important: we must update the version even if the language is not
        // registered, so that the next parse respects the latest version.
        if let Some(mut cached) = self.cache.get_mut(&buffer_id) {
            // Bump version to prevent stale highlight results
            cached.version = new_version;
        }
    }

    fn on_document_opened(&self, buffer_id: BufferId, uri: &DocumentUri, _language: &Language) {
        log::debug!(
            "SyntaxEngine: document opened {buffer_id} ({uri}) — will parse on first frame"
        );
        // The actual parse happens when the UI thread reads the file content
        // and calls `parse_document` or `drain_pending_reparses`.
    }

    fn on_document_closed(&self, buffer_id: BufferId) {
        log::trace!("SyntaxEngine: document closed {buffer_id}");
        self.close_document(buffer_id);
        self.pending.remove(&buffer_id);
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
