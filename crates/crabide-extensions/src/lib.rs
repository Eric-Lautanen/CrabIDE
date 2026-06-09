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
    SidebarPaneRegistration, StatusBarAlignment, TextEdit,
};
pub use registry::{DownloadResult, RegistryClient, RegistryExtension};
