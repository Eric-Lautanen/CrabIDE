//! `crabideApp` — the eframe application struct.
//!
//! Responsibilities each frame:
//! 1. Drain the background-event channel (non-blocking `try_iter`).
//! 2. Apply events to `UiState` (diagnostics, git diffs, LSP status, …).
//! 3. Call `crabide_ui::render` to draw the full UI.
//! 4. Dispatch the returned `Vec<Action>` to the action handler.
//!
//! # Editing flow
//! All text mutations go through `Workspace` so that undo/redo history,
//! observer notifications (LSP `didChange`), and the VFS remain consistent.
//! After every mutation the active tab's `lines` snapshot is re-synced from
//! the workspace so the UI always renders the current document state.

use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender};
use eframe::{egui, CreationContext};
use std::path::PathBuf;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering as AtomicOrdering;
use tokio::runtime::Handle;

use crabide_buffer::{CursorSet, Document};
use crabide_config::{Action, ConfigManager};
use crabide_core::{
    event::{
        DapEvent, EditorEvent, FileStatus, GitEvent, LspEvent, OutputCategory, StatusKind,
        TerminalEvent, VfsEvent,
    },
    traits::TextBuffer,
    types::{BufferId, DocumentUri, Language, Position, Range, TextEdit},
};
use crabide_dap::{load_launch_configs, DapClient};
use crabide_extensions::{
    ExtensionContext, ExtensionHost, ExtensionOutput, ExtensionSeverity, RegistryClient,
};
use crabide_git::GitService;
use crabide_lsp::LspServerManager;
use crabide_search::{grep_workspace, index_workspace_files};
use crabide_syntax::{grammar::grammar_registry, queries, SyntaxEngine};
use crabide_terminal::{TerminalManager, TerminalProfile};
use crabide_ui::{
    cfg_to_egui, EditorTab, FileNode, GitDecoration, LspStatus, SidebarPaneUiState,
    TerminalInstance, UiState,
};
use crabide_vfs::{LocalVfs, VfsWatcher};
use crabide_workspace::Workspace;

// ── crabideApp ────────────────────────────────────────────────────────────────

/// Top-level application struct handed to eframe.
#[allow(non_camel_case_types)]
pub struct crabideApp {
    /// Handle to the background Tokio runtime.
    rt: Handle,

    /// Receiver drained every frame.
    event_rx: Receiver<EditorEvent>,

    /// Sender kept alive so background services (git, future LSP, DAP) can
    /// be spawned at any point during the session.
    event_tx: Sender<EditorEvent>,

    /// Config manager (themes, keybindings, settings).
    config: ConfigManager,

    /// Config change notifications (hot-reload of settings, keybindings, themes).
    config_rx: crossbeam_channel::Receiver<crabide_config::ConfigEvent>,

    /// All mutable UI display state.
    ui_state: UiState,

    /// Central document manager with history and observer hooks.
    workspace: Arc<Workspace>,

    /// Editor-internal clipboard (for Copy / Cut / Paste within the editor).
    clipboard: String,

    /// Tree-sitter syntax engine for parsing and highlight-span generation.
    syntax: SyntaxEngine,

    /// Debounced VFS watcher; `None` if the OS watcher is unavailable.
    vfs_watcher: Option<VfsWatcher>,

    /// Background git service; `None` until a folder with a git repo is opened.
    git_service: Option<GitService>,

    /// Terminal manager — owns all running PTY handles.
    terminal_manager: TerminalManager,

    /// Debug adapter client; `None` until a debug session is started.
    dap_client: Option<DapClient>,

    /// Extension host — manages all native and WASM extensions.
    extension_host: ExtensionHost,

    /// Extension registry client (search / download stub via ureq).
    registry: RegistryClient,

    /// LSP server manager — owns all running language server processes.
    lsp_manager: LspServerManager,

    /// Monotonic request ID counter for LSP requests.
    lsp_request_id: Arc<AtomicU32>,

    /// Shared flag set by Ctrl+C handler; checked each frame to trigger graceful shutdown.
    shutdown_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
}

impl crabideApp {
    pub fn new(cc: &CreationContext, rt: Handle, initial_paths: Vec<PathBuf>) -> Self {
        let (event_tx, event_rx) = crossbeam_channel::bounded::<EditorEvent>(4096);

        let (config, config_rx) = ConfigManager::new(None);
        let theme = config.active_theme();
        let keybindings = crabide_config::KeybindingEngine::with_defaults();

        let ui_state = UiState::new(theme, keybindings);

        configure_fonts(&cc.egui_ctx);
        configure_egui_style(&cc.egui_ctx, &ui_state);

        let vfs = Arc::new(LocalVfs);
        let workspace = Arc::new(Workspace::new(vfs));

        // Wire the VFS watcher through the main event bus.
        // The watcher runs on notify's internal OS thread and sends VfsEvents;
        // a lightweight bridge thread wraps them as EditorEvent::Vfs for the UI.
        let (vfs_tx, vfs_rx) = crossbeam_channel::bounded::<VfsEvent>(256);
        let event_tx_vfs = event_tx.clone();
        std::thread::Builder::new()
            .name("crabide-vfs-bridge".into())
            .spawn(move || {
                for evt in vfs_rx {
                    let _ = event_tx_vfs.send(EditorEvent::Vfs(evt));
                }
            })
            .expect("failed to spawn VFS bridge thread");
        // Keep event_tx alive in the app struct so git service and future
        // background services (LSP, DAP) can receive clones at any time.

        let vfs_watcher = VfsWatcher::new(vfs_tx)
            .map_err(|e| log::warn!("VFS file watcher unavailable: {e}"))
            .ok();

        // Register tree-sitter grammars once at startup.
        register_grammars();
        let syntax = SyntaxEngine::new();

        let terminal_manager = TerminalManager::new(event_tx.clone(), rt.clone());

        // Pre-open any files passed on the command line.
        let mut extension_host = ExtensionHost::new();
        let registry = RegistryClient::new();
        let lsp_manager = LspServerManager::new(event_tx.clone());
        let lsp_request_id = Arc::new(AtomicU32::new(1));

        // Set up extension directory and scan for user-installed extensions.
        {
            let ext_dir = dirs_ext();
            extension_host.set_extensions_dir(ext_dir.clone());
            let loaded = extension_host.scan_extensions_dir();
            if !loaded.is_empty() {
                log::info!(
                    "Loaded {} user extension(s) from {:?}",
                    loaded.len(),
                    ext_dir
                );
            }
        }

        let mut app = Self {
            rt,
            event_rx,
            event_tx,
            config,
            config_rx,
            ui_state,
            workspace,
            clipboard: String::new(),
            syntax,
            vfs_watcher,
            git_service: None,
            terminal_manager,
            dap_client: None,
            extension_host,
            registry,
            lsp_manager,
            lsp_request_id,
            shutdown_flag: None,
        };
        for path in initial_paths {
            app.open_path(path);
        }
        app
    }

    // ── Git pending actions ───────────────────────────────────────────────────

    /// Drain all pending git panel actions and forward them to the git service.
    fn drain_git_pending(&mut self) {
        // Collect all pending values before touching git_service borrow.
        let stage_file = self.ui_state.git_panel.pending_stage_file.take();
        let unstage_file = self.ui_state.git_panel.pending_unstage_file.take();
        let stage_all = std::mem::replace(&mut self.ui_state.git_panel.pending_stage_all, false);
        let unstage_all =
            std::mem::replace(&mut self.ui_state.git_panel.pending_unstage_all, false);
        let do_commit = std::mem::replace(&mut self.ui_state.git_panel.pending_commit, false);
        let commit_msg = if do_commit {
            let m = self.ui_state.git_panel.commit_message.clone();
            if !m.is_empty() {
                self.ui_state.git_panel.commit_message.clear();
            }
            m
        } else {
            String::new()
        };
        let blame_req = self.ui_state.git_panel.pending_blame_request.take();
        let discard_file = self.ui_state.git_panel.pending_discard_file.take();

        let Some(svc) = &self.git_service else { return };

        if let Some(path) = stage_file {
            svc.stage_file(path);
        }
        if let Some(path) = unstage_file {
            svc.unstage_file(path);
        }
        if stage_all {
            svc.stage_all();
        }
        if unstage_all {
            svc.unstage_all();
        }
        if do_commit && !commit_msg.is_empty() {
            svc.commit(commit_msg);
        }
        if let Some(path) = blame_req {
            if let Some(uri) = DocumentUri::from_file_path(&path) {
                svc.request_blame(uri, path);
            }
        }
        if let Some(path) = discard_file {
            svc.discard_file(path);
        }
    }

    // ── Terminal pending actions ──────────────────────────────────────────────

    /// Drain all pending terminal panel actions and forward them to the manager.
    fn drain_terminal_pending(&mut self) {
        // Collect pending values.
        let new_terminal = std::mem::replace(&mut self.ui_state.terminal.pending_new, false);
        let kill_id = self.ui_state.terminal.pending_kill.take();
        let resize = self.ui_state.terminal.pending_resize.take();
        let input: Vec<(u32, Vec<u8>)> = std::mem::take(&mut self.ui_state.terminal.pending_input);

        if new_terminal {
            let cwd = self
                .workspace
                .roots()
                .into_iter()
                .next()
                .or_else(|| std::env::current_dir().ok());
            // Default size (will be immediately corrected by the panel's resize signal).
            let id = self
                .terminal_manager
                .new_terminal(80, 24, cwd, &TerminalProfile::default());
            if let Some(id) = id {
                let inst = TerminalInstance::new(id, 80, 24);
                self.ui_state.terminal.instances.push(inst);
                self.ui_state.terminal.active_idx = self.ui_state.terminal.instances.len() - 1;
                self.ui_state.terminal.has_focus = true;
                log::info!("terminal {id} spawned");
            } else {
                self.ui_state.set_status("Failed to spawn terminal");
            }
        }

        if let Some(id) = kill_id {
            self.terminal_manager.kill(id);
            self.ui_state.terminal.remove_by_id(id);
            log::info!("terminal {id} killed");
        }

        if let Some((id, cols, rows)) = resize {
            self.terminal_manager.resize(id, cols, rows);
            if let Some(inst) = self.ui_state.terminal.by_id_mut(id) {
                inst.resize(cols, rows);
            }
        }

        for (id, bytes) in input {
            self.terminal_manager.write_input(id, bytes);
        }
    }

    // ── DAP pending actions ───────────────────────────────────────────────────

    /// Drain all pending debug panel actions and forward them to the DAP client.
    fn drain_dap_pending(&mut self) {
        let pending_launch = std::mem::replace(&mut self.ui_state.dap_panel.pending_launch, false);
        let pending_continue =
            std::mem::replace(&mut self.ui_state.dap_panel.pending_continue, false);
        let pending_step_over =
            std::mem::replace(&mut self.ui_state.dap_panel.pending_step_over, false);
        let pending_step_in =
            std::mem::replace(&mut self.ui_state.dap_panel.pending_step_in, false);
        let pending_step_out =
            std::mem::replace(&mut self.ui_state.dap_panel.pending_step_out, false);
        let pending_stop = std::mem::replace(&mut self.ui_state.dap_panel.pending_stop, false);
        let pending_restart =
            std::mem::replace(&mut self.ui_state.dap_panel.pending_restart, false);
        let pending_pause = std::mem::replace(&mut self.ui_state.dap_panel.pending_pause, false);
        let pending_stack_trace =
            std::mem::replace(&mut self.ui_state.dap_panel.pending_stack_trace, false);
        let pending_expand_var = self.ui_state.dap_panel.pending_expand_var.take();
        let pending_bps: Vec<(std::path::PathBuf, Vec<u32>)> =
            std::mem::take(&mut self.ui_state.dap_panel.pending_set_breakpoints);

        // Launch: start a new debug session.
        if pending_launch {
            let config_idx = self.ui_state.dap_panel.selected_config_idx;
            if let Some(config) = self
                .ui_state
                .dap_panel
                .launch_configs
                .get(config_idx)
                .cloned()
            {
                if let Some(client) = DapClient::start(
                    &config.adapter_command,
                    &config.adapter_args,
                    self.event_tx.clone(),
                    self.rt.clone(),
                ) {
                    client.initialize();
                    client.launch(&config);
                    self.ui_state.dap_panel.session_active = true;
                    self.ui_state
                        .set_status(format!("Debugger started: {}", config.name));
                    self.dap_client = Some(client);
                } else {
                    self.ui_state.set_status("Failed to start debug adapter");
                }
            } else {
                self.ui_state.set_status("No launch configuration selected");
            }
        }

        let Some(client) = &self.dap_client else {
            return;
        };
        let thread_id = self.ui_state.dap_panel.paused_thread_id.unwrap_or(1);

        if pending_stop {
            client.stop();
        }
        if pending_restart {
            client.restart();
        }
        if pending_continue {
            client.continue_(thread_id);
        }
        if pending_step_over {
            client.step_over(thread_id);
        }
        if pending_step_in {
            client.step_in(thread_id);
        }
        if pending_step_out {
            client.step_out(thread_id);
        }
        if pending_pause {
            client.pause(thread_id);
        }
        if pending_stack_trace {
            client.request_stack_trace(thread_id);
        }
        if let Some(var_ref) = pending_expand_var {
            client.expand_variable(var_ref);
        }
        for (path, lines) in pending_bps {
            client.set_breakpoints(path, lines);
        }
    }

    // ── Extension pending actions ─────────────────────────────────────────────

