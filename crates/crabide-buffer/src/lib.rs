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
    clippy::too_many_lines,
    clippy::match_same_arms,
    clippy::assigning_clones,
    clippy::needless_pass_by_value,
    clippy::manual_let_else,
    clippy::items_after_statements,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::fn_params_excessive_bools,
    clippy::used_underscore_binding,
    clippy::semicolon_if_nothing_returned,
    clippy::return_self_not_must_use,
    clippy::default_trait_access,
    clippy::cast_possible_wrap,
    clippy::redundant_closure,
    clippy::if_not_else,
    clippy::format_push_string,
    clippy::format_collect,
    clippy::trivially_copy_pass_by_ref,
    clippy::unnecessary_wraps,
    clippy::unnecessary_debug_formatting,
    clippy::wildcard_imports,
    clippy::match_wildcard_for_single_variants,
    clippy::explicit_iter_loop,
    clippy::needless_continue,
    clippy::float_cmp,
    clippy::unused_self,
    clippy::collapsible_else_if,
    clippy::redundant_else,
    clippy::unnecessary_map_or,
    clippy::many_single_char_names,
    clippy::redundant_closure_for_method_calls,
    clippy::uninlined_format_args,
    clippy::cast_lossless,
    clippy::map_unwrap_or
)]
//! Text buffer subsystem for crabide Editor.
//!
//! # Components
//!
//! - [`buffer`] — `Document`: the primary text buffer backed by `ropey::Rope`.
//!   Implements `TextBuffer` for read access and provides edit operations.
//! - [`history`] — `EditHistory`: branching undo/redo stack with named checkpoints.
//! - [`cursor`] — `Cursor` and `MultiCursor`: cursor, selection, and column-select state.
//! - [`snippet`] — `SnippetEngine`: VS Code–compatible snippet expansion with tabstops.

pub mod buffer;
pub mod cursor;
pub mod history;
pub mod snippet;

pub use buffer::Document;
pub use cursor::{Cursor, CursorSet, SelectionMode};
pub use history::{EditHistory, HistoryCheckpoint};
pub use snippet::{Snippet, SnippetContext, SnippetEngine, SnippetExpansion, SnippetTabstop};
