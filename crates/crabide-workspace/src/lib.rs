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