    /// Drain all pending extension panel actions, run `poll_all()`, and apply outputs.
    fn drain_extension_pending(&mut self) {
        // ── Refresh installed list so the panel can render it ─────────────────
        self.ui_state.extensions_panel.installed = self.extension_host.installed().to_vec();

        // ── Sync dynamically-registered panels from extension host ────────────
        // Register any panels we haven't seen before. Never remove existing ones
        // (they may have user-set open state).
        for reg in self.extension_host.registered_panels() {
            use crabide_ui::state::ExtensionPanelUiState;
            self.ui_state
                .extension_panels
                .entry(reg.id.clone())
                .or_insert_with(|| ExtensionPanelUiState {
                    open: reg.initially_open,
                    content: Vec::new(),
                    registration: reg,
                });
        }

        // ── Sync sidebar panes from extension host ────────────────────────────
        for reg in self.extension_host.registered_sidebar_panes() {
            self.ui_state
                .sidebar_panes
                .entry(reg.id.clone())
                .or_insert_with(|| SidebarPaneUiState {
                    visible: true,
                    content: Vec::new(),
                    registration: reg,
                });
        }

        // ── Sync context menu contributions ───────────────────────────────────
        self.ui_state.registered_context_menus = self.extension_host.registered_context_menus();

        // ── Sync registered extension commands (for palette + keybindings) ────
        let cmds = self.extension_host.registered_commands();
        self.ui_state.registered_ext_commands = cmds
            .iter()
            .map(|c| (c.id.clone(), c.title.clone()))
            .collect();
        // Register default keybindings for extension commands (once per command).
        for cmd in &cmds {
            if let Some(ref kb_str) = cmd.default_keybinding {
                // Only bind if not already bound to this action.
                let action = crabide_config::Action::Custom(cmd.id.clone());
                if self
                    .ui_state
                    .keybindings
                    .chords_for_action(&action)
                    .is_empty()
                {
                    self.ui_state.keybindings.bind(kb_str, action);
                }
            }
        }

        // ── Lazy-load recommended list (first frame only) ─────────────────────
        if self.ui_state.extensions_panel.recommended.is_empty() {
            self.ui_state.extensions_panel.recommended = self.registry.recommended();
        }

        // ── Collect all pending actions from the panel state ──────────────────
        let install_local = std::mem::replace(
            &mut self.ui_state.extensions_panel.pending_install_local,
            false,
        );
        let toggle_id = self.ui_state.extensions_panel.pending_toggle.take();
        let uninstall_id = self.ui_state.extensions_panel.pending_uninstall.take();
        let search_query = self.ui_state.extensions_panel.pending_search.take();
        let install_registry = self
            .ui_state
            .extensions_panel
            .pending_install_registry
            .take();
        let execute_cmd = self
            .ui_state
            .extensions_panel
            .pending_execute_command
            .take();

        // ── Build context data from current UI state (owned, then borrowed) ───
        let active_text: Option<String> = self
            .ui_state
            .active_tab
            .and_then(|i| self.ui_state.tabs.get(i))
            .map(|t| t.lines.join("\n"));

        let active_uri: Option<String> = self
            .ui_state
            .active_tab
            .and_then(|i| self.ui_state.tabs.get(i))
            .map(|t| t.uri.to_string());

        let active_language: String = self
            .ui_state
            .active_tab
            .and_then(|i| self.ui_state.tabs.get(i))
            .map(|t| {
                let uri = t.uri.to_string();
                if uri.ends_with(".rs") {
                    "rust"
                } else if uri.ends_with(".py") {
                    "python"
                } else if uri.ends_with(".js") {
                    "javascript"
                } else if uri.ends_with(".ts") {
                    "typescript"
                } else if uri.ends_with(".md") || uri.ends_with(".markdown") {
                    "markdown"
                } else if uri.ends_with(".go") {
                    "go"
                } else if uri.ends_with(".c") || uri.ends_with(".h") {
                    "c"
                } else {
                    "text"
                }
            })
            .unwrap_or("text")
            .to_owned();

        let cursor_line: u32 = self
            .ui_state
            .active_tab
            .and_then(|i| self.ui_state.tabs.get(i))
            .map(|t| t.cursors.primary().pos().line)
            .unwrap_or(0);

        let cursor_col: u32 = self
            .ui_state
            .active_tab
            .and_then(|i| self.ui_state.tabs.get(i))
            .map(|t| t.cursors.primary().pos().character)
            .unwrap_or(0);

        let workspace_roots: Vec<PathBuf> = self.workspace.roots();

        let blame_lines: Vec<(u32, String)> = self
            .ui_state
            .active_tab
            .and_then(|i| self.ui_state.tabs.get(i))
            .and_then(|t| t.uri.as_url().to_file_path().ok())
            .and_then(|p| self.ui_state.git_panel.blame_lines.get(&p))
            .map(|lines| {
                lines
                    .iter()
                    .map(|bl| {
                        let hash = &bl.commit_hash[..8.min(bl.commit_hash.len())];
                        (bl.line, format!("{hash} {} — {}", bl.author, bl.summary))
                    })
                    .collect()
            })
            .unwrap_or_default();

        // ── Install from local file (native dialog) ───────────────────────────
        if install_local {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("WASM Extension", &["wasm"])
                .pick_file()
            {
                match self.extension_host.install_local(path) {
                    Ok(id) => self
                        .ui_state
                        .set_status(format!("Extension installed: {id}")),
                    Err(msg) => self.ui_state.set_status(format!("Install failed: {msg}")),
                }
            }
        }

        // ── Toggle enabled / disabled ─────────────────────────────────────────
        if let Some(id) = toggle_id {
            let currently_enabled = self
                .extension_host
                .installed()
                .iter()
                .find(|e| e.manifest.id == id)
                .map(|e| e.enabled)
                .unwrap_or(false);
            let ctx = ExtensionContext {
                active_text: active_text.as_deref(),
                active_uri: active_uri.as_deref(),
                active_language: &active_language,
                workspace_roots: &workspace_roots,
                blame_lines: &blame_lines,
                cursor_line,
                cursor_col,
                selection: None,
                current_theme_id: &self.ui_state.theme.id,
            };
            self.extension_host
                .set_enabled(&id, !currently_enabled, &ctx);
            if currently_enabled {
                self.ui_state
                    .extensions_panel
                    .status_bar_items
                    .shift_remove(&id);
            }
            let state = if currently_enabled {
                "disabled"
            } else {
                "enabled"
            };
            self.ui_state.set_status(format!("Extension {state}: {id}"));
        }

        // ── Uninstall ─────────────────────────────────────────────────────────
        if let Some(id) = uninstall_id {
            match self.extension_host.uninstall(&id) {
                Ok(()) => {
                    self.ui_state
                        .extensions_panel
                        .status_bar_items
                        .shift_remove(&id);
                    self.ui_state
                        .set_status(format!("Extension uninstalled: {id}"));
                }
                Err(msg) => self.ui_state.set_status(format!("Uninstall failed: {msg}")),
            }
        }

        // ── Registry search ───────────────────────────────────────────────────
        if let Some(query) = search_query {
            self.ui_state.extensions_panel.is_searching = false;
            self.ui_state.extensions_panel.search_results = self.registry.search(&query);
        }

        // ── Install from registry ─────────────────────────────────────────────
        if let Some(id) = install_registry {
            let found: Option<crabide_extensions::RegistryExtension> = self
                .ui_state
                .extensions_panel
                .search_results
                .iter()
                .chain(self.ui_state.extensions_panel.recommended.iter())
                .find(|e| e.id == id)
                .cloned();

            if let Some(ext) = found {
                match self.extension_host.install_registry(
                    ext.id.clone(),
                    ext.name.clone(),
                    ext.version.clone(),
                    ext.download_url.clone(),
                ) {
                    Ok(()) => self
                        .ui_state
                        .set_status(format!("Queued install: {}", ext.name)),
                    Err(msg) => self.ui_state.set_status(format!("Install failed: {msg}")),
                }
            }
        }

        // ── Execute command ───────────────────────────────────────────────────
        if let Some((cmd, args)) = execute_cmd {
            use crabide_extensions::CommandResult;
            if let CommandResult::Error(msg) = self.extension_host.execute_command(&cmd, &args) {
                self.ui_state.set_status(msg);
            }
        }

        // ── Poll all enabled extensions ───────────────────────────────────────
        // Build a temporary context, call poll_all (which borrows extension_host),
        // then drop the context before apply_extension_outputs borrows self again.
        let outputs = {
            let ctx = ExtensionContext {
                active_text: active_text.as_deref(),
                active_uri: active_uri.as_deref(),
                active_language: &active_language,
                workspace_roots: &workspace_roots,
                blame_lines: &blame_lines,
                cursor_line,
                cursor_col,
                selection: None,
                current_theme_id: &self.ui_state.theme.id,
            };
            self.extension_host.poll_all(&ctx)
        };
        self.apply_extension_outputs(outputs);

        // ── Flush debounced document-change notifications ─────────────────────
        {
            let ctx = ExtensionContext {
                active_text: active_text.as_deref(),
                active_uri: active_uri.as_deref(),
                active_language: &active_language,
                workspace_roots: &workspace_roots,
                blame_lines: &blame_lines,
                cursor_line,
                cursor_col,
                selection: None,
                current_theme_id: &self.ui_state.theme.id,
            };
            self.extension_host.flush_pending_doc_changes(&ctx);
            self.extension_host.check_hot_reload(&ctx);
        }
    }

    /// Apply `ExtensionOutput` items produced by `poll_all()`.
    fn apply_extension_outputs(&mut self, outputs: Vec<ExtensionOutput>) {
        use crabide_core::event::{Diagnostic, DiagnosticSeverity};
        use crabide_core::types::Range;

        for output in outputs {
            match output {
                ExtensionOutput::StatusBarText {
                    extension_id,
                    text,
                    tooltip,
                    command,
                    alignment,
                } => {
                    self.ui_state.extensions_panel.status_bar_items.insert(
                        extension_id,
                        crabide_ui::state::StatusBarItem {
                            text,
                            tooltip,
                            command,
                            alignment,
                        },
                    );
                }

                ExtensionOutput::Diagnostics {
                    extension_id: _,
                    uri,
                    items,
                } => {
                    // Convert extension diagnostics to the core Diagnostic type.
                    let diags: Vec<Diagnostic> = items
                        .into_iter()
                        .map(|d| Diagnostic {
                            range: Range {
                                start: crabide_core::types::Position::new(
                                    d.start_line,
                                    d.start_col,
                                ),
                                end: crabide_core::types::Position::new(d.end_line, d.end_col),
                            },
                            severity: match d.severity {
                                ExtensionSeverity::Error => DiagnosticSeverity::Error,
                                ExtensionSeverity::Warning => DiagnosticSeverity::Warning,
                                ExtensionSeverity::Information => DiagnosticSeverity::Information,
                                ExtensionSeverity::Hint => DiagnosticSeverity::Hint,
                            },
                            code: None,
                            source: Some(d.source),
                            message: d.message,
                            related_information: Vec::new(),
                            tags: Vec::new(),
                        })
                        .collect();

                    // Find the tab by URI and update its diagnostics.
                    if let Ok(doc_uri) = DocumentUri::parse(&uri) {
                        if let Some(tab) = tab_for_uri_mut(&mut self.ui_state, &doc_uri) {
                            // Extension diagnostics are additive with LSP diagnostics;
                            // replace only the entries with source == extension.
                            tab.diagnostics.retain(|d| {
                                d.source.as_deref()
                                    != Some(
                                        diags
                                            .first()
                                            .and_then(|d| d.source.as_deref())
                                            .unwrap_or(""),
                                    )
                            });
                            tab.diagnostics.extend(diags);
                        }
                    }
                }

                ExtensionOutput::PanelContent { panel_id, blocks } => {
                    if let Some(panel) = self.ui_state.extension_panels.get_mut(&panel_id) {
                        panel.content = blocks;
                    }
                }

                ExtensionOutput::Notification {
                    message,
                    is_error: _,
                } => {
                    self.ui_state.set_status(message);
                }

                ExtensionOutput::CycleTheme => {
                    self.ui_state.extensions_panel.pending_cycle_theme = true;
                }

                ExtensionOutput::SidebarPaneContent { pane_id, blocks } => {
                    if let Some(pane) = self.ui_state.sidebar_panes.get_mut(&pane_id) {
                        pane.content = blocks;
                    }
                }

                ExtensionOutput::WriteFile { path, content } => {
                    if let Err(e) = std::fs::write(&path, &content) {
                        self.ui_state
                            .set_status(format!("Extension WriteFile error: {e}"));
                    } else {
                        log::debug!("Extension wrote file: {}", path.display());
                        // If the file is open in a tab, reload it.
                        let uri_str = format!("file://{}", path.display());
                        if let Ok(doc_uri) = DocumentUri::parse(&uri_str) {
                            self.reload_tab_if_open(&doc_uri);
                        }
                    }
                }

                ExtensionOutput::SendToTerminal { terminal_id, data } => {
                    self.ui_state
                        .terminal
                        .pending_input
                        .push((terminal_id, data));
                }

                ExtensionOutput::OpenTerminal {
                    title: _,
                    command: _,
                } => {
                    // Open a new terminal (the title/command are hints; the PTY
                    // manager uses them when available).
                    self.ui_state.terminal.visible = true;
                    self.ui_state.terminal.pending_new = true;
                }

                ExtensionOutput::GutterMarkers {
                    extension_id: _,
                    uri,
                    markers,
                } => {
                    if let Ok(doc_uri) = DocumentUri::parse(&uri) {
                        if let Some(tab) = tab_for_uri_mut(&mut self.ui_state, &doc_uri) {
                            tab.extension_gutter_markers = markers;
                        }
                    }
                }

                ExtensionOutput::ShowPanel { panel_id } => {
                    if let Some(panel) = self.ui_state.extension_panels.get_mut(&panel_id) {
                        panel.open = true;
                    }
                }

                ExtensionOutput::HidePanel { panel_id } => {
                    if let Some(panel) = self.ui_state.extension_panels.get_mut(&panel_id) {
                        panel.open = false;
                    }
                }
            }
        }
    }

    /// Cycle to the next theme and persist the choice.
    ///
    /// Called only via `ExtensionOutput::CycleTheme` emitted by the
    /// theme-switcher extension (or any extension that emits this output).
    fn apply_theme_cycle(&mut self, ctx: &egui::Context) {
        let current_id = self.ui_state.theme.id.as_str();
        let next_id = if current_id == "crabide-dark" {
            "crabide-light"
        } else {
            "crabide-dark"
        };
        self.config.set_active_theme(next_id);
        self.ui_state.theme = self.config.active_theme();
        configure_egui_style(ctx, &self.ui_state);
        let mut settings = self.config.settings();
        settings.ui.color_theme = next_id.to_owned();
        if let Some(user_dir) = crabide_config::SettingsLoader::user_config_dir() {
            let path = user_dir.join("settings.toml");
            if let Err(e) = crabide_config::SettingsLoader::save(&settings, &path) {
                log::warn!("Failed to persist theme preference: {e}");
            }
        }
        self.ui_state.set_status(format!(
            "Theme: {}",
            if next_id == "crabide-dark" {
                "Dark"
            } else {
                "Light"
            }
        ));
    }

    fn reload_tab_if_open(&mut self, _uri: &crabide_core::types::DocumentUri) {
        // TODO: schedule a file reload for this URI.
        log::debug!("reload_tab_if_open called (stub)");
    }

    // ── Event polling ─────────────────────────────────────────────────────────

