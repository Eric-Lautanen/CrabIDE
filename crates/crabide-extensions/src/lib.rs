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
//! `crabide-extensions` — native and WASM extension host for crabide Editor.
//!
//! # Architecture
//!
//! Extensions are managed by [`ExtensionHost`].  Five built-in extensions are
//! compiled directly into the editor binary and implement the [`NativeExtension`]
//! trait.  External WASM components (loaded from `.wasm` files) are supported via
//! the `wasm-extensions` feature flag which pulls in wasmtime + WIT bindgen.
//!
//! # Extension lifecycle
//!
//! ```text
//!  ┌─────────────────────────────────────────────────────────────────┐
//!  │ crabide-app (each frame)                                         │
//!  │                                                                  │
//!  │  1. build ExtensionContext from UiState snapshot                 │
//!  │  2. host.poll_all(ctx) → Vec<ExtensionOutput>                   │
//!  │  3. apply outputs to UiState (diagnostics, status bar, todos…)  │
//!  └─────────────────────────────────────────────────────────────────┘
//! ```

mod extensions;
pub mod host;
pub mod hot_reload;
pub mod registry;
#[cfg(feature = "wasm-extensions")]
pub mod wasm_ext;

pub use host::{
    CommandResult, CompletionItem, CompletionKind, ContentBlock, ContextMenuContext,
    ContextMenuContribution, ExtensionCapabilities, ExtensionCategory, ExtensionContext,
    ExtensionDiagnostic, ExtensionHost, ExtensionManifest, ExtensionOutput, ExtensionSeverity,
    ExtensionSource, GutterMarker, HoverResult, InstalledExtension, NativeExtension,
    NavigateTarget, PanelLocation, PanelRegistration, RegisteredCommand, RowItem,
    SidebarPaneRegistration, StatusBarAlignment, TextEdit, is_output_allowed,
};
pub use registry::{DownloadResult, RegistryClient, RegistryExtension};
