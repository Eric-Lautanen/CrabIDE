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
//! `crabide-dap` — Debug Adapter Protocol client.
//!
//! # Architecture
//!
//! ```text
//!  ┌──────────────────┐     stdin/stdout      ┌──────────────────────┐
//!  │  crabide editor  │ ←── DapTransport ────→ │  Debug Adapter (DA)  │
//!  │                  │                         │  (codelldb, debugpy, │
//!  │  DapClient::start│ ──→  DapEvent ────────→ │   js-debug, …)       │
//!  └──────────────────┘   event_tx channel      └──────────────────────┘
//! ```
//!
//! The `DapClient` spawns the adapter process, performs the `initialize`
//! handshake, and exposes methods for all standard DAP operations.  Incoming
//! events from the adapter are translated to typed [`crabide_core::event::DapEvent`]s
//! and forwarded to the main editor event bus via a `crossbeam_channel::Sender`.

pub mod client;
pub mod transport;
pub mod types;

pub use client::{DapClient, resolve_adapter};
pub use crabide_core::error::{Result, crabideError};
pub use types::{LaunchConfig, load_launch_configs, parse_launch_json};
