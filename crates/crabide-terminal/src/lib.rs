//! `crabide-terminal` — built-in terminal emulator.
//!
//! Components:
//! - [`grid`]    — Custom VT100/VT220 grid state machine built on `vte` parser.
//! - [`pty`]     — portable-pty wrapper for cross-platform PTY creation.
//! - [`manager`] — Terminal instance manager (multiple terminals, profiles).

pub mod grid;
pub mod manager;
pub mod pty;

pub use crabide_core::error::{crabideError, Result};
pub use grid::Grid;
pub use manager::{TerminalManager, TerminalProfile};
