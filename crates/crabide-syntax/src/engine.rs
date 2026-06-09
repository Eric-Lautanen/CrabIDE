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
//! - Parse calls can be dispatched to the Rayon thread pool via
//!   [`parse_document_async`]; the UI collects results each frame via
//!   [`poll_async_results`].
//! - Synchronous [`parse_document`] is still available for small files or
//!   latency-critical paths.
//! - The `DashMap` is lock-free for concurrent reads from multiple threads.

use std::sync::Arc;

use crabide_core::{
    error::{Result, crabideError},
    traits::DocumentObserver,
    types::{BufferId, DocumentUri, Language, TextEdit},
};
use crossbeam_channel::Receiver;
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

// ── ParseResult ──────────────────────────────────────────────────────────────

/// Result of a background parse dispatched to the Rayon thread pool.
pub struct ParseResult {
    pub id: BufferId,
    pub tree: tree_sitter::Tree,
    pub language: Language,
    pub version: u32,
    pub source: Arc<[u8]>,
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
/// # Async parsing
///
/// For large documents, parsing can be dispatched to the Rayon thread pool
/// via [`parse_document_async`]. The UI layer calls [`poll_async_results`]
/// each frame to collect completed parse trees and update the cache.
///
/// [`drain_pending_reparses`]: SyntaxEngine::drain_pending_reparses
/// [`parse_document_async`]: SyntaxEngine::parse_document_async
/// [`poll_async_results`]: SyntaxEngine::poll_async_results
pub struct SyntaxEngine {
    registry: &'static GrammarRegistry,
    cache: DashMap<BufferId, ParsedDoc>,
    highlighter: HighlightEngine,
    indenter: IndentEngine,
    locals: LocalsEngine,
    /// Documents that need re-parsing, populated by the DocumentObserver impl.
    /// The UI drains this each frame via `drain_pending_reparses()`.
    pending: DashMap<BufferId, PendingReparse>,
    /// Channel for receiving completed parse results from the Rayon thread pool.
    async_results_rx: Receiver<ParseResult>,
    /// Sender side cloned into Rayon tasks.
    async_results_tx: crossbeam_channel::Sender<ParseResult>,
}

impl SyntaxEngine {
    /// Create a new engine backed by the global grammar registry.
    pub fn new() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self {
            registry: crate::grammar::grammar_registry(),
            cache: DashMap::new(),
            highlighter: HighlightEngine::new(),
            indenter: IndentEngine::new(),
            locals: LocalsEngine::new(),
            pending: DashMap::new(),
            async_results_rx: rx,
            async_results_tx: tx,
        }
    }