    fn poll_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            self.apply_event(event);
        }
        while let Ok(event) = self.config_rx.try_recv() {
            self.apply_config_event(event);
        }
    }

    fn apply_event(&mut self, event: EditorEvent) {
        match event {
            EditorEvent::Lsp(lsp) => self.apply_lsp_event(lsp),
            EditorEvent::Git(git) => self.apply_git_event(git),
            EditorEvent::Vfs(vfs) => self.apply_vfs_event(vfs),
            EditorEvent::Dap(dap) => self.apply_dap_event(dap),
            EditorEvent::Terminal(t) => self.apply_terminal_event(t),
            EditorEvent::Extension(ext) => self.apply_extension_event(ext),
        }
    }

    fn apply_lsp_event(&mut self, event: LspEvent) {
        use LspEvent::*;
        match event {
            ServerReady { language } => {
                log::info!("LSP ready: {language}");
                self.ui_state
                    .lsp_indicators
                    .insert(language.to_string(), LspStatus::Ready);
                self.ui_state
                    .set_status(format!("{language} language server ready"));
            }
            ServerCrashed { language, code } => {
                log::error!("LSP crashed: {language} (code: {code:?})");
                self.ui_state
                    .lsp_indicators
                    .insert(language.to_string(), LspStatus::Error);
                self.ui_state
                    .set_status(format!("[!] {language} server crashed"));
            }
            DiagnosticsPublished { uri, diagnostics } => {
                if let Some(tab) = tab_for_uri_mut(&mut self.ui_state, &uri) {
                    tab.diagnostics = diagnostics;
                }
            }
            LocationsReady {
                request_id: _,
                locations,
            } => {
                if let Some(loc) = locations.first() {
                    if let Ok(path) = loc.uri.as_url().to_file_path() {
                        let line = loc.range.start.line as usize;
                        if let Some(tab_idx) =
                            self.ui_state.tabs.iter().position(|t| t.uri == loc.uri)
                        {
                            self.ui_state.tabs[tab_idx]
                                .cursors
                                .set_single(Position::new(
                                    loc.range.start.line,
                                    loc.range.start.character,
                                ));
                            self.ui_state.pending_scroll_line = Some(line);
                            self.ui_state.active_tab = Some(tab_idx);
                        } else {
                            self.open_path(path);
                            if let Some(idx) = self.ui_state.active_tab {
                                self.ui_state.tabs[idx].cursors.set_single(Position::new(
                                    loc.range.start.line,
                                    loc.range.start.character,
                                ));
                                self.ui_state.pending_scroll_line = Some(line);
                            }
                        }
                    }
                    if locations.len() > 1 {
                        self.ui_state.set_status(format!(
                            "Found {} locations (showing first)",
                            locations.len()
                        ));
                    }
                } else {
                    self.ui_state.set_status("No locations found");
                }
            }
            HoverReady {
                request_id: _,
                contents,
                range: _,
            } => {
                if let Some(text) = contents {
                    self.ui_state.hover_text = Some(text);
                } else {
                    self.ui_state.set_status("No hover information available");
                }
            }
            CompletionReady {
                request_id: _,
                items,
                is_incomplete: _,
            } => {
                if !items.is_empty() {
                    self.ui_state.completion_items = items;
                    self.ui_state.completion_visible = true;
                }
            }
            FormattingReady {
                request_id: _,
                workspace_edit,
            } => {
                self.apply_workspace_edit(workspace_edit);
                self.ui_state.set_status("Document formatted");
            }
            RenameReady {
                request_id: _,
                workspace_edit,
            } => {
                self.apply_workspace_edit(workspace_edit);
                self.ui_state.set_status("Rename applied");
            }
            CodeActionsReady {
                request_id: _,
                actions,
            } => {
                if actions.is_empty() {
                    self.ui_state.set_status("No code actions available");
                } else {
                    self.ui_state.code_actions = actions;
                    self.ui_state.code_actions_visible = true;
                }
            }
            LogMessage {
                language: _,
                message,
            } => {
                log::debug!("LSP log: {message}");
            }
            InlayHintsUpdated { uri, hints } => {
                if let Some(tab) = tab_for_uri_mut(&mut self.ui_state, &uri) {
                    tab.inlay_hints = hints;
                }
            }
            SemanticTokensUpdated { uri, tokens } => {
                if let Some(tab) = tab_for_uri_mut(&mut self.ui_state, &uri) {
                    tab.semantic_tokens = tokens;
                }
            }
            CodeLensUpdated { uri, items } => {
                if let Some(tab) = tab_for_uri_mut(&mut self.ui_state, &uri) {
                    tab.code_lens = items;
                }
            }
        }
    }

    fn apply_git_event(&mut self, event: GitEvent) {
        use GitEvent::*;
        match event {
            HeadChanged { branch, commit } => {
                log::debug!(
                    "git HEAD: {} @ {}",
                    branch.as_deref().unwrap_or("(detached)"),
                    &commit[..8.min(commit.len())]
                );
                self.ui_state.git_branch = branch;
            }

            DiffHunksUpdated { uri, hunks } => {
                if let Some(tab) = tab_for_uri_mut(&mut self.ui_state, &uri) {
                    tab.git_hunks = hunks;
                }
            }

            StatusRefreshed { statuses } => {
                // Split into staged (index has changes) and unstaged (worktree has changes).
                let staged: Vec<FileStatus> = statuses
                    .iter()
                    .filter(|s| {
                        !matches!(
                            s.index_status,
                            StatusKind::Unmodified | StatusKind::Untracked | StatusKind::Ignored
                        )
                    })
                    .cloned()
                    .collect();
                let unstaged: Vec<FileStatus> = statuses
                    .iter()
                    .filter(|s| {
                        !matches!(
                            s.worktree_status,
                            StatusKind::Unmodified | StatusKind::Ignored
                        )
                    })
                    .cloned()
                    .collect();

                self.ui_state.git_panel.staged_files = staged;
                self.ui_state.git_panel.unstaged_files = unstaged;
                update_explorer_git_status(&mut self.ui_state, &statuses);
            }

            BlameUpdated { uri, lines } => {
                if let Ok(path) = uri.as_url().to_file_path() {
                    self.ui_state.git_panel.blame_lines.insert(path, lines);
                    // Cap blame cache to 8 entries (blame data is large — each
                    // BlameLine holds several heap-allocated Strings).
                    while self.ui_state.git_panel.blame_lines.len() > 8 {
                        self.ui_state.git_panel.blame_lines.shift_remove_index(0);
                    }
                }
            }

            OperationCompleted { operation } => {
                self.ui_state.set_status(format!("Git: {operation}"));
                // Re-query status after every successful operation.
                if let Some(svc) = &self.git_service {
                    svc.refresh();
                }
            }

            OperationFailed { operation, error } => {
                self.ui_state
                    .set_status(format!("Git error ({operation}): {error}"));
            }
        }
    }

    fn apply_vfs_event(&mut self, event: crabide_core::event::VfsEvent) {
        use crabide_core::event::VfsEvent::*;
        match event {
            FileModified(p) => log::trace!("modified: {}", p.display()),
            FileCreated(p) => log::trace!("created: {}", p.display()),
            FileDeleted(p) => log::trace!("deleted: {}", p.display()),
            FileRenamed { from, to } => {
                log::trace!("renamed: {} → {}", from.display(), to.display())
            }
            WatchError(e) => log::warn!("file watch error: {e}"),
        }
    }

    fn apply_dap_event(&mut self, event: DapEvent) {
        use DapEvent::*;
        match event {
            Initialized => {
                log::info!("DAP: session initialized");
                self.ui_state.set_status("Debugger initialized");
            }

            Stopped {
                reason,
                thread_id,
                description,
                ..
            } => {
                self.ui_state.dap_panel.paused = true;
                self.ui_state.dap_panel.paused_thread_id = thread_id;
                self.ui_state.dap_panel.stop_reason =
                    description.or_else(|| Some(format!("{reason:?}")));
                // Fetch the call stack so the debug panel and gutter update immediately.
                self.ui_state.dap_panel.pending_stack_trace = true;
                self.ui_state.set_status(format!("Paused: {reason:?}"));
            }

            Continued { .. } => {
                self.ui_state.dap_panel.paused = false;
                self.ui_state.dap_panel.paused_thread_id = None;
                self.ui_state.dap_panel.stop_reason = None;
                self.ui_state.dap_panel.call_stack.clear();
                self.ui_state.dap_panel.variables.clear();
                self.ui_state.set_status("Running");
            }

            Terminated => {
                self.dap_client = None;
                self.ui_state.dap_panel.session_active = false;
                self.ui_state.dap_panel.paused = false;
                self.ui_state.dap_panel.paused_thread_id = None;
                self.ui_state.dap_panel.call_stack.clear();
                self.ui_state.dap_panel.variables.clear();
                self.ui_state.dap_panel.breakpoint_states.clear();
                self.ui_state.set_status("Debug session ended");
            }

            BreakpointUpdated { breakpoint } => {
                // Update in place if we already have a state for this id, else append.
                if let Some(existing) = self
                    .ui_state
                    .dap_panel
                    .breakpoint_states
                    .iter_mut()
                    .find(|b| b.id.is_some() && b.id == breakpoint.id)
                {
                    *existing = breakpoint;
                } else {
                    self.ui_state.dap_panel.breakpoint_states.push(breakpoint);
                }
            }

            StackTraceReady { frames, .. } => {
                self.ui_state.dap_panel.call_stack = frames;
                // Auto-select first frame and request variables.
                if let Some(frame) = self.ui_state.dap_panel.call_stack.first().cloned() {
                    self.ui_state.dap_panel.active_frame_id = Some(frame.id);
                    if let Some(client) = &self.dap_client {
                        client.request_variables(frame.id);
                    }
                    // Navigate editor to the paused location.
                    if let Some(path) = &frame.source_path {
                        let line = frame.line.saturating_sub(1) as usize;
                        if let Some(uri) = DocumentUri::from_file_path(path) {
                            if let Some(tab_idx) =
                                self.ui_state.tabs.iter().position(|t| t.uri == uri)
                            {
                                self.ui_state.tabs[tab_idx]
                                    .cursors
                                    .set_single(Position::new(line as u32, 0));
                                self.ui_state.pending_scroll_line = Some(line);
                                self.ui_state.active_tab = Some(tab_idx);
                            } else {
                                // Open the source file if it's not already open.
                                self.open_path(path.clone());
                                if let Some(idx) = self.ui_state.active_tab {
                                    self.ui_state.tabs[idx]
                                        .cursors
                                        .set_single(Position::new(line as u32, 0));
                                    self.ui_state.pending_scroll_line = Some(line);
                                }
                            }
                        }
                    }
                }
            }

            VariablesReady {
                request_id,
                variables,
            } => {
                self.ui_state
                    .dap_panel
                    .variables
                    .insert(request_id, variables);
            }

            Output { category, output } => {
                self.ui_state.dap_panel.append_console(category, output);
            }

            Error { message } => {
                self.ui_state
                    .dap_panel
                    .append_console(OutputCategory::Stderr, format!("[DAP error] {message}"));
                self.ui_state
                    .set_status(format!("Debugger error: {message}"));
            }
        }
    }

    fn apply_extension_event(&mut self, event: crabide_core::event::ExtensionEvent) {
        use crabide_core::event::ExtensionEvent::*;
        match event {
            Loaded(id) => {
                log::info!("extension loaded: {id}");
                self.ui_state.set_status(format!("Extension loaded: {id}"));
            }
            LoadFailed { id, error } => {
                log::error!("extension failed: {id}: {error}");
                self.ui_state
                    .set_status(format!("[!] Extension load failed: {id}"));
            }
            Crashed { id, error } => {
                log::error!("extension crashed: {id}: {error}");
                self.ui_state
                    .set_status(format!("[!] Extension crashed: {id}"));
            }
            StatusBarUpdated { id, text, tooltip } => {
                self.ui_state.extensions_panel.status_bar_items.insert(
                    id.to_string(),
                    crabide_ui::state::StatusBarItem {
                        text,
                        tooltip,
                        command: None,
                        alignment: crabide_extensions::StatusBarAlignment::Left,
                    },
                );
            }
            DiagnosticsPublished {
                id: _,
                uri,
                diagnostics,
            } => {
                if let Some(tab) = tab_for_uri_mut(&mut self.ui_state, &uri) {
                    tab.diagnostics = diagnostics;
                }
            }
            CommandRegistered { id, command } => {
                log::debug!("extension {id} registered command: {command}");
            }
        }
    }

    fn apply_terminal_event(&mut self, event: TerminalEvent) {
        use TerminalEvent::*;
        match event {
            Output { terminal_id, delta } => {
                if let Some(inst) = self.ui_state.terminal.by_id_mut(terminal_id) {
                    inst.apply_delta(&delta);
                }
            }
            TitleChanged { terminal_id, title } => {
                if let Some(inst) = self.ui_state.terminal.by_id_mut(terminal_id) {
                    inst.title = title;
                }
            }
            CwdChanged { terminal_id, cwd } => {
                if let Some(inst) = self.ui_state.terminal.by_id_mut(terminal_id) {
                    inst.cwd = Some(cwd);
                }
            }
            Exited { terminal_id, .. } => {
                if let Some(inst) = self.ui_state.terminal.by_id_mut(terminal_id) {
                    inst.exited = true;
                    inst.title.push_str(" [exited]");
                }
                self.terminal_manager.kill(terminal_id);
                log::info!("terminal {terminal_id} exited");
            }
            CommandStarted { .. } | CommandFinished { .. } | LinkDetected { .. } => {}
        }
    }

    fn apply_config_event(&mut self, event: crabide_config::ConfigEvent) {
        use crabide_config::ConfigEvent::*;
        match event {
            SettingsChanged => {
                log::debug!("settings reloaded");
            }
            KeybindingsChanged => {
                self.ui_state.keybindings = crabide_config::KeybindingEngine::with_defaults();
                log::info!("keybindings reloaded");
            }
            ThemeChanged { theme_id } => {
                self.ui_state.theme = self.config.active_theme();
                log::info!("theme changed to {theme_id}");
                // ThemeChanged may arrive from the file watcher (user edited a
                // theme JSON).  ctx is not available in apply_config_event so
                // we can't call configure_egui_style here; the style will be
                // re-applied on the next theme cycle or app restart.
            }
        }
    }

    // ── Action dispatcher ─────────────────────────────────────────────────────

    fn dispatch_actions(&mut self, actions: Vec<Action>, ctx: &egui::Context) {
        for action in actions {
            self.handle_action(action, ctx);
        }
    }

    fn handle_action(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            // ── Lifecycle ─────────────────────────────────────────────────────
            Action::Quit => std::process::exit(0),

            // ── File operations ───────────────────────────────────────────────
            Action::NewFile => {
                let id = self.workspace.new_untitled(None);
                let uri = self
                    .workspace
                    .uri(id)
                    .unwrap_or_else(|| DocumentUri::parse("untitled://Untitled").unwrap());
                let title = uri
                    .to_string()
                    .rsplit("//")
                    .next()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Untitled")
                    .to_owned();
                let lines = self
                    .workspace
                    .get_lines(id)
                    .unwrap_or_else(|_| vec![String::new()]);
                let mut tab = EditorTab::new(id, title, uri, Language::PLAIN_TEXT);
                tab.lines = if lines.is_empty() {
                    vec![String::new()]
                } else {
                    lines
                };
                self.ui_state.open_tab(tab);
                // Nothing to parse for a blank untitled document, but still
                // call update_highlights so the cache entry exists.
                if let Some(idx) = self.ui_state.active_tab {
                    self.update_highlights(idx);
                }
            }

            Action::OpenFile => {
                if let Some(path) = self.ui_state.pending_open_path.take() {
                    if path.is_file() {
                        self.open_path(path);
                    } else if path.is_dir() {
                        populate_explorer_children(&mut self.ui_state, path);
                    }
                } else if let Some(path) = rfd::FileDialog::new().pick_file() {
                    self.open_path(path);
                }
            }

            Action::SaveFile => {
                if let Some(idx) = self.ui_state.active_tab {
                    if let Some(tab) = self.ui_state.tabs.get(idx) {
                        let id = tab.buffer_id;
                        let uri = tab.uri.clone();
                        // Collect extension notification data while `tab` is live.
                        let uri_str = tab.uri.to_string();
                        let lang_id = language_id_from_uri(&uri_str);
                        let text = tab.lines.join("\n");
                        let cursor_line = tab.cursors.primary().pos().line;
                        let cursor_col = tab.cursors.primary().pos().character;
                        let ws = Arc::clone(&self.workspace);
                        self.rt.spawn(async move {
                            if let Err(e) = ws.save(id).await {
                                log::error!("save {uri}: {e}");
                            }
                        });
                        self.ui_state.tabs[idx].is_dirty = false;
                        self.ui_state.set_status("File saved");

                        // Refresh git diff hunks for the saved file.
                        if let Some(svc) = &self.git_service {
                            let tab = &self.ui_state.tabs[idx];
                            if let Ok(path) = tab.uri.as_url().to_file_path() {
                                svc.request_diff_hunks(tab.uri.clone(), path);
                            }
                            svc.refresh();
                        }

                        // Notify extensions of the saved document.
                        let roots = self.workspace.roots();
                        let ext_ctx = ExtensionContext {
                            active_text: Some(&text),
                            active_uri: Some(&uri_str),
                            active_language: lang_id,
                            workspace_roots: &roots,
                            blame_lines: &[],
                            cursor_line,
                            cursor_col,
                            selection: None,
                            current_theme_id: &self.ui_state.theme.id,
                        };
                        self.extension_host.notify_document_save(&uri_str, &ext_ctx);
                    }
                }
            }

            Action::SaveAll => {
                // Collect per-tab save data before marking tabs dirty=false.
                let save_docs: Vec<(String, &'static str, String, u32, u32)> = self
                    .ui_state
                    .tabs
                    .iter()
                    .map(|t| {
                        let uri_str = t.uri.to_string();
                        let lang_id = language_id_from_uri(&uri_str);
                        let text = t.lines.join("\n");
                        let cursor_line = t.cursors.primary().pos().line;
                        let cursor_col = t.cursors.primary().pos().character;
                        (uri_str, lang_id, text, cursor_line, cursor_col)
                    })
                    .collect();
                let ids: Vec<BufferId> = self.ui_state.tabs.iter().map(|t| t.buffer_id).collect();
                let ws = Arc::clone(&self.workspace);
                self.rt.spawn(async move {
                    for id in ids {
                        if let Err(e) = ws.save(id).await {
                            log::error!("save-all {id}: {e}");
                        }
                    }
                });
                for tab in &mut self.ui_state.tabs {
                    tab.is_dirty = false;
                }
                self.ui_state.set_status("All files saved");

                // Notify extensions of each saved document.
                let roots = self.workspace.roots();
                for (uri_str, lang_id, text, cursor_line, cursor_col) in &save_docs {
                    let ext_ctx = ExtensionContext {
                        active_text: Some(text.as_str()),
                        active_uri: Some(uri_str.as_str()),
                        active_language: lang_id,
                        workspace_roots: &roots,
                        blame_lines: &[],
                        cursor_line: *cursor_line,
                        cursor_col: *cursor_col,
                        selection: None,
                        current_theme_id: &self.ui_state.theme.id,
                    };
                    self.extension_host.notify_document_save(uri_str, &ext_ctx);
                }
            }

            Action::OpenFolder => {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.workspace.add_root(folder.clone());
                    if let Some(watcher) = &mut self.vfs_watcher {
                        if let Err(e) = watcher.watch(&folder, true) {
                            log::warn!("Cannot watch {}: {e}", folder.display());
                        }
                    }
                    let root_node = build_file_node(folder.clone());
                    self.ui_state.file_explorer.roots.push(root_node);

                    // Start git service only when the user has enabled git AND
                    // we don't already have a running service.
                    if self.ui_state.git_enabled && self.git_service.is_none() {
                        if let Some(svc) = GitService::start(folder.clone(), self.event_tx.clone())
                        {
                            svc.refresh();
                            self.git_service = Some(svc);
                        }
                    }

                    // Pre-load DAP launch configurations from the opened folder.
                    if self.ui_state.dap_panel.enabled {
                        self.ui_state.dap_panel.launch_configs = load_launch_configs(&folder);
                    }

                    self.ui_state.set_status("Folder opened");
                }
            }

            Action::SaveFileAs => {
                self.ui_state.set_status("Save As — not yet implemented");
            }

            Action::CloseTab => {
                if let Some(id) = self.ui_state.pending_close_buffer.take() {
                    let _ = self.workspace.close(id, false);
                    self.syntax.close_document(id);
                    log::debug!("tab closed: {id}");
                }
            }

            // ── Editing ───────────────────────────────────────────────────────
            Action::InsertText(text) => {
                self.handle_insert_text(text, ctx);
            }

            Action::DeleteCharLeft => {
                self.handle_delete(DeleteKind::CharLeft);
            }
            Action::DeleteCharRight => {
                self.handle_delete(DeleteKind::CharRight);
            }
            Action::DeleteWordLeft => {
                self.handle_delete(DeleteKind::WordLeft);
            }
            Action::DeleteWordRight => {
                self.handle_delete(DeleteKind::WordRight);
            }
            Action::DeleteLineLeft => {
                self.handle_delete(DeleteKind::LineLeft);
            }
            Action::DeleteLineRight => {
                self.handle_delete(DeleteKind::LineRight);
            }

            // ── Undo / redo ───────────────────────────────────────────────────
            Action::Undo => {
                if let Some(idx) = self.ui_state.active_tab {
                    let id = self.ui_state.tabs[idx].buffer_id;
                    match self.workspace.undo(id) {
                        Ok(true) => {
                            self.sync_tab_from_workspace(idx);
                            self.ui_state.set_status("Undo");
                        }
                        Ok(false) => self.ui_state.set_status("Nothing to undo"),
                        Err(e) => log::error!("undo: {e}"),
                    }
                }
            }
            Action::Redo => {
                if let Some(idx) = self.ui_state.active_tab {
                    let id = self.ui_state.tabs[idx].buffer_id;
                    match self.workspace.redo(id) {
                        Ok(true) => {
                            self.sync_tab_from_workspace(idx);
                            self.ui_state.set_status("Redo");
                        }
                        Ok(false) => self.ui_state.set_status("Nothing to redo"),
                        Err(e) => log::error!("redo: {e}"),
                    }
                }
            }

            // ── Copy / Cut / Paste ────────────────────────────────────────────
            Action::Copy => {
                if let Some(idx) = self.ui_state.active_tab {
                    let maybe_text = selected_text(&self.ui_state.tabs[idx]);
                    if let Some(text) = maybe_text {
                        self.clipboard = text.clone();
                        ctx.copy_text(self.clipboard.clone());
                    }
                }
            }
            Action::Cut => {
                if let Some(idx) = self.ui_state.active_tab {
                    // Capture text while holding the immutable borrow, then drop it.
                    let maybe_text = selected_text(&self.ui_state.tabs[idx]);
                    if let Some(text) = maybe_text {
                        self.clipboard = text.clone();
                        ctx.copy_text(self.clipboard.clone());
                    }
                    // Delete the selection (CharLeft with selection active deletes it).
                    self.handle_delete(DeleteKind::CharLeft);
                }
            }
            Action::Paste => {
                if !self.clipboard.is_empty() {
                    let text = self.clipboard.clone();
                    self.handle_insert_text(text, ctx);
                }
            }

            // ── Line operations ───────────────────────────────────────────────
            Action::DuplicateLine => self.handle_duplicate_line(),
            Action::DeleteLine => self.handle_delete_line(),
            Action::MoveLineUp => self.handle_move_line(-1),
            Action::MoveLineDown => self.handle_move_line(1),

            Action::InsertNewlineAbove => {
                self.handle_insert_newline_beside(false);
            }
            Action::InsertNewlineBelow => {
                self.handle_insert_newline_beside(true);
            }

            // ── Indentation ───────────────────────────────────────────────────
            Action::IndentLine => {
                self.handle_indent(true);
            }
            Action::OutdentLine => {
                self.handle_indent(false);
            }

            // ── Comments ──────────────────────────────────────────────────────
            Action::ToggleLineComment => {
                self.handle_toggle_line_comment();
            }

            // ── Find / replace actions forwarded from UI ───────────────────────
            Action::FindNext | Action::FindPrevious => {
                // UI already navigated the match list; here we scroll to current match.
                self.scroll_to_current_match();
            }
            Action::ReplaceInFiles => {
                self.handle_replace_all();
            }
            Action::FindReplace => {
                self.handle_find_replace_current();
            }

            // ── Close all tabs ────────────────────────────────────────────────
            Action::CloseAllTabs => {
                let ids: Vec<BufferId> = self.ui_state.tabs.iter().map(|t| t.buffer_id).collect();
                for id in ids {
                    let _ = self.workspace.close(id, false);
                    self.syntax.close_document(id);
                }
                self.ui_state.tabs.clear();
                self.ui_state.active_tab = None;
            }

            // ── Fuzzy file finder ─────────────────────────────────────────────
            Action::FuzzyFindFile => {
                // Populate the file index from workspace roots (move, no clone).
                let roots = self.workspace.roots();
                let files = index_workspace_files(&roots);
                let is_empty = files.is_empty();
                self.ui_state.fuzzy_finder.finder.update_index(files);
                if is_empty {
                    self.ui_state
                        .set_status("Open a folder first (Ctrl+K Ctrl+O)");
                }
            }

            // ── Find in files (workspace grep) ────────────────────────────────
            Action::FindInFiles => {
                let query = self.ui_state.workspace_search.query.clone();
                if query.is_empty() {
                    self.ui_state.workspace_search.results.clear();
                    return;
                }
                let roots = self.workspace.roots();
                let re = self.ui_state.workspace_search.use_regex;
                let cs = self.ui_state.workspace_search.case_sensitive;
                self.ui_state.workspace_search.is_searching = true;
                let results = grep_workspace(&roots, &query, re, cs, 2000);
                self.ui_state.workspace_search.results = results;
                self.ui_state.workspace_search.is_searching = false;
                self.ui_state.workspace_search.selected_idx = 0;
                let count = self.ui_state.workspace_search.results.len();
                self.ui_state.set_status(format!(
                    "{count} result{} for \"{query}\"",
                    if count == 1 { "" } else { "s" }
                ));
            }

            // ── Go to line ────────────────────────────────────────────────────
            Action::GotoLine => {
                if let Some(idx) = self.ui_state.active_tab {
                    let max_lines = self.ui_state.tabs[idx].lines.len();
                    if let Some(line) = self.ui_state.goto_line.target_line(max_lines) {
                        let col = self.ui_state.tabs[idx]
                            .lines
                            .get(line)
                            .map(|_| 0u32)
                            .unwrap_or(0);
                        self.ui_state.tabs[idx]
                            .cursors
                            .set_single(Position::new(line as u32, col));
                        self.ui_state.pending_scroll_line = Some(line);
                        self.ui_state.set_status(format!("Line {}", line + 1));
                    } else {
                        self.ui_state.set_status("Invalid line number");
                    }
                }
            }

            // ── Go to symbol (Ctrl+Shift+O) ───────────────────────────────────
            Action::GotoSymbol => {
                // Symbols from syntax outline — open command palette pre-filtered
                // with "@" prefix (VS Code convention); for Phase 7 we open the
                // standard command palette as a placeholder.
                self.ui_state.command_palette.visible = true;
                self.ui_state
                    .set_status("Go to Symbol — use command palette for now");
            }

            // ── Add next occurrence (Ctrl+D) ──────────────────────────────────
            Action::AddNextOccurrence => {
                self.handle_add_next_occurrence();
            }

            // ── Select all occurrences (Ctrl+Shift+L) ─────────────────────────
            Action::SelectAllOccurrences => {
                self.handle_select_all_occurrences();
            }

            // ── Trim trailing whitespace ──────────────────────────────────────
            Action::TrimTrailingWhitespace => {
                self.handle_trim_trailing_whitespace();
            }

            // ── Toggle block comment ──────────────────────────────────────────
            Action::ToggleBlockComment => {
                self.handle_toggle_block_comment();
            }

            // ── Navigation history (go back / forward) ────────────────────────
            // Not yet tracked; silently no-op so the keybinding doesn't log noise.
            Action::GoBack | Action::GoForward => {}

            // ── Next / previous diagnostic ────────────────────────────────────
            Action::NextDiagnostic => self.jump_to_diagnostic(true),
            Action::PreviousDiagnostic => self.jump_to_diagnostic(false),

            // ── Split editor (stub) ───────────────────────────────────────────
            Action::SplitEditorRight | Action::SplitEditorDown | Action::CloseEditor => {
                self.ui_state.set_status("Split editor — Phase 5 layout");
            }

            // ── LSP navigation ────────────────────────────────────────────────
            Action::GotoDefinition => {
                self.lsp_goto("textDocument/definition");
            }
            Action::GotoReferences => {
                self.lsp_goto("textDocument/references");
            }
            Action::GotoImplementation => {
                self.lsp_goto("textDocument/implementation");
            }
            Action::GotoDeclaration => {
                self.lsp_goto("textDocument/declaration");
            }
            Action::GotoTypeDefinition => {
                self.lsp_goto("textDocument/typeDefinition");
            }

            Action::FormatDocument => {
                self.lsp_format(false);
            }
            Action::FormatSelection => {
                self.lsp_format(true);
            }
            Action::OrganizeImports => {
                self.ui_state
                    .set_status("Organize imports — not yet supported by LSP server");
            }

            Action::RenameSymbol => {
                self.lsp_rename();
            }
            Action::ShowHover => {
                self.lsp_hover();
            }
            Action::TriggerCompletion => {
                self.lsp_complete();
            }
            Action::ShowSignatureHelp => {
                self.ui_state
                    .set_status("Signature help — not yet implemented");
            }
            Action::ApplyCodeAction => {
                self.lsp_code_actions();
            }

            // ── Debug ─────────────────────────────────────────────────────────
            Action::ToggleDebug => {
                if self.ui_state.dap_panel.enabled {
                    // Disable: gracefully stop any active session, reset all state.
                    if let Some(client) = &self.dap_client {
                        client.stop();
                    }
                    self.dap_client = None;
                    self.ui_state.dap_panel.enabled = false;
                    self.ui_state.dap_panel.visible = false;
                    self.ui_state.dap_panel.session_active = false;
                    self.ui_state.dap_panel.paused = false;
                    self.ui_state.dap_panel.call_stack.clear();
                    self.ui_state.dap_panel.variables.clear();
                    self.ui_state.dap_panel.breakpoint_states.clear();
                    self.ui_state.set_status("Debugger disabled");
                } else {
                    self.ui_state.dap_panel.enabled = true;
                    // Load launch configs from current workspace root.
                    let root = self
                        .workspace
                        .roots()
                        .into_iter()
                        .next()
                        .or_else(|| std::env::current_dir().ok())
                        .unwrap_or_else(|| PathBuf::from("."));
                    self.ui_state.dap_panel.launch_configs = load_launch_configs(&root);
                    self.ui_state.set_status("Debugger enabled");
                }
            }

            Action::StartDebug => {
                if !self.ui_state.dap_panel.enabled {
                    self.ui_state
                        .set_status("Enable debugger first (Run → Enable Debugger)");
                    return;
                }
                self.ui_state.dap_panel.pending_launch = true;
                self.ui_state.dap_panel.visible = true;
            }

            Action::StopDebug => {
                if let Some(client) = &self.dap_client {
                    client.stop();
                }
                self.dap_client = None;
                self.ui_state.dap_panel.session_active = false;
                self.ui_state.dap_panel.paused = false;
                self.ui_state.dap_panel.paused_thread_id = None;
                self.ui_state.dap_panel.call_stack.clear();
                self.ui_state.dap_panel.variables.clear();
                self.ui_state.set_status("Debug session stopped");
            }

            Action::ContinueDebug => {
                if self.ui_state.dap_panel.paused {
                    self.ui_state.dap_panel.pending_continue = true;
                } else {
                    self.ui_state.dap_panel.pending_pause = true;
                }
            }

            Action::StepOver => {
                self.ui_state.dap_panel.pending_step_over = true;
            }
            Action::StepInto => {
                self.ui_state.dap_panel.pending_step_in = true;
            }
            Action::StepOut => {
                self.ui_state.dap_panel.pending_step_out = true;
            }
            Action::RestartDebug => {
                self.ui_state.dap_panel.pending_restart = true;
            }

            Action::ToggleBreakpoint => {
                if let Some(idx) = self.ui_state.active_tab {
                    let line = self.ui_state.tabs[idx].cursors.primary().pos().line;
                    let tab_path = self.ui_state.tabs[idx].uri.as_url().to_file_path().ok();
                    // Toggle the line in/out of the tab's breakpoint list.
                    let tab = &mut self.ui_state.tabs[idx];
                    if tab.breakpoints.contains(&line) {
                        tab.breakpoints.retain(|&l| l != line);
                    } else {
                        tab.breakpoints.push(line);
                    }
                    // Sync with adapter if a session is active.
                    if let Some(path) = tab_path {
                        let new_bps = self.ui_state.tabs[idx].breakpoints.clone();
                        self.ui_state
                            .dap_panel
                            .pending_set_breakpoints
                            .retain(|(p, _)| p != &path);
                        self.ui_state
                            .dap_panel
                            .pending_set_breakpoints
                            .push((path, new_bps));
                    }
                }
            }

            // ── Terminal ──────────────────────────────────────────────────────
            // ToggleTerminal is handled purely in the UI layer (handle_ui_action).
            // NewTerminal reaches here when the UI layer sets pending_new = true
            // AND returns false; drain_terminal_pending() handles the actual spawn.
            Action::ToggleTerminal => {} // already handled in handle_ui_action
            Action::NewTerminal => {
                // pending_new was already set by handle_ui_action; just ensure the
                // terminal panel is visible and the drain will handle the spawn.
                self.ui_state.terminal.visible = true;
            }
            Action::KillTerminal => {
                // pending_kill set by the UI; drain handles it.
            }

            // ── Git enable / disable ──────────────────────────────────────────
            Action::ToggleGit => {
                if self.ui_state.git_enabled {
                    // Disable: drop the service (its Drop impl sends Shutdown),
                    // then wipe all git-derived UI state.
                    self.git_service = None;
                    self.ui_state.git_enabled = false;
                    self.ui_state.git_branch = None;
                    self.ui_state.git_panel.staged_files.clear();
                    self.ui_state.git_panel.unstaged_files.clear();
                    self.ui_state.git_panel.blame_lines.clear();
                    self.ui_state.git_panel.visible = false;
                    for tab in &mut self.ui_state.tabs {
                        tab.git_hunks.clear();
                    }
                    clear_explorer_git_status(&mut self.ui_state);
                    self.ui_state.set_status("Git disabled");
                } else {
                    // Enable: try to find a repo from workspace roots, then cwd.
                    self.ui_state.git_enabled = true;
                    let start_path = self
                        .workspace
                        .roots()
                        .into_iter()
                        .next()
                        .or_else(|| std::env::current_dir().ok())
                        .unwrap_or_else(|| PathBuf::from("."));
                    if let Some(svc) = GitService::start(start_path, self.event_tx.clone()) {
                        svc.refresh();
                        self.git_service = Some(svc);
                        self.ui_state.set_status("Git enabled");
                    } else {
                        self.ui_state
                            .set_status("Git enabled — open a folder to start tracking");
                    }
                }
            }

            // ── Git ───────────────────────────────────────────────────────────
            Action::GitCommit => {
                let msg = self.ui_state.git_panel.commit_message.clone();
                if msg.is_empty() {
                    self.ui_state.set_status("Enter a commit message first");
                } else if let Some(svc) = &self.git_service {
                    svc.commit(msg);
                    self.ui_state.git_panel.commit_message.clear();
                } else {
                    self.ui_state.set_status("No git repository found");
                }
            }
            Action::GitStageAll => {
                if let Some(svc) = &self.git_service {
                    svc.stage_all();
                } else {
                    self.ui_state.set_status("No git repository found");
                }
            }
            Action::GitUnstageAll => {
                if let Some(svc) = &self.git_service {
                    svc.unstage_all();
                } else {
                    self.ui_state.set_status("No git repository found");
                }
            }
            Action::GitDiscardChanges => {
                self.ui_state
                    .set_status("Select a file in Source Control to discard");
            }

            // ── View toggles already handled in UI ────────────────────────────
            Action::TogglePanel
            | Action::ToggleGitPanel
            | Action::ToggleExtensionsPanel
            | Action::ToggleOutputPanel
            | Action::ToggleDebugPanel => {}

            // ── FuzzyFindSymbol — open command palette for now ────────────────
            Action::FuzzyFindSymbol => {
                self.ui_state.command_palette.visible = true;
            }

            // ── Extension custom commands ─────────────────────────────────────
            Action::Custom(cmd) => {
                use crabide_extensions::CommandResult;
                match self.extension_host.execute_command(&cmd, &[]) {
                    CommandResult::Ok => {}
                    CommandResult::Error(msg) => self.ui_state.set_status(msg),
                }
            }

            other => {
                log::debug!("unhandled action: {other}");
            }
        }
    }

    // ── Text insertion ────────────────────────────────────────────────────────

    fn handle_insert_text(&mut self, text: String, _ctx: &egui::Context) {
        let Some(active_idx) = self.ui_state.active_tab else {
            return;
        };
        let id = self.ui_state.tabs[active_idx].buffer_id;
        let lines = self.ui_state.tabs[active_idx].lines.clone();

        // Build one edit per cursor in reverse (highest position first)
        // so that earlier positions are not invalidated by later edits.
        struct CursorOp {
            edit: TextEdit,
            new_pos: Position,
        }

        let ops: Vec<CursorOp> = {
            let tab = &self.ui_state.tabs[active_idx];
            tab.cursors
                .all()
                .iter()
                .rev()
                .map(|cursor| {
                    if cursor.has_selection() {
                        let range = cursor.range();
                        let new_pos = advance_by_text(range.start, &text);
                        CursorOp {
                            edit: TextEdit {
                                range,
                                new_text: text.clone(),
                            },
                            new_pos,
                        }
                    } else {
                        let pos = cursor.pos();
                        let actual = if text == "\n" {
                            compute_newline_text(&lines, pos)
                        } else {
                            text.clone()
                        };
                        let new_pos = advance_by_text(pos, &actual);
                        CursorOp {
                            edit: TextEdit {
                                range: Range::new(pos, pos),
                                new_text: actual,
                            },
                            new_pos,
                        }
                    }
                })
                .collect()
        };

        if ops.is_empty() {
            return;
        }

        let (edits, new_positions): (Vec<TextEdit>, Vec<Position>) =
            ops.into_iter().map(|op| (op.edit, op.new_pos)).unzip();

        if let Err(e) = self.workspace.apply_edits(id, &edits, "type") {
            log::warn!("insert: {e}");
            return;
        }

        self.sync_tab_from_workspace(active_idx);

        // Update cursor positions. ops were in reverse cursor order (highest → lowest),
        // so new_positions[0] corresponds to the last cursor (highest index).
        {
            let tab = &mut self.ui_state.tabs[active_idx];
            let n = new_positions.len().min(tab.cursors.count());
            for i in 0..n {
                let rev_i = n - 1 - i; // map back to ascending cursor order
                tab.cursors.all_mut()[i].move_to(new_positions[rev_i]);
            }
        }

        // Auto-close bracket pairs for single-character insertions.
        if text.chars().count() == 1 {
            let ch = text.chars().next().unwrap();
            if let Some(close_ch) = bracket_close_pair(ch) {
                if self.ui_state.tabs[active_idx].cursors.count() == 1 {
                    let cursor_pos = self.ui_state.tabs[active_idx].cursors.primary().pos();
                    let close_edit = TextEdit {
                        range: Range::new(cursor_pos, cursor_pos),
                        new_text: close_ch.to_string(),
                    };
                    if self
                        .workspace
                        .apply_edit(id, close_edit, "auto-close")
                        .is_ok()
                    {
                        self.sync_tab_from_workspace(active_idx);
                        // Cursor stays between the pair (do not advance).
                    }
                }
            }
        }

        self.update_bracket_match(active_idx);
    }

    // ── Deletion ──────────────────────────────────────────────────────────────

    fn handle_delete(&mut self, kind: DeleteKind) {
        let Some(active_idx) = self.ui_state.active_tab else {
            return;
        };
        let id = self.ui_state.tabs[active_idx].buffer_id;
        let lines = self.ui_state.tabs[active_idx].lines.clone();

        struct CursorOp {
            edit: Option<TextEdit>,
            new_pos: Position,
        }

        let ops: Vec<CursorOp> = {
            let tab = &self.ui_state.tabs[active_idx];
            tab.cursors
                .all()
                .iter()
                .rev()
                .map(|cursor| {
                    if cursor.has_selection() {
                        let range = cursor.range();
                        CursorOp {
                            edit: Some(TextEdit {
                                range,
                                new_text: String::new(),
                            }),
                            new_pos: range.start,
                        }
                    } else {
                        let pos = cursor.pos();
                        match delete_range(&lines, pos, &kind) {
                            Some(range) => CursorOp {
                                edit: Some(TextEdit {
                                    range,
                                    new_text: String::new(),
                                }),
                                new_pos: range.start,
                            },
                            None => CursorOp {
                                edit: None,
                                new_pos: pos,
                            },
                        }
                    }
                })
                .collect()
        };

        let edits: Vec<TextEdit> = ops.iter().filter_map(|o| o.edit.clone()).collect();
        let new_positions: Vec<Position> = ops.iter().map(|o| o.new_pos).collect();

        if !edits.is_empty() {
            if let Err(e) = self.workspace.apply_edits(id, &edits, kind.label()) {
                log::warn!("delete: {e}");
                return;
            }
            self.sync_tab_from_workspace(active_idx);
        }

        // Update cursor positions.
        {
            let tab = &mut self.ui_state.tabs[active_idx];
            let n = new_positions.len().min(tab.cursors.count());
            for i in 0..n {
                let rev_i = n - 1 - i;
                tab.cursors.all_mut()[i].move_to(new_positions[rev_i]);
            }
        }

        self.update_bracket_match(active_idx);
    }

    // ── Line operations ───────────────────────────────────────────────────────

    fn handle_duplicate_line(&mut self) {
        let Some(active_idx) = self.ui_state.active_tab else {
            return;
        };
        let id = self.ui_state.tabs[active_idx].buffer_id;
        let lines = self.ui_state.tabs[active_idx].lines.clone();
        let line_idx = self.ui_state.tabs[active_idx].cursors.primary().pos().line as usize;

        let line_text = lines.get(line_idx).cloned().unwrap_or_default();
        let line_end_col = line_text.chars().count() as u32;
        let insert_pos = Position::new(line_idx as u32, line_end_col);
        let new_text = format!("\n{line_text}");

        let edit = TextEdit {
            range: Range::new(insert_pos, insert_pos),
            new_text,
        };
        if let Err(e) = self.workspace.apply_edit(id, edit, "duplicate-line") {
            log::warn!("duplicate-line: {e}");
            return;
        }
        self.sync_tab_from_workspace(active_idx);
        // Cursor moves to the start of the duplicated line.
        let new_pos = Position::new(line_idx as u32 + 1, 0);
        self.ui_state.tabs[active_idx].cursors.set_single(new_pos);
    }

    fn handle_delete_line(&mut self) {
        let Some(active_idx) = self.ui_state.active_tab else {
            return;
        };
        let id = self.ui_state.tabs[active_idx].buffer_id;
        let lines = self.ui_state.tabs[active_idx].lines.clone();
        let line_idx = self.ui_state.tabs[active_idx].cursors.primary().pos().line as usize;
        let n_lines = lines.len();

        // Range: from the start of this line to the start of the next line
        // (or end of file if this is the last line).
        let range = if line_idx + 1 < n_lines {
            Range::new(
                Position::new(line_idx as u32, 0),
                Position::new(line_idx as u32 + 1, 0),
            )
        } else if line_idx > 0 {
            let prev_len = lines[line_idx - 1].chars().count() as u32;
            Range::new(
                Position::new(line_idx as u32 - 1, prev_len),
                Position::new(line_idx as u32, lines[line_idx].chars().count() as u32),
            )
        } else {
            // Only line in file — clear it
            let len = lines[0].chars().count() as u32;
            Range::new(Position::new(0, 0), Position::new(0, len))
        };

        let edit = TextEdit {
            range,
            new_text: String::new(),
        };
        if let Err(e) = self.workspace.apply_edit(id, edit, "delete-line") {
            log::warn!("delete-line: {e}");
            return;
        }
        self.sync_tab_from_workspace(active_idx);
        let new_line = (line_idx as u32)
            .min(self.ui_state.tabs[active_idx].lines.len().saturating_sub(1) as u32);
        self.ui_state.tabs[active_idx]
            .cursors
            .set_single(Position::new(new_line, 0));
    }

    fn handle_move_line(&mut self, direction: i32) {
        let Some(active_idx) = self.ui_state.active_tab else {
            return;
        };
        let id = self.ui_state.tabs[active_idx].buffer_id;
        let lines = self.ui_state.tabs[active_idx].lines.clone();
        let n_lines = lines.len();
        let line_idx = self.ui_state.tabs[active_idx].cursors.primary().pos().line as usize;

        let target = if direction < 0 {
            if line_idx == 0 {
                return;
            }
            line_idx - 1
        } else {
            if line_idx + 1 >= n_lines {
                return;
            }
            line_idx + 1
        };

        let current_text = lines[line_idx].clone();
        let target_text = lines[target].clone();

        // Swap two lines via two edits applied in descending (higher-line-first) order.
        let higher_line = line_idx.max(target);
        let lower_line = line_idx.min(target);
        let higher_len = lines[higher_line].chars().count() as u32;
        let lower_len = lines[lower_line].chars().count() as u32;

        // After the swap, the line that was at `higher_line` gets `lower_text`, and vice-versa.
        let (higher_new, lower_new) = if higher_line == line_idx {
            // Moving down: current line (higher) becomes target text, target (lower) becomes current text.
            (target_text.clone(), current_text.clone())
        } else {
            // Moving up: current line (lower) becomes target text, target (higher) becomes current text.
            (current_text.clone(), target_text.clone())
        };

        let ordered_edits = [
            TextEdit {
                range: Range::new(
                    Position::new(higher_line as u32, 0),
                    Position::new(higher_line as u32, higher_len),
                ),
                new_text: higher_new,
            },
            TextEdit {
                range: Range::new(
                    Position::new(lower_line as u32, 0),
                    Position::new(lower_line as u32, lower_len),
                ),
                new_text: lower_new,
            },
        ];

        if let Err(e) = self.workspace.apply_edits(id, &ordered_edits, "move-line") {
            log::warn!("move-line: {e}");
            return;
        }
        self.sync_tab_from_workspace(active_idx);
        self.ui_state.tabs[active_idx]
            .cursors
            .set_single(Position::new(target as u32, 0));
    }

    fn handle_insert_newline_beside(&mut self, below: bool) {
        let Some(active_idx) = self.ui_state.active_tab else {
            return;
        };
        let id = self.ui_state.tabs[active_idx].buffer_id;
        let lines = self.ui_state.tabs[active_idx].lines.clone();
        let line_idx = self.ui_state.tabs[active_idx].cursors.primary().pos().line as usize;

        let (insert_pos, new_line_idx) = if below {
            let line_len = lines.get(line_idx).map(|l| l.chars().count()).unwrap_or(0) as u32;
            (
                Position::new(line_idx as u32, line_len),
                line_idx as u32 + 1,
            )
        } else {
            (Position::new(line_idx as u32, 0), line_idx as u32)
        };

        let indent = lines
            .get(line_idx)
            .map(|l| leading_whitespace(l).to_owned())
            .unwrap_or_default();
        let new_text = if below {
            format!("\n{indent}")
        } else {
            format!("{indent}\n")
        };

        let edit = TextEdit {
            range: Range::new(insert_pos, insert_pos),
            new_text,
        };
        if let Err(e) = self.workspace.apply_edit(id, edit, "newline-beside") {
            log::warn!("newline-beside: {e}");
            return;
        }
        self.sync_tab_from_workspace(active_idx);
        let col = indent.chars().count() as u32;
        self.ui_state.tabs[active_idx]
            .cursors
            .set_single(Position::new(new_line_idx, col));
    }

    fn handle_indent(&mut self, indent: bool) {
        let Some(active_idx) = self.ui_state.active_tab else {
            return;
        };
        let id = self.ui_state.tabs[active_idx].buffer_id;
        let lines = self.ui_state.tabs[active_idx].lines.clone();
        let line_idx = self.ui_state.tabs[active_idx].cursors.primary().pos().line as usize;

        let line_text = lines.get(line_idx).cloned().unwrap_or_default();
        let edit = if indent {
            TextEdit {
                range: Range::new(
                    Position::new(line_idx as u32, 0),
                    Position::new(line_idx as u32, 0),
                ),
                new_text: "    ".to_owned(),
            }
        } else {
            // Remove up to 4 leading spaces.
            let spaces: String = line_text
                .chars()
                .take(4)
                .take_while(|&c| c == ' ')
                .collect();
            let n = spaces.len() as u32;
            if n == 0 {
                return;
            }
            TextEdit {
                range: Range::new(
                    Position::new(line_idx as u32, 0),
                    Position::new(line_idx as u32, n),
                ),
                new_text: String::new(),
            }
        };

        if let Err(e) =
            self.workspace
                .apply_edit(id, edit, if indent { "indent" } else { "outdent" })
        {
            log::warn!("indent/outdent: {e}");
            return;
        }
        self.sync_tab_from_workspace(active_idx);
    }

    fn handle_toggle_line_comment(&mut self) {
        let Some(active_idx) = self.ui_state.active_tab else {
            return;
        };
        let id = self.ui_state.tabs[active_idx].buffer_id;
        let lines = self.ui_state.tabs[active_idx].lines.clone();
        let line_idx = self.ui_state.tabs[active_idx].cursors.primary().pos().line as usize;
        let language = &self.ui_state.tabs[active_idx].language;

        let prefix = line_comment_prefix(language);
        let line_text = lines.get(line_idx).cloned().unwrap_or_default();

        let (new_text, char_delta) = if line_text.trim_start().starts_with(prefix) {
            // Remove the comment prefix.
            let indent_len = leading_whitespace(&line_text).len();
            let full_prefix = format!("{}{prefix}", &line_text[..indent_len]);
            if line_text.starts_with(&full_prefix) {
                let removed = &line_text[full_prefix.len()..];
                // Remove exactly one space after prefix if present.
                let removed = removed.strip_prefix(' ').unwrap_or(removed);
                (
                    format!("{}{removed}", &line_text[..indent_len]),
                    -(prefix.len() as i32 + 1),
                )
            } else {
                return;
            }
        } else {
            let indent_len = leading_whitespace(&line_text).len();
            (
                format!(
                    "{}{prefix} {}",
                    &line_text[..indent_len],
                    &line_text[indent_len..]
                ),
                prefix.len() as i32 + 1,
            )
        };

        let line_len = line_text.chars().count() as u32;
        let edit = TextEdit {
            range: Range::new(
                Position::new(line_idx as u32, 0),
                Position::new(line_idx as u32, line_len),
            ),
            new_text,
        };
        if let Err(e) = self.workspace.apply_edit(id, edit, "toggle-comment") {
            log::warn!("toggle-comment: {e}");
            return;
        }
        self.sync_tab_from_workspace(active_idx);

        // Adjust cursor column.
        let col = self.ui_state.tabs[active_idx]
            .cursors
            .primary()
            .pos()
            .character as i32;
        let new_col = (col + char_delta).max(0) as u32;
        let line_idx_u32 = line_idx as u32;
        self.ui_state.tabs[active_idx]
            .cursors
            .primary_mut()
            .move_to(Position::new(line_idx_u32, new_col));
    }

    // ── Find/replace helpers ──────────────────────────────────────────────────

    fn scroll_to_current_match(&mut self) {
        let current = self.ui_state.find_replace.current_match();
        if let Some(range) = current {
            if let Some(idx) = self.ui_state.active_tab {
                self.ui_state.tabs[idx].cursors.set_single(range.start);
                self.ui_state.pending_scroll_line = Some(range.start.line as usize);
            }
        }
    }

    fn handle_find_replace_current(&mut self) {
        let Some(active_idx) = self.ui_state.active_tab else {
            return;
        };
        let id = self.ui_state.tabs[active_idx].buffer_id;

        let Some(range) = self.ui_state.find_replace.current_match() else {
            return;
        };
        let replacement = self.ui_state.find_replace.replacement.clone();
        let matched_text = {
            let lines = &self.ui_state.tabs[active_idx].lines;
            extract_text(lines, range)
        };
        let new_text =
            crabide_ui::panels::find_replace::apply_replacement(&matched_text, &replacement);

        let edit = TextEdit { range, new_text };
        if let Err(e) = self.workspace.apply_edit(id, edit, "replace") {
            log::warn!("replace: {e}");
            return;
        }
        self.sync_tab_from_workspace(active_idx);
        crabide_ui::panels::find_replace::recompute_matches(&mut self.ui_state);
    }

    fn handle_replace_all(&mut self) {
        let Some(active_idx) = self.ui_state.active_tab else {
            return;
        };
        let id = self.ui_state.tabs[active_idx].buffer_id;
        let replacement = self.ui_state.find_replace.replacement.clone();

        // Collect all match ranges in reverse order.
        let matches: Vec<Range> = self
            .ui_state
            .find_replace
            .match_ranges
            .iter()
            .rev()
            .cloned()
            .collect();
        let count = matches.len();
        if count == 0 {
            return;
        }

        let edits: Vec<TextEdit> = matches
            .iter()
            .map(|&range| {
                let matched = extract_text(&self.ui_state.tabs[active_idx].lines, range);
                TextEdit {
                    range,
                    new_text: crabide_ui::panels::find_replace::apply_replacement(
                        &matched,
                        &replacement,
                    ),
                }
            })
            .collect();

        if let Err(e) = self.workspace.apply_edits(id, &edits, "replace-all") {
            log::warn!("replace-all: {e}");
            return;
        }
        self.sync_tab_from_workspace(active_idx);
        crabide_ui::panels::find_replace::recompute_matches(&mut self.ui_state);
        self.ui_state
            .set_status(format!("Replaced {count} occurrences"));
    }

    // ── Add next occurrence (Ctrl+D) ──────────────────────────────────────────

    fn handle_add_next_occurrence(&mut self) {
        let Some(active_idx) = self.ui_state.active_tab else {
            return;
        };
        let tab = &self.ui_state.tabs[active_idx];

        // Get the word/text to search for.
        let search_text = if let Some(sel) = selected_text(tab) {
            if sel.is_empty() {
                return;
            }
            sel
        } else {
            word_at_cursor(tab)
        };
        if search_text.is_empty() {
            return;
        }

        // Find the next occurrence after the last cursor.
        let last_cursor = tab.cursors.all().last().cloned();
        let start_pos = last_cursor
            .as_ref()
            .map(|c| {
                if c.has_selection() {
                    // search after end of selection
                    let r = c.range();
                    Position::new(r.end.line, r.end.character)
                } else {
                    c.pos()
                }
            })
            .unwrap_or(Position::new(0, 0));

        let lines = tab.lines.clone();
        let tab_mut = &mut self.ui_state.tabs[active_idx];
        if let Some(range) = find_next_occurrence(&lines, &search_text, start_pos) {
            tab_mut.cursors.add_cursor_at_range(range);
            self.ui_state
                .set_status(format!("Added cursor at line {}", range.start.line + 1));
        } else {
            // Wrap around to beginning.
            if let Some(range) = find_next_occurrence(&lines, &search_text, Position::new(0, 0)) {
                tab_mut.cursors.add_cursor_at_range(range);
                self.ui_state.set_status(format!(
                    "Wrapped: added cursor at line {}",
                    range.start.line + 1
                ));
            }
        }
    }

    // ── Select all occurrences (Ctrl+Shift+L) ─────────────────────────────────

    fn handle_select_all_occurrences(&mut self) {
        let Some(active_idx) = self.ui_state.active_tab else {
            return;
        };
        let tab = &self.ui_state.tabs[active_idx];

        let search_text = if let Some(sel) = selected_text(tab) {
            if sel.is_empty() {
                return;
            }
            sel
        } else {
            word_at_cursor(tab)
        };
        if search_text.is_empty() {
            return;
        }

        // Collect all occurrences.
        let lines = tab.lines.clone();
        let mut ranges: Vec<Range> = Vec::new();
        let mut pos = Position::new(0, 0);
        while let Some(r) = find_next_occurrence(&lines, &search_text, pos) {
            ranges.push(r);
            pos = r.end;
            if ranges.len() > 1000 {
                break;
            } // safety cap
        }

        if ranges.is_empty() {
            return;
        }
        let count = ranges.len();
        let tab_mut = &mut self.ui_state.tabs[active_idx];
        tab_mut.cursors.set_multi_selection(&ranges);
        self.ui_state
            .set_status(format!("Selected {count} occurrences"));
    }

    // ── Trim trailing whitespace ───────────────────────────────────────────────

    fn handle_trim_trailing_whitespace(&mut self) {
        let Some(active_idx) = self.ui_state.active_tab else {
            return;
        };
        let tab = &self.ui_state.tabs[active_idx];
        let id = tab.buffer_id;

        let edits: Vec<TextEdit> = tab
            .lines
            .iter()
            .enumerate()
            .filter_map(|(li, line): (usize, &String)| {
                let trimmed = line.trim_end_matches([' ', '\t']);
                if trimmed.len() == line.len() {
                    return None;
                }
                let line_u32 = li as u32;
                Some(TextEdit {
                    range: Range::new(
                        Position::new(line_u32, trimmed.len() as u32),
                        Position::new(line_u32, line.len() as u32),
                    ),
                    new_text: String::new(),
                })
            })
            .collect();

        if edits.is_empty() {
            self.ui_state.set_status("No trailing whitespace found");
            return;
        }
        let count = edits.len();
        if let Err(e) = self.workspace.apply_edits(id, &edits, "trim-whitespace") {
            log::warn!("trim-trailing-whitespace: {e}");
            return;
        }
        self.sync_tab_from_workspace(active_idx);
        self.ui_state
            .set_status(format!("Trimmed trailing whitespace on {count} line(s)"));
    }

    // ── Toggle block comment ───────────────────────────────────────────────────

    fn handle_toggle_block_comment(&mut self) {
        let Some(active_idx) = self.ui_state.active_tab else {
            return;
        };
        let tab = &self.ui_state.tabs[active_idx];
        let id = tab.buffer_id;
        let lang = tab.language.clone();

        // Determine the range of selected lines (or just the cursor line).
        let cursor = *tab.cursors.primary();
        let (start_line, end_line) = if cursor.has_selection() {
            let r = cursor.range();
            (r.start.line as usize, r.end.line as usize)
        } else {
            let l = cursor.pos().line as usize;
            (l, l)
        };

        // Block-comment syntax per language.
        let (open_str, close_str): (&str, &str) = match lang.as_str() {
            "rust" | "c" | "cpp" | "java" | "javascript" | "typescript" | "go" | "swift"
            | "kotlin" | "csharp" | "dart" | "css" => ("/*", "*/"),
            "html" | "xml" | "svelte" => ("<!--", "-->"),
            "python" | "ruby" => ("#=", "=#"), // Ruby, Python have no block comment; use line comment fallback
            _ => ("/*", "*/"),
        };

        let lines: &Vec<String> = &tab.lines;
        // Check if first selected line already starts with open_str (toggle off).
        let first_trimmed = lines
            .get(start_line)
            .map(|l: &String| l.trim())
            .unwrap_or("");
        let toggling_off = first_trimmed.starts_with(open_str);

        let mut edits: Vec<TextEdit> = Vec::new();
        if toggling_off {
            // Remove open comment from first line and close from last line.
            if let Some(line) = lines.get(start_line) {
                let s: &str = line.as_str();
                if let Some(pos) = s.find(open_str) {
                    edits.push(TextEdit {
                        range: Range::new(
                            Position::new(start_line as u32, pos as u32),
                            Position::new(start_line as u32, (pos + open_str.len()) as u32),
                        ),
                        new_text: String::new(),
                    });
                }
            }
            if let Some(line) = lines.get(end_line) {
                let s: &str = line.as_str();
                if let Some(pos) = s.rfind(close_str) {
                    edits.push(TextEdit {
                        range: Range::new(
                            Position::new(end_line as u32, pos as u32),
                            Position::new(end_line as u32, (pos + close_str.len()) as u32),
                        ),
                        new_text: String::new(),
                    });
                }
            }
        } else {
            // Wrap: prepend open on first line, append close on last line.
            if let Some(line) = lines.get(start_line) {
                let indent = leading_whitespace(line.as_str());
                let indent_len = indent.len() as u32;
                edits.push(TextEdit {
                    range: Range::new(
                        Position::new(start_line as u32, indent_len),
                        Position::new(start_line as u32, indent_len),
                    ),
                    new_text: format!("{open_str} "),
                });
            }
            if let Some(line) = lines.get(end_line) {
                let end_col = line.as_str().len() as u32;
                edits.push(TextEdit {
                    range: Range::new(
                        Position::new(end_line as u32, end_col),
                        Position::new(end_line as u32, end_col),
                    ),
                    new_text: format!(" {close_str}"),
                });
            }
        }

        if edits.is_empty() {
            return;
        }
        if let Err(e) = self
            .workspace
            .apply_edits(id, &edits, "toggle-block-comment")
        {
            log::warn!("toggle-block-comment: {e}");
            return;
        }
        self.sync_tab_from_workspace(active_idx);
    }

    // ── Jump to next / prev diagnostic ────────────────────────────────────────

    fn jump_to_diagnostic(&mut self, next: bool) {
        let Some(active_idx) = self.ui_state.active_tab else {
            return;
        };
        let (cur_line, diag_ranges_and_msgs): (u32, Vec<(Range, String)>) = {
            let tab = &self.ui_state.tabs[active_idx];
            if tab.diagnostics.is_empty() {
                self.ui_state.set_status("No diagnostics");
                return;
            }
            let cur = tab.cursors.primary().pos().line;
            let collected = tab
                .diagnostics
                .iter()
                .map(|d| (d.range, d.message.clone()))
                .collect();
            (cur, collected)
        };
        let target = if next {
            diag_ranges_and_msgs
                .iter()
                .find(|(r, _)| r.start.line > cur_line)
                .or_else(|| diag_ranges_and_msgs.first())
        } else {
            diag_ranges_and_msgs
                .iter()
                .rev()
                .find(|(r, _)| r.start.line < cur_line)
                .or_else(|| diag_ranges_and_msgs.last())
        };
        if let Some((range, msg)) = target {
            let pos = range.start;
            self.ui_state.tabs[active_idx].cursors.move_primary_to(pos);
            self.ui_state.pending_scroll_line = Some(pos.line as usize);
            self.ui_state.set_status(format!(
                "Diagnostic on line {}: {}",
                pos.line + 1,
                msg.chars().take(60).collect::<String>()
            ));
        }
    }

    // ── Workspace sync ────────────────────────────────────────────────────────

    /// Sync the active tab's `lines` snapshot from the workspace document,
    /// then re-parse and update syntax highlight spans.
    fn sync_tab_from_workspace(&mut self, tab_idx: usize) {
        let Some(tab) = self.ui_state.tabs.get(tab_idx) else {
            return;
        };
        let id = tab.buffer_id;
        if let Ok(lines) = self.workspace.get_lines(id) {
            if let Some(tab) = self.ui_state.tabs.get_mut(tab_idx) {
                tab.lines = lines;
                tab.is_dirty = self.workspace.is_dirty(id);
                // After a buffer change (undo, redo, external edit) lines may be
                // shorter or fewer than before.  Clamp every cursor so that
                // subsequent inserts / deletes always produce valid TextEdit ranges
                // and never hit "Column N exceeds line L length M" errors.
                clamp_cursors_to_content(&mut tab.cursors, &tab.lines);
            }
        }
        self.update_highlights(tab_idx);

        // Queue a debounced document-change notification so extensions don't
        // receive one call per keystroke — they are batched after 300 ms idle.
        if let Some(uri_str) = self.ui_state.tabs.get(tab_idx).map(|t| t.uri.to_string()) {
            self.extension_host.queue_document_change(&uri_str);
        }
    }

    /// Re-parse the document in `tab_idx` and store fresh highlight spans.
    fn update_highlights(&mut self, tab_idx: usize) {
        let Some(tab) = self.ui_state.tabs.get(tab_idx) else {
            return;
        };
        let id = tab.buffer_id;
        let language = tab.language.clone();
        // Reconstruct full source from line snapshot (lines have no trailing \n).
        let source = tab.lines.join("\n");
        let version = self.syntax.version(id).unwrap_or(0).wrapping_add(1);

        self.syntax.parse_document(id, &language, &source, version);
        let spans = self.syntax.highlights(id);
        if let Some(tab) = self.ui_state.tabs.get_mut(tab_idx) {
            tab.highlight_spans = spans;
        }
    }

    /// Open a file path as a new editor tab and immediately parse it.
    fn open_path(&mut self, path: PathBuf) {
        let before = self.ui_state.tabs.len();
        open_path_as_tab(&mut self.ui_state, &self.workspace, path.clone());
        // If a new tab was added, parse it for syntax highlighting.
        if self.ui_state.tabs.len() > before {
            if let Some(idx) = self.ui_state.active_tab {
                self.update_highlights(idx);

                // Collect extension notification data before `path` is moved.
                let lang_id = language_id_from_uri(&path.to_string_lossy());
                let uri_str = self.ui_state.tabs[idx].uri.to_string();
                let text = self.ui_state.tabs[idx].lines.join("\n");

                // Request diff hunks for the newly opened file.
                if let Some(svc) = &self.git_service {
                    if let Some(uri) = DocumentUri::from_file_path(&path) {
                        svc.request_diff_hunks(uri, path);
                    }
                }

                // Notify extensions of the newly opened document.
                let roots = self.workspace.roots();
                let ext_ctx = ExtensionContext {
                    active_text: Some(&text),
                    active_uri: Some(&uri_str),
                    active_language: lang_id,
                    workspace_roots: &roots,
                    blame_lines: &[],
                    cursor_line: 0,
                    cursor_col: 0,
                    selection: None,
                    current_theme_id: &self.ui_state.theme.id,
                };
                self.extension_host
                    .notify_document_open(&uri_str, lang_id, &ext_ctx);
            }
        }
    }

    /// Update the bracket match highlight for the active tab.
    fn update_bracket_match(&mut self, tab_idx: usize) {
        let Some(tab) = self.ui_state.tabs.get_mut(tab_idx) else {
            return;
        };
        let pos = tab.cursors.primary().pos();
        tab.bracket_match = compute_bracket_match(&tab.lines, pos);
    }

    /// Set the shared shutdown flag (set by Ctrl+C handler).
    pub fn set_shutdown_flag(&mut self, flag: Arc<std::sync::atomic::AtomicBool>) {
        self.shutdown_flag = Some(flag);
    }

    // ── LSP helpers ─────────────────────────────────────────────────────────

    /// Get the language of the active tab, if any.
    fn active_language(&self) -> Option<Language> {
        self.ui_state
            .active_tab
            .and_then(|i| self.ui_state.tabs.get(i))
            .map(|t| t.language.clone())
    }

    /// Get the URI and cursor position of the active tab.
    fn active_uri_and_position(&self) -> Option<(DocumentUri, Position)> {
        self.ui_state.active_tab.and_then(|i| {
            let tab = self.ui_state.tabs.get(i)?;
            Some((tab.uri.clone(), tab.cursors.primary().pos()))
        })
    }

    /// Dispatch a go-to LSP request (definition, references, implementation, etc.).
    fn lsp_goto(&mut self, method: &'static str) {
        let Some((uri, pos)) = self.active_uri_and_position() else {
            self.ui_state.set_status("No active document");
            return;
        };
        let Some(lang) = self.active_language() else {
            self.ui_state.set_status("Unknown language");
            return;
        };
        let Some(client) = self.lsp_manager.get_client(&lang) else {
            self.ui_state
                .set_status(format!("No language server running for {lang}"));
            return;
        };
        let req_id = self.lsp_request_id.fetch_add(1, AtomicOrdering::Relaxed);
        match method {
            "textDocument/definition" => client.go_to_definition(uri, pos, req_id),
            "textDocument/references" => client.references(uri, pos, req_id),
            "textDocument/implementation" => client.implementation(uri, pos, req_id),
            "textDocument/typeDefinition" => client.type_definition(uri, pos, req_id),
            "textDocument/declaration" => client.declaration(uri, pos, req_id),
            _ => {
                self.ui_state
                    .set_status(format!("Unknown LSP method: {method}"));
                return;
            }
        }
        self.ui_state.set_status(format!("Requesting {method}…"));
    }

    /// Request document formatting from the LSP server.
    fn lsp_format(&mut self, _selection_only: bool) {
        let Some((uri, _)) = self.active_uri_and_position() else {
            self.ui_state.set_status("No active document");
            return;
        };
        let Some(lang) = self.active_language() else {
            self.ui_state.set_status("Unknown language");
            return;
        };
        let Some(client) = self.lsp_manager.get_client(&lang) else {
            self.ui_state
                .set_status(format!("No language server running for {lang}"));
            return;
        };
        let req_id = self.lsp_request_id.fetch_add(1, AtomicOrdering::Relaxed);
        let tab_size = self.config.settings().editor.tab_size;
        let insert_spaces = self.config.settings().editor.insert_spaces;
        client.format(uri, tab_size, insert_spaces, req_id);
        self.ui_state.set_status("Formatting document…");
    }

    /// Request hover information from the LSP server.
    fn lsp_hover(&mut self) {
        let Some((uri, pos)) = self.active_uri_and_position() else {
            self.ui_state.set_status("No active document");
            return;
        };
        let Some(lang) = self.active_language() else {
            self.ui_state.set_status("Unknown language");
            return;
        };
        let Some(client) = self.lsp_manager.get_client(&lang) else {
            self.ui_state
                .set_status(format!("No language server running for {lang}"));
            return;
        };
        let req_id = self.lsp_request_id.fetch_add(1, AtomicOrdering::Relaxed);
        client.hover(uri, pos, req_id);
        self.ui_state.set_status("Requesting hover…");
    }

    /// Request completion from the LSP server.
    fn lsp_complete(&self) {
        let Some((uri, pos)) = self.active_uri_and_position() else {
            return;
        };
        let Some(lang) = self.active_language() else {
            return;
        };
        let Some(client) = self.lsp_manager.get_client(&lang) else {
            return;
        };
        let req_id = self.lsp_request_id.fetch_add(1, AtomicOrdering::Relaxed);
        client.complete(uri, pos, req_id);
    }

    /// Request code actions from the LSP server.
    fn lsp_code_actions(&mut self) {
        let Some(idx) = self.ui_state.active_tab else {
            return;
        };
        let tab = &self.ui_state.tabs[idx];
        let uri = tab.uri.clone();
        let pos = tab.cursors.primary().pos();
        let lang = tab.language.clone();
        let range = if tab.cursors.primary().has_selection() {
            tab.cursors.primary().range()
        } else {
            Range::new(pos, pos)
        };
        let diagnostics = tab.diagnostics.clone();
        let Some(client) = self.lsp_manager.get_client(&lang) else {
            self.ui_state
                .set_status(format!("No language server running for {lang}"));
            return;
        };
        let req_id = self.lsp_request_id.fetch_add(1, AtomicOrdering::Relaxed);
        client.code_actions(uri, range, diagnostics, req_id);
        self.ui_state.set_status("Requesting code actions…");
    }

    /// Request rename from the LSP server.
    fn lsp_rename(&mut self) {
        let Some((uri, pos)) = self.active_uri_and_position() else {
            self.ui_state.set_status("No active document");
            return;
        };
        let Some(lang) = self.active_language() else {
            self.ui_state.set_status("Unknown language");
            return;
        };
        let Some(client) = self.lsp_manager.get_client(&lang) else {
            self.ui_state
                .set_status(format!("No language server running for {lang}"));
            return;
        };
        let new_name = self
            .ui_state
            .active_tab
            .map(|i| word_at_cursor(&self.ui_state.tabs[i]))
            .unwrap_or_default();
        if new_name.is_empty() {
            self.ui_state.set_status("No symbol under cursor");
            return;
        }
        let req_id = self.lsp_request_id.fetch_add(1, AtomicOrdering::Relaxed);
        client.rename(uri, pos, new_name, req_id);
        self.ui_state.set_status("Requesting rename…");
    }

    /// Apply a workspace edit (from formatting, rename, or code action).
    fn apply_workspace_edit(&mut self, edit: crabide_core::event::WorkspaceEdit) {
        for doc_edit in edit.document_changes {
            let Some(id) = self.workspace.get_buffer_id(&doc_edit.uri) else {
                log::warn!("apply_workspace_edit: no buffer for {}", doc_edit.uri);
                continue;
            };
            for te in &doc_edit.edits {
                if let Err(e) = self
                    .workspace
                    .apply_edit(id, te.clone(), "lsp-workspace-edit")
                {
                    log::warn!("apply_workspace_edit: {e}");
                }
            }
            self.sync_tab_after_edit(&doc_edit.uri);
        }
    }

    /// Re-sync a tab's lines snapshot after an edit was applied to the workspace.
    fn sync_tab_after_edit(&mut self, uri: &DocumentUri) {
        if let Some(tab) = tab_for_uri_mut(&mut self.ui_state, uri) {
            let id = tab.buffer_id;
            if let Ok(lines) = self.workspace.get_lines(id) {
                tab.lines = lines;
                tab.is_dirty = self.workspace.is_dirty(id);
            }
        }
    }
}

