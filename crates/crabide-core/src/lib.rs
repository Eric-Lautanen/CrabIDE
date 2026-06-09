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
