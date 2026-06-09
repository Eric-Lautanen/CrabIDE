//! DAP client: manages the lifecycle of a debug adapter process and translates
//! DAP protocol messages to typed `DapEvent`s for the editor event bus.

use crossbeam_channel::Sender;
use serde_json::json;
use std::path::PathBuf;
use tokio::process::{Child, Command};

use crabide_core::event::{
    BreakpointState, DapEvent, DapThread, EditorEvent, GotoTarget, OutputCategory, StackFrame,
    StopReason, Variable,
};

use crate::types::{
    AttachRequestArguments, DisconnectArguments, EvaluateArguments, InitializeRequestArguments,
    LaunchConfig, LaunchRequestArguments, ScopesArguments, SetBreakpointsArguments,
    SetExceptionBreakpointsArguments, SetFunctionBreakpointsArguments, SetVariableArguments,
    Source, SourceBreakpoint, StackTraceArguments, VariablesArguments,
};

use crate::transport::DapTransport;

// ── DapClient ─────────────────────────────────────────────────────────────────

/// Handle to a running debug adapter process + the associated Tokio tasks.
///
/// The client runs its communication on the shared Tokio runtime.  Background
/// tasks send `DapEvent`s to the main editor event bus via the provided
/// `Sender<EditorEvent>`.
///
/// Drop the `DapClient` to shut down the session; the inner `Dropper` will
/// send `disconnect` to the adapter.
pub struct DapClient {
    transport: DapTransport,
    event_tx: Sender<EditorEvent>,
    /// The adapter child process (kept alive to prevent zombie).
    _process: Child,
    rt: tokio::runtime::Handle,
}

impl DapClient {
    /// Spawn a debug adapter and perform the DAP `initialize` handshake.
    ///
    /// Returns `None` if the adapter process fails to start or the handshake
    /// fails.  `adapter_command` is the executable path/name; `adapter_args`
    /// are its arguments (e.g. `["--interpreter=dap"]` for gdb).
    pub fn start(
        adapter_command: &str,
        adapter_args: &[String],
        event_tx: Sender<EditorEvent>,
        rt: tokio::runtime::Handle,
    ) -> Option<Self> {
        if adapter_command.is_empty() {
            log::warn!("DAP: adapter_command is empty — cannot start debug session");
            return None;
        }

        let mut cmd = Command::new(adapter_command);
        cmd.args(adapter_args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                log::error!("DAP: failed to spawn adapter {adapter_command:?}: {e}");
                return None;
            }
        };

        let stdin = child.stdin.take()?;
        let stdout = child.stdout.take()?;

        let (transport, mut in_rx) = DapTransport::spawn(stdin, stdout);

        // Spawn the event-dispatch task.
        let event_tx_task = event_tx.clone();
        let transport_clone = transport.clone();
        rt.spawn(async move {
            while let Some(msg) = in_rx.recv().await {
                if let Some(event_name) = &msg.event {
                    dispatch_event(event_name, msg.body, &event_tx_task);
                } else if msg.msg_type == "request" {
                    // Handle reverse-requests from the adapter.
                    handle_reverse_request(&msg, &transport_clone, &event_tx_task);
                }
            }
            log::info!("DAP: event dispatch task exited");
            let _ = event_tx_task.send(EditorEvent::Dap(DapEvent::Terminated));
        });

        Some(Self {
            transport,
            event_tx,
            _process: child,
            rt,
        })
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    /// Run the `initialize` + `configurationDone` handshake asynchronously and
    /// report `DapEvent::Initialized` on completion.
    pub fn initialize(&self) {
        let transport = self.transport.clone();
        let event_tx = self.event_tx.clone();
        self.rt.spawn(async move {
            let args = InitializeRequestArguments::default();
            let args_val = serde_json::to_value(args).unwrap_or(json!({}));
            match transport.request("initialize", args_val).await {
                Ok(Some(body)) => {
                    // Discard capabilities; we don't use them yet but log them.
                    if let Some(caps) = body.get("capabilities") {
                        log::debug!("DAP: adapter capabilities: {caps}");
                    }
                    log::info!("DAP: initialize OK");
                    let _ = event_tx.send(EditorEvent::Dap(DapEvent::Initialized));
                }
                Ok(None) => {
                    log::info!("DAP: initialize OK (no body)");
                    let _ = event_tx.send(EditorEvent::Dap(DapEvent::Initialized));
                }
                Err(e) => {
                    let _ = event_tx.send(EditorEvent::Dap(DapEvent::Error {
                        message: e.to_string(),
                    }));
                }
            }
        });
    }