// eframe::App

impl eframe::App for crabideApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Check Ctrl+C shutdown flag.
        if let Some(ref flag) = self.shutdown_flag {
            if flag.load(std::sync::atomic::Ordering::Relaxed) {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }

        let ctx = ui.ctx().clone();
        self.poll_events();
        self.drain_git_pending();
        self.drain_terminal_pending();
        self.drain_dap_pending();
        self.drain_extension_pending();
        if self.ui_state.extensions_panel.pending_cycle_theme {
            self.ui_state.extensions_panel.pending_cycle_theme = false;
            self.apply_theme_cycle(&ctx);
        }
        let actions = crabide_ui::render(ui, &mut self.ui_state);
        self.dispatch_actions(actions, &ctx);

        // ── Extension panel navigation ────────────────────────────────────────
        if let Some(nav) = self.ui_state.pending_navigate.take() {
            use crabide_extensions::NavigateTarget;
            match nav {
                NavigateTarget::FileAt { path, line } => {
                    self.ui_state.pending_open_path = Some(path);
                    self.ui_state.pending_scroll_line = Some(line as usize);
                }
                NavigateTarget::Command(cmd) => {
                    use crabide_extensions::CommandResult;
                    if let CommandResult::Error(msg) =
                        self.extension_host.execute_command(&cmd, &[])
                    {
                        self.ui_state.set_status(msg);
                    }
                }
            }
        }

        if !self.event_rx.is_empty() {
            ctx.request_repaint();
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        log::info!("crabide exiting");
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Return a mutable reference to the tab whose URI matches `uri`.
fn tab_for_uri_mut<'a>(state: &'a mut UiState, uri: &DocumentUri) -> Option<&'a mut EditorTab> {
    state.tabs.iter_mut().find(|t| &t.uri == uri)
}

