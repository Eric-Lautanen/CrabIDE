//! Fundamental domain types shared across all crabide crates.
//!
//! Keep types here as plain data — no methods that require heavy deps.
//! Constructors and conversion impls are fine; business logic belongs in the
//! crate that owns that domain.

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::fmt;
use uuid::Uuid;

// ── Identifiers ──────────────────────────────────────────────────────────────

/// Unique identifier for an open workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkspaceId(Uuid);

impl WorkspaceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for WorkspaceId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for an open text buffer (may not yet be saved to disk).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BufferId(Uuid);

impl BufferId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for BufferId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for BufferId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "buf:{}", self.0)
    }
}

/// A document URI, wrapping the LSP-compatible `url::Url`.
///
/// All document addressing in crabide goes through URIs, not raw paths.
/// This allows the VFS to transparently handle local files, remote SSH files,
/// and in-memory scratch buffers with the same interface.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DocumentUri(url::Url);

impl DocumentUri {
    pub fn from_file_path(path: impl AsRef<std::path::Path>) -> Option<Self> {
        url::Url::from_file_path(path).ok().map(DocumentUri)
    }

    pub fn parse(s: &str) -> Result<Self, url::ParseError> {
        Ok(DocumentUri(url::Url::parse(s)?))
    }

    pub fn as_url(&self) -> &url::Url {
        &self.0
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns the file path if this is a `file://` URI.
    pub fn to_file_path(&self) -> Option<std::path::PathBuf> {
        self.0.to_file_path().ok()
    }

    pub fn is_untitled(&self) -> bool {
        self.0.scheme() == "untitled"
    }
}

impl fmt::Display for DocumentUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Combined document identifier (URI + optional buffer ID for unsaved docs).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DocumentId {
    pub uri: DocumentUri,
    pub buffer_id: BufferId,
}

impl DocumentId {
    pub fn new(uri: DocumentUri) -> Self {
        Self {
            uri,
            buffer_id: BufferId::new(),
        }
    }
}

impl fmt::Display for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.uri, self.buffer_id)
    }
}

/// Identifier for an installed extension.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExtensionId {
    pub publisher: SmolStr,
    pub name: SmolStr,
    pub version: SmolStr,
}

impl ExtensionId {
    pub fn new(publisher: &str, name: &str, version: &str) -> Self {
        Self {
            publisher: SmolStr::new(publisher),
            name: SmolStr::new(name),
            version: SmolStr::new(version),
        }
    }
}

impl fmt::Display for ExtensionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}@{}", self.publisher, self.name, self.version)
    }
}

// ── Text Coordinates ─────────────────────────────────────────────────────────

/// A zero-indexed position in a text document.
///
/// `line` is the 0-based line number.
/// `character` is the 0-based UTF-16 code unit offset within that line,
/// matching LSP's default position encoding. The buffer layer provides
/// conversion to/from byte and char offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl Position {
    pub const fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }

    pub const ZERO: Self = Self {
        line: 0,
        character: 0,
    };
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line + 1, self.character + 1)
    }
}

/// A half-open range `[start, end)` in a text document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    pub fn new(start: Position, end: Position) -> Self {
        debug_assert!(start <= end, "Range start must not exceed end");
        Self { start, end }
    }

    pub fn point(pos: Position) -> Self {
        Self {
            start: pos,
            end: pos,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub fn contains(&self, pos: Position) -> bool {
        pos >= self.start && pos < self.end
    }

    pub fn contains_inclusive(&self, pos: Position) -> bool {
        pos >= self.start && pos <= self.end
    }
}

/// A text selection, with an `anchor` (where selection started) and
/// `active` cursor (where it currently ends). The anchor may be after
/// the active position for reverse selections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Selection {
    /// Where the selection was started.
    pub anchor: Position,
    /// The current cursor position (may be before anchor for reversed selections).
    pub active: Position,
}

impl Selection {
    pub fn cursor(pos: Position) -> Self {
        Self {
            anchor: pos,
            active: pos,
        }
    }

    pub fn new(anchor: Position, active: Position) -> Self {
        Self { anchor, active }
    }

    /// Returns the range covered by this selection, normalized so start <= end.
    pub fn as_range(&self) -> Range {
        if self.anchor <= self.active {
            Range::new(self.anchor, self.active)
        } else {
            Range::new(self.active, self.anchor)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.active
    }

    pub fn is_reversed(&self) -> bool {
        self.active < self.anchor
    }
}

/// A text replacement operation: replace `range` with `new_text`.
/// An empty `range` is an insertion; an empty `new_text` is a deletion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEdit {
    pub range: Range,
    pub new_text: String,
}

impl TextEdit {
    pub fn insert(pos: Position, text: String) -> Self {
        Self {
            range: Range::point(pos),
            new_text: text,
        }
    }

    pub fn delete(range: Range) -> Self {
        Self {
            range,
            new_text: String::new(),
        }
    }

    pub fn replace(range: Range, new_text: String) -> Self {
        Self { range, new_text }
    }
}

// ── Language ──────────────────────────────────────────────────────────────────

/// A programming language identifier, using the LSP language ID convention.
///
/// Examples: `Language::new("rust")`, `Language::new("typescript")`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Language(SmolStr);

impl Language {
    pub fn new(id: &str) -> Self {
        Self(SmolStr::new(id))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    // Well-known language IDs as constants
    pub const RUST: Self = Self(SmolStr::new_static("rust"));
    pub const PYTHON: Self = Self(SmolStr::new_static("python"));
    pub const TYPESCRIPT: Self = Self(SmolStr::new_static("typescript"));
    pub const JAVASCRIPT: Self = Self(SmolStr::new_static("javascript"));
    pub const GO: Self = Self(SmolStr::new_static("go"));
    pub const C: Self = Self(SmolStr::new_static("c"));
    pub const CPP: Self = Self(SmolStr::new_static("cpp"));
    pub const JAVA: Self = Self(SmolStr::new_static("java"));
    pub const JSON: Self = Self(SmolStr::new_static("json"));
    pub const TOML: Self = Self(SmolStr::new_static("toml"));
    pub const YAML: Self = Self(SmolStr::new_static("yaml"));
    pub const MARKDOWN: Self = Self(SmolStr::new_static("markdown"));
    pub const PLAIN_TEXT: Self = Self(SmolStr::new_static("plaintext"));
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<&str> for Language {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

/// Guess the language from a file extension.
pub fn language_from_extension(ext: &str) -> Language {
    match ext {
        "rs" => Language::RUST,
        "py" | "pyw" => Language::PYTHON,
        "ts" | "tsx" => Language::TYPESCRIPT,
        "js" | "jsx" | "mjs" | "cjs" => Language::JAVASCRIPT,
        "go" => Language::GO,
        "c" | "h" => Language::C,
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Language::CPP,
        "java" => Language::JAVA,
        "json" | "jsonc" => Language::JSON,
        "toml" => Language::TOML,
        "yaml" | "yml" => Language::YAML,
        "md" | "mdx" => Language::MARKDOWN,
        _ => Language::PLAIN_TEXT,
    }
}
