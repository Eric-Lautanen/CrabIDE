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
//! `crabide-vfs` — Virtual filesystem: local, remote SSH, Dev Containers.
//!
//! # Architecture
//!
//! The VFS layer abstracts all file I/O behind the [`VirtualFileSystem`] trait
//! (defined in `crabide-core`). The UI and other crates never touch the
//! filesystem directly — they talk to a `Arc<dyn VirtualFileSystem>` obtained
//! from this crate.
//!
//! ## Implementations
//! - [`LocalVfs`]  — local disk via `tokio::fs` (always available)
//! - `SshVfs`      — remote SSH via `russh`     (feature = `remote-ssh`)
//! - `ContainerVfs`— Docker containers via `bollard` (feature = `dev-containers`)
//!
//! ## File watching
//! [`VfsWatcher`] wraps `notify-debouncer-full` and translates OS events into
//! typed [`VfsEvent`]s that are sent through a crossbeam channel to the UI.

pub mod helpers;
pub mod local;
pub mod memory;
pub mod read_only;
pub mod resolver;
pub mod watcher;

pub use helpers::{
    canonical_uri, is_descendant, path_to_uri, relative_path, uri_extension, uri_file_name,
    uri_file_stem, uri_to_path,
};
pub use local::LocalVfs;
pub use memory::MemoryVfs;
pub use read_only::ReadOnlyVfs;
pub use resolver::{VfsKind, VfsResolver};
pub use watcher::VfsWatcher;

// Re-export core VFS types for convenience.
pub use crabide_core::error::{Result, crabideError};
pub use crabide_core::event::VfsEvent;
pub use crabide_core::traits::{DirEntry, DirEntryKind, VirtualFileSystem};
