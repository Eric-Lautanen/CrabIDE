//! `Workspace` — multi-root document lifecycle manager.
//!
//! The `Workspace` is the central hub connecting the VFS, text buffers,
//! edit history, and all observer services (LSP, syntax highlighter, UI).
//!
//! # Threading
//! - All async methods (`open_file`, `save`, …) are called from the Tokio pool.
//! - Synchronous methods (`apply_edit`, `undo`, …) can be called from any thread.
//! - Internal state uses `DashMap` (lock-free reads) and `parking_lot::RwLock`.
//! - Observers are notified synchronously under no lock — they must be fast
//!   (typically they just `tx.try_send(event)` on a crossbeam channel).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use parking_lot::RwLock;

use crabide_buffer::{CursorSet, Document, EditHistory};
use crabide_core::{
    error::{crabideError, Result},
    traits::{DocumentObserver, TextBuffer, VirtualFileSystem},
    types::{language_from_extension, BufferId, DocumentUri, Language, TextEdit},
};

// ── DocumentEntry ─────────────────────────────────────────────────────────────

/// All per-document state kept by the workspace.
pub struct DocumentEntry {
    pub document: Document,
    pub history: EditHistory,
    pub cursors: CursorSet,
}

impl DocumentEntry {
    fn new(document: Document) -> Self {
        let initial_rope = document.rope_snapshot();
        Self {
            document,
            history: EditHistory::new(initial_rope),
            cursors: CursorSet::new(),
        }
    }
}

// ── CloseResult ───────────────────────────────────────────────────────────────

/// Result of a `close()` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseResult {
    /// The document was closed successfully.
    Closed,
    /// The document has unsaved changes and `force` was false.
    /// The caller must prompt the user and call `close(id, true)` to confirm.
    UnsavedChanges,
}

// ── Workspace ─────────────────────────────────────────────────────────────────

/// Manages the set of open workspace roots and all open documents.
pub struct Workspace {
    /// Open workspace root directories (may be empty for single-file mode).
    roots: RwLock<Vec<PathBuf>>,
    /// Open documents keyed by their `BufferId`.
    documents: DashMap<BufferId, Arc<RwLock<DocumentEntry>>>,
    /// URI → BufferId index for fast "is this already open?" lookups.
    uri_index: DashMap<DocumentUri, BufferId>,
    /// The virtual filesystem used for all I/O.
    vfs: Arc<dyn VirtualFileSystem>,
    /// Registered observers (LSP client, syntax engine, UI, etc.).
    observers: RwLock<Vec<Arc<dyn DocumentObserver>>>,
    /// Counter used to generate unique untitled document titles.
    untitled_counter: AtomicU32,
}

impl Workspace {
    /// Create a new empty workspace backed by `vfs`.
    pub fn new(vfs: Arc<dyn VirtualFileSystem>) -> Self {
        Self {
            roots: RwLock::new(Vec::new()),
            documents: DashMap::new(),
            uri_index: DashMap::new(),
            vfs,
            observers: RwLock::new(Vec::new()),
            untitled_counter: AtomicU32::new(1),
        }
    }

    // ── Workspace roots ───────────────────────────────────────────────────────

    pub fn add_root(&self, path: PathBuf) {
        self.roots.write().push(path);
    }

    pub fn remove_root(&self, path: &Path) {
        self.roots.write().retain(|p| p != path);
    }

    pub fn roots(&self) -> Vec<PathBuf> {
        self.roots.read().clone()
    }

    // ── Observer registration ─────────────────────────────────────────────────

    /// Register an observer to receive document lifecycle events.
    pub fn add_observer(&self, observer: Arc<dyn DocumentObserver>) {
        self.observers.write().push(observer);
    }

    // ── Document queries ──────────────────────────────────────────────────────

    /// Return the `BufferId` for an already-open URI, if any.
    pub fn get_buffer_id(&self, uri: &DocumentUri) -> Option<BufferId> {
        self.uri_index.get(uri).map(|r| *r)
    }

    /// Return all currently open buffer IDs.
    pub fn open_buffer_ids(&self) -> Vec<BufferId> {
        self.documents.iter().map(|r| *r.key()).collect()
    }

