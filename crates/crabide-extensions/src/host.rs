//! Extension host — manages native and WASM extension lifecycle.
//!
//! # Architecture
//!
//! Each extension implements [`NativeExtension`]. The host calls:
//! - `activate()` on load
//! - `deactivate()` on unload
//! - `on_document_open/change/save()` on document lifecycle events
//! - `poll()` every frame to collect [`ExtensionOutput`] items that the app
//!   applies to `UiState`.

use std::path::PathBuf;

// ── Gutter marker types ───────────────────────────────────────────────────────

/// A gutter decoration marker contributed by an extension.
#[derive(Debug, Clone)]
pub struct GutterMarker {
    /// 0-based line number.
    pub line: u32,
    /// Icon character or emoji (e.g. "●", "⬡").
    pub icon: String,
    pub tooltip: Option<String>,
    pub severity: Option<ExtensionSeverity>,
    /// Command executed when the marker is clicked.
    pub command: Option<String>,
}

// ── Hover / completion types ──────────────────────────────────────────────────

/// Hover information result returned by a hover-provider extension.
#[derive(Debug, Clone)]
pub struct HoverResult {
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
    /// Markdown or plain text content.
    pub contents: String,
}

/// A single completion suggestion.
#[derive(Debug, Clone)]
pub struct CompletionItem {
    pub label: String,
    pub detail: Option<String>,
    pub insert_text: String,
    pub kind: CompletionKind,
}

/// Completion item kind (mirrors LSP CompletionItemKind).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    Text,
    Method,
    Function,
    Ctor,
    Field,
    Variable,
    Class,
    Iface,
    Module,
    Property,
    Keyword,
    Snippet,
    Color,
    File,
    Reference,
    Folder,
    Constant,
    Struct,
    Operator,
    TypeParameter,
}

// ── Shared types ──────────────────────────────────────────────────────────────

/// Metadata for an extension (builtin or installed).
#[derive(Debug, Clone)]
pub struct ExtensionManifest {
    /// Unique slug, e.g. `"git-blame-inline"`.
    pub id: String,
    /// Human-readable name shown in the panel.
    pub name: String,
    /// Short one-line description.
    pub description: String,
    /// Version string, e.g. `"1.0.0"`.
    pub version: String,
    /// Author / publisher name.
    pub author: String,
    /// Functional categories for the extension.
    pub categories: Vec<ExtensionCategory>,
    /// True for extensions compiled into the binary that cannot be uninstalled.
    /// The 5 pre-installed extensions ship with `is_builtin: false` so they
    /// can be removed; set this only for truly core, non-removable components.
    pub is_builtin: bool,
}

/// Functional category for grouping extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionCategory {
    Git,
    Languages,
    Linters,
    Themes,
    Productivity,
    Debuggers,
    Formatters,
    Other,
}

impl ExtensionCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Git => "Git",
            Self::Languages => "Languages",
            Self::Linters => "Linters",
            Self::Themes => "Themes",
            Self::Productivity => "Productivity",
            Self::Debuggers => "Debuggers",
            Self::Formatters => "Formatters",
            Self::Other => "Other",
        }
    }

    /// Accent color (R,G,B) used to tint extension icons.
    pub fn color(self) -> (u8, u8, u8) {
        match self {
            Self::Git => (0xf1, 0x50, 0x2f),
            Self::Languages => (0x56, 0x9c, 0xd6),
            Self::Linters => (0xff, 0xcc, 0x00),
            Self::Themes => (0xb5, 0x7e, 0xff),
            Self::Productivity => (0x4e, 0xc9, 0xb0),
            Self::Debuggers => (0xce, 0x91, 0x78),
            Self::Formatters => (0x6a, 0x99, 0x55),
            Self::Other => (0x80, 0x80, 0x80),
        }
    }
}

/// Where an installed extension comes from.
#[derive(Debug, Clone)]
pub enum ExtensionSource {
    /// Compiled into the editor binary.
    Builtin,
    /// Loaded from a local `.wasm` file.
    Local(PathBuf),
    /// Downloaded from the extension registry.
    Registry { download_url: String },
}

/// Installed extension record (metadata + enabled state).
#[derive(Debug, Clone)]
pub struct InstalledExtension {
    pub manifest: ExtensionManifest,
    pub enabled: bool,
    pub source: ExtensionSource,
}

// ── Extension I/O ─────────────────────────────────────────────────────────────

/// Lightweight read-only view of editor state passed to extensions each frame.
pub struct ExtensionContext<'a> {
    /// Full text of the active document, if any.
    pub active_text: Option<&'a str>,
    /// URI (file path as string) of the active document.
    pub active_uri: Option<&'a str>,
    /// Language identifier of the active document (e.g. `"rust"`, `"markdown"`).
    pub active_language: &'a str,
    /// Open workspace roots.
    pub workspace_roots: &'a [PathBuf],
    /// Git blame lines for the active file: `(0-based line, display string)`.
    pub blame_lines: &'a [(u32, String)],
    /// 0-based cursor line in the active document.
    pub cursor_line: u32,
    /// 0-based cursor character column in the active document.
    pub cursor_col: u32,
    /// Current selection as (start_line, start_col, end_line, end_col), if any.
    pub selection: Option<(u32, u32, u32, u32)>,
    /// Active theme identifier (e.g. `"crabide-dark"`, `"crabide-light"`).
    pub current_theme_id: &'a str,
}

