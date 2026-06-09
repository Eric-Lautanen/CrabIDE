#![warn(clippy::pedantic)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::doc_markdown,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::struct_excessive_bools,
    clippy::similar_names,
)]
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

pub use crabide_core::error::{Result, crabideError};
