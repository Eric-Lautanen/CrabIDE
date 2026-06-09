#![warn(clippy::pedantic)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_lossless,
    clippy::cast_sign_loss,
    clippy::doc_markdown,
    clippy::fn_params_excessive_bools,
    clippy::if_not_else,
    clippy::manual_let_else,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::needless_pass_by_value,
    clippy::redundant_closure_for_method_calls,
    clippy::return_self_not_must_use,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::single_match_else,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unreadable_literal,
    clippy::wildcard_imports,
)]
//! `crabide-core` — foundational types, errors, events, and traits.
//!
//! This crate is the dependency root of the crabide workspace. Every other
//! internal crate depends on it. Keep it lean: no heavy deps, no I/O, no UI.

pub mod error;
pub mod event;
pub mod traits;
pub mod types;

// Re-export the most commonly used items at the crate root
pub use error::{Result, crabideError};
pub use types::{
    BufferId, DocumentId, DocumentUri, ExtensionId, Language, Position, Range, Selection, TextEdit,
    WorkspaceId,
};
