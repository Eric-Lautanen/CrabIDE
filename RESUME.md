# Resume — full codebase audit session

> **Session goal:** Review the entire codebase — zero code edits, review only.  
> Produce a comprehensive ROADMAP.md prioritising every discovered issue, then hand off for implementation.

---

## Session 2 completed (2026-06-xx)

**What was done:**
- Audited `crabide-syntax` ✅ — Grammar registry, highlight engine, outline, fold, indent, queries
- Audited `crabide-lsp` ✅ — Custom JSON-RPC transport, LspClient, ServerManager, type conversions
- Audited `crabide-dap` ✅ — Content-Length transport, DapClient, in-house DAP types
- Audited `crabide-terminal` ✅ — VT grid state machine (vte), PTY spawn (portable-pty), manager/profiles
- Audited `crabide-git` ✅ — libgit2-backed GitService, diff, blame, branch/stash/tag/remote/submodule
- Audited `crabide-extensions` ✅ — ExtensionHost, WASM loader (wasmtime), registry client, hot-reload
- Audited `crabide-search` ✅ — Fuzzy file finder (nucleo), workspace grep (rayon), file index
- Audited `crabide-workspace` ✅ — Workspace, document lifecycle, DocumentObserver hooks
- Audited `crabide-ui` ✅ — UiState, all panels, overlays, layout, palette, tab bar
- Audited `crabide-app` ✅ — crabideApp, main (CountingAlloc), icon_data, window_state, CLI parser
- Audited root workspace ✅ — Workspace Cargo.toml, feature flags, build scripts, README
- Ran `cargo clippy` ✅ — zero warnings
- Compiled findings into updated ROADMAP.md

**Key new findings (Session 2):**

| # | Issue | Severity | File |
|---|-------|----------|------|
| H-1 | DAP process leak — missing `kill_on_drop(true)` | 🔴 High | `crabide-dap/src/client.rs` |
| H-2 | Terminal PTY process leak — child dropped immediately | 🔴 High | `crabide-terminal/src/pty.rs` |
| H-3 | WASM epoch timeout not wired up — `increment_epoch` never called | 🔴 High | `crabide-extensions/src/wasm_ext.rs` |
| H-4 | LSP server→client request handling broken — can't respond with correct id | 🔴 High | `crabide-lsp/src/client.rs` |
| H-5 | ~30 public enums still missing `#[non_exhaustive]` | 🔴 High | Multiple crates |
| M-5 | Injection highlighting dead code — `compute_highlights` used instead of `compute_highlights_with_injections` | 🟡 Medium | `crabide-syntax/src/engine.rs` |

**Previously known issues (Session 1):**
- C-1: 4 unsafe blocks without `// SAFETY:` comments
- C-2: `unsafe fn` without `#[deny(unsafe_op_in_unsafe_fn)]`
- H-6: Unused dependencies in buffer and config crates
- M-1/M-2: ConfigManager clone-heavy hot path
- M-4: CursorSet normalise sort overhead

---

## Session checklist

- [x] crabide-core audited
- [x] crabide-buffer audited
- [x] crabide-config audited
- [x] crabide-vfs audited
- [x] crabide-syntax audited
- [x] crabide-lsp audited
- [x] crabide-dap audited
- [x] crabide-terminal audited
- [x] crabide-git audited
- [x] crabide-extensions audited
- [x] crabide-search audited
- [x] crabide-workspace audited
- [x] crabide-ui audited
- [x] crabide-app audited
- [x] Root workspace, CI, build scripts audited
- [x] ROADMAP.md updated with all findings
- [x] `cargo clippy` — zero warnings

---

## Workflow rules (ALL CAPITAL — MUST FOLLOW)

1. **Handoff tool MUST be called** at the end of EVERY session (after updating RESUME.md and ROADMAP.md) using `handoff` with a clear reason summarising what was done and what remains.
2. **Never end a session without calling the handoff tool** — even if the task list shows "completed". The handoff is the signal that the session is voluntarily ending so another can continue.
3. **Update RESUME.md → Update ROADMAP.md → Call handoff tool** — in that exact order. Do not skip any step.
4. **ROADMAP.md checkboxes**: Every issue must have a `- [ ]` checkbox. As sessions complete items, `[ ]` becomes `[x]`. When ROADMAP.md is fully `[x]`, the audit is done.
5. **Todo list**: Keep the `todo_list` tool updated with `status: "pending"` / `"in_progress"` / `"completed"` for every audit crate and output step.
6. **Context monitoring**: If context token usage exceeds 70%, call handoff immediately to avoid truncation. Save all work first.

---

## Handoff instructions for next session

1. **Read `RESUME.md`** for current state.
2. **Read `ROADMAP.md`** for current findings.
3. **Begin remediation** of findings, starting from 🔴 Critical (C-1, C-2) then 🔴 High (H-1 through H-6).
4. **Specific remediation items:**
   - **C-1/C-2**: Add `// SAFETY:` comments to all unsafe blocks; add `#[deny(unsafe_op_in_unsafe_fn)]` to main.rs
   - **H-1**: Add `kill_on_drop(true)` to DAP Command
   - **H-2**: Store child handle in `PtyHandle` instead of dropping it
   - **H-3**: Spawn background thread for `Engine::increment_epoch()`
   - **H-4**: Fix LSP server→client request handling (pass message id to handler)
   - **H-5**: Add `#[non_exhaustive]` to all public enums listed in ROADMAP
   - **H-6**: Remove unused dependencies
   - **M-1/M-2**: Optimize ConfigManager hot path
   - **M-5**: Wire injection highlighting in SyntaxEngine
   - **L-1 through L-4**: Style cleanup
   - **N-1**: Add unit tests to crates with zero coverage
5. **Run tooling**: `cargo clippy`, `cargo test`, `cargo udeps` after each round of fixes.
6. **Update `ROADMAP.md`** checkboxes as items are completed.
7. **Update `RESUME.md`** checklist.
8. **Call `handoff` tool** — in this exact order: update RESUME.md → update ROADMAP.md → call handoff.

---

*Last verified against Rust 1.95.0 (April 2026), egui 0.34.3 (May 2026), wasmtime 45.0.0 (May 2026).*
*Session 2 completed: all 14 crates audited; ROADMAP.md fully updated with 55 actionable items.*
