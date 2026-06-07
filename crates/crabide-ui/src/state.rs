//! `UiState` — the complete mutable display state owned by the UI thread.
//!
//! The app crate creates one `UiState`, updates it when background events
//! arrive (new diagnostics, git diffs, LSP hover, etc.), and passes it by
//! mutable reference into `crabide_ui::render` every egui frame.
//!
//! `UiState` is **pure data** — it has no channels, async handles, or locks.
//! Background-event integration happens in `crabide-app`.

use std::path::PathBuf;
use std::time::Instant;

use crabide_buffer::{CursorSet, SnippetEngine, SnippetTabstop};
use crabide_config::{Action, Color, ColorTheme, KeybindingEngine, WhenContext};
use crabide_core::{
    event::{
        BlameLine, Diagnostic, DiffHunk, FileStatus, OutputCategory, StackFrame, TerminalCell,
        TerminalColor, Variable,
    },
    types::{BufferId, DocumentUri, Language, Position, Range},
};
use crabide_dap::LaunchConfig;
use crabide_extensions::{
    ContentBlock, ContextMenuContribution, GutterMarker, InstalledExtension, NavigateTarget,
    PanelRegistration, RegistryExtension, SidebarPaneRegistration, StatusBarAlignment,
};
use crabide_search::{FuzzyFileFinder, GrepAbortHandle, GrepMatch};
use crabide_syntax::HighlightSpan;
use indexmap::IndexMap;

use crate::layout::{default_layout, PaneKind};

// ── Helper ────────────────────────────────────────────────────────────────────

/// Convert a `crabide_config::Color` to an egui `Color32`.
pub fn cfg_to_egui(c: Color) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

// ── FindReplaceState ──────────────────────────────────────────────────────────

/// State for the find/replace overlay bar (Ctrl+F / Ctrl+H).
#[derive(Default)]
pub struct FindReplaceState {
    /// Whether the bar is visible.
    pub visible: bool,
    /// True when the replace row is also shown (opened with Ctrl+H).
    pub replace_visible: bool,
    /// The search query string.
    pub query: String,
    /// The replacement string.
    pub replacement: String,
    /// Use regular expressions for matching.
    pub use_regex: bool,
    /// Case-sensitive matching.
    pub case_sensitive: bool,
    /// Match whole words only.
    pub whole_word: bool,
    /// All match ranges in the current document (recomputed on query change).
    pub match_ranges: Vec<Range>,
    /// Index of the currently selected match within `match_ranges`.
    pub current_match_idx: usize,
    /// The query that was used to compute `match_ranges` (to detect changes).
    pub last_computed_query: String,
    /// Set to true when the bar is first opened so the query field can grab focus.
    pub just_opened: bool,
    /// Set to true after find/prev so the query TextEdit regains focus next frame.
    pub needs_refocus: bool,
}

impl FindReplaceState {
    /// Whether the current query is non-empty and has computed results.
    pub fn has_matches(&self) -> bool {
        !self.query.is_empty() && !self.match_ranges.is_empty()
    }

    /// Navigate to next match (wraps around).
    pub fn next_match(&mut self) {
        if self.match_ranges.is_empty() {
            return;
        }
        self.current_match_idx = (self.current_match_idx + 1) % self.match_ranges.len();
    }

    /// Navigate to previous match (wraps around).
    pub fn prev_match(&mut self) {
        if self.match_ranges.is_empty() {
            return;
        }
        if self.current_match_idx == 0 {
            self.current_match_idx = self.match_ranges.len() - 1;
        } else {
            self.current_match_idx -= 1;
        }
    }

    /// Current match range, if any.
    pub fn current_match(&self) -> Option<Range> {
        self.match_ranges.get(self.current_match_idx).copied()
    }
}

// ── FuzzyFinderState ──────────────────────────────────────────────────────────

/// State for the Ctrl+P fuzzy file finder overlay.
#[derive(Default)]
pub struct FuzzyFinderState {
    /// Whether the overlay is visible.
    pub visible: bool,
    /// Current query typed by the user.
    pub query: String,
    /// Persistent fuzzy finder (holds the file index; rebuilt only on workspace changes).
    pub finder: FuzzyFileFinder,
    /// Matching paths after fuzzy scoring.
    pub results: Vec<PathBuf>,
    /// Index of the currently highlighted result row.
    pub selected_idx: usize,
    /// Display strings parallel to `results` (e.g. relative paths).
    pub result_labels: Vec<String>,
    /// Set to `true` when VFS file changes occur; the index should be rebuilt
    /// on the next `open()` call.
    pub index_stale: bool,
}

