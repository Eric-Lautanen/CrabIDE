//! Unified error type for all crabide crates.
//!
//! Internal crate boundaries use `anyhow::Result` for ergonomic propagation
//! with context. Public API boundaries and channel events use `crabideError`
//! so callers can match on specific variants.

use thiserror::Error;

/// The canonical error type for crabide public APIs.
#[allow(non_camel_case_types)]
#[derive(Debug, Error)]
pub enum crabideError {
    // ── I/O ────────────────────────────────────────────────────────────────
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    // ── Buffer / document ──────────────────────────────────────────────────
    #[error("Document not found: {uri}")]
    DocumentNotFound { uri: String },

    #[error("Buffer error: {0}")]
    Buffer(String),

    #[error("Position out of bounds: line {line}, col {col}")]
    PositionOutOfBounds { line: u32, col: u32 },

    // ── LSP ────────────────────────────────────────────────────────────────
    #[error("LSP server error: {server}: {message}")]
    LspServer { server: String, message: String },

    #[error("LSP transport error: {0}")]
    LspTransport(String),

    #[error("LSP server '{0}' not running")]
    LspServerNotRunning(String),

    // ── DAP ────────────────────────────────────────────────────────────────
    #[error("DAP adapter error: {0}")]
    Dap(String),

    #[error("No active debug session")]
    NoDebugSession,

    // ── Terminal ───────────────────────────────────────────────────────────
    #[error("PTY error: {0}")]
    Pty(String),

    // ── Git ────────────────────────────────────────────────────────────────
    #[error("Git error: {0}")]
    Git(String),

    // ── Extensions ─────────────────────────────────────────────────────────
    #[error("Extension '{id}' error: {message}")]
    Extension { id: String, message: String },

    #[error("WASM error: {0}")]
    Wasm(String),

    #[error("Extension not found: {0}")]
    ExtensionNotFound(String),

    // ── Config ─────────────────────────────────────────────────────────────
    #[error("Config parse error in {file}: {message}")]
    ConfigParse { file: String, message: String },

    #[error("Config key not found: {0}")]
    ConfigKeyNotFound(String),

    // ── Syntax / Grammars ──────────────────────────────────────────
    #[error("Grammar error: {0}")]
    Grammar(String),

    #[error("Query error in '{language}': {message}")]
    QueryError { language: String, message: String },

    // ── Workspace ──────────────────────────────────────────────────────────
    #[error("Workspace error: {0}")]
    Workspace(String),

    // ── Serialization ──────────────────────────────────────────────────────
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),

    // ── General ────────────────────────────────────────────────────────────
    #[error("Operation cancelled")]
    Cancelled,

    #[error("Operation timed out")]
    Timeout,

    #[error("{0}")]
    Other(String),
}

/// Shorthand `Result` type that defaults to `crabideError`.
pub type Result<T, E = crabideError> = std::result::Result<T, E>;

impl From<anyhow::Error> for crabideError {
    fn from(e: anyhow::Error) -> Self {
        crabideError::Other(format!("{e:#}"))
    }
}

impl From<String> for crabideError {
    fn from(s: String) -> Self {
        crabideError::Other(s)
    }
}

impl From<&str> for crabideError {
    fn from(s: &str) -> Self {
        crabideError::Other(s.to_owned())
    }
}
