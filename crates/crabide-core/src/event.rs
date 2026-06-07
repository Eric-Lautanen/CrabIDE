//! Typed event bus for background → UI communication.
//!
//! All background services (LSP, DAP, terminal, git, file watcher, extensions)
//! communicate with the UI exclusively through these typed channel events.
//!
//! The UI thread drains all channels via `try_recv()` loops each frame —
//! never blocking. Back-pressure is provided by `crossbeam_channel::bounded`.
//!
//! # Architecture
//!
//! ```text
//!  ┌──────────────────────────────────────────────────────────┐
//!  │ Background services (Tokio / Rayon)                       │
//!  │                                                           │
//!  │  LSP Client ──── LspEvent ────────────────────────────┐  │
//!  │  DAP Client ──── DapEvent ────────────────────────────┤  │
//!  │  Terminal   ──── TerminalEvent ───────────────────────┤  │
//!  │  Git        ──── GitEvent ─────────────────────────── ┤  │
//!  │  FileWatcher─── VfsEvent ─────────────────────────────┤  │
//!  │  Extensions ──── ExtensionEvent ──────────────────────┤  │
//!  └──────────────────────────────────────────────────────── ┤  │
//!                                               crossbeam │  │
//!                                              bounded    │  │
//!  ┌─────────────────────────────────────────────────────── ▼  │
//!  │ UI Thread (egui render loop, 60–144 Hz)                    │
//!  │   for event in rx.try_iter() { state.apply(event); }      │
//!  └────────────────────────────────────────────────────────────┘
//! ```

use crate::types::{DocumentUri, ExtensionId, Language, Position, Range};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

// ── LSP Events ───────────────────────────────────────────────────────────────

/// Events sent from the LSP client to the UI.
#[derive(Debug, Clone)]
pub enum LspEvent {
    /// Server has started and is ready to accept requests.
    ServerReady { language: Language },

    /// Server exited unexpectedly.
    ServerCrashed {
        language: Language,
        code: Option<i32>,
    },

    /// Diagnostic list for a document updated.
    DiagnosticsPublished {
        uri: DocumentUri,
        diagnostics: Vec<Diagnostic>,
    },

    /// Completion items ready (in response to a completion request).
    CompletionReady {
        request_id: u32,
        items: Vec<CompletionItem>,
        is_incomplete: bool,
    },

    /// Hover documentation ready.
    HoverReady {
        request_id: u32,
        contents: Option<String>,
        range: Option<Range>,
    },

    /// Inlay hints updated for a document region.
    InlayHintsUpdated {
        uri: DocumentUri,
        hints: Vec<InlayHint>,
    },

    /// Semantic token highlights for a document.
    SemanticTokensUpdated {
        uri: DocumentUri,
        tokens: Vec<SemanticToken>,
    },

    /// Code lens items for a document.
    CodeLensUpdated {
        uri: DocumentUri,
        items: Vec<CodeLens>,
    },

    /// Go-to-definition / references result.
    LocationsReady {
        request_id: u32,
        locations: Vec<Location>,
    },

    /// Rename result (list of edits to apply).
    RenameReady {
        request_id: u32,
        workspace_edit: WorkspaceEdit,
    },

    /// Formatting result (list of edits to apply).
    FormattingReady {
        request_id: u32,
        workspace_edit: WorkspaceEdit,
    },

    /// Code actions available at a position.
    CodeActionsReady {
        request_id: u32,
        actions: Vec<CodeAction>,
    },

    /// Signature help result (in response to textDocument/signatureHelp).
    SignatureHelpReady {
        request_id: u32,
        signature_help: Option<SignatureHelp>,
    },

    /// Log message from the language server (shows in Output panel).
    LogMessage { language: Language, message: String },
}

// ── DAP Events ───────────────────────────────────────────────────────────────

/// Events sent from the DAP client to the UI.
#[derive(Debug, Clone)]
pub enum DapEvent {
    /// Debugger has started and is initialised.
    Initialized,

    /// Program execution has stopped at a breakpoint / step / exception.
    Stopped {
        reason: StopReason,
        thread_id: Option<u64>,
        hit_breakpoint_ids: Vec<u64>,
        description: Option<String>,
    },

    /// Program execution has continued.
    Continued { thread_id: Option<u64> },

    /// Debug session has terminated.
    Terminated,

    /// Breakpoint status updated (e.g. verified / unverified after source mapping).
    BreakpointUpdated { breakpoint: BreakpointState },

