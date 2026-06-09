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
