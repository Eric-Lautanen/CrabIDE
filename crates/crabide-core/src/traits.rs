//! Core abstraction traits for crabide's service layer.
//!
//! These traits define the contracts that each service crate must implement.
//! The UI and other consumers depend on traits, not concrete types — enabling
//! testing with mock implementations and future swaps of backing services.

use crate::error::Result;
use crate::types::{BufferId, DocumentUri, Language, Position, Range, TextEdit};

// ── Text Buffer trait ─────────────────────────────────────────────────────────

/// Read-only view of a text document's content.
///
/// Implemented by `crabide-buffer`'s `Document` type. Passed as `&dyn TextBuffer`
/// to services that need to read content (e.g. syntax highlighter, LSP position
/// conversion) without taking ownership.
pub trait TextBuffer: Send + Sync {
    /// Total number of lines in the buffer (always >= 1).
    fn line_count(&self) -> u32;

    /// Total number of Unicode scalar values (chars) in the buffer.
    fn char_len(&self) -> usize;

    /// Total number of UTF-8 bytes in the buffer.
    fn byte_len(&self) -> usize;

    /// Returns the text content of a specific line, without the line terminator.
    /// Returns `None` if `line` is out of range.
    fn line_str(&self, line: u32) -> Option<String>;

    /// Returns the length of the given line in chars (excluding line terminator).
    fn line_char_len(&self, line: u32) -> Option<u32>;

    /// Returns the entire buffer content as a `String`.
    /// Named `text_content` (not `to_string`) to avoid shadowing `ToString`.
    fn text_content(&self) -> String;

    /// Returns a substring of the buffer for the given range.
    fn slice(&self, range: Range) -> Option<String>;

    /// Returns the char offset for a line/column position.
    fn position_to_char_offset(&self, pos: Position) -> Option<usize>;

    /// Returns the Position for a given char offset.
    fn char_offset_to_position(&self, offset: usize) -> Option<Position>;

    /// The detected / assigned language for this buffer.
    fn language(&self) -> &Language;

    /// The URI of this buffer (may be an untitled:// URI for unsaved buffers).
    fn uri(&self) -> &DocumentUri;

    /// The buffer's unique ID.
    fn id(&self) -> BufferId;
}

// ── Document Observer ─────────────────────────────────────────────────────────

/// Implemented by services that need to be notified of document changes.
pub trait DocumentObserver: Send + Sync {
    /// Called after a batch of edits has been applied to the buffer.
    fn on_document_changed(&self, buffer_id: BufferId, edits: &[TextEdit], new_version: u32);

    /// Called when a document is opened (first load or creation).
    fn on_document_opened(&self, buffer_id: BufferId, uri: &DocumentUri, language: &Language);

    /// Called when a document is closed (tab closed, file deleted, etc.).
    fn on_document_closed(&self, buffer_id: BufferId);
}

// ── VFS trait ─────────────────────────────────────────────────────────────────

/// Virtual filesystem — abstracts local disk, SSH remote, and Docker containers.
///
/// All file operations in crabide go through this trait. The local
/// implementation uses `std::fs`; SSH and container impls live in their
/// respective crates.
#[async_trait::async_trait]
pub trait VirtualFileSystem: Send + Sync {
    /// Read the entire contents of a file.
    async fn read_file(&self, uri: &DocumentUri) -> Result<Vec<u8>>;

    /// Write contents to a file, creating it if it doesn't exist.
    async fn write_file(&self, uri: &DocumentUri, contents: &[u8]) -> Result<()>;

    /// Delete a file or directory. If `recursive`, also deletes non-empty directories.
    async fn delete(&self, uri: &DocumentUri, recursive: bool) -> Result<()>;

    /// Rename / move a file.
    async fn rename(&self, from: &DocumentUri, to: &DocumentUri) -> Result<()>;

    /// Create a directory (and all intermediate dirs).
    async fn create_dir(&self, uri: &DocumentUri) -> Result<()>;

    /// List directory entries.
    async fn read_dir(&self, uri: &DocumentUri) -> Result<Vec<DirEntry>>;

    /// Check if a file or directory exists.
    async fn exists(&self, uri: &DocumentUri) -> Result<bool>;

    /// Return the canonical URI for a potentially relative path.
    fn canonical_uri(&self, uri: &DocumentUri) -> Result<DocumentUri>;
}

/// A directory entry returned by `VirtualFileSystem::read_dir`.
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub uri: DocumentUri,
    pub name: String,
    pub kind: DirEntryKind,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirEntryKind {
    File,
    Directory,
    Symlink,
    Other,
}