    /// Send `launch` with the given configuration.
    pub fn launch(&self, config: &LaunchConfig) {
        let transport = self.transport.clone();
        let event_tx = self.event_tx.clone();
        let launch_args = LaunchRequestArguments {
            stop_on_entry: config.stop_on_entry,
            program: config.program.clone(),
            args: config.args.clone(),
            cwd: config.cwd.as_ref().map(|p| p.display().to_string()),
            env: config.env.clone(),
            extra: config.extra.clone(),
        };
        let args_val = match serde_json::to_value(&launch_args) {
            Ok(v) => v,
            Err(e) => {
                log::error!("DAP launch serialise: {e}");
                return;
            }
        };
        self.rt.spawn(async move {
            match transport.request("launch", args_val).await {
                Ok(_) => {
                    log::info!("DAP: launch OK");
                    // After successful launch send configurationDone.
                    let _ = transport.notify("configurationDone", json!({}));
                }
                Err(e) => {
                    let _ = event_tx.send(EditorEvent::Dap(DapEvent::Error {
                        message: e.to_string(),
                    }));
                }
            }
        });
    }

    // ── Attach ─────────────────────────────────────────────────────────────────

    /// Send `attach` with the given configuration (connect to a running process).
    pub fn attach(&self, config: &LaunchConfig) {
        let transport = self.transport.clone();
        let event_tx = self.event_tx.clone();
        let attach_args = AttachRequestArguments {
            stop_on_entry: config.stop_on_entry,
            program: config.program.clone(),
            process_id: config.port.map(u64::from),
            cwd: config.cwd.as_ref().map(|p| p.display().to_string()),
            env: config.env.clone(),
            extra: config.extra.clone(),
        };
        let args_val = match serde_json::to_value(&attach_args) {
            Ok(v) => v,
            Err(e) => {
                log::error!("DAP attach serialise: {e}");
                return;
            }
        };
        self.rt.spawn(async move {
            match transport.request("attach", args_val).await {
                Ok(_) => {
                    log::info!("DAP: attach OK");
                    // After successful attach send configurationDone.
                    let _ = transport.notify("configurationDone", json!({}));
                }
                Err(e) => {
                    let _ = event_tx.send(EditorEvent::Dap(DapEvent::Error {
                        message: e.to_string(),
                    }));
                }
            }
        });
    }

    // ── Breakpoints ───────────────────────────────────────────────────────────

    /// Set breakpoints for a file.  `lines` are 0-based; they are converted to
    /// 1-based before sending to the adapter.
    pub fn set_breakpoints(&self, path: PathBuf, lines: Vec<u32>) {
        let transport = self.transport.clone();
        let event_tx = self.event_tx.clone();
        self.rt.spawn(async move {
            let args = SetBreakpointsArguments {
                source: Source::from_path(&path),
                breakpoints: lines
                    .iter()
                    .map(|&l| SourceBreakpoint {
                        line: l + 1,
                        column: None,
                        condition: None,
                        hit_condition: None,
                        log_message: None,
                    })
                    .collect(),
            };
            let args_val = match serde_json::to_value(&args) {
                Ok(v) => v,
                Err(e) => {
                    log::error!("DAP setBreakpoints: {e}");
                    return;
                }
            };
            match transport.request("setBreakpoints", args_val).await {
                Ok(Some(body)) => {
                    if let Ok(resp) =
                        serde_json::from_value::<crate::types::SetBreakpointsResponse>(body)
                    {
                        for bp in resp.breakpoints {
                            let state = BreakpointState {
                                id: bp.id,
                                verified: bp.verified,
                                message: bp.message,
                                source_path: bp.source.and_then(|s| s.path).map(PathBuf::from),
                                line: bp.line,
                                column: bp.column,
                            };
                            let _ = event_tx.send(EditorEvent::Dap(DapEvent::BreakpointUpdated {
                                breakpoint: state,
                            }));
                        }
                    }
                }
                Err(e) => log::warn!("DAP setBreakpoints failed: {e}"),
                Ok(None) => {}
            }
        });
    }

    // ── Execution control ─────────────────────────────────────────────────────

