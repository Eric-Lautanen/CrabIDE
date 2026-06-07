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

#[cfg(test)]
mod tests {
    use super::*;

    // ── WorkspaceId ────────────────────────────────────────────────────────

    #[test]
    fn workspace_id_new_is_unique() {
        let a = WorkspaceId::new();
        let b = WorkspaceId::new();
        assert_ne!(a, b, "two WorkspaceIds must differ");
    }

    #[test]
    fn workspace_id_default_is_new() {
        let _ = WorkspaceId::default();
    }

    // ── BufferId ───────────────────────────────────────────────────────────

    #[test]
    fn buffer_id_new_is_unique() {
        let a = BufferId::new();
        let b = BufferId::new();
        assert_ne!(a, b, "two BufferIds must differ");
    }

    #[test]
    fn buffer_id_display_format() {
        let id = BufferId::new();
        let s = format!("{id}");
        assert!(
            s.starts_with("buf:"),
            "BufferId display should start with 'buf:'"
        );
    }

    #[test]
    fn buffer_id_default_is_new() {
        let _ = BufferId::default();
    }

    #[test]
    fn buffer_id_uuid_roundtrip() {
        let id = BufferId::new();
        let uuid = id.uuid();
        assert_eq!(uuid, id.uuid(), "uuid() should be stable");
    }

    // ── DocumentUri ────────────────────────────────────────────────────────

    #[test]
    fn document_uri_from_file_path() {
        let uri = DocumentUri::from_file_path(if cfg!(windows) {
            r"C:\foo\bar.rs"
        } else {
            "/foo/bar.rs"
        });
        assert!(uri.is_some(), "absolute path should produce a URI");
        let uri = uri.unwrap();
        assert_eq!(uri.as_url().scheme(), "file");
    }

    #[test]
    fn document_uri_from_relative_path_is_none() {
        let uri = DocumentUri::from_file_path("relative/path.rs");
        assert!(uri.is_none(), "relative path should not produce a file URI");
    }

    #[test]
    fn document_uri_parse_valid() {
        let uri = DocumentUri::parse("file:///foo/bar.rs");
        assert!(uri.is_ok());
    }

    #[test]
    fn document_uri_parse_invalid() {
        let uri = DocumentUri::parse("not a url");
        assert!(uri.is_err());
    }

    #[test]
    fn document_uri_to_file_path() {
        let path = if cfg!(windows) {
            r"C:\foo\bar.rs"
        } else {
            "/foo/bar.rs"
        };
        let uri = DocumentUri::from_file_path(path).unwrap();
        let back = uri.to_file_path();
        assert!(back.is_some());
    }

    #[test]
    fn document_uri_is_untitled() {
        let uri = DocumentUri::parse("untitled:1").unwrap();
        assert!(uri.is_untitled());
        let file_path = if cfg!(windows) {
            r"C:\foo.rs"
        } else {
            "/foo.rs"
        };
        let file_uri = DocumentUri::from_file_path(file_path).unwrap();
        assert!(!file_uri.is_untitled());
    }

    #[test]
    fn document_uri_display() {
        let path = if cfg!(windows) {
            r"C:\foo\bar.rs"
        } else {
            "/foo/bar.rs"
        };
        let uri = DocumentUri::from_file_path(path).unwrap();
        let s = format!("{uri}");
        assert!(s.starts_with("file://"));
    }

    #[test]
    fn document_uri_equality() {
        let path = if cfg!(windows) {
            r"C:\foo.rs"
        } else {
            "/foo.rs"
        };
        let a = DocumentUri::from_file_path(path).unwrap();
        let b = DocumentUri::from_file_path(path).unwrap();
        assert_eq!(a, b);
    }

    // ── DocumentId ─────────────────────────────────────────────────────────

    #[test]
    fn document_id_new_has_unique_buffer_id() {
        let path = if cfg!(windows) {
            r"C:\foo.rs"
        } else {
            "/foo.rs"
        };
        let uri = DocumentUri::from_file_path(path).unwrap();
        let a = DocumentId::new(uri.clone());
        let b = DocumentId::new(uri);
        assert_eq!(a.uri, b.uri, "same URI");
        assert_ne!(a.buffer_id, b.buffer_id, "different buffer IDs");
    }

    #[test]
    fn document_id_display_contains_uri_and_buffer() {
        let path = if cfg!(windows) {
            r"C:\foo.rs"
        } else {
            "/foo.rs"
        };
        let uri = DocumentUri::from_file_path(path).unwrap();
        let id = DocumentId::new(uri);
        let s = format!("{id}");
        assert!(s.contains("#"), "DocumentId display should contain '#'");
    }

    // ── ExtensionId ────────────────────────────────────────────────────────

    #[test]
    fn extension_id_display() {
        let id = ExtensionId::new("crabide", "rust-tools", "1.0.0");
        let s = format!("{id}");
        assert_eq!(s, "crabide.rust-tools@1.0.0");
    }

    #[test]
    fn extension_id_equality() {
        let a = ExtensionId::new("pub", "ext", "1.0");
        let b = ExtensionId::new("pub", "ext", "1.0");
        assert_eq!(a, b);
    }

