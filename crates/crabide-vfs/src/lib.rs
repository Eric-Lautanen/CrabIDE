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
