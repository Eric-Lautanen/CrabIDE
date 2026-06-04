//! LSP server configuration.

#![allow(dead_code)]

use crabide_core::types::{DocumentUri, Language};
use serde::{Deserialize, Serialize};

/// Configuration for a single language server instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspServerConfig {
    /// Language IDs handled by this server (e.g. `["rust"]` for rust-analyzer,
    /// `["typescript", "javascript"]` for tsserver).
    pub language_ids: Vec<Language>,

    /// Server executable name (resolved via `PATH`) or absolute path.
    pub command: String,

    /// Command-line arguments passed to the server process.
    pub args: Vec<String>,

    /// Workspace root URI. Passed as `rootUri` in `initialize`.
    /// `None` for single-file mode.
    pub root_uri: Option<DocumentUri>,

    /// Passed verbatim as `initializationOptions` in the `initialize` request.
    pub initialization_options: Option<serde_json::Value>,

    /// Extra environment variables injected into the server process.
    pub env: Vec<(String, String)>,

    /// How long to wait (ms) before auto-restarting a crashed server.
    pub restart_delay_ms: u64,

    /// Maximum number of consecutive auto-restarts before giving up.
    pub max_restarts: u32,
}

impl LspServerConfig {
    /// Create a minimal config: just the command and language IDs.
    pub fn new(command: impl Into<String>, language_ids: Vec<Language>) -> Self {
        Self {
            language_ids,
            command: command.into(),
            args: Vec::new(),
            root_uri: None,
            initialization_options: None,
            env: Vec::new(),
            restart_delay_ms: 1_000,
            max_restarts: 5,
        }
    }

    /// Set the workspace root URI.
    pub fn with_root(mut self, root_uri: DocumentUri) -> Self {
        self.root_uri = Some(root_uri);
        self
    }

    /// Append command-line arguments.
    pub fn with_args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Set custom initialization options.
    pub fn with_init_options(mut self, opts: serde_json::Value) -> Self {
        self.initialization_options = Some(opts);
        self
    }
}