    // ── Position ───────────────────────────────────────────────────────────

    #[test]
    fn position_new() {
        let p = Position::new(3, 10);
        assert_eq!(p.line, 3);
        assert_eq!(p.character, 10);
    }

    #[test]
    fn position_zero() {
        assert_eq!(Position::ZERO, Position::new(0, 0));
    }

    #[test]
    fn position_ordering() {
        let a = Position::new(0, 5);
        let b = Position::new(1, 0);
        let c = Position::new(0, 10);
        assert!(a < c, "same line, smaller char");
        assert!(c < b, "smaller line");
    }

    #[test]
    fn position_display_is_one_indexed() {
        let p = Position::new(0, 0);
        assert_eq!(format!("{p}"), "1:1");
        let p = Position::new(2, 5);
        assert_eq!(format!("{p}"), "3:6");
    }

    // ── Range ──────────────────────────────────────────────────────────────

    #[test]
    fn range_new() {
        let r = Range::new(Position::new(0, 0), Position::new(1, 10));
        assert_eq!(r.start, Position::new(0, 0));
        assert_eq!(r.end, Position::new(1, 10));
    }

    #[test]
    fn range_point_is_empty() {
        let r = Range::point(Position::new(5, 5));
        assert!(r.is_empty());
    }

    #[test]
    fn range_non_empty() {
        let r = Range::new(Position::new(0, 0), Position::new(0, 5));
        assert!(!r.is_empty());
    }

    #[test]
    fn range_contains() {
        let r = Range::new(Position::new(0, 3), Position::new(0, 8));
        assert!(r.contains(Position::new(0, 5)));
        assert!(r.contains(Position::new(0, 3)), "half-open: start included");
        assert!(!r.contains(Position::new(0, 8)), "half-open: end excluded");
    }

    #[test]
    fn range_contains_inclusive() {
        let r = Range::new(Position::new(0, 3), Position::new(0, 8));
        assert!(r.contains_inclusive(Position::new(0, 3)));
        assert!(r.contains_inclusive(Position::new(0, 8)));
        assert!(!r.contains_inclusive(Position::new(0, 2)));
        assert!(!r.contains_inclusive(Position::new(0, 9)));
    }

    #[test]
    fn range_contains_multiline() {
        let r = Range::new(Position::new(1, 0), Position::new(3, 0));
        assert!(r.contains(Position::new(2, 0)));
        assert!(r.contains(Position::new(1, 5)));
        assert!(!r.contains(Position::new(0, 0)));
        assert!(!r.contains(Position::new(3, 0)));
    }

    // ── Selection ──────────────────────────────────────────────────────────

    #[test]
    fn selection_cursor_is_empty() {
        let s = Selection::cursor(Position::new(5, 5));
        assert!(s.is_empty());
        assert!(!s.is_reversed());
    }

    #[test]
    fn selection_forward() {
        let s = Selection::new(Position::new(0, 3), Position::new(0, 8));
        assert!(!s.is_reversed());
        let r = s.as_range();
        assert_eq!(r.start, Position::new(0, 3));
        assert_eq!(r.end, Position::new(0, 8));
    }

    #[test]
    fn selection_reversed() {
        let s = Selection::new(Position::new(0, 8), Position::new(0, 3));
        assert!(s.is_reversed());
        let r = s.as_range();
        assert_eq!(r.start, Position::new(0, 3), "as_range normalizes");
        assert_eq!(r.end, Position::new(0, 8));
    }

    // ── TextEdit ───────────────────────────────────────────────────────────

    #[test]
    fn text_edit_insert() {
        let edit = TextEdit::insert(Position::new(0, 5), "hello".to_owned());
        assert!(edit.range.is_empty());
        assert_eq!(edit.new_text, "hello");
    }

    #[test]
    fn text_edit_delete() {
        let range = Range::new(Position::new(0, 2), Position::new(0, 7));
        let edit = TextEdit::delete(range);
        assert!(edit.new_text.is_empty());
    }

    #[test]
    fn text_edit_replace() {
        let range = Range::new(Position::new(1, 0), Position::new(1, 10));
        let edit = TextEdit::replace(range, "new".to_owned());
        assert_eq!(edit.new_text, "new");
    }

    // ── Language ───────────────────────────────────────────────────────────

    #[test]
    fn language_constants() {
        assert_eq!(Language::RUST.as_str(), "rust");
        assert_eq!(Language::PYTHON.as_str(), "python");
        assert_eq!(Language::TYPESCRIPT.as_str(), "typescript");
        assert_eq!(Language::JAVASCRIPT.as_str(), "javascript");
        assert_eq!(Language::GO.as_str(), "go");
        assert_eq!(Language::C.as_str(), "c");
        assert_eq!(Language::CPP.as_str(), "cpp");
        assert_eq!(Language::JAVA.as_str(), "java");
        assert_eq!(Language::JSON.as_str(), "json");
        assert_eq!(Language::TOML.as_str(), "toml");
        assert_eq!(Language::YAML.as_str(), "yaml");
        assert_eq!(Language::MARKDOWN.as_str(), "markdown");
        assert_eq!(Language::PLAIN_TEXT.as_str(), "plaintext");
    }