    pub fn continue_(&self, thread_id: u64) {
        let transport = self.transport.clone();
        self.rt.spawn(async move {
            if let Err(e) = transport
                .request("continue", json!({ "threadId": thread_id }))
                .await
            {
                log::warn!("DAP continue: {e}");
            }
        });
    }

    pub fn step_over(&self, thread_id: u64) {
        let t = self.transport.clone();
        self.rt.spawn(async move {
            if let Err(e) = t.request("next", json!({ "threadId": thread_id })).await {
                log::warn!("DAP next: {e}");
            }
        });
    }

    pub fn step_in(&self, thread_id: u64) {
        let t = self.transport.clone();
        self.rt.spawn(async move {
            if let Err(e) = t.request("stepIn", json!({ "threadId": thread_id })).await {
                log::warn!("DAP stepIn: {e}");
            }
        });
    }

    pub fn step_out(&self, thread_id: u64) {
        let t = self.transport.clone();
        self.rt.spawn(async move {
            if let Err(e) = t.request("stepOut", json!({ "threadId": thread_id })).await {
                log::warn!("DAP stepOut: {e}");
            }
        });
    }

    pub fn pause(&self, thread_id: u64) {
        let t = self.transport.clone();
        self.rt.spawn(async move {
            if let Err(e) = t.request("pause", json!({ "threadId": thread_id })).await {
                log::warn!("DAP pause: {e}");
            }
        });
    }

    pub fn restart(&self) {
        let t = self.transport.clone();
        self.rt.spawn(async move {
            if let Err(e) = t.request("restart", json!({})).await {
                // Some adapters don't support restart; fall through silently.
                log::debug!("DAP restart: {e}");
            }
        });
    }

    pub fn stop(&self) {
        let args = DisconnectArguments {
            restart: None,
            terminate_debuggee: Some(true),
        };
        let args_val = serde_json::to_value(args).unwrap_or(json!({}));
        let t = self.transport.clone();
        self.rt.spawn(async move {
            if let Err(e) = t.request("disconnect", args_val).await {
                log::debug!("DAP disconnect: {e}");
            }
        });
    }

    // ── Stack trace ───────────────────────────────────────────────────────────

    /// Request the call stack for `thread_id`.  Result is delivered via
    /// `DapEvent::StackTraceReady`.
    pub fn request_stack_trace(&self, thread_id: u64) {
        let t = self.transport.clone();
        let event_tx = self.event_tx.clone();
        self.rt.spawn(async move {
            let args = StackTraceArguments {
                thread_id,
                start_frame: None,
                levels: Some(50),
            };
            let val = serde_json::to_value(args).unwrap_or(json!({}));
            match t.request("stackTrace", val).await {
                Ok(Some(body)) => {
                    if let Ok(resp) =
                        serde_json::from_value::<crate::types::StackTraceResponse>(body)
                    {
                        let frames: Vec<StackFrame> = resp
                            .stack_frames
                            .iter()
                            .map(|f| StackFrame {
                                id: f.id,
                                name: f.name.clone(),
                                source_path: f
                                    .source
                                    .as_ref()
                                    .and_then(|s| s.path.as_ref())
                                    .map(PathBuf::from),
                                line: f.line,
                                column: f.column,
                            })
                            .collect();
                        let total = resp.total_frames;
                        let _ = event_tx.send(EditorEvent::Dap(DapEvent::StackTraceReady {
                            request_id: 0,
                            frames,
                            total_frames: total,
                        }));
                    }
                }
                Err(e) => log::warn!("DAP stackTrace: {e}"),
                Ok(None) => {}
            }
        });
    }

    // ── Scopes + Variables ────────────────────────────────────────────────────

