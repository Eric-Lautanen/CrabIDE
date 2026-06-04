//! `Document` — the core text buffer backed by `ropey::Rope`.
//!
//! A `Document` represents one open text file (or untitled buffer). It wraps a
//! `ropey::Rope` for O(log n) edits and O(1) clones. The clone property is
//! critical: `EditHistory` stores full `Rope` snapshots cheaply via Arc sharing.
//!
//! # Thread safety
//!
//! `Document` is not itself `Sync` — callers hold it behind a `parking_lot::RwLock`.
//! The `TextBuffer` trait is implemented on `&Document` (read-only). Write
//! operations (`apply_edit`, `apply_edits`) are called under exclusive lock.

use anyhow::{anyhow, Context};
use crabide_core::{
    traits::TextBuffer,
    types::{language_from_extension, BufferId, DocumentUri, Language, Position, Range, TextEdit},
};
use ropey::Rope;

/// A text document buffer.
pub struct Document {
    id: BufferId,
    uri: DocumentUri,
    language: Language,
    rope: Rope,
    /// Monotonically increasing version number. Incremented on every edit.
    version: u32,
    /// True if there are unsaved changes since last save / initial load.
    is_dirty: bool,
    /// The line ending style detected on load.
    line_ending: LineEnding,
    /// The encoding BOM detected on load (informational; we store UTF-8 internally).
    encoding: Encoding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,   // Unix
    CrLf, // Windows
    Cr,   // Classic Mac (rare)
}

impl LineEnding {
    pub fn as_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
            LineEnding::Cr => "\r",
        }
    }

    /// Detect the dominant line ending in a string.
    pub fn detect(text: &str) -> Self {
        let crlf = text.matches("\r\n").count();
        let lf = text.matches('\n').count().saturating_sub(crlf);
        let cr = text.matches('\r').count().saturating_sub(crlf);

        if crlf > lf && crlf > cr {
            LineEnding::CrLf
        } else if cr > lf {
            LineEnding::Cr
        } else {
            LineEnding::Lf
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
    // We only store UTF-8 internally, but we remember the original BOM so
    // we can write it back on save if the file had one.
}

impl Document {
    /// Create a new empty untitled document.
    pub fn new_untitled(language: Language) -> Self {
        let id = BufferId::new();
        let uri = DocumentUri::parse(&format!("untitled:///{}", id.uuid()))
            .expect("untitled URI is always valid");

        Self {
            id,
            uri,
            language,
            rope: Rope::new(),
            version: 0,
            is_dirty: false,
            line_ending: LineEnding::Lf,
            encoding: Encoding::Utf8,
        }
    }

    /// Load a document from raw bytes (as read from disk/VFS).
    pub fn from_bytes(uri: DocumentUri, bytes: &[u8]) -> anyhow::Result<Self> {
        // Detect BOM
        let (text_bytes, encoding) = if bytes.starts_with(b"\xEF\xBB\xBF") {
            (&bytes[3..], Encoding::Utf8Bom)
        } else {
            (bytes, Encoding::Utf8)
        };

        let text = std::str::from_utf8(text_bytes).context("File is not valid UTF-8")?;

        // Normalise CRLF → LF in the internal rope; remember original line ending.
        let line_ending = LineEnding::detect(text);
        let normalised: String = text.replace("\r\n", "\n").replace('\r', "\n");

        let language = uri
            .to_file_path()
            .and_then(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .map(language_from_extension)
            })
            .unwrap_or(Language::PLAIN_TEXT);

        Ok(Self {
            id: BufferId::new(),
            uri,
            language,
            rope: Rope::from_str(&normalised),
            version: 0,
            is_dirty: false,
            line_ending,
            encoding,
        })
    }

    /// Serialise the buffer back to bytes for saving.
    /// Restores the original line ending and BOM.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut text = self.rope.to_string();

        // Restore line endings if needed
        if self.line_ending != LineEnding::Lf {
            text = text.replace('\n', self.line_ending.as_str());
        }

        let mut bytes = Vec::with_capacity(text.len() + 3);

        if self.encoding == Encoding::Utf8Bom {
            bytes.extend_from_slice(b"\xEF\xBB\xBF");
        }

        bytes.extend_from_slice(text.as_bytes());
        bytes
    }

    // ── Accessors ────────────────────────────────────────────────────────────

    pub fn version(&self) -> u32 {
        self.version
    }
    pub fn is_dirty(&self) -> bool {
        self.is_dirty
    }
    pub fn set_language(&mut self, lang: Language) {
        self.language = lang;
    }
    pub fn set_uri(&mut self, uri: DocumentUri) {
        self.uri = uri;
    }
    pub fn mark_saved(&mut self) {
        self.is_dirty = false;
    }

    /// A cheap O(1) clone of the internal Rope (Arc sharing).
    /// Used by `EditHistory` to snapshot state without copying bytes.
    pub fn rope_snapshot(&self) -> Rope {
        self.rope.clone()
    }

    /// Replace the rope with a snapshot (for undo/redo).
    pub fn restore_rope(&mut self, rope: Rope) {
        self.rope = rope;
        self.version += 1;
        self.is_dirty = true;
    }

    /// Clear the document content, leaving it empty.
    pub fn clear(&mut self) {
        self.rope = Rope::new();
        self.version += 1;
        self.is_dirty = true;
    }

    /// Reload the document from new bytes (e.g. after an external file change).
    /// Resets version to 0 and clears the dirty flag.
    pub fn reload(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        let (text_bytes, encoding) = if bytes.starts_with(b"\xEF\xBB\xBF") {
            (&bytes[3..], Encoding::Utf8Bom)
        } else {
            (bytes, Encoding::Utf8)
        };

        let text = std::str::from_utf8(text_bytes).context("File is not valid UTF-8")?;
        let line_ending = LineEnding::detect(text);
        let normalised: String = text.replace("\r\n", "\n").replace('\r', "\n");

        self.rope = Rope::from_str(&normalised);
        self.version = 0;
        self.is_dirty = false;
        self.line_ending = line_ending;
        self.encoding = encoding;

        Ok(())
    }

    // ── Edit API ─────────────────────────────────────────────────────────────

    /// Apply a single text edit to the buffer.
    ///
    /// Returns the actual text that was replaced (useful for undo construction).
    pub fn apply_edit(&mut self, edit: &TextEdit) -> anyhow::Result<String> {
        let start_char = self.position_to_char_internal(edit.range.start)?;
        let end_char = self.position_to_char_internal(edit.range.end)?;

        if start_char > end_char {
            return Err(anyhow!(
                "Edit range start ({}) > end ({})",
                start_char,
                end_char
            ));
        }

        // Extract replaced text before modifying
        let removed: String = self.rope.slice(start_char..end_char).to_string();

        // Remove the range
        self.rope.remove(start_char..end_char);

        // Insert the new text
        if !edit.new_text.is_empty() {
            self.rope.insert(start_char, &edit.new_text);
        }

        self.version += 1;
        self.is_dirty = true;

        Ok(removed)
    }

    /// Apply multiple edits atomically. Edits must be sorted in descending
    /// order by range start to avoid invalidating earlier offsets.
    pub fn apply_edits(&mut self, edits: &[TextEdit]) -> anyhow::Result<()> {
        // Validate sort order: edits must be applied back-to-front
        for window in edits.windows(2) {
            if window[0].range.start < window[1].range.start {
                return Err(anyhow!(
                    "Edits must be sorted in descending range order for apply_edits"
                ));
            }
        }

        for edit in edits {
            self.apply_edit(edit)?;
        }

        Ok(())
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    fn position_to_char_internal(&self, pos: Position) -> anyhow::Result<usize> {
        let line = pos.line as usize;
        let col = pos.character as usize;

        if line >= self.rope.len_lines() {
            return Err(anyhow!(
                "Line {} is out of range ({})",
                line,
                self.rope.len_lines()
            ));
        }

        let line_start_char = self.rope.line_to_char(line);
        let line_len = self.rope.line(line).len_chars();

        // Allow col == line_len (insertion at end of line)
        if col > line_len {
            return Err(anyhow!(
                "Column {} exceeds line {} length {}",
                col,
                line,
                line_len
            ));
        }

        Ok(line_start_char + col)
    }
}