    #[test]
    fn language_display() {
        assert_eq!(format!("{}", Language::RUST), "rust");
    }

    #[test]
    fn language_from_str() {
        let lang: Language = "rust".into();
        assert_eq!(lang, Language::RUST);
    }

    #[test]
    fn language_equality() {
        assert_eq!(Language::new("rust"), Language::RUST);
        assert_ne!(Language::new("rust"), Language::PYTHON);
    }

    // ── language_from_extension ────────────────────────────────────────────

    #[test]
    fn language_from_ext_rust() {
        assert_eq!(language_from_extension("rs"), Language::RUST);
    }

    #[test]
    fn language_from_ext_python() {
        assert_eq!(language_from_extension("py"), Language::PYTHON);
        assert_eq!(language_from_extension("pyw"), Language::PYTHON);
    }

    #[test]
    fn language_from_ext_typescript() {
        assert_eq!(language_from_extension("ts"), Language::TYPESCRIPT);
        assert_eq!(language_from_extension("tsx"), Language::TYPESCRIPT);
    }

    #[test]
    fn language_from_ext_javascript() {
        assert_eq!(language_from_extension("js"), Language::JAVASCRIPT);
        assert_eq!(language_from_extension("jsx"), Language::JAVASCRIPT);
        assert_eq!(language_from_extension("mjs"), Language::JAVASCRIPT);
        assert_eq!(language_from_extension("cjs"), Language::JAVASCRIPT);
    }

    #[test]
    fn language_from_ext_go() {
        assert_eq!(language_from_extension("go"), Language::GO);
    }

    #[test]
    fn language_from_ext_c() {
        assert_eq!(language_from_extension("c"), Language::C);
        assert_eq!(language_from_extension("h"), Language::C);
    }

    #[test]
    fn language_from_ext_cpp() {
        assert_eq!(language_from_extension("cpp"), Language::CPP);
        assert_eq!(language_from_extension("cc"), Language::CPP);
        assert_eq!(language_from_extension("cxx"), Language::CPP);
        assert_eq!(language_from_extension("hpp"), Language::CPP);
        assert_eq!(language_from_extension("hxx"), Language::CPP);
    }

    #[test]
    fn language_from_ext_java() {
        assert_eq!(language_from_extension("java"), Language::JAVA);
    }

    #[test]
    fn language_from_ext_json() {
        assert_eq!(language_from_extension("json"), Language::JSON);
        assert_eq!(language_from_extension("jsonc"), Language::JSON);
    }

    #[test]
    fn language_from_ext_toml() {
        assert_eq!(language_from_extension("toml"), Language::TOML);
    }

    #[test]
    fn language_from_ext_yaml() {
        assert_eq!(language_from_extension("yaml"), Language::YAML);
        assert_eq!(language_from_extension("yml"), Language::YAML);
    }

    #[test]
    fn language_from_ext_markdown() {
        assert_eq!(language_from_extension("md"), Language::MARKDOWN);
        assert_eq!(language_from_extension("mdx"), Language::MARKDOWN);
    }

    #[test]
    fn language_from_ext_unknown() {
        assert_eq!(language_from_extension("xyz"), Language::PLAIN_TEXT);
        assert_eq!(language_from_extension(""), Language::PLAIN_TEXT);
    }

    // ── Serialization roundtrips ───────────────────────────────────────────

    #[test]
    fn position_serde_roundtrip() {
        let p = Position::new(10, 20);
        let json = serde_json::to_string(&p).unwrap();
        let back: Position = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn range_serde_roundtrip() {
        let r = Range::new(Position::new(1, 2), Position::new(3, 4));
        let json = serde_json::to_string(&r).unwrap();
        let back: Range = serde_json::from_str(&json).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn selection_serde_roundtrip() {
        let s = Selection::new(Position::new(0, 5), Position::new(2, 0));
        let json = serde_json::to_string(&s).unwrap();
        let back: Selection = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn text_edit_serde_roundtrip() {
        let edit = TextEdit::replace(
            Range::new(Position::new(0, 0), Position::new(0, 5)),
            "hello".to_owned(),
        );
        let json = serde_json::to_string(&edit).unwrap();
        let back: TextEdit = serde_json::from_str(&json).unwrap();
        assert_eq!(edit, back);
    }

    #[test]
    fn language_serde_roundtrip() {
        let lang = Language::RUST;
        let json = serde_json::to_string(&lang).unwrap();
        let back: Language = serde_json::from_str(&json).unwrap();
        assert_eq!(lang, back);
    }

    #[test]
    fn buffer_id_serde_roundtrip() {
        let id = BufferId::new();
        let json = serde_json::to_string(&id).unwrap();
        let back: BufferId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn extension_id_serde_roundtrip() {
        let id = ExtensionId::new("pub", "ext", "2.0");
        let json = serde_json::to_string(&id).unwrap();
        let back: ExtensionId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }
}