/// Result from `execute_command`.
#[derive(Debug, Clone)]
pub enum CommandResult {
    Ok,
    Error(String),
}

/// A lightweight diagnostic emitted by a native extension.
#[derive(Debug, Clone)]
pub struct ExtensionDiagnostic {
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub severity: ExtensionSeverity,
    pub message: String,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

// ── Panel contribution types ───────────────────────────────────────────────────

/// Where a contributed extension panel docks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelLocation {
    /// Bottom strip (stacked with Terminal, Git, Debug panels).
    Bottom,
    /// Right side panel (e.g. Markdown preview).
    Right,
}

/// A block of rich content contributed by an extension to its panel.
#[derive(Debug, Clone)]
pub enum ContentBlock {
    /// A plain-text paragraph (wrapping label).
    Paragraph(String),
    /// A monospace / preformatted block (code, output, etc.).
    Preformatted(String),
    /// A bold section heading.
    Heading(String),
    /// A horizontal separator rule.
    Separator,
    /// A list of clickable rows.
    Rows(Vec<RowItem>),
}

/// A single clickable row within a `ContentBlock::Rows` list.
#[derive(Debug, Clone)]
pub struct RowItem {
    /// Short icon or emoji prefix (e.g. `"✗"`, `"📌"`).
    pub icon: String,
    /// Main display text for the row.
    pub text: String,
    /// Optional tooltip shown on hover.
    pub tooltip: Option<String>,
    /// Action performed when the row is clicked.
    pub on_click: Option<NavigateTarget>,
}

/// What happens when a user clicks a row item in an extension panel.
#[derive(Debug, Clone)]
pub enum NavigateTarget {
    /// Open a file and scroll to a specific line.
    FileAt { path: PathBuf, line: u32 },
    /// Execute a registered extension command by id.
    Command(String),
}

/// A command that an extension contributes to the editor.
#[derive(Debug, Clone)]
pub struct RegisteredCommand {
    /// Unique command id, e.g. `"markdown-preview.toggle"`.
    pub id: String,
    /// Human-readable title shown in the command palette.
    pub title: String,
    /// Optional default keybinding string, e.g. `"ctrl+shift+v"`.
    pub default_keybinding: Option<String>,
}

/// Static registration of a panel surface that an extension contributes.
///
/// Extensions return these from [`NativeExtension::panels`] to declare what
/// UI surfaces they need. The editor shell renders them dynamically.
#[derive(Debug, Clone)]
pub struct PanelRegistration {
    /// Unique panel id, e.g. `"markdown-preview.panel"`.
    pub id: String,
    /// Title shown in the panel header strip.
    pub title: String,
    /// Where the panel is docked.
    pub location: PanelLocation,
    /// Minimum height (Bottom) or width (Right) in logical pixels.
    pub min_size: f32,
    /// Default height / width in logical pixels.
    pub default_size: f32,
    /// Whether the panel starts visible when first registered.
    pub initially_open: bool,
    /// Command id of the toggle command (shown in View menu automatically).
    pub toggle_command: Option<String>,
}

/// Which side of the status bar an extension item docks to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusBarAlignment {
    /// Left section, next to the git/debug indicators (default).
    #[default]
    Left,
    /// Right section, to the left of the LSP/document info.
    Right,
}

/// A text replacement operation (mirrors WIT text-edit and LSP TextEdit).
#[derive(Debug, Clone)]
pub struct TextEdit {
    pub range: (u32, u32, u32, u32), // start_line, start_col, end_line, end_col
    pub new_text: String,
}

/// Output produced by an extension during `poll()`.
pub enum ExtensionOutput {
    /// Update the extension's status-bar slot.
    StatusBarText {
        extension_id: String,
        text: String,
        tooltip: Option<String>,
        /// Extension command to execute when the item is clicked, if any.
        command: Option<String>,
        /// Which side of the status bar this item docks to.
        alignment: StatusBarAlignment,
    },
    /// Push diagnostics for a document URI (replaces previous set from this extension).
    Diagnostics {
        extension_id: String,
        uri: String,
        items: Vec<ExtensionDiagnostic>,
    },
    /// Content update for a registered extension panel.
    PanelContent {
        panel_id: String,
        blocks: Vec<ContentBlock>,
    },
    /// Show a brief notification in the status bar.
    Notification { message: String, is_error: bool },
    /// Request the app perform a theme cycle.
    CycleTheme,
    /// Content update for a left-sidebar pane registered by this extension.
    SidebarPaneContent {
        pane_id: String,
        blocks: Vec<ContentBlock>,
    },
    /// Write (create or overwrite) a file on disk.
    ///
    /// Only honoured for extensions whose [`ExtensionCapabilities::file_write`]
    /// is `true`.
    WriteFile {
        path: std::path::PathBuf,
        content: String,
    },
    /// Send raw bytes to a running terminal instance.
    ///
    /// Only honoured for extensions whose [`ExtensionCapabilities::terminal`]
    /// is `true`.
    SendToTerminal { terminal_id: u32, data: Vec<u8> },
    /// Ask the editor to open a new terminal.
    OpenTerminal {
        title: String,
        /// Optional shell command to run inside the new terminal.
        command: Option<String>,
    },
    /// Set gutter markers for a URI from this extension.
    GutterMarkers {
        extension_id: String,
        uri: String,
        markers: Vec<GutterMarker>,
    },
    /// Show a registered extension panel by id.
    ShowPanel { panel_id: String },
    /// Hide a registered extension panel by id.
    HidePanel { panel_id: String },
    /// Request that the cursor be moved to the given position in the active document.
    SetCursorPosition { line: u32, character: u32 },
    /// Request that text edits be applied to a document.
    ApplyEdits { uri: String, edits: Vec<TextEdit> },
    /// Request that text be inserted at the current cursor position.
    InsertAtCursor { text: String },
    /// Set status bar slot visibility.
    StatusBarVisible { extension_id: String, visible: bool },
}