    /// Request scopes for a stack frame, then fetch variables for each scope.
    /// Results are delivered as `DapEvent::VariablesReady` (one per scope).
    pub fn request_variables(&self, frame_id: u64) {
        let t = self.transport.clone();
        let event_tx = self.event_tx.clone();
        self.rt.spawn(async move {
            let scopes_args = ScopesArguments { frame_id };
            let val = serde_json::to_value(scopes_args).unwrap_or(json!({}));
            let scopes = match t.request("scopes", val).await {
                Ok(Some(body)) => serde_json::from_value::<crate::types::ScopesResponse>(body)
                    .map(|r| r.scopes)
                    .unwrap_or_default(),
                _ => return,
            };
            for scope in scopes {
                let vars_args = VariablesArguments {
                    variables_reference: scope.variables_reference,
                    filter: None,
                    start: None,
                    count: None,
                };
                let val = serde_json::to_value(vars_args).unwrap_or(json!({}));
                match t.request("variables", val).await {
                    Ok(Some(body)) => {
                        if let Ok(resp) =
                            serde_json::from_value::<crate::types::VariablesResponse>(body)
                        {
                            let vars: Vec<Variable> = resp
                                .variables
                                .iter()
                                .map(|v| Variable {
                                    name: v.name.clone(),
                                    value: v.value.clone(),
                                    type_name: v.type_name.clone(),
                                    variables_reference: v.variables_reference,
                                    named_variables: v.named_variables,
                                    indexed_variables: v.indexed_variables,
                                })
                                .collect();
                            let _ = event_tx.send(EditorEvent::Dap(DapEvent::VariablesReady {
                                request_id: scope.variables_reference as u32,
                                variables: vars,
                            }));
                        }
                    }
                    Err(e) => log::warn!("DAP variables (ref {}): {e}", scope.variables_reference),
                    Ok(None) => {}
                }
            }
        });
    }

    /// Expand a specific `variables_reference` (e.g. a struct's children).
    pub fn expand_variable(&self, variables_reference: u64) {
        let t = self.transport.clone();
        let event_tx = self.event_tx.clone();
        self.rt.spawn(async move {
            let args = VariablesArguments {
                variables_reference,
                filter: None,
                start: None,
                count: None,
            };
            let val = serde_json::to_value(args).unwrap_or(json!({}));
            match t.request("variables", val).await {
                Ok(Some(body)) => {
                    if let Ok(resp) =
                        serde_json::from_value::<crate::types::VariablesResponse>(body)
                    {
                        let vars: Vec<Variable> = resp
                            .variables
                            .iter()
                            .map(|v| Variable {
                                name: v.name.clone(),
                                value: v.value.clone(),
                                type_name: v.type_name.clone(),
                                variables_reference: v.variables_reference,
                                named_variables: v.named_variables,
                                indexed_variables: v.indexed_variables,
                            })
                            .collect();
                        let _ = event_tx.send(EditorEvent::Dap(DapEvent::VariablesReady {
                            request_id: variables_reference as u32,
                            variables: vars,
                        }));
                    }
                }
                Err(e) => log::warn!("DAP expand_variable: {e}"),
                Ok(None) => {}
            }
        });
    }

    // ── Evaluate ────────────────────────────────────────────────────────────────

    /// Evaluate an expression in the context of a stack frame (debug console REPL).
    pub fn evaluate(&self, expression: String, frame_id: Option<u64>, context: Option<String>) {
        let t = self.transport.clone();
        let event_tx = self.event_tx.clone();
        self.rt.spawn(async move {
            let args = EvaluateArguments {
                expression,
                context,
                frame_id,
            };
            let val = serde_json::to_value(args).unwrap_or(json!({}));
            match t.request("evaluate", val).await {
                Ok(Some(body)) => {
                    if let Ok(resp) = serde_json::from_value::<crate::types::EvaluateResponse>(body)
                    {
                        let _ = event_tx.send(EditorEvent::Dap(DapEvent::EvaluateReady {
                            request_id: 0,
                            result: resp.result,
                            type_name: resp.type_name,
                            variables_reference: resp.variables_reference,
                            named_variables: resp.named_variables,
                            indexed_variables: resp.indexed_variables,
                        }));
                    }
                }
                Err(e) => log::warn!("DAP evaluate: {e}"),
                Ok(None) => {}
            }
        });
    }

    // ── Threads ─────────────────────────────────────────────────────────────────

    /// List all threads in the debuggee.
    pub fn request_threads(&self) {
        let t = self.transport.clone();
        let event_tx = self.event_tx.clone();
        self.rt.spawn(async move {
            match t.request("threads", json!({})).await {
                Ok(Some(body)) => {
                    if let Ok(resp) = serde_json::from_value::<crate::types::ThreadsResponse>(body)
                    {
                        let threads: Vec<DapThread> = resp
                            .threads
                            .into_iter()
                            .map(|th| DapThread {
                                id: th.id,
                                name: th.name,
                            })
                            .collect();
                        let _ = event_tx.send(EditorEvent::Dap(DapEvent::ThreadsReady { threads }));
                    }
                }
                Err(e) => log::warn!("DAP threads: {e}"),
                Ok(None) => {}
            }
        });
    }