/// Update file-explorer node `git_status` fields from the latest git status list.
/// We match nodes by file name (best-effort; paths from git2 are relative to workdir).
fn update_explorer_git_status(state: &mut UiState, statuses: &[FileStatus]) {
    let map: std::collections::HashMap<&std::path::Path, GitDecoration> = statuses
        .iter()
        .filter_map(|fs| {
            let dec = file_status_decoration(fs)?;
            Some((fs.path.as_path(), dec))
        })
        .collect();

    for root in &mut state.file_explorer.roots {
        apply_decoration_to_node(root, &map);
    }
}

/// Clear all git decorations from the file explorer (called when git is disabled).
fn clear_explorer_git_status(state: &mut UiState) {
    for root in &mut state.file_explorer.roots {
        clear_decoration_on_node(root);
    }
}

fn clear_decoration_on_node(node: &mut FileNode) {
    node.git_status = None;
    for child in &mut node.children {
        clear_decoration_on_node(child);
    }
}

fn file_status_decoration(fs: &FileStatus) -> Option<GitDecoration> {
    match fs.index_status {
        StatusKind::Added => return Some(GitDecoration::Added),
        StatusKind::Modified => return Some(GitDecoration::Modified),
        StatusKind::Deleted => return Some(GitDecoration::Deleted),
        StatusKind::Renamed => return Some(GitDecoration::Modified),
        _ => {}
    }
    match fs.worktree_status {
        StatusKind::Untracked => Some(GitDecoration::Untracked),
        StatusKind::Modified => Some(GitDecoration::Modified),
        StatusKind::Deleted => Some(GitDecoration::Deleted),
        StatusKind::Conflicted => Some(GitDecoration::Conflicted),
        _ => None,
    }
}

