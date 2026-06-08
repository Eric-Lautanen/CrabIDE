# Resume — crabide project

## Session summary

**Window state persistence ✅**
- Added `WindowState` struct with serde JSON serialization
- Saved to `~/.crabide/window_state.json` on app exit via `on_exit()`
- Loaded on startup in `main.rs` and applied to `eframe::ViewportBuilder`
- Window size, position (from `outer_rect`), and maximized state tracked each frame
- `serde` and `serde_json` workspace deps added to crabide-app crate

**Session restore (reopen files from last session) ✅**
- Added `SessionState` struct with `open_files: Vec<String>` saved to `~/.crabide/session.json`
- On exit, collects all open file:// URIs from editor tabs and saves them
- On startup, loads session file list and opens any files that still exist on disk
- Deduplicates against CLI-provided paths to avoid reopening the same file twice
- Handles missing/deleted files gracefully (silently skipped)

**`docs/` directory ✅**
- Created `docs/README.md` with links to ARCHITECTURE, BUILD, CONTRIBUTING
- Created `docs/ARCHITECTURE.md` with crate dependency graph and phase overview
- All workspace tests pass, zero new clippy warnings

**Unicode-width crate ✅**
- Added `unicode-width` 0.2.2 workspace dependency
- Replaced hand-rolled CJK/fullwidth/emoji width ranges with `unicode_width::UnicodeWidthChar::width()`
- All 102 terminal tests pass

**Git branch listing, deletion, staged diff ✅**
- `GitService::list_branches()` → `BranchesListed` event
- `GitService::delete_branch()` → `OperationCompleted` / `OperationFailed`
- `GitService::request_diff_staged()` → `DiffStagedUpdated` event
- `GitPanelState.branches` and `EditorTab.git_staged_hunks` fields added to UI state

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
- **TESTS** — all workspace tests pass (160 terminal tests, 112 UI tests, etc.)

## Remaining roadmap items — pick next available

### Easy / self-contained (pick these first)

**Phase 7 (Terminal):**
- [x] Implement mouse reporting (DECSET 1000/1002/1003)
- [x] Implement content reflow on terminal resize
- [x] Implement OSC 8 hyperlinks
- [x] Implement OSC 133 shell integration markers
- [x] Add configurable color scheme / theme to TerminalProfile
- [x] Add Unicode width proper crate to replace approximate `unicode_width()`
- [x] Add more unit tests to crabide-terminal (now 160, up from 102)

## Remaining roadmap items — pick next available

### Easy / self-contained (pick these first)

**Phase 10 (App):**
- [x] Window state persistence (size, position, maximized state)
- [x] Session restore (reopen files from last session)

**Phase 7 (Terminal):**
- [x] Implement mouse reporting (DECSET 1000/1002/1003)
- [x] Implement content reflow on terminal resize
- [x] Implement OSC 8 hyperlinks
- [x] Implement OSC 133 shell integration markers
- [x] Add configurable color scheme / theme to TerminalProfile
- [x] Add Unicode width proper crate to replace approximate `unicode_width()`
- [ ] Add more unit tests to crabide-terminal (currently 102)

**Phase 6 (Git):**
- [x] Add branch listing (local + remote)
- [x] Add branch deletion
- [x] Add diff for staged changes (index vs HEAD)

**Cross-cutting:**
- [x] `docs/` directory (now has README.md and ARCHITECTURE.md)

### Medium tasks (after easy items are done)

**Phase 2 (Syntax):**
- [ ] Add language support for: HTML, CSS/SCSS/LESS, YAML, Shell/Bash, SQL, Java, C#, Kotlin, Ruby, PHP
- [ ] More unit test coverage for crabide-syntax

**Phase 6 (Git):**
- [ ] fetch/pull/push/merge/rebase
- [ ] stash (push, pop, list, drop)
- [ ] log / history / graph view
- [ ] tag management, remote management, submodule support, conflict resolution

**Phase 8 (DAP):**
- [ ] attach workflow, evaluate, setVariable, threads, function/exception breakpoints

**Phase 9 (Extensions):**
- [ ] WASM editor/workspace/commands host implementations
- [ ] Registry download with checksum verification

**Phase 4 (UI):**
- [ ] Minimap, split editor, context menu, drag-and-drop tabs, scrollbar annotations, peek view, output panel, settings UI, keybindings editor, theme picker, welcome screen, multi-cursor Alt+Click, column select mode

**Phase 12:**
- [ ] Update checker, crash reporter, installers (Windows/macOS/Linux), CI workflows, performance pass, README/docs site