    // ── Set variable ────────────────────────────────────────────────────────────

    /// Modify a variable's value.
    pub fn set_variable(&self, variables_reference: u64, name: String, value: String) {
        let t = self.transport.clone();
        let event_tx = self.event_tx.clone();
        self.rt.spawn(async move {
            let args = SetVariableArguments {
                variables_reference,
                name,
                value,
            };
            let val = serde_json::to_value(args).unwrap_or(json!({}));
            match t.request("setVariable", val).await {
                Ok(_) => {
                    let _ = event_tx.send(EditorEvent::Dap(DapEvent::SetVariableDone {
                        request_id: 0,
                        success: true,
                    }));
                }
                Err(e) => {
                    log::warn!("DAP setVariable: {e}");
                    let _ = event_tx.send(EditorEvent::Dap(DapEvent::SetVariableDone {
                        request_id: 0,
                        success: false,
                    }));
                }
            }
        });
    }

    // ── Function breakpoints ────────────────────────────────────────────────────

    /// Set function breakpoints.
    pub fn set_function_breakpoints(&self, breakpoints: Vec<String>) {
        let t = self.transport.clone();
        let event_tx = self.event_tx.clone();
        self.rt.spawn(async move {
            let fb: Vec<crate::types::FunctionBreakpoint> = breakpoints
                .into_iter()
                .map(|name| crate::types::FunctionBreakpoint {
                    name,
                    condition: None,
                    hit_condition: None,
                })
                .collect();
            let args = SetFunctionBreakpointsArguments { breakpoints: fb };
            let val = serde_json::to_value(args).unwrap_or(json!({}));
            match t.request("setFunctionBreakpoints", val).await {
                Ok(Some(body)) => {
                    if let Ok(resp) =
                        serde_json::from_value::<crate::types::SetBreakpointsResponse>(body)
                    {
                        let states: Vec<BreakpointState> = resp
                            .breakpoints
                            .into_iter()
                            .map(|bp| BreakpointState {
                                id: bp.id,
                                verified: bp.verified,
                                message: bp.message,
                                source_path: bp.source.and_then(|s| s.path).map(PathBuf::from),
                                line: bp.line,
                                column: bp.column,
                            })
                            .collect();
                        let _ =
                            event_tx.send(EditorEvent::Dap(DapEvent::FunctionBreakpointsReady {
                                breakpoints: states,
                            }));
                    }
                }
                Err(e) => log::warn!("DAP setFunctionBreakpoints: {e}"),
                Ok(None) => {}
            }
        });
    }

    // ── Exception breakpoints ──────────────────────────────────────────────────

    /// Set exception breakpoints (which exception types should break).
    pub fn set_exception_breakpoints(&self, filters: Vec<String>) {
        let t = self.transport.clone();
        let event_tx = self.event_tx.clone();
        self.rt.spawn(async move {
            let args = SetExceptionBreakpointsArguments {
                filters,
                exception_options: Vec::new(),
            };
            let val = serde_json::to_value(args).unwrap_or(json!({}));
            match t.request("setExceptionBreakpoints", val).await {
                Ok(_) => {
                    let _ = event_tx.send(EditorEvent::Dap(DapEvent::ExceptionBreakpointsSet));
                }
                Err(e) => log::warn!("DAP setExceptionBreakpoints: {e}"),
            }
        });
    }

    // ── Exception info ─────────────────────────────────────────────────────────

    /// Request exception info for a stopped thread.
    pub fn request_exception_info(&self, thread_id: u64) {
        let t = self.transport.clone();
        let event_tx = self.event_tx.clone();
        self.rt.spawn(async move {
            let args = crate::types::ExceptionInfoArguments { thread_id };
            let val = serde_json::to_value(args).unwrap_or(json!({}));
            match t.request("exceptionInfo", val).await {
                Ok(Some(body)) => {
                    if let Ok(resp) =
                        serde_json::from_value::<crate::types::ExceptionInfoResponse>(body)
                    {
                        let exception_id = resp.exception_id.clone();
                        let _ = event_tx.send(EditorEvent::Dap(DapEvent::ExceptionInfoReady {
                            request_id: 0,
                            description: resp.description.or(exception_id),
                            exception_type: resp.exception_id,
                            break_mode: resp.break_mode,
                        }));
                    }
                }
                Err(e) => log::warn!("DAP exceptionInfo: {e}"),
                Ok(None) => {}
            }
        });
    }