impl FuzzyFinderState {
    /// Open (or re-open) the finder and clear the previous query/results.
    pub fn open(&mut self) {
        self.visible = true;
        self.query = String::new();
        self.selected_idx = 0;
        self.results.clear();
        self.result_labels.clear();
        // If the file index is stale due to VFS changes, rebuild it on next open.
        // The caller (app) should call `self.finder.update_index(...)` before
        // or after `open()` if `self.index_stale` is true.
    }

    /// Close and reset the finder.
    pub fn close(&mut self) {
        self.visible = false;
        self.query = String::new();
        self.selected_idx = 0;
        self.results.clear();
        self.result_labels.clear();
    }
}

// ── WorkspaceSearchState ──────────────────────────────────────────────────────

/// State for the Ctrl+Shift+F workspace-grep panel.
#[derive(Default)]
pub struct WorkspaceSearchState {
    /// Whether the panel is visible.
    pub visible: bool,
    /// Query string.
    pub query: String,
    /// Use regular expressions.
    pub use_regex: bool,
    /// Case-sensitive search.
    pub case_sensitive: bool,
    /// Grep results populated by the app.
    pub results: Vec<GrepMatch>,
    /// Index of the currently highlighted result.
    pub selected_idx: usize,
    /// Set to `true` while the app is running the grep in the background.
    pub is_searching: bool,
    /// Set to true when the panel first opens so the query field grabs focus.
    pub just_opened: bool,
    /// Abort handle for the currently running grep (if any).
    pub abort_handle: GrepAbortHandle,
}

// ── GotoLineState ─────────────────────────────────────────────────────────────

/// State for the Ctrl+G go-to-line dialog.
#[derive(Default)]
pub struct GotoLineState {
    /// Whether the dialog is visible.
    pub visible: bool,
    /// Raw user input (a line number string).
    pub query: String,
}

impl GotoLineState {
    /// Parse `query` as a 1-based line number and return the 0-based index,
    /// or `None` if unparseable or out of range.
    pub fn target_line(&self, max_lines: usize) -> Option<usize> {
        let n: usize = self.query.trim().parse().ok()?;
        if n == 0 {
            return None;
        }
        let zero_based = n - 1;
        if zero_based < max_lines {
            Some(zero_based)
        } else {
            None
        }
    }
}

/// A search result for the symbol outline overlay.
#[derive(Debug, Clone)]
pub struct SymbolOutlineEntry {
    pub name: String,
    pub kind: String,
    pub line: u32,
}

/// State for the Go-to-symbol (Ctrl+Shift+O) overlay.
#[derive(Default)]
pub struct SymbolOutlineState {
    pub visible: bool,
    pub query: String,
    pub entries: Vec<SymbolOutlineEntry>,
    pub selected_idx: usize,
}

// ── DapPanelState ─────────────────────────────────────────────────────────────

/// All state for the integrated debugger bottom panel.
pub struct DapPanelState {
    /// Whether the debug panel is visible.
    pub visible: bool,
    /// Whether the debugger feature is enabled (user-toggled).
    pub enabled: bool,

    // ── Session state ─────────────────────────────────────────────────────────
    /// Whether a debug session is currently active.
    pub session_active: bool,
    /// Whether execution is currently paused.
    pub paused: bool,
    /// The thread that triggered the last stop.
    pub paused_thread_id: Option<u64>,
    /// Human-readable reason for the last stop ("breakpoint", "step", …).
    pub stop_reason: Option<String>,

    // ── Launch configuration ──────────────────────────────────────────────────
    /// Available launch configurations parsed from launch.json.
    pub launch_configs: Vec<LaunchConfig>,
    /// Index into `launch_configs` for the selected config.
    pub selected_config_idx: usize,

    // ── Call stack ────────────────────────────────────────────────────────────
    pub call_stack: Vec<StackFrame>,
    /// The currently selected (active) stack frame id.
    pub active_frame_id: Option<u64>,

    // ── Variables (scope ref → variables) ────────────────────────────────────
    pub variables: IndexMap<u32, Vec<Variable>>,
    /// Variable references that have been expanded to show children.
    pub expanded_var_refs: std::collections::HashSet<u64>,

    // ── Watch expressions ─────────────────────────────────────────────────────
    pub watch_expressions: Vec<String>,
    /// Input buffer for adding a new watch expression.
    pub watch_input: String,

    // ── Debug console output ──────────────────────────────────────────────────
    pub console_lines: Vec<(OutputCategory, String)>,
    /// Whether the console should auto-scroll to the bottom.
    pub console_scroll_to_bottom: bool,

    // ── Active sub-tab (0=call stack, 1=variables, 2=watch, 3=console) ────────
    pub active_tab: usize,

    // ── Breakpoint verification status (id → state) ───────────────────────────
    /// Verified/unverified state for breakpoints returned by the adapter.
    pub breakpoint_states: Vec<crabide_core::event::BreakpointState>,