    /// Returns true if the document has unsaved changes.
    pub fn is_dirty(&self, id: BufferId) -> bool {
        self.documents
            .get(&id)
            .map(|e| e.read().document.is_dirty())
            .unwrap_or(false)
    }

    /// Returns the URI of an open document.
    pub fn uri(&self, id: BufferId) -> Option<DocumentUri> {
        self.documents
            .get(&id)
            .map(|e| e.read().document.uri().clone())
    }

    /// Returns the language of an open document.
    pub fn language(&self, id: BufferId) -> Option<Language> {
        self.documents
            .get(&id)
            .map(|e| e.read().document.language().clone())
    }

    /// Returns whether undo is available.
    pub fn can_undo(&self, id: BufferId) -> bool {
        self.documents
            .get(&id)
            .map(|e| e.read().history.can_undo())
            .unwrap_or(false)
    }

    /// Returns whether redo is available.
    pub fn can_redo(&self, id: BufferId) -> bool {
        self.documents
            .get(&id)
            .map(|e| e.read().history.can_redo())
            .unwrap_or(false)
    }

    /// Run a read-only closure over a document.
    pub fn with_document<F, R>(&self, id: BufferId, f: F) -> Result<R>
    where
        F: FnOnce(&DocumentEntry) -> R,
    {
        let entry = self
            .documents
            .get(&id)
            .ok_or_else(|| crabideError::DocumentNotFound {
                uri: format!("BufferId({id})"),
            })?;
        let guard = entry.read();
        Ok(f(&guard))
    }

    /// Run an exclusive closure over a document.
    pub fn with_document_mut<F, R>(&self, id: BufferId, f: F) -> Result<R>
    where
        F: FnOnce(&mut DocumentEntry) -> R,
    {
        let entry = self
            .documents
            .get(&id)
            .ok_or_else(|| crabideError::DocumentNotFound {
                uri: format!("BufferId({id})"),
            })?;
        let mut guard = entry.write();
        Ok(f(&mut guard))
    }

    // ── Open ──────────────────────────────────────────────────────────────────

    /// Open a file from the VFS. If it is already open, returns its existing `BufferId`.
    pub async fn open_file(&self, uri: DocumentUri) -> Result<BufferId> {
        // Already open → return existing id
        if let Some(id) = self.get_buffer_id(&uri) {
            return Ok(id);
        }
        let bytes = self.vfs.read_file(&uri).await?;
        let document = Document::from_bytes(uri.clone(), &bytes)
            .map_err(|e| crabideError::Buffer(e.to_string()))?;
        Ok(self.register(document))
    }

    /// Open a file if it exists, otherwise create an untitled buffer for that URI.
    ///
    /// Useful for opening files that may or may not exist yet.
    pub async fn open_or_create(&self, uri: DocumentUri) -> Result<BufferId> {
        if let Some(id) = self.get_buffer_id(&uri) {
            return Ok(id);
        }
        let bytes = match self.vfs.read_file(&uri).await {
            Ok(b) => b,
            Err(crabideError::DocumentNotFound { .. }) => Vec::new(),
            Err(e) => return Err(e),
        };
        let document = if bytes.is_empty() {
            let lang = uri_language(&uri);
            Document::new_untitled(lang) // will use a fresh id+uri; caller may need save_as
        } else {
            Document::from_bytes(uri.clone(), &bytes)
                .map_err(|e| crabideError::Buffer(e.to_string()))?
        };
        Ok(self.register(document))
    }

    /// Create a new untitled (unsaved) buffer. The language defaults to `plaintext`
    /// unless overridden.
    pub fn new_untitled(&self, language: Option<Language>) -> BufferId {
        let n = self.untitled_counter.fetch_add(1, Ordering::Relaxed);
        let lang = language.unwrap_or(Language::PLAIN_TEXT);
        let mut doc = Document::new_untitled(lang.clone());

        // Assign a human-readable URI: untitled://Untitled-1, Untitled-2, …
        if let Ok(uri) = DocumentUri::parse(&format!("untitled://Untitled-{n}")) {
            doc.set_uri(uri);
        }

        self.register(doc)
    }

