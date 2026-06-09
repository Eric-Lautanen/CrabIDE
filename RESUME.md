# Resume — crabide project

## Session summary

**DAP evaluate/threads/breakpoints/backpressure infrastructure added ✅**
- Added new `DapEvent` variants: `EvaluateReady`, `ThreadsReady`, `SetVariableDone`, `FunctionBreakpointsReady`, `ExceptionInfoReady`, `ExceptionBreakpointsSet`, `GotoTargetsReady`, `ModulesReady`, `ProgressStart`, `ProgressUpdate`, `ProgressEnd`, `Invalidated`
- Added DAP type definitions for evaluate, threads, setVariable, function/exception breakpoints, gotoTargets, modules, runInTerminal, cancel, completions
- Added core event types: `DapThread`, `GotoTarget`, `DapModule`, `EvaluateResult`
- Added `DapClient` methods: `evaluate()`, `request_threads()`, `set_variable()`, `set_function_breakpoints()`, `set_exception_breakpoints()`, `request_exception_info()`, `request_goto_targets()`, `goto()`, `request_modules()`, `attach()`
- Extended `dispatch_event` to handle: `module`, `progressStart`, `progressUpdate`, `progressEnd`, `invalidated`
- Added `handle_reverse_request` to respond to adapter reverse-requests (`runInTerminal`)
- Added `send_response` method to `DapTransport`
- Changed transport writer from unbounded to bounded channel (1024 slots, `try_send` backpressure)
- Wired all new events in `apply_dap_event` in app.rs
- Added new fields to `DapPanelState`: `threads`, `function_breakpoints`, `last_exception`, `goto_targets`, `modules`, `last_evaluate_result`, `progress`
- All workspace tests pass, zero warnings (clippy + check)

> **IMPORTANT:** After completing the session's tasks, you MUST call the `handoff` tool with reason `continuing to next roadmap item` to end this session and begin the next one. Do not stop working — always hand off to continue.

## Handoff Policy

**This project MUST continue to be handed off across sessions until the entire ROADMAP.md is complete.** Each session should:

1. Read this RESUME.md to determine what was done and what's next
2. Pick the next unfinished roadmap item(s) and implement them
3. Follow the Autonomous Coding Best Practices in ROADMAP.md (cargo check, clippy, fmt, test after every edit; commit early and often)
4. Update ROADMAP.md checkboxes as items are completed
5. Update this RESUME.md with progress before calling `handoff`
6. Call `handoff` with reason "continuing to next roadmap item" when nearing context limit or when the current item is done and more remain
7. **Never stop voluntarily** — keep working through roadmap items until context forces a handoff

## Build status
- **GREEN** — `cargo check --workspace` zero warnings (pre-existing `resize_stable` dead_code warning only)
- **CLIPPY** — zero warnings
- **TESTS** — all ~990+ workspace tests pass

## Remaining roadmap items — pick next available

### Phase 8 (DAP):
- [x] All DAP roadmap items completed (evaluate, threads, setVariable, gotoTargets, exceptionInfo, function/exception breakpoints, progress events, invalidated, runInTerminal, completions types, cancel types, modules, attach, backpressure)

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