    // ── Pending actions drained by the app each frame ─────────────────────────
    /// Start a debug session with the selected launch config.
    pub pending_launch: bool,
    pub pending_continue: bool,
    pub pending_step_over: bool,
    pub pending_step_in: bool,
    pub pending_step_out: bool,
    pub pending_stop: bool,
    pub pending_restart: bool,
    pub pending_pause: bool,
    /// Request stack trace for the paused thread.
    pub pending_stack_trace: bool,
    /// Expand a variable reference (fetch its children).
    pub pending_expand_var: Option<u64>,
    /// Set/clear breakpoints for these files (path → 0-based line list).
    pub pending_set_breakpoints: Vec<(std::path::PathBuf, Vec<u32>)>,
}

impl Default for DapPanelState {
    fn default() -> Self {
        Self {
            visible: false,
            enabled: false,
            session_active: false,
            paused: false,
            paused_thread_id: None,
            stop_reason: None,
            launch_configs: Vec::new(),
            selected_config_idx: 0,
            call_stack: Vec::new(),
            active_frame_id: None,
            variables: IndexMap::new(),
            expanded_var_refs: std::collections::HashSet::new(),
            watch_expressions: Vec::new(),
            watch_input: String::new(),
            console_lines: Vec::new(),
            console_scroll_to_bottom: true,
            active_tab: 0,
            breakpoint_states: Vec::new(),
            pending_launch: false,
            pending_continue: false,
            pending_step_over: false,
            pending_step_in: false,
            pending_step_out: false,
            pending_stop: false,
            pending_restart: false,
            pending_pause: false,
            pending_stack_trace: false,
            pending_expand_var: None,
            pending_set_breakpoints: Vec::new(),
        }
    }
}

impl DapPanelState {
    /// Append a line to the debug console (capped at 2000 entries).
    pub fn append_console(&mut self, category: OutputCategory, text: String) {
        // Split on newlines so each logical output line is separate.
        for line in text.lines() {
            if self.console_lines.len() >= 2000 {
                self.console_lines.remove(0);
            }
            self.console_lines.push((category.clone(), line.to_owned()));
        }
        self.console_scroll_to_bottom = true;
    }

    /// Reset all session-specific state after termination.
    pub fn reset_session(&mut self) {
        self.session_active = false;
        self.paused = false;
        self.paused_thread_id = None;
        self.stop_reason = None;
        self.call_stack.clear();
        self.active_frame_id = None;
        self.variables.clear();
        self.expanded_var_refs.clear();
        self.breakpoint_states.clear();
    }
}

// ── EditorTab ─────────────────────────────────────────────────────────────────

/// All display state for one open document tab.
pub struct EditorTab {
    pub buffer_id: BufferId,
    pub title: String,
    pub uri: DocumentUri,
    pub language: Language,
    pub is_dirty: bool,
    /// Snapshot of document lines — updated by the app on every edit.
    pub lines: Vec<String>,
    /// Syntax highlight spans for the current snapshot, sorted by start.
    pub highlight_spans: Vec<HighlightSpan>,
    /// LSP diagnostics for this file.
    pub diagnostics: Vec<Diagnostic>,
    /// Git diff hunks for gutter markers.
    pub git_hunks: Vec<DiffHunk>,
    /// Breakpoints set in this file (0-based line numbers).
    pub breakpoints: Vec<u32>,
    /// Gutter markers contributed by extensions for this document.
    pub extension_gutter_markers: Vec<GutterMarker>,
    /// Per-tab cursor / selection state (owned by UI).
    pub cursors: CursorSet,
    /// Snippet engine for tabstop cycling (owned by UI).
    pub snippet_engine: SnippetEngine,
    /// Matching bracket pair for the cursor position: (open_range, close_range).
    pub bracket_match: Option<(Range, Range)>,
    /// Scroll state id used by egui's `ScrollArea` to persist scroll position.
    pub scroll_id: egui::Id,
    /// Anchor position when the user is drag-selecting with the mouse.
    /// `None` when no drag is in progress.
    pub drag_anchor: Option<Position>,
    /// Timestamp (egui time) of the most recent primary-button press in this tab.
    pub last_click_time: f64,
    /// Document position of the most recent primary-button press.
    pub last_click_pos: Option<Position>,
    /// Consecutive click count at the same position (1 = single, 2 = double, 3+ = triple).
    pub click_count: u32,
    /// LSP inlay hints (parameter names, type hints) rendered inline.
    pub inlay_hints: Vec<crabide_core::event::InlayHint>,
    /// LSP semantic tokens for syntax highlighting.
    pub semantic_tokens: Vec<crabide_core::event::SemanticToken>,
    /// LSP code lens items (clickable links above functions).
    pub code_lens: Vec<crabide_core::event::CodeLens>,
}