    // ── Close ─────────────────────────────────────────────────────────────────

    /// Close a document.
    ///
    /// If the document has unsaved changes and `force` is `false`, returns
    /// [`CloseResult::UnsavedChanges`] — the caller should prompt the user,
    /// then call `close(id, true)` to confirm.
    pub fn close(&self, id: BufferId, force: bool) -> Result<CloseResult> {
        let entry = self
            .documents
            .get(&id)
            .ok_or_else(|| crabideError::DocumentNotFound {
                uri: format!("BufferId({id})"),
            })?;

        if !force && entry.read().document.is_dirty() {
            return Ok(CloseResult::UnsavedChanges);
        }

        // Grab URI before dropping
        let uri = entry.read().document.uri().clone();
        drop(entry); // release the DashMap ref before removing

        self.documents.remove(&id);
        self.uri_index.remove(&uri);

        self.notify_closed(id);
        log::debug!("Closed document {id} ({uri})");
        Ok(CloseResult::Closed)
    }

    // ── Save ──────────────────────────────────────────────────────────────────

    /// Save the document to its current URI.
    pub async fn save(&self, id: BufferId) -> Result<()> {
        let (uri, bytes) = {
            let entry = self
                .documents
                .get(&id)
                .ok_or_else(|| crabideError::DocumentNotFound {
                    uri: format!("BufferId({id})"),
                })?;
            let guard = entry.read();
            (guard.document.uri().clone(), guard.document.to_bytes())
        };

        self.vfs.write_file(&uri, &bytes).await?;

        // Mark saved
        if let Some(entry) = self.documents.get(&id) {
            entry.write().document.mark_saved();
        }

        log::debug!("Saved document {id} → {uri}");
        Ok(())
    }

    /// Save to a new URI (Save As). Updates the document's URI and language.
    pub async fn save_as(&self, id: BufferId, new_uri: DocumentUri) -> Result<()> {
        let bytes = {
            let entry = self
                .documents
                .get(&id)
                .ok_or_else(|| crabideError::DocumentNotFound {
                    uri: format!("BufferId({id})"),
                })?;
            let guard = entry.read();
            guard.document.to_bytes()
        };

        self.vfs.write_file(&new_uri, &bytes).await?;

        if let Some(entry) = self.documents.get(&id) {
            let mut guard = entry.write();
            // Remove old URI from index
            let old_uri = guard.document.uri().clone();
            self.uri_index.remove(&old_uri);

            // Update document
            let new_lang = uri_language(&new_uri);
            guard.document.set_uri(new_uri.clone());
            guard.document.set_language(new_lang);
            guard.document.mark_saved();
        }

        // Add new URI to index
        self.uri_index.insert(new_uri.clone(), id);

        log::debug!("Saved document {id} as {new_uri}");
        Ok(())
    }

    // ── Edit operations ───────────────────────────────────────────────────────

    /// Apply a single edit, push it to history, and notify observers.
    pub fn apply_edit(&self, id: BufferId, edit: TextEdit, label: &str) -> Result<()> {
        self.apply_edits(id, &[edit], label)
    }

    /// Apply a batch of edits atomically (single history entry).
    pub fn apply_edits(&self, id: BufferId, edits: &[TextEdit], label: &str) -> Result<()> {
        let version = {
            let entry = self
                .documents
                .get(&id)
                .ok_or_else(|| crabideError::DocumentNotFound {
                    uri: format!("BufferId({id})"),
                })?;
            let mut guard = entry.write();

            // Snapshot for history before applying
            let snapshot = guard.document.rope_snapshot();
            guard.history.push(snapshot, label, vec![]);

            // Apply all edits
            for edit in edits {
                guard
                    .document
                    .apply_edit(edit)
                    .map_err(|e| crabideError::Buffer(e.to_string()))?;
            }

            guard.document.version()
        };

        self.notify_changed(id, edits, version);
        Ok(())
    }

    // ── Undo / Redo ───────────────────────────────────────────────────────────