fn apply_decoration_to_node(
    node: &mut FileNode,
    map: &std::collections::HashMap<&std::path::Path, GitDecoration>,
) {
    if !node.is_dir {
        // Try exact path match first, then just file name match.
        node.git_status = map
            .get(node.path.as_path())
            .or_else(|| {
                node.path.file_name().and_then(|name| {
                    map.iter()
                        .find(|(p, _)| p.file_name() == Some(name))
                        .map(|(_, d)| d)
                })
            })
            .copied();
    }
    for child in &mut node.children {
        apply_decoration_to_node(child, map);
    }
}

/// Open `path` as a new editor tab using the workspace for buffer management.
fn open_path_as_tab(state: &mut UiState, workspace: &Arc<Workspace>, path: PathBuf) {
    let Some(uri) = DocumentUri::from_file_path(&path) else {
        log::warn!("cannot convert to URI: {}", path.display());
        return;
    };

    // Avoid opening the same file twice.
    if let Some(idx) = state.tabs.iter().position(|t| t.uri == uri) {
        state.active_tab = Some(idx);
        return;
    }

    let title = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("(unknown)")
        .to_owned();

    let language = path
        .extension()
        .and_then(|e| e.to_str())
        .map(crabide_core::types::language_from_extension)
        .unwrap_or(Language::PLAIN_TEXT);

    let content = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) => {
            log::warn!("cannot read {}: {e}", path.display());
            Vec::new()
        }
    };

    let document = match Document::from_bytes(uri.clone(), &content) {
        Ok(doc) => doc,
        Err(e) => {
            log::error!("parse {}: {e}", path.display());
            return;
        }
    };

    let n_lines = document.line_count() as usize;
    let lines: Vec<String> = (0..n_lines)
        .filter_map(|i| document.line_str(i as u32))
        .collect();

    let buffer_id = workspace.register_document(document);
    let mut tab = EditorTab::new(buffer_id, title, uri, language);
    tab.lines = if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    };

    state.open_tab(tab);
}

