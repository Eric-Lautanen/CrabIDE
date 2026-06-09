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
    clippy::assigning_clones,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    clippy::collapsible_else_if,
    clippy::default_trait_access,
    clippy::explicit_iter_loop,
    clippy::float_cmp,
    clippy::fn_params_excessive_bools,
    clippy::format_collect,
    clippy::format_push_string,
    clippy::if_not_else,
    clippy::items_after_statements,
    clippy::manual_let_else,
    clippy::many_single_char_names,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::match_wildcard_for_single_variants,
    clippy::needless_continue,
    clippy::needless_pass_by_value,
    clippy::redundant_closure,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_else,
    clippy::return_self_not_must_use,
    clippy::semicolon_if_nothing_returned,
    clippy::too_many_lines,
    clippy::trivially_copy_pass_by_ref,
    clippy::uninlined_format_args,
    clippy::unnecessary_debug_formatting,
    clippy::unnecessary_map_or,
    clippy::unnecessary_wraps,
    clippy::unused_self,
    clippy::used_underscore_binding,
    clippy::wildcard_imports
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