impl EditorTab {
    pub fn new(buffer_id: BufferId, title: String, uri: DocumentUri, language: Language) -> Self {
        let scroll_id = egui::Id::new(("tab_scroll", buffer_id));
        Self {
            buffer_id,
            title,
            uri,
            language,
            is_dirty: false,
            lines: Vec::new(),
            highlight_spans: Vec::new(),
            diagnostics: Vec::new(),
            git_hunks: Vec::new(),
            breakpoints: Vec::new(),
            extension_gutter_markers: Vec::new(),
            cursors: CursorSet::new(),
            snippet_engine: SnippetEngine::new(),
            bracket_match: None,
            scroll_id,
            drag_anchor: None,
            last_click_time: 0.0,
            last_click_pos: None,
            click_count: 0,
            inlay_hints: Vec::new(),
            semantic_tokens: Vec::new(),
            code_lens: Vec::new(),
        }
    }

    /// Returns a copy of the current tabstop being edited, if any.
    pub fn active_tabstop(&self) -> Option<SnippetTabstop> {
        self.snippet_engine.current_tabstop().cloned()
    }
}

// ── FileExplorer ──────────────────────────────────────────────────────────────

/// Decoration from git status shown on a file tree node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitDecoration {
    Modified,
    Added,
    Deleted,
    Untracked,
    Conflicted,
}

/// A node in the file-explorer tree.
#[derive(Debug, Clone)]
pub struct FileNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    /// Only non-empty for directories after expansion.
    pub children: Vec<FileNode>,
    pub expanded: bool,
    pub git_status: Option<GitDecoration>,
}

/// State of the file-explorer sidebar panel.
#[derive(Debug, Default)]
pub struct FileExplorerState {
    /// Top-level workspace roots (populated by the app).
    pub roots: Vec<FileNode>,
}

// ── GitPanelState ─────────────────────────────────────────────────────────────

/// All state for the Source Control git panel.
pub struct GitPanelState {
    /// Whether the git panel is visible as a bottom strip.
    pub visible: bool,

    /// Files whose index (staged) status is non-trivial.
    pub staged_files: Vec<FileStatus>,
    /// Files whose worktree (unstaged) status is non-trivial.
    pub unstaged_files: Vec<FileStatus>,

    /// Commit message typed by the user.
    pub commit_message: String,

    /// Blame lines keyed by absolute path, populated on request.
    pub blame_lines: IndexMap<PathBuf, Vec<BlameLine>>,

    // ── Pending actions drained by the app each frame ─────────────────────────
    pub pending_stage_file: Option<PathBuf>,
    pub pending_unstage_file: Option<PathBuf>,
    pub pending_stage_all: bool,
    pub pending_unstage_all: bool,
    /// When true the app should call `git_service.commit(commit_message)`.
    pub pending_commit: bool,
    pub pending_blame_request: Option<PathBuf>,
    pub pending_discard_file: Option<PathBuf>,
}

impl Default for GitPanelState {
    fn default() -> Self {
        Self {
            visible: false,
            staged_files: Vec::new(),
            unstaged_files: Vec::new(),
            commit_message: String::new(),
            blame_lines: IndexMap::new(),
            pending_stage_file: None,
            pending_unstage_file: None,
            pending_stage_all: false,
            pending_unstage_all: false,
            pending_commit: false,
            pending_blame_request: None,
            pending_discard_file: None,
        }
    }
}

// ── Terminal ──────────────────────────────────────────────────────────────────

/// A single cell in the terminal display grid (UI copy).
#[derive(Clone, Copy)]
pub struct DisplayCell {
    pub ch: char,
    pub fg: TerminalColor,
    pub bg: TerminalColor,
    pub attrs: crabide_core::event::CellAttrs,
}

impl DisplayCell {
    pub const BLANK: Self = Self {
        ch: ' ',
        fg: TerminalColor::Default,
        bg: TerminalColor::Default,
        attrs: crabide_core::event::CellAttrs::empty(),
    };
}

impl From<TerminalCell> for DisplayCell {
    fn from(c: TerminalCell) -> Self {
        Self {
            ch: c.ch,
            fg: c.fg,
            bg: c.bg,
            attrs: c.attrs,
        }
    }
}

/// UI-side state for one running terminal instance.
pub struct TerminalInstance {
    pub id: u32,
    pub title: String,
    pub cwd: Option<PathBuf>,
    /// Visible grid rows (row-major: `rows[row][col]`).
    pub rows: Vec<Vec<DisplayCell>>,
    pub cols: u16,
    pub grid_rows: u16,
    /// Cursor position in the visible grid.
    pub cursor_col: u16,
    pub cursor_row: u16,
    /// How far the user has scrolled up into scrollback (0 = bottom).
    pub scroll_offset: u32,
    /// Total scrollback rows available (updated from delta).
    pub scrollback_len: u32,
    /// When true the terminal process has exited.
    pub exited: bool,
}