// ── Capabilities ──────────────────────────────────────────────────────────────

/// What system resources an extension declares it needs.
///
/// Users are shown a permission prompt before a non-builtin extension
/// with elevated capabilities is enabled.
#[derive(Debug, Clone, Default)]
pub struct ExtensionCapabilities {
    /// Extension may read files outside the workspace.
    pub file_read: bool,
    /// Extension may write, create, or delete files.
    pub file_write: bool,
    /// Extension may interact with the integrated terminal.
    pub terminal: bool,
    /// Extension may make outbound network requests.
    pub network: bool,
}

/// Whether the given `ExtensionOutput` is allowed given the extension's capabilities.
///
/// Returns `true` if the output does not require any restricted capability, or if
/// the extension has declared the required capability.  Sensitive outputs that
/// carry no `extension_id` are checked here at the host level.
pub fn is_output_allowed(output: &ExtensionOutput, caps: &ExtensionCapabilities) -> bool {
    match output {
        ExtensionOutput::WriteFile { .. } => caps.file_write,
        ExtensionOutput::SendToTerminal { .. } | ExtensionOutput::OpenTerminal { .. } => {
            caps.terminal
        }
        ExtensionOutput::ApplyEdits { .. }
        | ExtensionOutput::InsertAtCursor { .. }
        | ExtensionOutput::SetCursorPosition { .. } => caps.file_write,
        // All other outputs (diagnostics, status bar, gutter markers, panels,
        // notifications, cycle theme, sidebar content, show/hide panel,
        // status bar visibility) are safe and do not require capability checks.
        _ => true,
    }
}

// ── Sidebar pane ──────────────────────────────────────────────────────────────

/// Registration for a left-sidebar activity-bar pane contribution.
#[derive(Debug, Clone)]
pub struct SidebarPaneRegistration {
    /// Unique pane id, e.g. `"my-ext.sidebar"`.
    pub id: String,
    /// Display title shown when the pane is active.
    pub title: String,
    /// Icon displayed in the activity bar (emoji / single char).
    pub icon: String,
    /// Optional command to toggle this pane's visibility.
    pub toggle_command: Option<String>,
}

// ── Context menus ─────────────────────────────────────────────────────────────

/// Which UI area a context-menu contribution attaches to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMenuContext {
    /// Right-click inside the code editor.
    Editor,
    /// Right-click on a file/folder in the explorer tree.
    FileExplorer,
    /// Right-click on an editor tab.
    TabBar,
    /// Right-click inside the terminal panel.
    Terminal,
}

/// A single item contributed to a right-click context menu.
#[derive(Debug, Clone)]
pub struct ContextMenuContribution {
    /// Which surface this item appears in.
    pub context: ContextMenuContext,
    /// Unique item id.
    pub id: String,
    /// Label shown in the menu.
    pub label: String,
    /// Command executed when the item is chosen.
    pub command: String,
}

// ── NativeExtension trait ─────────────────────────────────────────────────────

/// Implement this trait to create a built-in or locally-loaded extension.
pub trait NativeExtension: Send + Sync {
    /// Return the static manifest for this extension.
    fn manifest(&self) -> &ExtensionManifest;

    /// Declare the system resources this extension requires.
    ///
    /// The editor may restrict outputs that exceed declared capabilities.
    fn capabilities(&self) -> ExtensionCapabilities {
        ExtensionCapabilities::default()
    }

    /// Return left-sidebar pane contributions.
    ///
    /// Each pane gets an activity-bar icon slot and a content area that
    /// the extension populates via [`ExtensionOutput::SidebarPaneContent`].
    fn sidebar_panes(&self) -> Vec<SidebarPaneRegistration> {
        vec![]
    }

    /// Return context-menu item contributions.
    ///
    /// Items are shown in the matching surface's right-click menu and
    /// trigger [`execute_command`] when chosen.
    fn context_menus(&self) -> Vec<ContextMenuContribution> {
        vec![]
    }

    /// Called whenever a terminal produces output.
    ///
    /// The extension can return [`ExtensionOutput::SendToTerminal`] or
    /// [`ExtensionOutput::PanelContent`] items in response.
    ///
    /// Only called for extensions with [`ExtensionCapabilities::terminal`].
    fn on_terminal_output(&mut self, _terminal_id: u32, _data: &[u8]) -> Vec<ExtensionOutput> {
        vec![]
    }

    /// Return the panels this extension contributes.
    ///
    /// Called once on activation and whenever the extension is re-enabled.
    /// The editor shell uses these registrations to create panel slots
    /// dynamically — no editor source file changes are needed per-extension.
    fn panels(&self) -> Vec<PanelRegistration> {
        vec![]
    }