/// Map a document URI string (or file path string) to an LSP language identifier.
fn language_id_from_uri(uri: &str) -> &'static str {
    if uri.ends_with(".rs") {
        "rust"
    } else if uri.ends_with(".py") {
        "python"
    } else if uri.ends_with(".js") {
        "javascript"
    } else if uri.ends_with(".ts") {
        "typescript"
    } else if uri.ends_with(".md") || uri.ends_with(".markdown") {
        "markdown"
    } else if uri.ends_with(".go") {
        "go"
    } else if uri.ends_with(".c") || uri.ends_with(".h") {
        "c"
    } else {
        "text"
    }
}

/// Build a `FileNode` for `path`, eagerly reading its immediate children.
fn build_file_node(path: PathBuf) -> crabide_ui::FileNode {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace")
        .to_owned();
    let is_dir = path.is_dir();
    let children = if is_dir {
        read_dir_children(&path)
    } else {
        Vec::new()
    };
    crabide_ui::FileNode {
        name,
        path,
        is_dir,
        children,
        expanded: is_dir, // open the root by default
        git_status: None,
    }
}

/// Synchronously read the immediate children of `dir` into `FileNode`s.
fn read_dir_children(dir: &std::path::Path) -> Vec<crabide_ui::FileNode> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut nodes: Vec<crabide_ui::FileNode> = entries
        .filter_map(|e| e.ok())
        .map(|e| {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            let is_dir = path.is_dir();
            crabide_ui::FileNode {
                name,
                path,
                is_dir,
                children: Vec::new(), // lazily loaded when the node is expanded
                expanded: false,
                git_status: None,
            }
        })
        .collect();
    // Directories first, then alphabetical (case-insensitive).
    nodes.sort_unstable_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    nodes
}

/// Populate the children of whatever `FileNode` in the explorer has this `path`.
///
/// Searches the entire tree recursively so directories at any depth can be
/// lazily loaded.
fn populate_explorer_children(state: &mut UiState, path: PathBuf) {
    let children = read_dir_children(&path);
    for root in &mut state.file_explorer.roots {
        if populate_node_children(root, &path, children.clone()) {
            return;
        }
    }
}

/// Recursively search `node` and its descendants for a dir matching `path`.
/// When found, populates its children (if empty) and returns `true`.
fn populate_node_children(node: &mut FileNode, path: &PathBuf, children: Vec<FileNode>) -> bool {
    if node.path == *path {
        if node.children.is_empty() {
            node.children = children;
        }
        return true;
    }
    for child in &mut node.children {
        if populate_node_children(child, path, children.clone()) {
            return true;
        }
    }
    false
}

/// Returns the platform-appropriate user extensions directory.
///
/// - Windows: `%APPDATA%\crabide\extensions`
/// - macOS:   `~/Library/Application Support/crabide/extensions`
/// - Linux:   `~/.config/crabide/extensions`
fn dirs_ext() -> std::path::PathBuf {
    let base = if cfg!(target_os = "windows") {
        std::env::var("APPDATA").unwrap_or_else(|_| ".".into())
    } else if cfg!(target_os = "macos") {
        std::env::var("HOME")
            .map(|h| format!("{h}/Library/Application Support"))
            .unwrap_or_else(|_| ".".into())
    } else {
        std::env::var("XDG_CONFIG_HOME")
            .or_else(|_| std::env::var("HOME").map(|h| format!("{h}/.config")))
            .unwrap_or_else(|_| ".".into())
    };
    std::path::PathBuf::from(base)
        .join("crabide")
        .join("extensions")
}

/// Apply the egui visual style from the active theme.
///
/// Explicitly initialise egui's font stack so all emoji and technical Unicode
/// symbols render correctly.
///
/// egui bundles a *subset* of NotoEmoji (~1.3 MB, ~1 200 glyphs) and uses it
/// as a fallback for Proportional and Monospace text.  That subset covers most
/// common emoji but misses some glyphs we use (e.g. U+2387 ⎇, U+2935 ⤵,
/// U+23F9 ⏹, U+1F5D1 🗑, U+1F4CB 📋, U+1F4E6 📦).
///
/// Loading the platform's system symbol font as an *additional* fallback fills
/// those gaps without downloading anything extra:
///
/// | Platform | Font loaded                          | Coverage benefit       |
/// |----------|--------------------------------------|------------------------|
/// | Windows  | Segoe UI Symbol (seguisym.ttf)       | All of Misc Technical, |
/// |          |                                      | Supplemental Arrows,   |
/// |          |                                      | many Pictographs       |
/// | macOS    | Apple Symbols                        | Similar broad coverage |
/// | Linux    | DejaVu Sans (or Noto Sans fallback)  | ~6 000 extra glyphs    |
///
/// The system font is placed *after* the built-in NotoEmoji so emoji-weight
/// glyphs in NotoEmoji take priority, and the system font only fills gaps.
fn configure_fonts(ctx: &egui::Context) {
    use egui::{FontData, FontDefinitions, FontFamily};

    // Start from egui's defaults: Ubuntu-Light → NotoEmoji-Regular → emoji-icon-font
    // for Proportional; Hack → Ubuntu-Light → NotoEmoji-Regular for Monospace.
    let mut fonts = FontDefinitions::default();

    // Load the platform system symbol font and register it as a last-resort
    // fallback for every text family.
    for (name, data) in system_symbol_fonts() {
        fonts
            .font_data
            .insert(name.clone(), FontData::from_owned(data).into());
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push(name.clone());
        }
    }

    ctx.set_fonts(fonts);
}

/// Return (font_name, raw_bytes) for the best available system Unicode-symbol
/// font on the current platform.  Returns an empty Vec if nothing is readable.
fn system_symbol_fonts() -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();

    // ── Windows ───────────────────────────────────────────────────────────────
    // Segoe UI Symbol ships with every Windows 7+ installation and covers:
    //   • Miscellaneous Technical (⎇ ⤵ ⏳ ⏸ ⏹ ▶ and hundreds more)
    //   • Supplemental Arrows A/B
    //   • Letterlike / Mathematical Operators / Geometric Shapes
    // Segoe UI Emoji (seguiemj.ttf) is a colour font that egui can't render;
    // Symbol is the correct companion for monochrome emoji/symbol fallback.
    #[cfg(target_os = "windows")]
    {
        let candidates = [
            r"C:\Windows\Fonts\seguisym.ttf", // Segoe UI Symbol (primary)
            r"C:\Windows\Fonts\segoeui.ttf",  // Segoe UI (secondary — broad Latin + some symbols)
        ];
        for path in &candidates {
            if let Ok(data) = std::fs::read(path) {
                let name = std::path::Path::new(path)
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .unwrap_or("SystemSymbol")
                    .to_string();
                out.push((name, data));
                break; // first successful read is enough
            }
        }
    }

    // ── macOS ─────────────────────────────────────────────────────────────────
    #[cfg(target_os = "macos")]
    {
        let candidates = [
            "/Library/Fonts/Apple Symbols.ttf",
            "/System/Library/Fonts/Apple Symbols.ttf",
        ];
        for path in &candidates {
            if let Ok(data) = std::fs::read(path) {
                out.push(("AppleSymbols".to_string(), data));
                break;
            }
        }
    }

    // ── Linux ─────────────────────────────────────────────────────────────────
    #[cfg(target_os = "linux")]
    {
        let candidates = [
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        ];
        for path in &candidates {
            if let Ok(data) = std::fs::read(path) {
                let name = std::path::Path::new(path)
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .unwrap_or("SystemFont")
                    .to_string();
                out.push((name, data));
                break;
            }
        }
    }

    out
}

/// Starts from egui's canonical dark or light base visuals, then overrides
/// individual colors from the active `ColorTheme` so every widget picks up
/// the correct palette automatically.
fn configure_egui_style(ctx: &egui::Context, state: &UiState) {
    use crabide_config::{Color, ThemeType};

    let t = &state.theme;
    let is_dark = matches!(t.theme_type, ThemeType::Dark | ThemeType::HighContrastDark);

    // Start from the canonical base so all widgets get sane defaults.
    let mut visuals = if is_dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    // ── Structural fills ──────────────────────────────────────────────────────
    visuals.panel_fill = cfg_to_egui(t.ui_or("editor.background", Color::rgb(0x1e, 0x1e, 0x1e)));
    // Popups, menus, dialogs all use dropdown.background so they look distinct from panels.
    visuals.window_fill = cfg_to_egui(t.ui_or("dropdown.background", Color::rgb(0x25, 0x25, 0x26)));
    visuals.faint_bg_color =
        cfg_to_egui(t.ui_or("list.hoverBackground", Color::rgb(0x2a, 0x2d, 0x2e)));
    visuals.extreme_bg_color =
        cfg_to_egui(t.ui_or("input.background", Color::rgb(0x3c, 0x3c, 0x3c)));

    // Popup/window border (panel.border gives a subtle frame around floating panels).
    let popup_border = cfg_to_egui(t.ui_or("panel.border", Color::rgb(0x45, 0x45, 0x45)));
    visuals.window_stroke = egui::Stroke::new(1.0, popup_border);

    // ── Text ──────────────────────────────────────────────────────────────────
    let text_color = cfg_to_egui(t.ui_or("editor.foreground", Color::rgb(0xd4, 0xd4, 0xd4)));
    visuals.override_text_color = Some(text_color);

    // ── Window / popup chrome ─────────────────────────────────────────────────
    visuals.window_corner_radius = egui::CornerRadius::same(4);
    visuals.window_shadow = if is_dark {
        egui::Shadow {
            offset: [0, 4],
            blur: 16,
            spread: 0,
            color: egui::Color32::from_black_alpha(80),
        }
    } else {
        egui::Shadow {
            offset: [0, 2],
            blur: 8,
            spread: 0,
            color: egui::Color32::from_black_alpha(32),
        }
    };
    visuals.popup_shadow = visuals.window_shadow;

    // ── Widgets ───────────────────────────────────────────────────────────────
    let btn_bg = cfg_to_egui(t.ui_or("button.background", Color::rgb(0x0e, 0x63, 0x9c)));
    let btn_fg = cfg_to_egui(t.ui_or("button.foreground", Color::rgb(0xff, 0xff, 0xff)));
    let btn_hover = cfg_to_egui(t.ui_or("button.hoverBackground", Color::rgb(0x11, 0x77, 0xbb)));
    let input_bg = cfg_to_egui(t.ui_or("input.background", Color::rgb(0x3c, 0x3c, 0x3c)));
    let input_border = cfg_to_egui(t.ui_or("input.border", Color::rgb(0x3c, 0x3c, 0x3c)));
    let selection_bg = cfg_to_egui(t.ui_or(
        "list.activeSelectionBackground",
        Color::rgb(0x09, 0x47, 0x71),
    ));
    let hover_bg = cfg_to_egui(t.ui_or("list.hoverBackground", Color::rgb(0x2a, 0x2d, 0x2e)));

    // Inactive (default resting state)
    visuals.widgets.inactive.weak_bg_fill = input_bg;
    visuals.widgets.inactive.bg_fill = input_bg;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, input_border);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, text_color);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(3);

    // Hovered
    visuals.widgets.hovered.weak_bg_fill = hover_bg;
    visuals.widgets.hovered.bg_fill = hover_bg;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, btn_bg);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, btn_fg);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(3);

    // Active (pressed)
    visuals.widgets.active.weak_bg_fill = btn_hover;
    visuals.widgets.active.bg_fill = btn_hover;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, btn_hover);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5, btn_fg);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(3);

    // Open (combo boxes / dropdowns in open state)
    visuals.widgets.open.weak_bg_fill = btn_bg;
    visuals.widgets.open.bg_fill = btn_bg;
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0, btn_fg);
    visuals.widgets.open.corner_radius = egui::CornerRadius::same(3);

    // Non-interactive (labels, etc.)
    visuals.widgets.noninteractive.weak_bg_fill = visuals.panel_fill;
    visuals.widgets.noninteractive.bg_fill = visuals.panel_fill;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, text_color);
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(3);

    // ── Selection highlight ───────────────────────────────────────────────────
    visuals.selection.bg_fill = selection_bg;
    visuals.selection.stroke = egui::Stroke::NONE;

    // ── Hyperlinks ────────────────────────────────────────────────────────────
    visuals.hyperlink_color = btn_hover;

    // NOTE: Do NOT override widget bg_fill values again after this point.
    // The scrollbar in egui 0.33 uses `extreme_bg_color` for the track and
    // the inherited widget styles above for the handle — no separate assignment
    // needed. Overwriting bg_fill here with scrollbar colors would undo the
    // carefully chosen button/input fill colours set above.

    // ── Apply ─────────────────────────────────────────────────────────────────
    let mut style = (*ctx.global_style()).clone();
    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 3.0);
    style.spacing.window_margin = egui::Margin::same(8);
    ctx.set_global_style(style);
}