impl TerminalInstance {
    pub fn new(id: u32, cols: u16, grid_rows: u16) -> Self {
        let blank_row = vec![DisplayCell::BLANK; cols as usize];
        Self {
            id,
            title: format!("Terminal {id}"),
            cwd: None,
            rows: vec![blank_row; grid_rows as usize],
            cols,
            grid_rows,
            cursor_col: 0,
            cursor_row: 0,
            scroll_offset: 0,
            scrollback_len: 0,
            exited: false,
        }
    }

    /// Apply a grid delta received from the PTY reader.
    pub fn apply_delta(&mut self, delta: &crabide_core::event::TerminalGridDelta) {
        self.cursor_col = delta.cursor_col;
        self.cursor_row = delta.cursor_row;
        self.scrollback_len = delta.scroll_top;

        for changed in &delta.rows {
            let row = changed.row as usize;
            // Grow rows vec if needed (terminal resized larger).
            while self.rows.len() <= row {
                self.rows.push(vec![DisplayCell::BLANK; self.cols as usize]);
            }
            // Ensure each row is wide enough.
            if self.rows[row].len() < changed.cells.len() {
                self.rows[row].resize(changed.cells.len(), DisplayCell::BLANK);
            }
            for (col, cell) in changed.cells.iter().enumerate() {
                if col < self.rows[row].len() {
                    self.rows[row][col] = DisplayCell::from(*cell);
                }
            }
        }
    }

    /// Handle a PTY resize: update cols/rows and re-blank.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.grid_rows = rows;
        // Resize each row
        for row in &mut self.rows {
            row.resize(cols as usize, DisplayCell::BLANK);
        }
        // Add or remove rows
        let blank_row = || vec![DisplayCell::BLANK; cols as usize];
        self.rows.resize_with(rows as usize, blank_row);
    }
}

/// State for the integrated terminal bottom panel.
#[derive(Default)]
pub struct TerminalPanelState {
    /// Whether the terminal panel is visible.
    pub visible: bool,
    /// All running terminal instances.
    pub instances: Vec<TerminalInstance>,
    /// Index into `instances` for the focused terminal.
    pub active_idx: usize,
    /// Whether the terminal area has keyboard focus.
    pub has_focus: bool,

    // ── Pending actions drained by the app each frame ─────────────────────────
    /// Request to open a new terminal.
    pub pending_new: bool,
    /// Request to kill terminal by id.
    pub pending_kill: Option<u32>,
    /// Request to send input bytes to a terminal.
    pub pending_input: Vec<(u32, Vec<u8>)>,
    /// Request to resize a terminal (id, cols, rows).
    pub pending_resize: Option<(u32, u16, u16)>,
    /// Debounce: last resize dimensions seen (cols, rows). Counts how many
    /// consecutive frames the same size has been observed before sending PTY resize.
    pub(crate) resize_stable: Option<(u16, u16, u8)>,
}

impl TerminalPanelState {
    /// The active terminal instance, if any.
    pub fn active(&self) -> Option<&TerminalInstance> {
        self.instances.get(self.active_idx)
    }

    /// The active terminal instance (mutable), if any.
    pub fn active_mut(&mut self) -> Option<&mut TerminalInstance> {
        self.instances.get_mut(self.active_idx)
    }

    /// Find a mutable reference to a terminal by id.
    pub fn by_id_mut(&mut self, id: u32) -> Option<&mut TerminalInstance> {
        self.instances.iter_mut().find(|t| t.id == id)
    }

    /// Remove a terminal by id.
    pub fn remove_by_id(&mut self, id: u32) {
        if let Some(pos) = self.instances.iter().position(|t| t.id == id) {
            self.instances.remove(pos);
            if self.active_idx >= self.instances.len() && !self.instances.is_empty() {
                self.active_idx = self.instances.len() - 1;
            }
        }
    }
}

// ── CommandPalette ────────────────────────────────────────────────────────────

/// A single entry shown in the command palette results list.
#[derive(Clone)]
pub struct PaletteEntry {
    pub action: Action,
    pub label: String,
    /// Formatted keybinding string, e.g. `"Ctrl+Shift+P"`.
    pub shortcut: String,
}

/// State for the command-palette overlay (Ctrl+Shift+P).
#[derive(Default)]
pub struct CommandPaletteState {
    pub visible: bool,
    pub query: String,
    pub entries: Vec<PaletteEntry>,
    pub selected_idx: usize,
}

// ── Sidebar tab ───────────────────────────────────────────────────────────────