    /// Undo the last edit. Returns `true` if something was undone.
    pub fn undo(&self, id: BufferId) -> Result<bool> {
        let (undone, version, edits) = {
            let entry = self
                .documents
                .get(&id)
                .ok_or_else(|| crabideError::DocumentNotFound {
                    uri: format!("BufferId({id})"),
                })?;
            let mut guard = entry.write();
            match guard.history.undo() {
                None => (false, 0, vec![]),
                Some(hist) => {
                    let rope = hist.rope.clone();
                    guard.document.restore_rope(rope);
                    (
                        true,
                        guard.document.version(),
                        vec![full_range_edit(&guard.document)],
                    )
                }
            }
        };

        if undone {
            self.notify_changed(id, &edits, version);
        }
        Ok(undone)
    }

    /// Redo the next edit. Returns `true` if something was redone.
    pub fn redo(&self, id: BufferId) -> Result<bool> {
        let (redone, version, edits) = {
            let entry = self
                .documents
                .get(&id)
                .ok_or_else(|| crabideError::DocumentNotFound {
                    uri: format!("BufferId({id})"),
                })?;
            let mut guard = entry.write();
            match guard.history.redo() {
                None => (false, 0, vec![]),
                Some(hist) => {
                    let rope = hist.rope.clone();
                    guard.document.restore_rope(rope);
                    (
                        true,
                        guard.document.version(),
                        vec![full_range_edit(&guard.document)],
                    )
                }
            }
        };

        if redone {
            self.notify_changed(id, &edits, version);
        }
        Ok(redone)
    }

    // ── Public document registration ─────────────────────────────────────────

    /// Register a pre-created `Document` in the workspace.
    ///
    /// Used when a document is constructed externally (e.g. loaded synchronously
    /// at startup) and needs to be tracked by the workspace for undo/redo and
    /// observer notifications.
    pub fn register_document(&self, document: Document) -> BufferId {
        self.register(document)
    }

    /// Collect all lines of a document as a `Vec<String>`.
    ///
    /// Returns an error if the document is not open.
    pub fn get_lines(&self, id: BufferId) -> Result<Vec<String>> {
        self.with_document(id, |entry| {
            let n = entry.document.line_count() as usize;
            (0..n)
                .filter_map(|i| entry.document.line_str(i as u32))
                .collect()
        })
    }

    // ── Internals ─────────────────────────────────────────────────────────────

    fn register(&self, document: Document) -> BufferId {
        let id = document.id();
        let uri = document.uri().clone();
        let lang = document.language().clone();
        let entry = Arc::new(RwLock::new(DocumentEntry::new(document)));

        self.documents.insert(id, entry);
        self.uri_index.insert(uri.clone(), id);

        self.notify_opened(id, &uri, &lang);
        log::debug!("Opened document {id} ({uri})");
        id
    }

    fn notify_opened(&self, id: BufferId, uri: &DocumentUri, lang: &Language) {
        let obs = self.observers.read();
        for o in obs.iter() {
            o.on_document_opened(id, uri, lang);
        }
    }

    fn notify_changed(&self, id: BufferId, edits: &[TextEdit], version: u32) {
        let obs = self.observers.read();
        for o in obs.iter() {
            o.on_document_changed(id, edits, version);
        }
    }

    fn notify_closed(&self, id: BufferId) {
        let obs = self.observers.read();
        for o in obs.iter() {
            o.on_document_closed(id);
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Guess language from a URI's file extension.
fn uri_language(uri: &DocumentUri) -> Language {
    uri.to_file_path()
        .and_then(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(language_from_extension)
        })
        .unwrap_or(Language::PLAIN_TEXT)
}

/// Construct a synthetic "whole-document replaced" TextEdit used to notify
/// observers after an undo/redo where we don't have the incremental diff.
fn full_range_edit(doc: &Document) -> TextEdit {
    use crabide_core::types::{Position, Range};
    let content = doc.text_content();
    let lines = doc.line_count();
    let last_col = doc.line_char_len(lines.saturating_sub(1)).unwrap_or(0);
    TextEdit {
        range: Range::new(
            Position::ZERO,
            Position::new(lines.saturating_sub(1), last_col),
        ),
        new_text: content,
    }
}