// ── Edit helpers ──────────────────────────────────────────────────────────────

/// Advance `pos` by the characters in `text` (handles newlines).
fn advance_by_text(pos: Position, text: &str) -> Position {
    let newlines = text.chars().filter(|&c| c == '\n').count();
    if newlines == 0 {
        Position::new(pos.line, pos.character + text.chars().count() as u32)
    } else {
        let last_seg = text.rsplit('\n').next().unwrap_or("");
        Position::new(pos.line + newlines as u32, last_seg.chars().count() as u32)
    }
}

/// Compute the text to insert for an Enter keypress (with auto-indent).
fn compute_newline_text(lines: &[String], pos: Position) -> String {
    let line = lines
        .get(pos.line as usize)
        .map(String::as_str)
        .unwrap_or("");
    let indent_end = line
        .find(|c: char| !c.is_whitespace())
        .unwrap_or(line.len());
    let base_indent = &line[..indent_end];

    let chars: Vec<char> = line.chars().collect();
    let col = (pos.character as usize).min(chars.len());
    let prev_char = if col > 0 {
        chars.get(col - 1).copied()
    } else {
        None
    };

    let extra = match prev_char {
        Some('{') | Some('(') | Some('[') | Some(':') => "    ",
        _ => "",
    };

    format!("\n{base_indent}{extra}")
}

enum DeleteKind {
    CharLeft,
    CharRight,
    WordLeft,
    WordRight,
    LineLeft,
    LineRight,
}

impl DeleteKind {
    fn label(&self) -> &'static str {
        match self {
            DeleteKind::CharLeft => "backspace",
            DeleteKind::CharRight => "delete",
            DeleteKind::WordLeft => "delete-word-left",
            DeleteKind::WordRight => "delete-word-right",
            DeleteKind::LineLeft => "delete-line-left",
            DeleteKind::LineRight => "delete-line-right",
        }
    }
}

/// Compute the range to delete for a given `DeleteKind` at `pos`.
fn delete_range(lines: &[String], pos: Position, kind: &DeleteKind) -> Option<Range> {
    let line = pos.line as usize;
    let col = pos.character as usize;

    match kind {
        DeleteKind::CharLeft => {
            if col > 0 {
                Some(Range::new(Position::new(pos.line, col as u32 - 1), pos))
            } else if line > 0 {
                let prev_len = lines.get(line - 1)?.chars().count() as u32;
                Some(Range::new(Position::new(pos.line - 1, prev_len), pos))
            } else {
                None
            }
        }
        DeleteKind::CharRight => {
            let line_len = lines.get(line)?.chars().count();
            if col < line_len {
                Some(Range::new(pos, Position::new(pos.line, col as u32 + 1)))
            } else if line + 1 < lines.len() {
                Some(Range::new(pos, Position::new(pos.line + 1, 0)))
            } else {
                None
            }
        }
        DeleteKind::WordLeft => {
            let line_str = lines.get(line).map(String::as_str).unwrap_or("");
            let new_col = word_boundary_left(line_str, col);
            if new_col < col {
                Some(Range::new(Position::new(pos.line, new_col as u32), pos))
            } else if col == 0 && line > 0 {
                let prev_len = lines.get(line - 1)?.chars().count() as u32;
                Some(Range::new(Position::new(pos.line - 1, prev_len), pos))
            } else {
                None
            }
        }
        DeleteKind::WordRight => {
            let line_str = lines.get(line).map(String::as_str).unwrap_or("");
            let line_len = line_str.chars().count();
            let new_col = word_boundary_right(line_str, col);
            if new_col > col {
                Some(Range::new(pos, Position::new(pos.line, new_col as u32)))
            } else if col == line_len && line + 1 < lines.len() {
                Some(Range::new(pos, Position::new(pos.line + 1, 0)))
            } else {
                None
            }
        }
        DeleteKind::LineLeft => Some(Range::new(Position::new(pos.line, 0), pos)),
        DeleteKind::LineRight => {
            let line_len = lines.get(line)?.chars().count() as u32;
            Some(Range::new(pos, Position::new(pos.line, line_len)))
        }
    }
}

fn word_boundary_left(line: &str, col: usize) -> usize {
    let chars: Vec<char> = line.chars().collect();
    let mut i = col.min(chars.len());
    while i > 0 && chars[i - 1].is_whitespace() {
        i -= 1;
    }
    if i == 0 {
        return 0;
    }
    let word_mode = is_word_char(chars[i - 1]);
    while i > 0 && is_word_char(chars[i - 1]) == word_mode {
        i -= 1;
    }
    i
}

fn word_boundary_right(line: &str, col: usize) -> usize {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = col.min(len);
    if i >= len {
        return len;
    }
    let word_mode = is_word_char(chars[i]);
    while i < len && is_word_char(chars[i]) == word_mode {
        i += 1;
    }
    while i < len && chars[i].is_whitespace() {
        i += 1;
    }
    i
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Register all compile-time tree-sitter grammars with the global registry.
///
/// Must be called once at startup before any syntax highlighting is attempted.
fn register_grammars() {
    let reg = grammar_registry();
    reg.register(
        Language::RUST,
        tree_sitter_rust::LANGUAGE.into(),
        queries::RUST_HIGHLIGHTS,
        "",
        "",
    );
    reg.register(
        Language::PYTHON,
        tree_sitter_python::LANGUAGE.into(),
        queries::PYTHON_HIGHLIGHTS,
        "",
        "",
    );
    reg.register(
        Language::JAVASCRIPT,
        tree_sitter_javascript::LANGUAGE.into(),
        queries::JAVASCRIPT_HIGHLIGHTS,
        "",
        "",
    );
    reg.register(
        Language::JSON,
        tree_sitter_json::LANGUAGE.into(),
        queries::JSON_HIGHLIGHTS,
        "",
        "",
    );
    reg.register(
        Language::C,
        tree_sitter_c::LANGUAGE.into(),
        queries::C_HIGHLIGHTS,
        "",
        "",
    );
    reg.register(
        Language::GO,
        tree_sitter_go::LANGUAGE.into(),
        queries::GO_HIGHLIGHTS,
        "",
        "",
    );
}

/// Extract text from document lines for a given range.
/// Return the word under the primary cursor (alphanumeric + `_`).
fn word_at_cursor(tab: &EditorTab) -> String {
    let pos = tab.cursors.primary().pos();
    let line = match tab.lines.get(pos.line as usize) {
        Some(l) => l,
        None => return String::new(),
    };
    let chars: Vec<char> = line.chars().collect();
    let col = (pos.character as usize).min(chars.len());
    if col == chars.len() || (!chars[col].is_alphanumeric() && chars[col] != '_') {
        return String::new();
    }
    let start = chars[..col]
        .iter()
        .rposition(|c| !c.is_alphanumeric() && *c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);
    let end = col
        + chars[col..]
            .iter()
            .position(|c| !c.is_alphanumeric() && *c != '_')
            .unwrap_or(chars.len() - col);
    chars[start..end].iter().collect()
}

/// Find the next occurrence of `needle` starting at or after `from`.
/// Returns the `Range` of the match, or `None` if not found.
fn find_next_occurrence(lines: &[String], needle: &str, from: Position) -> Option<Range> {
    if needle.is_empty() {
        return None;
    }
    let needle_chars: Vec<char> = needle.chars().collect();
    let needle_len = needle_chars.len();

    for (li, line) in lines.iter().enumerate() {
        let li_u32 = li as u32;
        let chars: Vec<char> = line.chars().collect();
        let start_col = if li_u32 == from.line {
            from.character as usize
        } else {
            0
        };

        for ci in start_col..chars.len() {
            if ci + needle_len > chars.len() {
                break;
            }
            if chars[ci..ci + needle_len] == needle_chars[..] {
                return Some(Range::new(
                    Position::new(li_u32, ci as u32),
                    Position::new(li_u32, (ci + needle_len) as u32),
                ));
            }
        }
    }
    None
}

fn extract_text(lines: &[String], range: Range) -> String {
    let start_line = range.start.line as usize;
    let end_line = range.end.line as usize;

    if start_line == end_line {
        let line = lines.get(start_line).map(String::as_str).unwrap_or("");
        let chars: Vec<char> = line.chars().collect();
        let s = (range.start.character as usize).min(chars.len());
        let e = (range.end.character as usize).min(chars.len());
        chars[s..e].iter().collect()
    } else {
        let mut result = String::new();
        for l in start_line..=end_line {
            if !result.is_empty() {
                result.push('\n');
            }
            let line = lines.get(l).map(String::as_str).unwrap_or("");
            let chars: Vec<char> = line.chars().collect();
            let s = if l == start_line {
                (range.start.character as usize).min(chars.len())
            } else {
                0
            };
            let e = if l == end_line {
                (range.end.character as usize).min(chars.len())
            } else {
                chars.len()
            };
            result.extend(&chars[s..e]);
        }
        result
    }
}

/// Return the selected text of the primary cursor.
fn selected_text(tab: &EditorTab) -> Option<String> {
    let cursor = tab.cursors.primary();
    if !cursor.has_selection() {
        return None;
    }
    Some(extract_text(&tab.lines, cursor.range()))
}

/// Return the bracket that auto-closes `ch`, if any.
fn bracket_close_pair(ch: char) -> Option<char> {
    match ch {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        '\'' => Some('\''),
        '`' => Some('`'),
        _ => None,
    }
}

/// Leading whitespace of a line.
fn leading_whitespace(line: &str) -> &str {
    let end = line
        .find(|c: char| !c.is_whitespace())
        .unwrap_or(line.len());
    &line[..end]
}

/// Return the line-comment prefix for a language.
fn line_comment_prefix(lang: &Language) -> &'static str {
    let id = lang.as_str();
    match id {
        "rust" | "c" | "cpp" | "java" | "javascript" | "typescript" | "go" | "swift" | "kotlin"
        | "csharp" | "dart" | "scala" | "groovy" | "gradle" | "json5" | "jsonc" => "//",
        "python" | "ruby" | "shell" | "bash" | "yaml" | "toml" | "r" | "elixir" | "perl"
        | "powershell" => "#",
        "lua" | "haskell" | "sql" => "--",
        "html" | "xml" => "<!--",
        _ => "//",
    }
}

// ── Bracket matching ──────────────────────────────────────────────────────────

/// Compute the bracket pair highlight for the cursor position.
fn compute_bracket_match(lines: &[String], pos: Position) -> Option<(Range, Range)> {
    let line = lines.get(pos.line as usize)?;
    let chars: Vec<char> = line.chars().collect();
    let col = (pos.character as usize).min(chars.len());

    // Check char at cursor (opening bracket).
    if let Some(&ch) = chars.get(col) {
        if let Some(close) = matching_close(ch) {
            if let Some(close_pos) = find_forward(lines, pos, ch, close) {
                let open_range = Range::new(pos, Position::new(pos.line, pos.character + 1));
                let close_range = Range::new(
                    close_pos,
                    Position::new(close_pos.line, close_pos.character + 1),
                );
                return Some((open_range, close_range));
            }
        }
    }

    // Check char before cursor (closing bracket).
    if col > 0 {
        if let Some(&ch) = chars.get(col - 1) {
            if let Some(open) = matching_open(ch) {
                let before = Position::new(pos.line, pos.character - 1);
                if let Some(open_pos) = find_backward(lines, before, open, ch) {
                    let open_range = Range::new(
                        open_pos,
                        Position::new(open_pos.line, open_pos.character + 1),
                    );
                    let close_range = Range::new(before, pos);
                    return Some((open_range, close_range));
                }
            }
        }
    }

    None
}

fn matching_close(ch: char) -> Option<char> {
    match ch {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        _ => None,
    }
}

fn matching_open(ch: char) -> Option<char> {
    match ch {
        ')' => Some('('),
        ']' => Some('['),
        '}' => Some('{'),
        _ => None,
    }
}

/// Find the matching closing bracket scanning forward from `pos`.
fn find_forward(lines: &[String], pos: Position, open: char, close: char) -> Option<Position> {
    let mut depth = 0i32;
    let start_line = pos.line as usize;
    let start_col = pos.character as usize;

    for (li, line) in lines.iter().enumerate().skip(start_line) {
        let chars: Vec<char> = line.chars().collect();
        let start_c = if li == start_line { start_col } else { 0 };
        for (ci, &ch) in chars.iter().enumerate().skip(start_c) {
            if ch == open {
                depth += 1;
            }
            if ch == close {
                depth -= 1;
                if depth == 0 {
                    return Some(Position::new(li as u32, ci as u32));
                }
            }
        }
    }
    None
}

/// Find the matching opening bracket scanning backward from `pos`.
fn find_backward(lines: &[String], pos: Position, open: char, close: char) -> Option<Position> {
    let mut depth = 0i32;
    let start_line = pos.line as usize;
    let start_col = pos.character as usize;

    for li in (0..=start_line).rev() {
        let line = lines.get(li)?;
        let chars: Vec<char> = line.chars().collect();
        let end_c = if li == start_line {
            start_col + 1
        } else {
            chars.len()
        };
        for ci in (0..end_c.min(chars.len())).rev() {
            let ch = chars[ci];
            if ch == close {
                depth += 1;
            }
            if ch == open {
                depth -= 1;
                if depth == 0 {
                    return Some(Position::new(li as u32, ci as u32));
                }
            }
        }
    }
    None
}

/// Clamp every cursor's active position and selection anchor so that neither
/// exceeds the actual line count or column count in `lines`.
///
/// Must be called after any operation that replaces the `lines` snapshot
/// (undo, redo, external file reload) to prevent "Column N exceeds line L
/// length M" buffer errors on the next insert or delete.
fn clamp_cursors_to_content(cursors: &mut CursorSet, lines: &[String]) {
    let n_lines = lines.len().max(1) as u32;

    let clamp_pos = |line: u32, col: u32| -> (u32, u32) {
        let line = line.min(n_lines - 1);
        let line_len = lines
            .get(line as usize)
            .map(|l| l.chars().count() as u32)
            .unwrap_or(0);
        (line, col.min(line_len))
    };

    for cursor in cursors.all_mut() {
        let (al, ac) = clamp_pos(
            cursor.selection.active.line,
            cursor.selection.active.character,
        );
        cursor.selection.active = Position::new(al, ac);
        cursor.preferred_col = ac;

        let (nl, nc) = clamp_pos(
            cursor.selection.anchor.line,
            cursor.selection.anchor.character,
        );
        cursor.selection.anchor = Position::new(nl, nc);
    }
}