/// Which view is shown in the collapsible left sidebar.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SidebarTab {
    #[default]
    Explorer,
    Extensions,
    /// An extension-contributed left-sidebar pane (identified by pane id).
    ExtensionPane(String),
}

// ── ExtensionsPanelState ──────────────────────────────────────────────────────

/// Active sub-tab in the Extensions panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExtensionsPanelTab {
    #[default]
    Installed,
    Search,
    Recommended,
}

/// A single status-bar contribution from an extension.
pub struct StatusBarItem {
    pub text: String,
    pub tooltip: Option<String>,
    /// Extension command to execute when the item is clicked.
    pub command: Option<String>,
    /// Which side of the status bar this item docks to.
    pub alignment: StatusBarAlignment,
}

/// All state for the Extensions manager panel.
pub struct ExtensionsPanelState {
    // ── Panel navigation ──────────────────────────────────────────────────────
    pub active_tab: ExtensionsPanelTab,
    /// Currently selected extension id for detail / action.
    pub selected_id: Option<String>,

    // ── Search tab ────────────────────────────────────────────────────────────
    pub search_query: String,
    pub search_results: Vec<RegistryExtension>,
    pub is_searching: bool,
    /// Focus the search input on next frame.
    pub just_opened_search: bool,

    // ── Recommended tab cache ─────────────────────────────────────────────────
    pub recommended: Vec<RegistryExtension>,

    // ── Installed extension cache (refreshed by app each frame) ──────────────
    /// Snapshot of `ExtensionHost::installed()` so the panel can render the list.
    pub installed: Vec<InstalledExtension>,

    // ── Extension outputs displayed inside this panel ─────────────────────────
    /// Status bar text slots keyed by extension id.
    pub status_bar_items: IndexMap<String, StatusBarItem>,

    // ── Pending actions drained by the app each frame ─────────────────────────
    /// Open the native file picker to load a `.wasm` extension.
    pub pending_install_local: bool,
    /// Toggle enabled state of an extension (id).
    pub pending_toggle: Option<String>,
    /// Uninstall an extension (id).
    pub pending_uninstall: Option<String>,
    /// Search registry with this query string.
    pub pending_search: Option<String>,
    /// Install an extension from the registry (id).
    pub pending_install_registry: Option<String>,
    /// Execute an extension command.
    pub pending_execute_command: Option<(String, Vec<String>)>,
    /// Request the app cycle to the next theme (set by ThemeSwitcher extension).
    pub pending_cycle_theme: bool,
}

impl Default for ExtensionsPanelState {
    fn default() -> Self {
        Self {
            active_tab: ExtensionsPanelTab::Installed,
            selected_id: None,
            installed: Vec::new(),
            search_query: String::new(),
            search_results: Vec::new(),
            is_searching: false,
            just_opened_search: false,
            recommended: Vec::new(),
            status_bar_items: IndexMap::new(),
            pending_install_local: false,
            pending_toggle: None,
            pending_uninstall: None,
            pending_search: None,
            pending_install_registry: None,
            pending_execute_command: None,
            pending_cycle_theme: false,
        }
    }
}

// ── ExtensionPanelUiState ─────────────────────────────────────────────────────

/// Runtime state for one dynamically-registered extension panel.
pub struct ExtensionPanelUiState {
    /// The static registration provided by the extension.
    pub registration: PanelRegistration,
    /// Latest content blocks pushed by the extension's `poll()`.
    pub content: Vec<ContentBlock>,
    /// Whether the panel is currently visible.
    pub open: bool,
}

// ── SidebarPaneUiState ────────────────────────────────────────────────────────

/// Runtime state for one dynamically-registered left-sidebar pane.
pub struct SidebarPaneUiState {
    pub registration: SidebarPaneRegistration,
    pub content: Vec<crabide_extensions::ContentBlock>,
    pub visible: bool,
}

// ── LSP indicator ─────────────────────────────────────────────────────────────

/// Displayed in the status bar per language server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspStatus {
    Starting,
    Ready,
    Error,
}

// ── UiState ───────────────────────────────────────────────────────────────────

/// Complete mutable UI state for the editor, owned by the application.
///
/// All render functions borrow this mutably so they can update scroll
/// positions, cursor state, palette input, etc. in the same frame they render.
pub struct UiState {
    // ── Theme & bindings ──────────────────────────────────────────────────────
    pub theme: ColorTheme,
    pub keybindings: KeybindingEngine,
    /// Runtime context for evaluating keybinding `when` conditions.
    /// Populated by the app layer before each frame and consumed during key
    /// event processing in the UI layer.
    pub when_context: WhenContext,

    // ── Editor tabs ───────────────────────────────────────────────────────────
    pub tabs: Vec<EditorTab>,
    pub active_tab: Option<usize>,

