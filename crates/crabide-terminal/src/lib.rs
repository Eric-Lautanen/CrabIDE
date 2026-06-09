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
//! `crabide-terminal` — built-in terminal emulator.
//!
//! Components:
//! - [`grid`]    — Custom VT100/VT220 grid state machine built on `vte` parser.
//! - [`pty`]     — portable-pty wrapper for cross-platform PTY creation.
//! - [`manager`] — Terminal instance manager (multiple terminals, profiles).

pub mod grid;
pub mod manager;
pub mod pty;

pub use crabide_core::error::{Result, crabideError};
pub use grid::{
    Grid, MouseButton, ScrollDirection, encode_mouse_motion, encode_mouse_press,
    encode_mouse_release, encode_mouse_scroll,
};
pub use manager::{TerminalManager, TerminalProfile};
