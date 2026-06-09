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
//! `crabide-workspace` — workspace and document lifecycle management.
//!
//! # Overview
//!
//! The [`Workspace`] struct is the single source of truth for all open documents.
//! It connects:
//! - **VFS** (`crabide-vfs`) — all disk I/O goes through the `VirtualFileSystem` trait.
//! - **Buffers** (`crabide-buffer`) — each document has a `Document` + `EditHistory` + `CursorSet`.
//! - **Observers** (`DocumentObserver`) — LSP client, syntax engine, and UI register here
//!   to receive `on_document_opened / changed / closed` callbacks.
//!
//! # Multi-root
//! A workspace can have zero or more root directories (like VS Code's multi-root workspaces).
//! Each root is watched by the VFS file watcher; changes to watched files are surfaced as
//! `VfsEvent`s on the event bus (Phase 1).
//!
//! # Untitled buffers
//! `new_untitled()` creates a buffer with a synthetic `untitled://Untitled-N` URI.
//! Saving it for the first time calls `save_as()` which upgrades the URI to a real path.

pub mod workspace;

pub use workspace::{CloseResult, DocumentEntry, Workspace};

pub use crabide_buffer::Document;
pub use crabide_core::error::{Result, crabideError};
pub use crabide_core::types::{BufferId, DocumentUri, Language};