    // ── Goto targets / run to cursor ───────────────────────────────────────────

    /// Request goto targets for a given source location.
    pub fn request_goto_targets(&self, source_path: String, line: u32, column: Option<u32>) {
        let t = self.transport.clone();
        let event_tx = self.event_tx.clone();
        self.rt.spawn(async move {
            let source = Source {
                name: None,
                path: Some(source_path),
                source_reference: None,
            };
            let args = crate::types::GotoTargetsArguments {
                source,
                line,
                column,
            };
            let val = serde_json::to_value(args).unwrap_or(json!({}));
            match t.request("gotoTargets", val).await {
                Ok(Some(body)) => {
                    if let Ok(resp) =
                        serde_json::from_value::<crate::types::GotoTargetsResponse>(body)
                    {
                        let targets: Vec<GotoTarget> = resp
                            .targets
                            .into_iter()
                            .map(|gt| GotoTarget {
                                id: gt.id,
                                label: gt.label,
                                line: gt.line,
                                column: gt.column,
                                end_line: gt.end_line,
                                end_column: gt.end_column,
                            })
                            .collect();
                        let _ = event_tx.send(EditorEvent::Dap(DapEvent::GotoTargetsReady {
                            request_id: 0,
                            targets,
                        }));
                    }
                }
                Err(e) => log::warn!("DAP gotoTargets: {e}"),
                Ok(None) => {}
            }
        });
    }

    /// Execute a goto (run to cursor).
    pub fn goto(&self, thread_id: u64, target_id: u64) {
        let t = self.transport.clone();
        self.rt.spawn(async move {
            let args = crate::types::GotoArguments {
                thread_id,
                target_id,
            };
            let val = serde_json::to_value(args).unwrap_or(json!({}));
            if let Err(e) = t.request("goto", val).await {
                log::warn!("DAP goto: {e}");
            }
        });
    }

    // ── Modules ─────────────────────────────────────────────────────────────────

    /// Request loaded modules.
    pub fn request_modules(&self) {
        let t = self.transport.clone();
        let event_tx = self.event_tx.clone();
        self.rt.spawn(async move {
            let args = crate::types::ModulesArguments {
                start_module: None,
                module_count: None,
            };
            let val = serde_json::to_value(args).unwrap_or(json!({}));
            match t.request("modules", val).await {
                Ok(Some(body)) => {
                    if let Ok(resp) = serde_json::from_value::<crate::types::ModulesResponse>(body)
                    {
                        let modules: Vec<crabide_core::event::DapModule> = resp
                            .modules
                            .into_iter()
                            .map(|m| crabide_core::event::DapModule {
                                id: m.id,
                                name: m.name,
                                path: m.path,
                                is_optimized: m.is_optimized,
                                is_user_code: m.is_user_code,
                                version: m.version,
                                symbol_status: m.symbol_status,
                            })
                            .collect();
                        let _ = event_tx.send(EditorEvent::Dap(DapEvent::ModulesReady { modules }));
                    }
                }
                Err(e) => log::warn!("DAP modules: {e}"),
                Ok(None) => {}
            }
        });
    }
}

// ── Shutdown ────────────────────────────────────────────────────────────

impl DapClient {
    /// Gracefully shut down the debug adapter: send `disconnect` and wait for
    /// the process to exit.  If the adapter doesn't respond within 3 seconds,
    /// the child will be killed on drop (due to `kill_on_drop(true)`).
    pub async fn shutdown(&self) {
        let args = DisconnectArguments {
            restart: None,
            terminate_debuggee: Some(true),
        };
        let args_val = serde_json::to_value(args).unwrap_or(json!({}));
        let t = self.transport.clone();
        let _ = t.request("disconnect", args_val).await;
        // The adapter should exit after disconnect; kill_on_drop handles
        // forceful termination if it doesn't.
    }
}

// SAFETY: `DapClient` only owns a `Child` handle (killed on drop), a transport
// (which is `Send + Sync`), a `Sender`, and a `Handle` — all `Send + Sync`.
unsafe impl Send for DapClient {}
unsafe impl Sync for DapClient {}

// ── Event dispatch helper ─────────────────────────────────────────────────────

