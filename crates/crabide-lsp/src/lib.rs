//! `crabide-lsp` — Language Server Protocol client.
//!
//! Uses a custom JSON-RPC transport instead of `async-lsp`, eliminating the
//! `tower` middleware dependency chain. The transport is Content-Length framed
//! JSON over stdin/stdout (~300 LOC; see [`transport`]).
//!
//! # Components
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`transport`]   | JSON-RPC 2.0 framing over process stdio |
//! | [`config`]      | [`LspServerConfig`]: command, args, root URI, language IDs |
//! | [`client`]      | [`LspClient`]: initialize handshake + all LSP methods |
//! | [`server_mgr`]  | [`LspServerManager`]: spawn, crash detect, auto-restart |
//! | [`convert`]     | Internal type conversions between crabide ↔ lsp-types |

pub mod client;
pub mod config;
pub mod convert;
pub mod server_mgr;
pub mod transport;

pub use client::LspClient;
pub use config::LspServerConfig;
pub use server_mgr::LspServerManager;
pub use transport::LspTransport;

pub use crabide_core::error::{crabideError, Result};