    /// Return the commands this extension contributes.
    ///
    /// The editor adds these to the command palette and optionally binds
    /// their `default_keybinding` at runtime.
    fn commands(&self) -> Vec<RegisteredCommand> {
        vec![]
    }

    /// Called when a document is closed.
    fn on_document_close(&mut self, _uri: &str, _ctx: &ExtensionContext) {}

    /// Called when the cursor moves in a document.
    fn on_cursor_move(&mut self, _uri: &str, _line: u32, _col: u32, _ctx: &ExtensionContext) {}

    /// Called when the selection changes.
    fn on_selection_change(
        &mut self,
        _uri: &str,
        _start_line: u32,
        _start_col: u32,
        _end_line: u32,
        _end_col: u32,
        _ctx: &ExtensionContext,
    ) {
    }

    /// Called when an enabled extension should provide hover info.
    ///
    /// Returns `None` if this extension has no hover info for this position.
    fn provide_hover(
        &mut self,
        _uri: &str,
        _line: u32,
        _col: u32,
        _ctx: &ExtensionContext,
    ) -> Option<HoverResult> {
        None
    }

    /// Called when an enabled extension should provide completions.
    fn provide_completions(
        &mut self,
        _uri: &str,
        _line: u32,
        _col: u32,
        _ctx: &ExtensionContext,
    ) -> Vec<CompletionItem> {
        vec![]
    }

    /// Called when an enabled extension should provide gutter markers for a URI.
    fn provide_gutter_markers(&mut self, _uri: &str, _ctx: &ExtensionContext) -> Vec<GutterMarker> {
        vec![]
    }

    /// Called once when the extension is enabled / first loaded.
    fn activate(&mut self, ctx: &ExtensionContext);

    /// Called when the extension is disabled or the editor is shutting down.
    fn deactivate(&mut self);

    /// Called when a document is opened.
    fn on_document_open(&mut self, uri: &str, language_id: &str, ctx: &ExtensionContext);

    /// Called after document content changes.
    fn on_document_change(&mut self, uri: &str, ctx: &ExtensionContext);

    /// Called when a document is saved.
    fn on_document_save(&mut self, uri: &str, ctx: &ExtensionContext);

    /// Called to execute a command this extension registered.
    fn execute_command(&mut self, command: &str, args: &[String]) -> CommandResult;

    /// Called every frame while the extension is enabled.
    /// Returns any outputs the extension wants to push to the UI.
    fn poll(&mut self, ctx: &ExtensionContext) -> Vec<ExtensionOutput>;
}

// ── ExtensionHost ─────────────────────────────────────────────────────────────

/// Manages all installed extensions and drives their lifecycle.
pub struct ExtensionHost {
    /// Installed extension records (manifest + enabled flag + source).
    installed: Vec<InstalledExtension>,
    /// Runtime instances — parallel vec to `installed` (same order).
    instances: Vec<Option<Box<dyn NativeExtension>>>,
    /// Directory where user-installed extension `.wasm` files are stored.
    extensions_dir: Option<PathBuf>,
    /// Channel receiver for hot-reload events (WASM file changed on disk).
    /// `None` if the hot-reload watcher failed to start.
    hot_reload_rx: Option<crossbeam_channel::Receiver<std::path::PathBuf>>,
    /// Pending document-change notifications keyed by URI.
    /// Maps uri → time the last change was queued.
    pending_doc_changes: std::collections::HashMap<String, std::time::Instant>,
}

impl ExtensionHost {
    /// Create the host and register all builtin extensions (activated by default).
    pub fn new() -> Self {
        use crate::extensions::builtin_extensions;
        let builtins = builtin_extensions();

        let mut installed = Vec::with_capacity(builtins.len());
        let mut instances = Vec::with_capacity(builtins.len());

        let empty_ctx = ExtensionContext {
            active_text: None,
            active_uri: None,
            active_language: "",
            workspace_roots: &[],
            blame_lines: &[],
            cursor_line: 0,
            cursor_col: 0,
            selection: None,
            current_theme_id: "crabide-dark",
        };

        for mut ext in builtins {
            let manifest = ext.manifest().clone();
            ext.activate(&empty_ctx);
            installed.push(InstalledExtension {
                manifest,
                enabled: true,
                source: ExtensionSource::Builtin,
            });
            instances.push(Some(ext));
        }

        Self {
            installed,
            instances,
            extensions_dir: None,
            hot_reload_rx: None,
            pending_doc_changes: std::collections::HashMap::new(),
        }
    }

    /// All installed extension records.
    pub fn installed(&self) -> &[InstalledExtension] {
        &self.installed
    }

    /// Enable or disable an extension by id.
    ///
    /// Calls `activate` / `deactivate` on the underlying instance.
    pub fn set_enabled(&mut self, id: &str, enabled: bool, ctx: &ExtensionContext) {
        if let Some(idx) = self.installed.iter().position(|e| e.manifest.id == id) {
            if self.installed[idx].enabled == enabled {
                return;
            }
            self.installed[idx].enabled = enabled;
            if let Some(inst) = self.instances[idx].as_mut() {
                if enabled {
                    inst.activate(ctx);
                } else {
                    inst.deactivate();
                }
            }
        }
    }