fn dispatch_event(
    event_name: &str,
    body: Option<serde_json::Value>,
    event_tx: &Sender<EditorEvent>,
) {
    use crate::types::{
        BreakpointEventBody, ContinuedEventBody, OutputEventBody, StoppedEventBody,
    };

    match event_name {
        "initialized" => {
            let _ = event_tx.send(EditorEvent::Dap(DapEvent::Initialized));
        }

        "stopped" => {
            if let Some(Ok(body)) = body.map(serde_json::from_value::<StoppedEventBody>) {
                let reason = parse_stop_reason(&body.reason);
                let _ = event_tx.send(EditorEvent::Dap(DapEvent::Stopped {
                    reason,
                    thread_id: body.thread_id,
                    hit_breakpoint_ids: body.hit_breakpoint_ids,
                    description: body.description,
                }));
            }
        }

        "continued" => {
            if let Some(Ok(body)) = body.map(serde_json::from_value::<ContinuedEventBody>) {
                let _ = event_tx.send(EditorEvent::Dap(DapEvent::Continued {
                    thread_id: Some(body.thread_id),
                }));
            }
        }

        "terminated" | "exited" => {
            let _ = event_tx.send(EditorEvent::Dap(DapEvent::Terminated));
        }

        "output" => {
            if let Some(Ok(body)) = body.map(serde_json::from_value::<OutputEventBody>) {
                let category = match body.category.as_deref() {
                    Some("stdout") => OutputCategory::Stdout,
                    Some("stderr") => OutputCategory::Stderr,
                    Some("important") => OutputCategory::Important,
                    Some("telemetry") => OutputCategory::Telemetry,
                    _ => OutputCategory::Console,
                };
                let _ = event_tx.send(EditorEvent::Dap(DapEvent::Output {
                    category,
                    output: body.output,
                }));
            }
        }

        "breakpoint" => {
            if let Some(Ok(body)) = body.map(serde_json::from_value::<BreakpointEventBody>) {
                let bp = &body.breakpoint;
                let state = BreakpointState {
                    id: bp.id,
                    verified: bp.verified,
                    message: bp.message.clone(),
                    source_path: bp
                        .source
                        .as_ref()
                        .and_then(|s| s.path.as_ref())
                        .map(PathBuf::from),
                    line: bp.line,
                    column: bp.column,
                };
                let _ = event_tx.send(EditorEvent::Dap(DapEvent::BreakpointUpdated {
                    breakpoint: state,
                }));
            }
        }

        "module" => {
            if let Some(Some(body)) = body.as_ref().map(|b| b.get("module")) {
                let mid = body
                    .get("id")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let name = body
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();
                log::debug!("DAP module event: {name} (id={mid})");
            }
        }

        "progressStart" => {
            if let Some(body) = body {
                let progress_id = body
                    .get("progressId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();
                let title = body
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();
                let message = body
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
                let percentage = body.get("percentage").and_then(serde_json::Value::as_f64);
                let _ = event_tx.send(EditorEvent::Dap(DapEvent::ProgressStart {
                    progress_id,
                    title,
                    message,
                    percentage,
                }));
            }
        }

        "progressUpdate" => {
            if let Some(body) = body {
                let progress_id = body
                    .get("progressId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();
                let message = body
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
                let percentage = body.get("percentage").and_then(serde_json::Value::as_f64);
                let _ = event_tx.send(EditorEvent::Dap(DapEvent::ProgressUpdate {
                    progress_id,
                    message,
                    percentage,
                }));
            }
        }

        "progressEnd" => {
            if let Some(body) = body {
                let progress_id = body
                    .get("progressId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();
                let _ = event_tx.send(EditorEvent::Dap(DapEvent::ProgressEnd { progress_id }));
            }
        }

        "invalidated" => {
            if let Some(body) = body {
                let areas: Vec<String> = body
                    .get("areas")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let thread_id = body.get("threadId").and_then(serde_json::Value::as_u64);
                let stack_frame_id = body.get("stackFrameId").and_then(serde_json::Value::as_u64);
                let _ = event_tx.send(EditorEvent::Dap(DapEvent::Invalidated {
                    areas,
                    thread_id,
                    stack_frame_id,
                }));
            }
        }

        other => log::debug!("DAP: unhandled event {other:?}"),
    }
}

/// Handle a reverse-request from the adapter (e.g. `runInTerminal`).
/// Reverse-requests are messages with `type == "request"` sent by the adapter
/// to ask the client to perform an action. The client must respond.
fn handle_reverse_request(
    msg: &crate::types::DapMessage,
    transport: &crate::transport::DapTransport,
    event_tx: &crossbeam_channel::Sender<EditorEvent>,
) {
    let Some(command) = msg.command.as_deref() else {
        return;
    };

    match command {
        "runInTerminal" => {
            log::debug!("DAP: runInTerminal reverse-request");
            let body = msg.body.clone().unwrap_or(serde_json::Value::Null);
            if let Ok(args) =
                serde_json::from_value::<crate::types::RunInTerminalArguments>(body.clone())
            {
                let _ = event_tx.send(EditorEvent::Dap(DapEvent::Output {
                    category: OutputCategory::Console,
                    output: format!(
                        "[Debug adapter requested terminal: {} {}]\n",
                        args.title,
                        args.args.join(" ")
                    ),
                }));
                // Respond with success (empty body).
                let response = serde_json::json!({
                    "processId": 0,
                    "shellProcessId": 0,
                });
                let resp_seq = msg.seq;
                let response_msg = crate::types::DapMessage {
                    seq: resp_seq,
                    msg_type: "response".into(),
                    command: None,
                    arguments: None,
                    request_seq: Some(resp_seq),
                    success: Some(true),
                    body: Some(response),
                    message: None,
                    event: None,
                };
                let _ = transport.send_response(response_msg);
            } else {
                // Respond with error.
                let resp_seq = msg.seq;
                let response_msg = crate::types::DapMessage {
                    seq: resp_seq,
                    msg_type: "response".into(),
                    command: None,
                    arguments: None,
                    request_seq: Some(resp_seq),
                    success: Some(false),
                    body: None,
                    message: Some("Failed to parse runInTerminal arguments".into()),
                    event: None,
                };
                let _ = transport.send_response(response_msg);
            }
        }

        other => {
            log::debug!("DAP: unhandled reverse-request {other:?}");
            // Respond with error for unknown commands.
            let resp_seq = msg.seq;
            let response_msg = crate::types::DapMessage {
                seq: resp_seq,
                msg_type: "response".into(),
                command: None,
                arguments: None,
                request_seq: Some(resp_seq),
                success: Some(false),
                body: None,
                message: Some(format!("Unknown reverse-request: {other}")),
                event: None,
            };
            let _ = transport.send_response(response_msg);
        }
    }
}

fn parse_stop_reason(reason: &str) -> StopReason {
    match reason {
        "breakpoint" => StopReason::Breakpoint,
        "step" => StopReason::Step,
        "exception" => StopReason::Exception,
        "pause" => StopReason::Pause,
        "entry" => StopReason::Entry,
        "goto" => StopReason::Goto,
        "function breakpoint" => StopReason::FunctionBreakpoint,
        "data breakpoint" => StopReason::DataBreakpoint,
        _ => StopReason::Pause,
    }
}

/// Resolve the adapter executable from a launch config.
/// Looks up well-known adapter types and falls back to running the command
/// directly if it's already an executable path.
pub fn resolve_adapter(config: &LaunchConfig) -> (String, Vec<String>) {
    // If the user explicitly set adapter_command, use it directly.
    if !config.adapter_command.is_empty() {
        return (config.adapter_command.clone(), config.adapter_args.clone());
    }

    // Try to resolve by adapter type name.
    match config.adapter_type.as_deref() {
        Some("python" | "debugpy") => (
            "debugpy".to_owned(),
            vec![
                "--listen".to_owned(),
                format!("{}", config.port.unwrap_or(0)),
            ],
        ),
        Some("node" | "node-debug" | "js-debug") => (
            "js-debug-dap".to_owned(),
            vec![format!("{}", config.port.unwrap_or(0))],
        ),
        Some("lldb" | "lldb-code" | "lldb-vscode") => ("lldb-dap".to_owned(), vec![]),
        Some("gdb") => ("gdb".to_owned(), vec!["--interpreter=dap".to_owned()]),
        Some("codelldb") => ("codelldb".to_owned(), vec![]),
        _ => {
            log::warn!(
                "DAP: unknown adapter_type {:?}; using adapter_command as-is",
                config.adapter_type
            );
            (config.adapter_command.clone(), config.adapter_args.clone())
        }
    }
}
