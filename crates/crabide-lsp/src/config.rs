//! LSP server configuration.

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

    /// Add extra environment variables for the server process.
    pub fn with_env(
        mut self,
        env: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        self.env
            .extend(env.into_iter().map(|(k, v)| (k.into(), v.into())));
        self
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_new_minimal() {
        let cfg = LspServerConfig::new("rust-analyzer", vec![Language::RUST]);
        assert_eq!(cfg.command, "rust-analyzer");
        assert_eq!(cfg.language_ids, vec![Language::RUST]);
        assert!(cfg.args.is_empty());
        assert!(cfg.root_uri.is_none());
        assert!(cfg.initialization_options.is_none());
        assert!(cfg.env.is_empty());
        assert_eq!(cfg.restart_delay_ms, 1_000);
        assert_eq!(cfg.max_restarts, 5);
    }

    #[test]
    fn config_with_root() {
        let uri = DocumentUri::parse("file:///workspace").unwrap();
        let cfg = LspServerConfig::new("server", vec![]).with_root(uri.clone());
        assert_eq!(cfg.root_uri, Some(uri));
    }

    #[test]
    fn config_with_args() {
        let cfg = LspServerConfig::new("server", vec![]).with_args(["--verbose", "--port=8080"]);
        assert_eq!(cfg.args, vec!["--verbose", "--port=8080"]);
    }

    #[test]
    fn config_with_args_from_vec() {
        let cfg = LspServerConfig::new("server", vec![]).with_args(vec!["--debug", "--no-daemon"]);
        assert_eq!(cfg.args, vec!["--debug", "--no-daemon"]);
    }

    #[test]
    fn config_with_init_options() {
        let opts = serde_json::json!({"setting": true});
        let cfg = LspServerConfig::new("server", vec![]).with_init_options(opts.clone());
        assert_eq!(cfg.initialization_options, Some(opts));
    }

    #[test]
    fn config_with_env() {
        let cfg = LspServerConfig::new("server", vec![])
            .with_env([("RUST_BACKTRACE", "1"), ("RUST_LOG", "debug")]);
        assert_eq!(
            cfg.env,
            vec![
                ("RUST_BACKTRACE".to_owned(), "1".to_owned()),
                ("RUST_LOG".to_owned(), "debug".to_owned()),
            ]
        );
    }

    #[test]
    fn config_with_env_iter() {
        let env = vec![("KEY", "value")];
        let cfg = LspServerConfig::new("server", vec![]).with_env(env);
        assert_eq!(cfg.env, vec![("KEY".to_owned(), "value".to_owned())]);
    }

    #[test]
    fn config_serialize_deserialize() {
        let cfg = LspServerConfig::new("test-server", vec![Language::RUST, Language::PYTHON])
            .with_args(["--verbose"])
            .with_root(DocumentUri::parse("file:///tmp").unwrap());
        let json = serde_json::to_string(&cfg).unwrap();
        let recovered: LspServerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(recovered.command, cfg.command);
        assert_eq!(recovered.language_ids, cfg.language_ids);
        assert_eq!(recovered.args, cfg.args);
        assert_eq!(recovered.root_uri, cfg.root_uri);
    }
}