    /// Stack frames ready (in response to stackTrace request).
    StackTraceReady {
        request_id: u32,
        frames: Vec<StackFrame>,
        total_frames: Option<u64>,
    },

    /// Variables ready (in response to variables request).
    VariablesReady {
        request_id: u32,
        variables: Vec<Variable>,
    },

    /// Output from the debuggee or debug adapter.
    Output {
        category: OutputCategory,
        output: String,
    },

    /// An error from the debug adapter.
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopReason {
    Breakpoint,
    Step,
    Exception,
    Pause,
    Entry,
    Goto,
    FunctionBreakpoint,
    DataBreakpoint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputCategory {
    Console,
    Stdout,
    Stderr,
    Telemetry,
    Important,
}

// ── Terminal Events ───────────────────────────────────────────────────────────

/// Events sent from the terminal subsystem to the UI.
#[derive(Debug, Clone)]
pub enum TerminalEvent {
    /// Raw bytes from the PTY (already VT-parsed into grid delta by terminal crate).
    /// The UI applies this delta to its copy of the terminal grid.
    Output {
        terminal_id: u32,
        delta: TerminalGridDelta,
    },

    /// Terminal title changed (e.g. via OSC 2 escape sequence).
    TitleChanged { terminal_id: u32, title: String },

    /// Terminal CWD changed (via shell integration OSC sequence).
    CwdChanged { terminal_id: u32, cwd: PathBuf },

    /// The shell command inside the terminal has started.
    CommandStarted { terminal_id: u32, command: String },

    /// The shell command inside the terminal has finished.
    CommandFinished { terminal_id: u32, exit_code: i32 },

    /// Terminal process exited.
    Exited { terminal_id: u32, code: Option<i32> },

    /// A clickable link was detected in the terminal output.
    LinkDetected {
        terminal_id: u32,
        range: TerminalRange,
        url: String,
    },
}

/// A minimal grid delta — only changed cells.
/// The UI merges this into its full grid state.
#[derive(Debug, Clone)]
pub struct TerminalGridDelta {
    pub rows: Vec<ChangedRow>,
    pub cursor_col: u16,
    pub cursor_row: u16,
    pub scroll_top: u32,
}

#[derive(Debug, Clone)]
pub struct ChangedRow {
    pub row: u16,
    pub cells: Vec<TerminalCell>,
}

#[derive(Debug, Clone, Copy)]
pub struct TerminalCell {
    pub ch: char,
    pub fg: TerminalColor,
    pub bg: TerminalColor,
    pub attrs: CellAttrs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalColor {
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct CellAttrs: u8 {
        const BOLD      = 0b0000_0001;
        const ITALIC    = 0b0000_0010;
        const UNDERLINE = 0b0000_0100;
        const BLINK     = 0b0000_1000;
        const REVERSE   = 0b0001_0000;
        const STRIKEOUT = 0b0010_0000;
        const DIM       = 0b0100_0000;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TerminalRange {
    pub start_row: u16,
    pub start_col: u16,
    pub end_row: u16,
    pub end_col: u16,
}

// ── Git Events ────────────────────────────────────────────────────────────────

/// Events from the git subsystem.
#[derive(Debug, Clone)]
pub enum GitEvent {
    /// Repository status (file tree decorations) refreshed.
    StatusRefreshed { statuses: Vec<FileStatus> },

    /// Diff hunks for a specific file updated (for gutter markers).
    DiffHunksUpdated {
        uri: DocumentUri,
        hunks: Vec<DiffHunk>,
    },

    /// Inline blame annotations for a file updated.
    BlameUpdated {
        uri: DocumentUri,
        lines: Vec<BlameLine>,
    },

    /// HEAD ref changed (branch switch, commit, etc.).
    HeadChanged {
        branch: Option<String>,
        commit: String,
    },

    /// An operation (commit, stage, etc.) completed.
    OperationCompleted { operation: String },

    /// An operation failed.
    OperationFailed { operation: String, error: String },
}

// ── VFS / File Events ─────────────────────────────────────────────────────────

/// Events from the virtual filesystem and file watcher.
#[derive(Debug, Clone)]
pub enum VfsEvent {
    FileCreated(PathBuf),
    FileModified(PathBuf),
    FileDeleted(PathBuf),
    FileRenamed { from: PathBuf, to: PathBuf },
    WatchError(String),
}

// ── Extension Events ──────────────────────────────────────────────────────────

/// Events from the extension host.
#[derive(Debug, Clone)]
pub enum ExtensionEvent {
    /// Extension loaded successfully.
    Loaded(ExtensionId),

    /// Extension failed to load.
    LoadFailed { id: ExtensionId, error: String },

    /// Extension registered a new command.
    CommandRegistered { id: ExtensionId, command: String },

    /// Extension wants to show a message in the status bar.
    StatusBarUpdated {
        id: ExtensionId,
        text: String,
        tooltip: Option<String>,
    },

    /// Extension published diagnostics.
    DiagnosticsPublished {
        id: ExtensionId,
        uri: DocumentUri,
        diagnostics: Vec<Diagnostic>,
    },

    /// Extension crashed.
    Crashed { id: ExtensionId, error: String },
}

// ── Shared LSP-compatible types ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: DiagnosticSeverity,
    pub code: Option<String>,
    pub source: Option<String>,
    pub message: String,
    pub related_information: Vec<DiagnosticRelated>,
    pub tags: Vec<DiagnosticTag>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticTag {
    Unnecessary,
    Deprecated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticRelated {
    pub location: Location,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub uri: DocumentUri,
    pub range: Range,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionItem {
    pub label: String,
    pub kind: Option<CompletionKind>,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub insert_text: Option<String>,
    pub sort_text: Option<String>,
    pub filter_text: Option<String>,
    pub preselect: bool,
    pub deprecated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompletionKind {
    Text,
    Method,
    Function,
    Constructor,
    Field,
    Variable,
    Class,
    Interface,
    Module,
    Property,
    Unit,
    Value,
    Enum,
    Keyword,
    Snippet,
    Color,
    File,
    Reference,
    Folder,
    EnumMember,
    Constant,
    Struct,
    Event,
    Operator,
    TypeParameter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlayHint {
    pub position: Position,
    pub label: String,
    pub kind: Option<InlayHintKind>,
    pub tooltip: Option<String>,
    pub padding_left: bool,
    pub padding_right: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InlayHintKind {
    Type,
    Parameter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticToken {
    pub range: Range,
    pub token_type: u32,
    pub token_modifiers: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeLens {
    pub range: Range,
    pub title: String,
    pub command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeAction {
    pub title: String,
    pub kind: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
    pub edit: Option<WorkspaceEdit>,
    pub command: Option<String>,
    pub is_preferred: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEdit {
    pub document_changes: Vec<DocumentEdit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentEdit {
    pub uri: DocumentUri,
    pub edits: Vec<crate::types::TextEdit>,
}

// Additional LSP shared types

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub range: Range,
    pub selection_range: Range,
    pub children: Vec<DocumentSymbol>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolKind {
    File,
    Module,
    Namespace,
    Package,
    Class,
    Method,
    Property,
    Field,
    Constructor,
    Enum,
    Interface,
    Function,
    Variable,
    Constant,
    String,
    Number,
    Boolean,
    Array,
    Object,
    Key,
    Null,
    EnumMember,
    Struct,
    Event,
    Operator,
    TypeParameter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureHelp {
    pub signatures: Vec<SignatureInformation>,
    pub active_signature: Option<u32>,
    pub active_parameter: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureInformation {
    pub label: String,
    pub documentation: Option<String>,
    pub parameters: Vec<ParameterInformation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterInformation {
    pub label: ParameterLabel,
    pub documentation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParameterLabel {
    Simple(String),
    Offsets(u32, u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoldingRange {
    pub start_line: u32,
    pub start_character: Option<u32>,
    pub end_line: u32,
    pub end_character: Option<u32>,
    pub kind: Option<FoldingRangeKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FoldingRangeKind {
    Comment,
    Imports,
    Region,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionRange {
    pub range: Range,
    pub parent: Option<Box<SelectionRange>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineCompletionItem {
    pub insert_text: String,
    pub range: Option<Range>,
    pub command: Option<String>,
}

// DAP types

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakpointState {
    pub id: Option<u64>,
    pub verified: bool,
    pub message: Option<String>,
    pub source_path: Option<PathBuf>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackFrame {
    pub id: u64,
    pub name: String,
    pub source_path: Option<PathBuf>,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variable {
    pub name: String,
    pub value: String,
    pub type_name: Option<String>,
    pub variables_reference: u64,
    pub named_variables: Option<u64>,
    pub indexed_variables: Option<u64>,
}

// ── Git types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStatus {
    pub path: PathBuf,
    pub index_status: StatusKind,
    pub worktree_status: StatusKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatusKind {
    Untracked,
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Unmodified,
    Conflicted,
    Ignored,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub kind: HunkKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HunkKind {
    Added,
    Removed,
    Modified,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlameLine {
    pub line: u32,
    pub commit_hash: String,
    pub author: String,
    pub author_email: String,
    pub commit_time: i64, // Unix timestamp
    pub summary: String,
}

// ── Top-level event enum ─────────────────────────────────────────────────────

/// All events that can be sent from background services to the UI.
/// Used when a single fan-in channel is preferred over per-service channels.
#[derive(Debug, Clone)]
pub enum EditorEvent {
    Lsp(LspEvent),
    Dap(DapEvent),
    Terminal(TerminalEvent),
    Git(GitEvent),
    Vfs(VfsEvent),
    Extension(ExtensionEvent),
}

impl From<LspEvent> for EditorEvent {
    fn from(e: LspEvent) -> Self {
        EditorEvent::Lsp(e)
    }
}
impl From<DapEvent> for EditorEvent {
    fn from(e: DapEvent) -> Self {
        EditorEvent::Dap(e)
    }
}
impl From<TerminalEvent> for EditorEvent {
    fn from(e: TerminalEvent) -> Self {
        EditorEvent::Terminal(e)
    }
}
impl From<GitEvent> for EditorEvent {
    fn from(e: GitEvent) -> Self {
        EditorEvent::Git(e)
    }
}
impl From<VfsEvent> for EditorEvent {
    fn from(e: VfsEvent) -> Self {
        EditorEvent::Vfs(e)
    }
}
impl From<ExtensionEvent> for EditorEvent {
    fn from(e: ExtensionEvent) -> Self {
        EditorEvent::Extension(e)
    }
}

// Display impls for debug logging

impl fmt::Display for LspEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LspEvent::ServerReady { language } => write!(f, "LSP ready: {language}"),
            LspEvent::ServerCrashed { language, code } => {
                write!(f, "LSP crashed: {language} (code={code:?})")
            }
            LspEvent::DiagnosticsPublished { uri, diagnostics } => {
                write!(f, "diagnostics: {} ({} items)", uri, diagnostics.len())
            }
            LspEvent::CompletionReady {
                request_id,
                items,
                is_incomplete,
            } => write!(
                f,
                "completion #{request_id}: {} items{}",
                items.len(),
                if *is_incomplete { " (incomplete)" } else { "" }
            ),
            LspEvent::HoverReady {
                request_id,
                contents,
                ..
            } => {
                write!(
                    f,
                    "hover #{request_id}: {}",
                    if contents.is_some() { "ok" } else { "empty" }
                )
            }
            LspEvent::InlayHintsUpdated { uri, hints } => {
                write!(f, "inlay hints: {} ({} hints)", uri, hints.len())
            }
            LspEvent::SemanticTokensUpdated { uri, tokens } => {
                write!(f, "semantic tokens: {} ({} tokens)", uri, tokens.len())
            }
            LspEvent::CodeLensUpdated { uri, items } => {
                write!(f, "code lens: {} ({} items)", uri, items.len())
            }
            LspEvent::LocationsReady {
                request_id,
                locations,
            } => {
                write!(f, "locations #{request_id}: {} results", locations.len())
            }
            LspEvent::RenameReady { request_id, .. } => write!(f, "rename #{request_id}"),
            LspEvent::FormattingReady { request_id, .. } => write!(f, "formatting #{request_id}"),
            LspEvent::CodeActionsReady {
                request_id,
                actions,
            } => {
                write!(f, "code actions #{request_id}: {} actions", actions.len())
            }
            LspEvent::LogMessage { language, .. } => write!(f, "LSP log: {language}"),
            LspEvent::SignatureHelpReady {
                request_id,
                signature_help,
            } => {
                write!(
                    f,
                    "signature help #{request_id}: {}",
                    if signature_help.is_some() {
                        "ok"
                    } else {
                        "empty"
                    }
                )
            }
        }
    }
}

impl fmt::Display for DapEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DapEvent::Initialized => write!(f, "DAP initialized"),
            DapEvent::Stopped {
                reason, thread_id, ..
            } => {
                write!(f, "DAP stopped: {reason:?} (thread={thread_id:?})")
            }
            DapEvent::Continued { thread_id } => {
                write!(f, "DAP continued (thread={thread_id:?})")
            }
            DapEvent::Terminated => write!(f, "DAP terminated"),
            DapEvent::BreakpointUpdated { .. } => write!(f, "DAP breakpoint updated"),
            DapEvent::StackTraceReady {
                request_id, frames, ..
            } => {
                write!(f, "stack trace #{request_id}: {} frames", frames.len())
            }
            DapEvent::VariablesReady {
                request_id,
                variables,
            } => {
                write!(f, "variables #{request_id}: {} vars", variables.len())
            }
            DapEvent::Output { category, .. } => write!(f, "DAP output: {category:?}"),
            DapEvent::Error { message } => write!(f, "DAP error: {message}"),
        }
    }
}

impl fmt::Display for TerminalEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TerminalEvent::Output { terminal_id, .. } => {
                write!(f, "terminal #{terminal_id} output")
            }
            TerminalEvent::TitleChanged { terminal_id, title } => {
                write!(f, "terminal #{terminal_id} title: {title}")
            }
            TerminalEvent::CwdChanged { terminal_id, cwd } => {
                write!(f, "terminal #{terminal_id} cwd: {}", cwd.display())
            }
            TerminalEvent::CommandStarted {
                terminal_id,
                command,
            } => {
                write!(f, "terminal #{terminal_id} started: {command}")
            }
            TerminalEvent::CommandFinished {
                terminal_id,
                exit_code,
            } => {
                write!(f, "terminal #{terminal_id} exited: {exit_code}")
            }
            TerminalEvent::Exited { terminal_id, code } => {
                write!(f, "terminal #{terminal_id} exited (code={code:?})")
            }
            TerminalEvent::LinkDetected {
                terminal_id, url, ..
            } => {
                write!(f, "terminal #{terminal_id} link: {url}")
            }
        }
    }
}

impl fmt::Display for GitEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitEvent::StatusRefreshed { statuses } => {
                write!(f, "git status: {} files", statuses.len())
            }
            GitEvent::DiffHunksUpdated { uri, hunks } => {
                write!(f, "git diff: {} ({} hunks)", uri, hunks.len())
            }
            GitEvent::BlameUpdated { uri, lines } => {
                write!(f, "git blame: {} ({} lines)", uri, lines.len())
            }
            GitEvent::HeadChanged { branch, commit } => {
                write!(
                    f,
                    "git HEAD: {} ({})",
                    branch.as_deref().unwrap_or("detached"),
                    commit
                )
            }
            GitEvent::OperationCompleted { operation } => {
                write!(f, "git {operation} completed")
            }
            GitEvent::OperationFailed { operation, error } => {
                write!(f, "git {operation} failed: {error}")
            }
        }
    }
}

impl fmt::Display for VfsEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VfsEvent::FileCreated(p) => write!(f, "vfs created: {}", p.display()),
            VfsEvent::FileModified(p) => write!(f, "vfs modified: {}", p.display()),
            VfsEvent::FileDeleted(p) => write!(f, "vfs deleted: {}", p.display()),
            VfsEvent::FileRenamed { from, to } => {
                write!(f, "vfs renamed: {} -> {}", from.display(), to.display())
            }
            VfsEvent::WatchError(e) => write!(f, "vfs watch error: {e}"),
        }
    }
}

impl fmt::Display for ExtensionEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExtensionEvent::Loaded(id) => write!(f, "extension loaded: {id}"),
            ExtensionEvent::LoadFailed { id, error } => {
                write!(f, "extension load failed: {id}: {error}")
            }
            ExtensionEvent::CommandRegistered { id, command } => {
                write!(f, "extension command: {id}/{command}")
            }
            ExtensionEvent::StatusBarUpdated { id, .. } => {
                write!(f, "extension status bar: {id}")
            }
            ExtensionEvent::DiagnosticsPublished {
                id,
                uri,
                diagnostics,
            } => {
                write!(
                    f,
                    "extension diagnostics: {id} {} ({} items)",
                    uri,
                    diagnostics.len()
                )
            }
            ExtensionEvent::Crashed { id, error } => {
                write!(f, "extension crashed: {id}: {error}")
            }
        }
    }
}

impl fmt::Display for EditorEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditorEvent::Lsp(e) => e.fmt(f),
            EditorEvent::Dap(e) => e.fmt(f),
            EditorEvent::Terminal(e) => e.fmt(f),
            EditorEvent::Git(e) => e.fmt(f),
            EditorEvent::Vfs(e) => e.fmt(f),
            EditorEvent::Extension(e) => e.fmt(f),
        }
    }
}
