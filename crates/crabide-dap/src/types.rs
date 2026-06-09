//! In-house DAP (Debug Adapter Protocol) type definitions.
//!
//! Covers all request/response/event structs needed to drive a compliant DAP
//! adapter.  Types follow the official DAP specification.
//! <https://microsoft.github.io/debug-adapter-protocol/specification>

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ── Wire envelope ─────────────────────────────────────────────────────────────

/// The DAP wire envelope.  All requests, responses, and events share this
/// struct; unused fields are omitted when serializing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DapMessage {
    /// Monotonically increasing sequence number.
    pub seq: u32,
    /// "request", "response", or "event".
    #[serde(rename = "type")]
    pub msg_type: String,

    // ── request fields ────────────────────────────────────────────────────────
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,

    // ── response fields ───────────────────────────────────────────────────────
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_seq: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
    /// Error message from the adapter (present when `success == false`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    // ── event fields ──────────────────────────────────────────────────────────
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
}

impl DapMessage {
    pub fn request(seq: u32, command: &str, arguments: serde_json::Value) -> Self {
        Self {
            seq,
            msg_type: "request".into(),
            command: Some(command.into()),
            arguments: Some(arguments),
            request_seq: None,
            success: None,
            body: None,
            message: None,
            event: None,
        }
    }

    pub fn is_response(&self) -> bool {
        self.msg_type == "response"
    }
    pub fn is_event(&self) -> bool {
        self.msg_type == "event"
    }
}

// ── initialize ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeRequestArguments {
    pub client_id: String,
    pub client_name: String,
    pub adapter_id: String,
    pub locale: String,
    pub lines_start_at1: bool,
    pub columns_start_at1: bool,
    pub path_format: String,
    pub supports_variable_type: bool,
    pub supports_run_in_terminal_request: bool,
}

impl Default for InitializeRequestArguments {
    fn default() -> Self {
        Self {
            client_id: "crabide".into(),
            client_name: "crabide Editor".into(),
            adapter_id: "crabide".into(),
            locale: "en-US".into(),
            lines_start_at1: true,
            columns_start_at1: true,
            path_format: "path".into(),
            supports_variable_type: true,
            supports_run_in_terminal_request: false,
        }
    }
}