    /// Uninstall any extension by id.
    ///
    /// All extensions — including pre-installed ones — can be uninstalled.
    /// For `ExtensionSource::Local` and `ExtensionSource::Registry` the
    /// backing file is deleted from the extensions directory.
    pub fn uninstall(&mut self, id: &str) -> Result<(), String> {
        if let Some(idx) = self.installed.iter().position(|e| e.manifest.id == id) {
            if let Some(inst) = self.instances[idx].as_mut() {
                inst.deactivate();
            }
            // Remove the file from disk.
            let source = self.installed[idx].source.clone();
            self.installed.remove(idx);
            self.instances.remove(idx);

            match source {
                ExtensionSource::Local(path) => {
                    if path.exists() {
                        if let Err(e) = std::fs::remove_file(&path) {
                            log::warn!("Could not delete extension file {:?}: {e}", path);
                        }
                    }
                }
                ExtensionSource::Registry { .. } => {
                    // Registry extensions: the .wasm is stored in extensions_dir/<id>.wasm
                    if let Some(ref dir) = self.extensions_dir {
                        let path = dir.join(format!("{id}.wasm"));
                        if path.exists() {
                            if let Err(e) = std::fs::remove_file(&path) {
                                log::warn!(
                                    "Could not delete registry extension file {:?}: {e}",
                                    path
                                );
                            }
                        }
                    }
                }
                ExtensionSource::Builtin => {}
            }

            Ok(())
        } else {
            Err(format!("Extension '{id}' not found"))
        }
    }