// ── TextBuffer implementation ─────────────────────────────────────────────────

impl TextBuffer for Document {
    fn line_count(&self) -> u32 {
        // ropey always includes a "line" even for an empty file
        self.rope.len_lines().max(1) as u32
    }

    fn char_len(&self) -> usize {
        self.rope.len_chars()
    }

    fn byte_len(&self) -> usize {
        self.rope.len_bytes()
    }

    fn line_str(&self, line: u32) -> Option<String> {
        let line = line as usize;
        if line >= self.rope.len_lines() {
            return None;
        }
        let s = self.rope.line(line).to_string();
        // Strip trailing newline characters from the returned string
        Some(s.trim_end_matches(['\n', '\r']).to_owned())
    }

    fn line_char_len(&self, line: u32) -> Option<u32> {
        let line = line as usize;
        if line >= self.rope.len_lines() {
            return None;
        }
        // ropey::iter::Chars does not implement DoubleEndedIterator, so .rev() is
        // unavailable. Convert to String (cheap for a single line) and trim there.
        let s = self.rope.line(line).to_string();
        let len = s.trim_end_matches(['\n', '\r']).chars().count();
        Some(len as u32)
    }

    fn text_content(&self) -> String {
        self.rope.to_string()
    }

    fn slice(&self, range: Range) -> Option<String> {
        let start = self.position_to_char_internal(range.start).ok()?;
        let end = self.position_to_char_internal(range.end).ok()?;
        if start > end || end > self.rope.len_chars() {
            return None;
        }
        Some(self.rope.slice(start..end).to_string())
    }

    fn position_to_char_offset(&self, pos: Position) -> Option<usize> {
        self.position_to_char_internal(pos).ok()
    }

    fn char_offset_to_position(&self, offset: usize) -> Option<Position> {
        if offset > self.rope.len_chars() {
            return None;
        }
        let line = self.rope.char_to_line(offset);
        let line_start = self.rope.line_to_char(line);
        let col = offset - line_start;
        Some(Position::new(line as u32, col as u32))
    }

    fn language(&self) -> &Language {
        &self.language
    }
    fn uri(&self) -> &DocumentUri {
        &self.uri
    }
    fn id(&self) -> BufferId {
        self.id
    }
}