// ── launch / attach ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchRequestArguments {
    /// Whether the adapter should pause at program entry.
    #[serde(default)]
    pub stop_on_entry: bool,
    /// Path to the executable/script being debugged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    /// Command-line arguments for the debuggee.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Working directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Environment variables.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
    /// Pass-through extras (adapter-specific fields).
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ── setBreakpoints ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceBreakpoint {
    /// 1-based line number.
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hit_condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetBreakpointsArguments {
    pub source: Source,
    #[serde(default)]
    pub breakpoints: Vec<SourceBreakpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetBreakpointsResponse {
    pub breakpoints: Vec<Breakpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Breakpoint {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    pub verified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_reference: Option<u64>,
}

impl Source {
    pub fn from_path(path: &Path) -> Self {
        Self {
            name: path.file_name().map(|n| n.to_string_lossy().into_owned()),
            path: Some(path.display().to_string()),
            source_reference: None,
        }
    }
}

// ── continue / next / stepIn / stepOut / pause ───────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadArguments {
    pub thread_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepArguments {
    pub thread_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub granularity: Option<String>,
}

// ── stackTrace ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StackTraceArguments {
    pub thread_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_frame: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub levels: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StackTraceResponse {
    pub stack_frames: Vec<StackFrameInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_frames: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StackFrameInfo {
    pub id: u64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    pub line: u32,
    pub column: u32,
}

// ── scopes / variables ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopesArguments {
    pub frame_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopesResponse {
    pub scopes: Vec<Scope>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    pub name: String,
    pub variables_reference: u64,
    pub expensive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub named_variables: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexed_variables: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariablesArguments {
    pub variables_reference: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariablesResponse {
    pub variables: Vec<VariableInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableInfo {
    pub name: String,
    pub value: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    pub variables_reference: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub named_variables: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexed_variables: Option<u64>,
}

// ── disconnect / terminate ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisconnectArguments {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminate_debuggee: Option<bool>,
}

// ── DAP event bodies ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoppedEventBody {
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<u64>,
    #[serde(default)]
    pub all_threads_stopped: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hit_breakpoint_ids: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContinuedEventBody {
    pub thread_id: u64,
    #[serde(default)]
    pub all_threads_continued: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputEventBody {
    pub category: Option<String>,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BreakpointEventBody {
    pub reason: String,
    pub breakpoint: Breakpoint,
}

// ── attach ────────────────────────────────────────────────────────────────────

/// Arguments for the `attach` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachRequestArguments {
    /// Whether the adapter should pause at program entry.
    #[serde(default)]
    pub stop_on_entry: bool,
    /// Path to the program being debugged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    /// Process ID to attach to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_id: Option<u64>,
    /// Working directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Environment variables.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
    /// Pass-through extras (adapter-specific fields).
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ── evaluate ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateArguments {
    /// The expression to evaluate.
    pub expression: String,
    /// The frame context (repl, hover, watch, clipboard).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// The stack frame ID for evaluation context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateResponse {
    pub result: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    pub variables_reference: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub named_variables: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexed_variables: Option<u64>,
}

// ── threads ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadsResponse {
    pub threads: Vec<ThreadInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadInfo {
    pub id: u64,
    pub name: String,
}

// ── setVariable ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetVariableArguments {
    /// The variable reference (from a Variable).
    pub variables_reference: u64,
    /// The name of the variable.
    pub name: String,
    /// The new value (as a string).
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetVariableResponse {
    /// The new value string.
    pub value: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables_reference: Option<u64>,
}

// ── setVariable ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetExpressionArguments {
    /// The expression to assign to.
    pub expression: String,
    /// The new value (as a string).
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetExpressionResponse {
    pub value: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables_reference: Option<u64>,
}

// ── setFunctionBreakpoints ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionBreakpoint {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hit_condition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetFunctionBreakpointsArguments {
    pub breakpoints: Vec<FunctionBreakpoint>,
}

// ── setExceptionBreakpoints ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetExceptionBreakpointsArguments {
    /// Filter options for exception breakpoints.
    pub filters: Vec<String>,
    /// Exception options (for more detailed configuration).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exception_options: Vec<ExceptionOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionOption {
    /// The path to the exception type.
    pub path: Vec<String>,
    /// The break mode: "never", "always", "unhandled", "userHandled".
    pub break_mode: String,
}

// ── exceptionInfo ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionInfoArguments {
    pub thread_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionInfoResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exception_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub break_mode: Option<String>,
    /// The structured exception details.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<ExceptionDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionDetail {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_trace: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inner_exception: Vec<ExceptionDetail>,
}

// ── gotoTargets / goto ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GotoTargetsArguments {
    pub source: Source,
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GotoTargetsResponse {
    pub targets: Vec<GotoTargetInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GotoTargetInfo {
    pub id: u64,
    pub label: String,
    pub line: u32,
    pub column: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GotoArguments {
    pub thread_id: u64,
    pub target_id: u64,
}

// ── Modules ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModulesArguments {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_module: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModulesResponse {
    pub modules: Vec<ModuleInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_modules: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleInfo {
    pub id: u64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default)]
    pub is_optimized: bool,
    #[serde(default)]
    pub is_user_code: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_status: Option<String>,
}

// ── runInTerminal (reverse request) ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunInTerminalArguments {
    /// What kind of terminal to use.
    #[serde(default)]
    pub kind: String,
    /// Title of the terminal.
    pub title: String,
    /// The command to run.
    pub args: Vec<String>,
    /// Working directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Environment variables.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
}

// ── Cancel ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelArguments {
    /// The request ID to cancel (from the client).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<u64>,
    /// A progress ID to cancel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_id: Option<String>,
}

// ── Completions ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionsArguments {
    /// The column for which to provide completions.
    pub column: u32,
    /// The line for which to provide completions.
    pub line: u32,
    /// The text in the current line up to the cursor position.
    pub text: String,
    /// The frame ID for which completions are requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionsResponse {
    pub targets: Vec<CompletionItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// "method", "function", "constructor", "field", "variable", "class",
    /// "interface", "module", "property", "unit", "value", "enum",
    /// "keyword", "snippet", "text", "color", "file", "reference", "custom".
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub sort_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_start: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_length: Option<u32>,
}

// ── Launch configuration ──────────────────────────────────────────────────────

/// A parsed debug launch configuration (from launch.json or built-in defaults).
#[derive(Debug, Clone)]
pub struct LaunchConfig {
    /// Display name for the picker, e.g. "Debug Program".
    pub name: String,
    /// "launch" or "attach".
    pub request: String,
    /// Path to the program to debug.
    pub program: Option<String>,
    /// Arguments passed to the program.
    pub args: Vec<String>,
    /// Working directory for the debuggee.
    pub cwd: Option<PathBuf>,
    /// Environment variables.
    pub env: HashMap<String, String>,
    /// Whether to pause at program entry.
    pub stop_on_entry: bool,
    /// Debug adapter executable (e.g. "codelldb", "python", "node").
    pub adapter_command: String,
    /// Additional arguments for the debug adapter itself.
    pub adapter_args: Vec<String>,
    /// Adapter type name for auto-resolution (e.g. "python", "lldb", "gdb").
    pub adapter_type: Option<String>,
    /// Port for debug adapter communication (used by some adapters).
    pub port: Option<u16>,
    /// Adapter-specific extra fields passed verbatim to the launch request.
    pub extra: HashMap<String, serde_json::Value>,
}

impl Default for LaunchConfig {
    fn default() -> Self {
        Self {
            name: "No launch configuration".into(),
            request: "launch".into(),
            program: None,
            args: Vec::new(),
            cwd: None,
            env: HashMap::new(),
            stop_on_entry: false,
            adapter_command: String::new(),
            adapter_args: Vec::new(),
            adapter_type: None,
            port: None,
            extra: HashMap::new(),
        }
    }
}

// ── launch.json parser ────────────────────────────────────────────────────────

/// Parse a VS Code / crabide `launch.json` file into a list of `LaunchConfig`.
pub fn parse_launch_json(json: &str) -> Vec<LaunchConfig> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(configurations) = value.get("configurations").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    configurations.iter().filter_map(parse_one_config).collect()
}

fn parse_one_config(v: &serde_json::Value) -> Option<LaunchConfig> {
    let obj = v.as_object()?;
    let name = obj.get("name")?.as_str()?.to_owned();
    let request = obj
        .get("request")
        .and_then(|r| r.as_str())
        .unwrap_or("launch")
        .to_owned();
    let program = obj
        .get("program")
        .and_then(|p| p.as_str())
        .map(str::to_owned);
    let args = obj
        .get("args")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let cwd = obj.get("cwd").and_then(|c| c.as_str()).map(PathBuf::from);
    let env: HashMap<String, String> = obj
        .get("env")
        .and_then(|e| e.as_object())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
                .collect()
        })
        .unwrap_or_default();
    let stop_on_entry = obj
        .get("stopOnEntry")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let adapter_command = obj
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_owned();

    // Collect extra fields (adapter-specific, passed through to launch request).
    let known = [
        "name",
        "request",
        "program",
        "args",
        "cwd",
        "env",
        "stopOnEntry",
        "type",
    ];
    let extra: HashMap<String, serde_json::Value> = obj
        .iter()
        .filter(|(k, _)| !known.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    Some(LaunchConfig {
        name,
        request,
        program,
        args,
        cwd,
        env,
        stop_on_entry,
        adapter_command,
        adapter_args: Vec::new(),
        adapter_type: obj.get("type").and_then(|v| v.as_str()).map(String::from),
        port: obj
            .get("port")
            .and_then(serde_json::Value::as_u64)
            .map(|p| p as u16),
        extra,
    })
}

/// Try to read `launch.json` from the given workspace root.
/// Checks `.crabide/launch.json` then `.vscode/launch.json`.
pub fn load_launch_configs(workspace_root: &std::path::Path) -> Vec<LaunchConfig> {
    let candidates = [
        workspace_root.join(".crabide").join("launch.json"),
        workspace_root.join(".vscode").join("launch.json"),
    ];
    for path in &candidates {
        if let Ok(content) = std::fs::read_to_string(path) {
            let configs = parse_launch_json(&content);
            if !configs.is_empty() {
                return configs;
            }
        }
    }
    Vec::new()
}