    /// Load a `.wasm` extension from a local path (stub — full wasmtime integration
    /// requires the `wasm-extensions` feature and a WIT-compiled component).
    pub fn install_local(&mut self, path: PathBuf) -> Result<String, String> {
        let ext_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_owned();

        if self.installed.iter().any(|e| e.manifest.id == ext_name) {
            return Err(format!("Extension '{ext_name}' is already installed"));
        }

        // Copy into the managed extensions directory if the file isn't already there.
        let stored_path = if let Some(ref dir) = self.extensions_dir {
            let dest = dir.join(
                path.file_name()
                    .unwrap_or(std::ffi::OsStr::new("extension.wasm")),
            );
            if path != dest {
                if let Err(e) = std::fs::copy(&path, &dest) {
                    return Err(format!("Failed to copy extension to extensions dir: {e}"));
                }
            }
            dest
        } else {
            path.clone()
        };

        let manifest = ExtensionManifest {
            id: ext_name.clone(),
            name: ext_name
                .replace('-', " ")
                .split_whitespace()
                .map(|w| {
                    let mut c = w.chars();
                    match c.next() {
                        None => String::new(),
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
            description: "Locally loaded WASM extension".into(),
            version: "0.0.0".into(),
            author: "Local".into(),
            categories: vec![ExtensionCategory::Other],
            is_builtin: false,
        };

        log::info!(
            "install_local: {} stored at {}",
            ext_name,
            stored_path.display()
        );
        #[cfg(feature = "wasm-extensions")]
        let instance: Option<Box<dyn NativeExtension>> = {
            let empty_ctx = ExtensionContext {
                active_text: None,
                active_uri: None,
                active_language: "",
                workspace_roots: &[],
                blame_lines: &[],
                cursor_line: 0,
                cursor_col: 0,
                selection: None,
                current_theme_id: "crabide-dark",
            };
            match crate::wasm_ext::WasmExtension::load(&stored_path) {
                Ok(mut ext) => {
                    ext.activate(&empty_ctx);
                    Some(ext)
                }
                Err(e) => {
                    log::error!("Failed to load WASM extension {ext_name}: {e}");
                    None
                }
            }
        };

        #[cfg(not(feature = "wasm-extensions"))]
        let instance: Option<Box<dyn NativeExtension>> = None;

        self.installed.push(InstalledExtension {
            manifest,
            enabled: instance.is_some(),
            source: ExtensionSource::Local(stored_path),
        });
        self.instances.push(instance);

        Ok(ext_name)
    }

    /// Install a registry extension by ID with checksum-verified download.
    ///
    /// Downloads the extension binary via `registry_client`, verifies its
    /// SHA-256 checksum, saves it to the extensions directory, and attempts
    /// to load it through the WASM engine.  Returns the installed extension
    /// name on success.
    pub fn install_registry(
        &mut self,
        ext: &crate::RegistryExtension,
        registry_client: &crate::RegistryClient,
    ) -> Result<String, String> {
        if self.installed.iter().any(|e| e.manifest.id == ext.id) {
            return Err(format!("Extension '{}' is already installed", ext.id));
        }

        // 1. Download with checksum verification.
        let result = registry_client.download(ext)?;

        // 2. Determine storage path.
        let dir = self
            .extensions_dir
            .clone()
            .ok_or_else(|| "Extensions directory not configured".to_string())?;
        if !dir.exists() {
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("Failed to create extensions dir: {e}"))?;
        }
        let file_name = format!("{}-{}.wasm", ext.id, ext.version);
        let dest = dir.join(&file_name);

        // 3. Write to disk (atomic: write to temp, rename).
        let tmp = dir.join(format!(".{}.tmp", file_name));
        std::fs::write(&tmp, &result.bytes)
            .map_err(|e| format!("Failed to write extension file: {e}"))?;
        std::fs::rename(&tmp, &dest)
            .map_err(|e| format!("Failed to rename extension file: {e}"))?;

        log::info!(
            "install_registry: {} v{} saved to {}",
            ext.id,
            ext.version,
            dest.display()
        );

        // 4. Build a proper manifest from registry metadata.
        let manifest = ExtensionManifest {
            id: ext.id.clone(),
            name: ext.name.clone(),
            description: ext.description.clone(),
            version: ext.version.clone(),
            author: ext.author.clone(),
            categories: vec![ExtensionCategory::Other],
            is_builtin: false,
        };

        // 5. Try to load via WASM engine (feature-gated).
        #[cfg(feature = "wasm-extensions")]
        let instance: Option<Box<dyn NativeExtension>> = {
            let empty_ctx = ExtensionContext {
                active_text: None,
                active_uri: None,
                active_language: "",
                workspace_roots: &[],
                blame_lines: &[],
                cursor_line: 0,
                cursor_col: 0,
                selection: None,
                current_theme_id: "crabide-dark",
            };
            match crate::wasm_ext::WasmExtension::load(&dest) {
                Ok(mut ext_inst) => {
                    ext_inst.activate(&empty_ctx);
                    Some(ext_inst)
                }
                Err(e) => {
                    log::error!("Failed to load registry extension '{}': {e}", ext.id);
                    None
                }
            }
        };

        #[cfg(not(feature = "wasm-extensions"))]
        let instance: Option<Box<dyn NativeExtension>> = None;

        self.installed.push(InstalledExtension {
            manifest,
            enabled: instance.is_some(),
            source: ExtensionSource::Registry {
                download_url: ext.download_url.clone(),
            },
        });
        self.instances.push(instance);

        Ok(ext.id.clone())
    }

    /// Notify enabled extensions that a document was opened.
    pub fn notify_document_open(&mut self, uri: &str, language_id: &str, ctx: &ExtensionContext) {
        for (info, inst) in self.installed.iter().zip(self.instances.iter_mut()) {
            if info.enabled {
                if let Some(ext) = inst.as_mut() {
                    ext.on_document_open(uri, language_id, ctx);
                }
            }
        }
    }

    /// Notify enabled extensions that a document's content changed.
    pub fn notify_document_change(&mut self, uri: &str, ctx: &ExtensionContext) {
        for (info, inst) in self.installed.iter().zip(self.instances.iter_mut()) {
            if info.enabled {
                if let Some(ext) = inst.as_mut() {
                    ext.on_document_change(uri, ctx);
                }
            }
        }
    }

    /// Notify enabled extensions that a document was saved.
    pub fn notify_document_save(&mut self, uri: &str, ctx: &ExtensionContext) {
        for (info, inst) in self.installed.iter().zip(self.instances.iter_mut()) {
            if info.enabled {
                if let Some(ext) = inst.as_mut() {
                    ext.on_document_save(uri, ctx);
                }
            }
        }
    }

    /// Poll all enabled extensions for output.  Called once per frame.
    ///
    /// Outputs that require capabilities the extension hasn't declared are filtered out
    /// and logged.  To pass the filter, the extension's [`NativeExtension::capabilities`]
    /// must include the corresponding capability for the output variant.
    pub fn poll_all(&mut self, ctx: &ExtensionContext) -> Vec<ExtensionOutput> {
        let mut out = Vec::new();
        for (info, inst) in self.installed.iter().zip(self.instances.iter_mut()) {
            if info.enabled {
                if let Some(ext) = inst.as_mut() {
                    let caps = ext.capabilities();
                    let id = info.manifest.id.as_str();
                    for output in ext.poll(ctx) {
                        if is_output_allowed(&output, &caps) {
                            out.push(output);
                        } else {
                            log::info!(
                                "Capability denied for '{id}': an output variant requires \
                                 caps that the extension did not declare"
                            );
                        }
                    }
                }
            }
        }
        out
    }

    /// Collect all `PanelRegistration`s from currently enabled extensions.
    pub fn registered_panels(&self) -> Vec<PanelRegistration> {
        self.installed
            .iter()
            .zip(self.instances.iter())
            .filter(|(info, _)| info.enabled)
            .flat_map(|(_, inst)| inst.as_ref().map(|e| e.panels()).unwrap_or_default())
            .collect()
    }

    /// Collect all `RegisteredCommand`s from currently enabled extensions.
    pub fn registered_commands(&self) -> Vec<RegisteredCommand> {
        self.installed
            .iter()
            .zip(self.instances.iter())
            .filter(|(info, _)| info.enabled)
            .flat_map(|(_, inst)| inst.as_ref().map(|e| e.commands()).unwrap_or_default())
            .collect()
    }

    /// Collect all `SidebarPaneRegistration`s from enabled extensions.
    pub fn registered_sidebar_panes(&self) -> Vec<SidebarPaneRegistration> {
        self.installed
            .iter()
            .zip(self.instances.iter())
            .filter(|(info, _)| info.enabled)
            .flat_map(|(_, inst)| inst.as_ref().map(|e| e.sidebar_panes()).unwrap_or_default())
            .collect()
    }

    /// Collect all `ContextMenuContribution`s from enabled extensions.
    pub fn registered_context_menus(&self) -> Vec<ContextMenuContribution> {
        self.installed
            .iter()
            .zip(self.instances.iter())
            .filter(|(info, _)| info.enabled)
            .flat_map(|(_, inst)| inst.as_ref().map(|e| e.context_menus()).unwrap_or_default())
            .collect()
    }

    /// Notify all terminal-capable enabled extensions of terminal output.
    ///
    /// Returns the combined `ExtensionOutput` items from all extensions that
    /// have the `terminal` capability.  Outputs are also filtered through
    /// [`is_output_allowed`] for defense-in-depth.
    pub fn notify_terminal_output(
        &mut self,
        terminal_id: u32,
        data: &[u8],
    ) -> Vec<ExtensionOutput> {
        let mut out = Vec::new();
        for (info, inst) in self.installed.iter().zip(self.instances.iter_mut()) {
            if info.enabled {
                if let Some(ext) = inst.as_mut() {
                    let caps = ext.capabilities();
                    if caps.terminal {
                        for output in ext.on_terminal_output(terminal_id, data) {
                            if is_output_allowed(&output, &caps) {
                                out.push(output);
                            }
                        }
                    }
                }
            }
        }
        out
    }

    /// Set the directory where user-installed extension files live.
    ///
    /// Should be called once during startup with the platform extensions path.
    pub fn set_extensions_dir(&mut self, dir: PathBuf) {
        self.extensions_dir = Some(dir);
        // Start the hot-reload watcher for WASM extension files.
        if let Some(ref dir) = self.extensions_dir {
            match crate::hot_reload::start_hot_reload_watcher(dir) {
                Ok(rx) => self.hot_reload_rx = Some(rx),
                Err(e) => log::warn!("Extension hot-reload watcher unavailable: {e}"),
            }
        }
    }

    /// Scan the extensions directory for `.wasm` files and load any that are
    /// not already installed.
    ///
    /// Returns the ids of newly loaded extensions.
    pub fn scan_extensions_dir(&mut self) -> Vec<String> {
        let dir = match &self.extensions_dir {
            Some(d) => d.clone(),
            None => return Vec::new(),
        };

        if !dir.exists() {
            if let Err(e) = std::fs::create_dir_all(&dir) {
                log::warn!("Failed to create extensions dir {:?}: {e}", dir);
            }
            return Vec::new();
        }

        let mut loaded = Vec::new();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("Cannot read extensions dir {:?}: {e}", dir);
                return Vec::new();
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("wasm") {
                continue;
            }
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_owned();
            if self.installed.iter().any(|e| e.manifest.id == id) {
                continue;
            }

            match self.install_local(path) {
                Ok(new_id) => loaded.push(new_id),
                Err(e) => log::warn!("Failed to load extension {id}: {e}"),
            }
        }

        loaded
    }

    /// Execute a command on whichever extension registered it.
    pub fn execute_command(&mut self, command: &str, args: &[String]) -> CommandResult {
        for (info, inst) in self.installed.iter().zip(self.instances.iter_mut()) {
            if info.enabled {
                if let Some(ext) = inst.as_mut() {
                    // Simple convention: command prefix matches extension id.
                    if command.starts_with(info.manifest.id.as_str()) {
                        return ext.execute_command(command, args);
                    }
                }
            }
        }
        CommandResult::Error(format!("Unknown command: {command}"))
    }

    /// Queue a document-change notification for debounced delivery.
    ///
    /// Actual `on_document_change` calls are batched and sent only after the
    /// document is idle for `DEBOUNCE_MS` milliseconds, preventing per-keystroke
    /// extension work.
    pub fn queue_document_change(&mut self, uri: &str) {
        self.pending_doc_changes
            .insert(uri.to_owned(), std::time::Instant::now());
    }

    /// Flush any document-change notifications that have been idle ≥ 300 ms.
    ///
    /// Call once per frame from the app update loop.
    pub fn flush_pending_doc_changes(&mut self, ctx: &ExtensionContext) {
        const DEBOUNCE_MS: u128 = 300;
        let now = std::time::Instant::now();
        let ready: Vec<String> = self
            .pending_doc_changes
            .iter()
            .filter(|(_, t)| now.duration_since(**t).as_millis() >= DEBOUNCE_MS)
            .map(|(uri, _)| uri.clone())
            .collect();
        for uri in ready {
            self.pending_doc_changes.remove(&uri);
            self.notify_document_change(&uri, ctx);
        }
    }

    /// Notify enabled extensions that a document was closed.
    pub fn notify_document_close(&mut self, uri: &str, ctx: &ExtensionContext) {
        for (info, inst) in self.installed.iter().zip(self.instances.iter_mut()) {
            if info.enabled {
                if let Some(ext) = inst.as_mut() {
                    ext.on_document_close(uri, ctx);
                }
            }
        }
    }

    /// Notify enabled extensions that the cursor moved.
    pub fn notify_cursor_move(&mut self, uri: &str, line: u32, col: u32, ctx: &ExtensionContext) {
        for (info, inst) in self.installed.iter().zip(self.instances.iter_mut()) {
            if info.enabled {
                if let Some(ext) = inst.as_mut() {
                    ext.on_cursor_move(uri, line, col, ctx);
                }
            }
        }
    }

    /// Notify enabled extensions of a selection change.
    pub fn notify_selection_change(
        &mut self,
        uri: &str,
        sl: u32,
        sc: u32,
        el: u32,
        ec: u32,
        ctx: &ExtensionContext,
    ) {
        for (info, inst) in self.installed.iter().zip(self.instances.iter_mut()) {
            if info.enabled {
                if let Some(ext) = inst.as_mut() {
                    ext.on_selection_change(uri, sl, sc, el, ec, ctx);
                }
            }
        }
    }

    /// Ask all enabled extensions for hover information at the given position.
    ///
    /// Returns the first non-None result.
    pub fn request_hover(
        &mut self,
        uri: &str,
        line: u32,
        col: u32,
        ctx: &ExtensionContext,
    ) -> Option<HoverResult> {
        for (info, inst) in self.installed.iter().zip(self.instances.iter_mut()) {
            if info.enabled {
                if let Some(ext) = inst.as_mut() {
                    if let Some(r) = ext.provide_hover(uri, line, col, ctx) {
                        return Some(r);
                    }
                }
            }
        }
        None
    }

    /// Ask all enabled extensions for completions at the given position.
    pub fn request_completions(
        &mut self,
        uri: &str,
        line: u32,
        col: u32,
        ctx: &ExtensionContext,
    ) -> Vec<CompletionItem> {
        let mut all = Vec::new();
        for (info, inst) in self.installed.iter().zip(self.instances.iter_mut()) {
            if info.enabled {
                if let Some(ext) = inst.as_mut() {
                    all.extend(ext.provide_completions(uri, line, col, ctx));
                }
            }
        }
        all
    }

    /// Collect gutter markers from all enabled extensions for a URI.
    ///
    /// Returns `ExtensionOutput::GutterMarkers` items ready for `apply_extension_outputs`.
    pub fn collect_gutter_markers(
        &mut self,
        uri: &str,
        ctx: &ExtensionContext,
    ) -> Vec<ExtensionOutput> {
        let mut out = Vec::new();
        for (info, inst) in self.installed.iter().zip(self.instances.iter_mut()) {
            if info.enabled {
                if let Some(ext) = inst.as_mut() {
                    let markers = ext.provide_gutter_markers(uri, ctx);
                    if !markers.is_empty() {
                        out.push(ExtensionOutput::GutterMarkers {
                            extension_id: info.manifest.id.clone(),
                            uri: uri.to_owned(),
                            markers,
                        });
                    }
                }
            }
        }
        out
    }

    /// Check for hot-reload events and reload changed WASM extensions.
    ///
    /// Call once per frame from the app update loop.
    pub fn check_hot_reload(&mut self, ctx: &ExtensionContext) {
        let Some(ref rx) = self.hot_reload_rx else {
            return;
        };
        let changed: Vec<std::path::PathBuf> = rx.try_iter().collect();
        for path in changed {
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_owned();
            if let Some(idx) = self.installed.iter().position(|e| e.manifest.id == id) {
                log::info!("Hot-reloading extension: {id}");
                // Deactivate old instance.
                if let Some(inst) = self.instances[idx].as_mut() {
                    inst.deactivate();
                }
                // Attempt to re-load (WASM only; native extensions can't hot-reload).
                #[cfg(feature = "wasm-extensions")]
                {
                    match crate::wasm_ext::WasmExtension::load(&path) {
                        Ok(mut new_inst) => {
                            new_inst.activate(ctx);
                            self.instances[idx] = Some(new_inst);
                            log::info!("Hot-reload succeeded: {id}");
                        }
                        Err(e) => {
                            log::error!("Hot-reload failed for {id}: {e}");
                            self.instances[idx] = None;
                        }
                    }
                }
                #[cfg(not(feature = "wasm-extensions"))]
                {
                    let _ = ctx;
                    self.instances[idx] = None;
                }
            }
        }
    }

    /// Reload a single extension by id (for manual reload / on-demand).
    ///
    /// Deactivates the current instance, re-instantiates from the source file,
    /// and activates the new instance.  No-op for built-in Rust extensions.
    pub fn reload_extension(&mut self, id: &str, ctx: &ExtensionContext) {
        let Some(idx) = self.installed.iter().position(|e| e.manifest.id == id) else {
            log::warn!("reload_extension: unknown id '{id}'");
            return;
        };
        let source = self.installed[idx].source.clone();
        if let Some(inst) = self.instances[idx].as_mut() {
            inst.deactivate();
        }
        match source {
            ExtensionSource::Local(path) => {
                #[cfg(feature = "wasm-extensions")]
                {
                    match crate::wasm_ext::WasmExtension::load(&path) {
                        Ok(mut new_inst) => {
                            new_inst.activate(ctx);
                            self.instances[idx] = Some(new_inst);
                            log::info!("Reloaded extension: {id}");
                        }
                        Err(e) => {
                            log::error!("Reload failed for {id}: {e}");
                            self.instances[idx] = None;
                        }
                    }
                }
                #[cfg(not(feature = "wasm-extensions"))]
                {
                    log::warn!(
                        "Cannot reload WASM extension '{id}': wasm-extensions feature not enabled"
                    );
                    let _ = path;
                    let _ = ctx;
                }
            }
            _ => {
                log::debug!("reload_extension: {id} is not a local WASM extension, skipping");
                let _ = ctx;
            }
        }
    }
}

impl Default for ExtensionHost {
    fn default() -> Self {
        Self::new()
    }
}
