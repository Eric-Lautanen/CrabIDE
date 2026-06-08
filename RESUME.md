# Resume — crabide project

## Session summary

**DAP evaluate/threads/breakpoints infrastructure added ✅**
- Added new `DapEvent` variants: `EvaluateReady`, `ThreadsReady`, `SetVariableDone`, `FunctionBreakpointsReady`, `ExceptionInfoReady`, `ExceptionBreakpointsSet`, `GotoTargetsReady`, `ModulesReady`, `ProgressStart`, `ProgressUpdate`, `ProgressEnd`, `Invalidated`
- Added DAP type definitions: `EvaluateArguments`/`EvaluateResponse`, `ThreadsResponse`/`ThreadInfo`, `SetVariableArguments`/`SetVariableResponse`, `SetExpressionArguments`/`SetExpressionResponse`, `FunctionBreakpoint`/`SetFunctionBreakpointsArguments`, `SetExceptionBreakpointsArguments`/`ExceptionOption`, `ExceptionInfoArguments`/`ExceptionInfoResponse`/`ExceptionDetail`, `GotoTargetsArguments`/`GotoTargetsResponse`/`GotoTargetInfo`/`GotoArguments`, `ModulesArguments`/`ModulesResponse`/`ModuleInfo`, `RunInTerminalArguments`, `CancelArguments`, `CompletionsArguments`/`CompletionsResponse`/`CompletionItem`
- Added core event types: `DapThread`, `GotoTarget`, `DapModule`, `EvaluateResult`
- Added `DapClient` methods: `evaluate()`, `request_threads()`, `set_variable()`, `set_function_breakpoints()`, `set_exception_breakpoints()`, `request_exception_info()`, `request_goto_targets()`, `goto()`, `request_modules()`
- Extended `dispatch_event` to handle: `module`, `progressStart`, `progressUpdate`, `progressEnd`, `invalidated`, `runInTerminal`
- Wired all new events in `apply_dap_event` in app.rs
- Added new fields to `DapPanelState`: `threads`, `function_breakpoints`, `last_exception`, `goto_targets`, `modules`, `last_evaluate_result`, `progress`
- All workspace tests pass, zero warnings (clippy + check)

## Build status
- **GREEN** — `cargo check --workspace` zero warnings (pre-existing `resize_stable` dead_code warning only)
- **CLIPPY** — zero warnings
- **TESTS** — all ~990+ workspace tests pass

## Remaining roadmap items — pick next available

### Phase 8 (DAP):
- [x] evaluate, threads, setVariable, gotoTargets, exceptionInfo, function/exception breakpoints, progress events, invalidated event, runInTerminal stub, completions types, cancel types, modules
- [ ] Add backpressure to writer (unbounded channel → bounded + semaphore)
- [ ] Fully implement `attach` workflow
- [ ] Fully implement `runInTerminal` handler

### Phase 6 (Git):
- [ ] tag management, remote management, submodule support, conflict resolution

### Phase 9 (Extensions):
- [ ] WASM editor/workspace/commands host implementations
- [ ] Registry download with checksum verification

### Phase 4 (UI):
- [ ] Minimap, split editor, context menu, drag-and-drop tabs, scrollbar annotations, peek view, output panel, settings UI, keybindings editor, theme picker, welcome screen, multi-cursor Alt+Click, column select mode

### Phase 12:
- [ ] Update checker, crash reporter, installers (Windows/macOS/Linux), CI workflows, performance pass, README/docs site

## Cross-cutting
- [ ] Feature flag matrix test
- [ ] `crabide-workspace` crate tracking in Phase 1/2