    // ── Panel layout (egui_tiles) ─────────────────────────────────────────────
    pub layout: egui_tiles::Tree<PaneKind>,
    pub sidebar_visible: bool,

    // ── File explorer ─────────────────────────────────────────────────────────
    pub file_explorer: FileExplorerState,

    // ── Command palette ───────────────────────────────────────────────────────
    pub command_palette: CommandPaletteState,

    // ── Find / Replace ────────────────────────────────────────────────────────
    pub find_replace: FindReplaceState,

    // ── Fuzzy file finder (Ctrl+P) ────────────────────────────────────────────
    pub fuzzy_finder: FuzzyFinderState,

    // ── Workspace grep (Ctrl+Shift+F) ─────────────────────────────────────────
    pub workspace_search: WorkspaceSearchState,

    // ── Go-to-line (Ctrl+G) ───────────────────────────────────────────────────
    pub goto_line: GotoLineState,

    // ── Symbol outline (Ctrl+Shift+O) ──────────────────────────────────────────
    pub symbol_outline: SymbolOutlineState,

    // ── Terminal panel ────────────────────────────────────────────────────────
    pub terminal: TerminalPanelState,

    // ── Git panel ─────────────────────────────────────────────────────────────
    pub git_panel: GitPanelState,

    // ── Git enable/disable ─────────────────────────────────────────────────────
    /// Whether the git service is enabled. Off by default to avoid the ~100 MB
    /// libgit2 RSS cost for users who don't need source control in every project.
    pub git_enabled: bool,

    // ── Debug panel ───────────────────────────────────────────────────────────
    pub dap_panel: DapPanelState,

    // ── Extensions panel ──────────────────────────────────────────────────────
    pub extensions_panel: ExtensionsPanelState,

    // ── Sidebar active tab ────────────────────────────────────────────────────
    pub sidebar_tab: SidebarTab,

    // ── Context info from background services ─────────────────────────────────
    pub git_branch: Option<String>,
    pub lsp_indicators: IndexMap<String, LspStatus>,

    // ── Timed status message (3-second TTL) ───────────────────────────────────
    pub status_message: Option<(String, Instant)>,

    // ── Caret blink ───────────────────────────────────────────────────────────
    pub(crate) caret_visible: bool,
    pub(crate) last_blink_toggle: f64, // egui time in seconds

    // ── Typography ────────────────────────────────────────────────────────────
    pub font_size: f32,

    // ── Word wrap ─────────────────────────────────────────────────────────────
    pub word_wrap: bool,

    // ── Pending file open (set by file explorer or fuzzy finder) ─────────────
    /// App drains this each frame; it maps to `Action::OpenFile`.
    pub pending_open_path: Option<PathBuf>,

    // ── Pending tab close (set by tab bar close button) ───────────────────────
    /// App drains this each frame after handling `Action::CloseTab`.
    pub pending_close_buffer: Option<BufferId>,

    // ── Pending scroll-to-line (set by goto-line / find-in-files) ────────────
    /// When Some, the editor scrolls to this 0-based line on the next frame.
    pub pending_scroll_line: Option<usize>,

    // ── Problems panel (diagnostics) ──────────────────────────────────────────
    /// True when the Problems bottom panel is visible.
    pub problems_panel_open: bool,

    // ── Dynamic extension panels ──────────────────────────────────────────────
    /// All dynamically-registered extension panels, keyed by panel id.
    pub extension_panels: IndexMap<String, ExtensionPanelUiState>,

    // ── Extension navigation request ──────────────────────────────────────────
    /// Set by extension panel row clicks; drained by the app each frame.
    pub pending_navigate: Option<NavigateTarget>,

    // ── Extension commands (for command palette) ───────────────────────────────
    /// Registry of custom actions contributed by extensions.
    pub action_registry: crabide_config::ActionRegistry,

    // ── Extension sidebar panes ───────────────────────────────────────────────
    /// Left-sidebar panes registered by extensions.
    pub sidebar_panes: IndexMap<String, SidebarPaneUiState>,

    // ── Extension context-menu contributions ──────────────────────────────────
    /// Context-menu items contributed by extensions (refreshed each frame).
    pub registered_context_menus: Vec<ContextMenuContribution>,

    // ── LSP hover / completion / code action state ────────────────────────────
    /// Text content of the hover popup (set by LSP HoverReady).
    pub hover_text: Option<String>,
    /// Completion items from the LSP server (set by CompletionReady).
    pub completion_items: Vec<crabide_core::event::CompletionItem>,
    /// Whether the completion popup is visible.
    pub completion_visible: bool,
    /// Code actions from the LSP server (set by CodeActionsReady).
    pub code_actions: Vec<crabide_core::event::CodeAction>,
    /// Whether the code actions popup is visible.
    pub code_actions_visible: bool,
    /// Signature help result from the LSP server (set by SignatureHelpReady).
    pub signature_help: Option<crabide_core::event::SignatureHelp>,

