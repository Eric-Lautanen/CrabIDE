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

    #[error("Position out of bounds: line {line}, character {character}")]
    PositionOutOfBounds { line: u32, character: u32 },

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

impl From<url::ParseError> for crabideError {
    fn from(e: url::ParseError) -> Self {
        crabideError::Other(e.to_string())
    }
}

impl From<std::path::StripPrefixError> for crabideError {
    fn from(e: std::path::StripPrefixError) -> Self {
        crabideError::Other(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err = crabideError::from(io_err);
        let msg = format!("{err}");
        assert!(
            msg.contains("I/O error"),
            "should contain 'I/O error': {msg}"
        );
    }

    #[test]
    fn document_not_found() {
        let err = crabideError::DocumentNotFound {
            uri: "file:///test.rs".to_owned(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("file:///test.rs"));
    }

    #[test]
    fn buffer_error() {
        let err = crabideError::Buffer("overflow".to_owned());
        let msg = format!("{err}");
        assert!(msg.contains("overflow"));
    }

    #[test]
    fn position_out_of_bounds() {
        let err = crabideError::PositionOutOfBounds {
            line: 5,
            character: 10,
        };
        let msg = format!("{err}");
        assert!(msg.contains('5') && msg.contains("10"));
    }

    #[test]
    fn lsp_server_error() {
        let err = crabideError::LspServer {
            server: "rust-analyzer".to_owned(),
            message: "crashed".to_owned(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("rust-analyzer") && msg.contains("crashed"));
    }

    #[test]
    fn lsp_transport_error() {
        let err = crabideError::LspTransport("timeout".to_owned());
        assert!(format!("{err}").contains("timeout"));
    }

    #[test]
    fn lsp_server_not_running() {
        let err = crabideError::LspServerNotRunning("gopls".to_owned());
        assert!(format!("{err}").contains("gopls"));
    }

    #[test]
    fn dap_error() {
        let err = crabideError::Dap("no session".to_owned());
        assert!(format!("{err}").contains("no session"));
    }

    #[test]
    fn no_debug_session() {
        let err = crabideError::NoDebugSession;
        assert!(format!("{err}").contains("active debug session"));
    }

    #[test]
    fn pty_error() {
        let err = crabideError::Pty("spawn failed".to_owned());
        assert!(format!("{err}").contains("spawn failed"));
    }

    #[test]
    fn git_error() {
        let err = crabideError::Git("merge conflict".to_owned());
        assert!(format!("{err}").contains("merge conflict"));
    }

    #[test]
    fn extension_error() {
        let err = crabideError::Extension {
            id: "my.ext".to_owned(),
            message: "panic".to_owned(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("my.ext") && msg.contains("panic"));
    }

    #[test]
    fn wasm_error() {
        let err = crabideError::Wasm("trap".to_owned());
        assert!(format!("{err}").contains("trap"));
    }

    #[test]
    fn extension_not_found() {
        let err = crabideError::ExtensionNotFound("missing".to_owned());
        assert!(format!("{err}").contains("missing"));
    }

    #[test]
    fn config_parse_error() {
        let err = crabideError::ConfigParse {
            file: "settings.toml".to_owned(),
            message: "invalid".to_owned(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("settings.toml") && msg.contains("invalid"));
    }

    #[test]
    fn config_key_not_found() {
        let err = crabideError::ConfigKeyNotFound("tab_size".to_owned());
        assert!(format!("{err}").contains("tab_size"));
    }

    #[test]
    fn grammar_error() {
        let err = crabideError::Grammar("parse failed".to_owned());
        assert!(format!("{err}").contains("parse failed"));
    }

    #[test]
    fn query_error() {
        let err = crabideError::QueryError {
            language: "rust".to_owned(),
            message: "bad node".to_owned(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("rust") && msg.contains("bad node"));
    }

    #[test]
    fn workspace_error() {
        let err = crabideError::Workspace("no root".to_owned());
        assert!(format!("{err}").contains("no root"));
    }

    #[test]
    fn json_error_conversion() {
        let json_err = serde_json::from_str::<i32>("not a number").unwrap_err();
        let err = crabideError::from(json_err);
        let msg = format!("{err}");
        assert!(
            msg.contains("JSON error"),
            "should contain 'JSON error': {msg}"
        );
    }

    #[test]
    fn cancelled_error() {
        let err = crabideError::Cancelled;
        assert!(format!("{err}").contains("cancelled"));
    }

    #[test]
    fn timeout_error() {
        let err = crabideError::Timeout;
        assert!(format!("{err}").contains("timed out"));
    }

    #[test]
    fn other_from_string() {
        let err = crabideError::from("custom error".to_owned());
        let msg = format!("{err}");
        assert!(msg.contains("custom error"));
    }

    #[test]
    fn other_from_str_ref() {
        let err = crabideError::from("static error");
        let msg = format!("{err}");
        assert!(msg.contains("static error"));
    }

    #[test]
    fn from_url_parse_error() {
        let url_err = url::Url::parse(":::bad").unwrap_err();
        let err = crabideError::from(url_err);
        let msg = format!("{err}");
        assert!(!msg.is_empty());
    }

    #[test]
    fn from_strip_prefix_error() {
        let path = std::path::Path::new("/foo");
        let strip_err = path.strip_prefix("bar").unwrap_err();
        let err = crabideError::from(strip_err);
        let msg = format!("{err}");
        assert!(!msg.is_empty());
    }

    #[test]
    fn from_anyhow_error() {
        let anyhow_err = anyhow::anyhow!("something went wrong");
        let err = crabideError::from(anyhow_err);
        let msg = format!("{err}");
        assert!(msg.contains("something went wrong"));
    }

    #[test]
    fn result_ok() {
        let res: Result<i32> = Ok(42);
        assert_eq!(res.unwrap(), 42);
    }

    #[test]
    fn result_err() {
        let res: Result<i32> = Err(crabideError::Cancelled);
        assert!(res.is_err());
    }
}