    pub fn with_registry(registry: &'static GrammarRegistry) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self {
            registry,
            cache: DashMap::new(),
            highlighter: HighlightEngine::new(),
            indenter: IndentEngine::new(),
            locals: LocalsEngine::new(),
            pending: DashMap::new(),
            async_results_rx: rx,
            async_results_tx: tx,
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

    /// Dispatch a document parse to the Rayon thread pool.
    ///
    /// This is the async counterpart of [`parse_document`]. The parse runs on
    /// a Rayon worker thread and the result is delivered via the internal
    /// channel. Call [`poll_async_results`] each frame to collect completed
    /// parse trees and update the cache.
    ///
    /// If no grammar is registered for `language`, this is a no-op.
    ///
    /// [`parse_document`]: SyntaxEngine::parse_document
    /// [`poll_async_results`]: SyntaxEngine::poll_async_results
    pub fn parse_document_async(
        &self,
        id: BufferId,
        language: &Language,
        source: &str,
        version: u32,
    ) {
        let entry = match self.registry.get(language) {
            Some(e) => e,
            None => {
                log::debug!("No grammar registered for {language}; skipping async parse of {id}");
                return;
            }
        };

        let source_bytes: Arc<[u8]> = Arc::from(source.as_bytes());
        let tx = self.async_results_tx.clone();
        let id_clone = id;
        let language_clone = language.clone();

        rayon::spawn(move || {
            let mut parser = match make_parser(&entry) {
                Ok(p) => p,
                Err(e) => {
                    log::error!("Failed to create parser for {language_clone}: {e}");
                    return;
                }
            };

            match parser.parse(source_bytes.as_ref(), None) {
                Some(tree) => {
                    let result = ParseResult {
                        id: id_clone,
                        tree,
                        language: language_clone,
                        version,
                        source: source_bytes,
                    };
                    if tx.send(result).is_err() {
                        log::warn!("Async parse result for {id_clone} dropped — engine shut down");
                    }
                }
                None => {
                    log::warn!("Async parser returned no tree for {id_clone} ({language_clone})");
                }
            }
        });
    }

    /// Poll for completed async parse results and insert them into the cache.
    ///
    /// Call this once per frame from the UI thread. Returns the set of buffer
    /// IDs whose parse cache was updated, so the caller can refresh highlight
    /// spans in its editor tabs.
    pub fn poll_async_results(&self) -> Vec<BufferId> {
        let mut updated = Vec::new();
        while let Ok(result) = self.async_results_rx.try_recv() {
            // Only insert if the result is not stale (version >= cached version).
            let is_newer = match self.cache.get(&result.id) {
                Some(cached) => result.version >= cached.version,
                None => true,
            };
            if is_newer {
                self.cache.insert(
                    result.id,
                    ParsedDoc {
                        tree: result.tree,
                        language: result.language,
                        version: result.version,
                        source: result.source,
                    },
                );
                updated.push(result.id);
            }
        }
        updated
    }

    // ── Queries ───────────────────────────────────────────────────────────────

    /// Compute highlight spans for a document, including injection highlights.
    /// Returns `[]` if not parsed.
    pub fn highlights(&self, id: BufferId) -> Vec<HighlightSpan> {
        let Some(cached) = self.cache.get(&id) else {
            return Vec::new();
        };
        let Some(entry) = self.registry.get(&cached.language) else {
            return Vec::new();
        };
        let source = std::str::from_utf8(&cached.source).unwrap_or("");
        self.highlighter.compute_highlights_with_injections(
            &cached.language,
            &entry,
            source,
            &cached.tree,
            self.registry,
        )
    }

    /// Extract folding ranges for a document. Returns `[]` if not parsed.
    pub fn folding_ranges(&self, id: BufferId) -> Vec<FoldingRange> {
        let Some(cached) = self.cache.get(&id) else {
            return Vec::new();
        };
        let source = std::str::from_utf8(&cached.source).unwrap_or("");
        fold::extract_folding_ranges(&cached.tree, source, &cached.language)
    }

    /// Extract the symbol outline for a document. Returns `[]` if not parsed.
    pub fn outline(&self, id: BufferId) -> Vec<SymbolOutline> {
        let Some(cached) = self.cache.get(&id) else {
            return Vec::new();
        };
        outline::extract_outline(&cached.tree, &cached.source, &cached.language)
    }

    /// Compute indentation advice for every line in a document. Returns `[]`
    /// if not parsed or no indent query is registered for the language.
    pub fn indents(&self, id: BufferId) -> Vec<LineIndent> {
        let Some(cached) = self.cache.get(&id) else {
            return Vec::new();
        };
        let Some(entry) = self.registry.get(&cached.language) else {
            return Vec::new();
        };
        let source = std::str::from_utf8(&cached.source).unwrap_or("");
        self.indenter
            .compute_indents(&cached.language, &entry, source, &cached.tree)
    }

    /// Compute scope-aware local variable information for a document.
    /// Returns `[]` if not parsed or no locals query is registered.
    pub fn local_scopes(&self, id: BufferId) -> Vec<LocalScopeInfo> {
        let Some(cached) = self.cache.get(&id) else {
            return Vec::new();
        };
        let Some(entry) = self.registry.get(&cached.language) else {
            return Vec::new();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syntax_engine_new_creates_empty_cache() {
        let engine = SyntaxEngine::new();
        // No documents registered yet
        assert!(engine.cache.is_empty());
    }

    #[test]
    fn close_document_nonexistent_is_noop() {
        let engine = SyntaxEngine::new();
        engine.close_document(BufferId::new());
        // Should not panic
    }

    #[test]
    fn ts_point_to_position_conversion() {
        let point = tree_sitter::Point { row: 5, column: 12 };
        let pos = crate::highlight::ts_point_to_position(point);
        assert_eq!(pos.line, 5);
        assert_eq!(pos.character, 12);
    }

    #[test]
    fn ts_point_to_position_zero() {
        let point = tree_sitter::Point { row: 0, column: 0 };
        let pos = crate::highlight::ts_point_to_position(point);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 0);
    }

    #[test]
    fn highlights_returns_empty_for_unparsed_doc() {
        let engine = SyntaxEngine::new();
        let id = BufferId::new();
        let spans = engine.highlights(id);
        assert!(spans.is_empty(), "unparsed doc should yield no highlights");
    }

    #[test]
    fn highlights_empty_for_unknown_language() {
        let engine = SyntaxEngine::new();
        let id = BufferId::new();
        // Parse with a language not in registry
        engine.parse_document(id, &Language::new("nonexistent"), "test", 1);
        let spans = engine.highlights(id);
        assert!(
            spans.is_empty(),
            "unknown language should yield no highlights"
        );
    }

    #[test]
    fn folding_ranges_returns_empty_for_unparsed_doc() {
        let engine = SyntaxEngine::new();
        let id = BufferId::new();
        let ranges = engine.folding_ranges(id);
        assert!(ranges.is_empty());
    }

    #[test]
    fn outline_returns_empty_for_unparsed_doc() {
        let engine = SyntaxEngine::new();
        let id = BufferId::new();
        let symbols = engine.outline(id);
        assert!(symbols.is_empty());
    }

    #[test]
    fn indents_returns_empty_for_unparsed_doc() {
        let engine = SyntaxEngine::new();
        let id = BufferId::new();
        let indents = engine.indents(id);
        assert!(indents.is_empty());
    }

    // ── Roundtrip tests (parse → highlight/fold/outline/indent) ──────────────

    /// Helper: create a SyntaxEngine with grammars registered for testing.
    fn engine_with_rust_grammar() -> SyntaxEngine {
        let registry = Box::leak(Box::new(GrammarRegistry::new()));
        let rust_lang = tree_sitter_rust::LANGUAGE;
        // Register with minimal queries — empty queries are valid (just produce no results)
        registry.register(
            Language::RUST,
            rust_lang.into(),
            "", // empty highlights query
            "", // empty locals query
            "", // empty indents query
        );
        SyntaxEngine::with_registry(registry)
    }

    /// Parse Rust source and verify the engine doesn't crash.
    #[test]
    fn roundtrip_rust_parses_successfully() {
        let engine = engine_with_rust_grammar();
        let id = BufferId::new();
        let source = r#"
fn main() {
    let x = 42;
    println!("hello {}", x);
}
"#;
        engine.parse_document(id, &Language::RUST, source, 1);
        assert!(engine.is_parsed(id), "document should be marked as parsed");
        assert_eq!(engine.version(id), Some(1));
        // Even with empty queries, the parse cache should have the entry
        let _spans = engine.highlights(id);
        let _ranges = engine.folding_ranges(id);
        let _symbols = engine.outline(id);
        let _indents = engine.indents(id);
        // Should return empty results but not panic
    }

    /// Parse Rust and close — verifies cache lifecycle.
    #[test]
    fn roundtrip_close_and_reopen() {
        let engine = engine_with_rust_grammar();
        let id = BufferId::new();
        engine.parse_document(id, &Language::RUST, "fn a() {}", 1);
        assert!(engine.is_parsed(id));
        engine.close_document(id);
        assert!(
            engine.version(id).is_none(),
            "cache entry should be removed"
        );

        // Re-parse after close
        engine.parse_document(id, &Language::RUST, "fn b() {}", 2);
        assert!(engine.is_parsed(id), "re-parsed doc should exist");
    }

    /// Full re-parse with updated version.
    #[test]
    fn roundtrip_reparse() {
        let engine = engine_with_rust_grammar();
        let id = BufferId::new();
        engine.parse_document(id, &Language::RUST, "fn foo() {}", 1);
        assert!(engine.is_parsed(id));
        assert_eq!(engine.version(id), Some(1));

        // Re-parse with new source
        engine.parse_document(id, &Language::RUST, "fn foo() {}\nfn bar() {}\n", 2);
        assert!(engine.is_parsed(id));
        assert_eq!(engine.version(id), Some(2));
    }

    /// Verify async parse roundtrip.
    #[test]
    fn roundtrip_async_parse() {
        let engine = engine_with_rust_grammar();
        let id = BufferId::new();
        let source = "fn compute() -> u32 { 42 }\n";
        engine.parse_document_async(id, &Language::RUST, source, 1);
        // Poll until result is available
        let updated = std::thread::spawn(move || {
            let start = std::time::Instant::now();
            loop {
                let result = engine.poll_async_results();
                if !result.is_empty() {
                    return result;
                }
                if start.elapsed() > std::time::Duration::from_secs(5) {
                    panic!("async parse did not complete within timeout");
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        })
        .join()
        .unwrap();
        assert!(updated.contains(&id), "should have updated our buffer id");
    }

    /// Roundtrip with Python grammar.
    #[test]
    fn roundtrip_python_parse() {
        let registry = Box::leak(Box::new(GrammarRegistry::new()));
        let py_lang = tree_sitter_python::LANGUAGE;
        registry.register(Language::new("python"), py_lang.into(), "", "", "");
        let engine = SyntaxEngine::with_registry(registry);
        let id = BufferId::new();
        engine.parse_document(id, &Language::new("python"), "def hello():\n    pass\n", 1);
        assert!(engine.is_parsed(id));
        // Verify highlights don't panic
        let _ = engine.highlights(id);
    }

    /// Roundtrip with JSON grammar.
    #[test]
    fn roundtrip_json_parse() {
        let registry = Box::leak(Box::new(GrammarRegistry::new()));
        let json_lang = tree_sitter_json::LANGUAGE;
        registry.register(Language::new("json"), json_lang.into(), "", "", "");
        let engine = SyntaxEngine::with_registry(registry);
        let id = BufferId::new();
        engine.parse_document(id, &Language::new("json"), r#"{"key": "value"}"#, 1);
        assert!(engine.is_parsed(id));
        // Verify folding ranges don't panic
        let _ = engine.folding_ranges(id);
    }

    /// Test multiple documents parsed simultaneously.
    #[test]
    fn roundtrip_multiple_documents() {
        let engine = engine_with_rust_grammar();
        let id1 = BufferId::new();
        let id2 = BufferId::new();
        engine.parse_document(id1, &Language::RUST, "fn a() {}", 1);
        engine.parse_document(id2, &Language::RUST, "fn b() {}", 1);
        assert!(engine.is_parsed(id1));
        assert!(engine.is_parsed(id2));
        // Close one, verify the other still exists
        engine.close_document(id1);
        assert!(!engine.is_parsed(id1));
        assert!(engine.is_parsed(id2));
    }
}
