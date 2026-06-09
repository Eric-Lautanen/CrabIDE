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
