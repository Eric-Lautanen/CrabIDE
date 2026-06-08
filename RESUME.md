# Resume — crabide project

## Session summary

**Git log / history / graph view infrastructure added ✅**
- Added `CommitEntry` type (hash, author, email, time, message, parents, ref_names) to crabide-core
- Added `GitEvent::LogReady` variant for rich result reporting
- Added `GitCommand::Log` variant with full git2 revwalk implementation (walk all refs or specific branch, decorations from reference database, sorted by time/topological order)
- Added `GitService::log()` public API method with optional branch filter and limit (default 100)
- Added `Action::GitLog` variant registered in command palette as "Git: Show History / Log"
- Wired action in app.rs dispatch with status message
- Added `log_entries` field to `GitPanelState` in UI state
- All 928+ workspace tests pass, zero warnings (clippy + check)

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
- **TESTS** — all 928+ workspace tests pass

## Remaining roadmap items — pick next available

### Phase 6 (Git):
- [x] log / history / graph view
- [ ] tag management, remote management, submodule support, conflict resolution

### Phase 8 (DAP):
- [ ] attach workflow, evaluate, setVariable, threads, function/exception breakpoints

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