    // ── LSP popup selection state ─────────────────────────────────────────
    /// Currently selected completion item index (for keyboard navigation).
    pub completion_selected_idx: usize,
    /// Currently selected code action index (for keyboard navigation).
    pub code_actions_selected_idx: usize,
    /// Pending completion insert text (set by popup, drained by app).
    pub pending_completion_insert: Option<String>,
    /// Pending code action index (set by popup, drained by app).
    pub pending_code_action_idx: Option<usize>,
}

impl UiState {
    pub fn new(theme: ColorTheme, keybindings: KeybindingEngine) -> Self {
        Self {
            theme,
            keybindings,
            when_context: WhenContext::new(),
            tabs: Vec::new(),
            active_tab: None,
            layout: default_layout(),
            sidebar_visible: true,
            file_explorer: FileExplorerState::default(),
            command_palette: CommandPaletteState::default(),
            find_replace: FindReplaceState::default(),
            fuzzy_finder: FuzzyFinderState::default(),
            workspace_search: WorkspaceSearchState::default(),
            goto_line: GotoLineState::default(),
            symbol_outline: SymbolOutlineState::default(),
            terminal: TerminalPanelState::default(),
            git_panel: GitPanelState::default(),
            git_enabled: false,
            dap_panel: DapPanelState::default(),
            extensions_panel: ExtensionsPanelState::default(),
            sidebar_tab: SidebarTab::Explorer,
            git_branch: None,
            lsp_indicators: IndexMap::new(),
            status_message: None,
            caret_visible: true,
            last_blink_toggle: 0.0,
            font_size: 14.0,
            word_wrap: false,
            pending_open_path: None,
            pending_close_buffer: None,
            pending_scroll_line: None,
            problems_panel_open: false,
            extension_panels: IndexMap::new(),
            pending_navigate: None,
            action_registry: crabide_config::ActionRegistry::new(),
            sidebar_panes: IndexMap::new(),
            registered_context_menus: Vec::new(),
            hover_text: None,
            completion_items: Vec::new(),
            completion_visible: false,
            code_actions: Vec::new(),
            code_actions_visible: false,
            signature_help: None,
            completion_selected_idx: 0,
            code_actions_selected_idx: 0,
            pending_completion_insert: None,
            pending_code_action_idx: None,
        }
    }

    // ── Tab management ────────────────────────────────────────────────────────

    /// Open a tab for `tab`, or activate an existing one for the same buffer.
    pub fn open_tab(&mut self, tab: EditorTab) {
        if let Some(idx) = self.tabs.iter().position(|t| t.buffer_id == tab.buffer_id) {
            self.active_tab = Some(idx);
        } else {
            self.active_tab = Some(self.tabs.len());
            self.tabs.push(tab);
        }
    }

    /// Close the tab at `idx`.  Returns the closed tab's `BufferId` if any.
    pub fn close_tab(&mut self, idx: usize) -> Option<BufferId> {
        if idx >= self.tabs.len() {
            return None;
        }
        let id = self.tabs[idx].buffer_id;
        self.tabs.remove(idx);
        self.active_tab = if self.tabs.is_empty() {
            None
        } else {
            Some(idx.saturating_sub(1).min(self.tabs.len() - 1))
        };
        Some(id)
    }

    /// Mutable reference to the active tab, if any.
    pub fn active_tab_mut(&mut self) -> Option<&mut EditorTab> {
        self.active_tab.and_then(|i| self.tabs.get_mut(i))
    }

    /// Shared reference to the active tab, if any.
    pub fn active_tab_ref(&self) -> Option<&EditorTab> {
        self.active_tab.and_then(|i| self.tabs.get(i))
    }

    // ── Status message ────────────────────────────────────────────────────────

    /// Display a timed status message.
    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some((msg.into(), Instant::now()));
    }

    /// Expire status messages older than 3 seconds.
    pub fn expire_status(&mut self) {
        if let Some((_, ts)) = &self.status_message {
            if ts.elapsed().as_secs() >= 3 {
                self.status_message = None;
            }
        }
    }

    // ── Caret blink ───────────────────────────────────────────────────────────

    /// Advance caret blink state using current egui time.
    /// Returns `true` when a repaint is needed (caret toggled this call).
    pub fn tick_caret(&mut self, now_secs: f64) -> bool {
        if now_secs - self.last_blink_toggle >= 0.530 {
            self.caret_visible = !self.caret_visible;
            self.last_blink_toggle = now_secs;
            true
        } else {
            false
        }
    }
}
