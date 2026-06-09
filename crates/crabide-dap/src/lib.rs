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
