# Resume — crabide project

## Session summary

OSC 8 hyperlinks and OSC 133 shell integration are now fully implemented and verified:

**OSC 8 hyperlinks ✅**
- `Cell` struct gained `hyperlink: Option<String>` field (non-Copy, Clone retained)
- `TerminalCell` and `DisplayCell` both carry hyperlink URLs to the UI
- `cur_hyperlink` state tracked in `Grid`, applied to all printed cells
- Parse `ESC ] 8 ; params ; url BEL` in `osc_dispatch` (open/close)
- Hyperlinks flow through `take_delta()` to the UI layer
- 4 new unit tests

**OSC 133 shell integration ✅**
- `Grid` now tracks `command_started: Option<String>` and `command_finished: Option<i32>`
- Parse `ESC ] 133 ; C [; cmd] BEL` and `ESC ] 133 ; D [; code] BEL`
- `pty_reader_loop` emits `CommandStarted` / `CommandFinished` events on change
- 7 new unit tests

**TerminalColorScheme ✅**
- `TerminalColorScheme` defined in `crabide-core::event` with dark/light/default
- 16 ANSI colors + default fg/bg/cursor + selection bg are customizable
- `TerminalProfile.color_scheme` field stores per-terminal theme
- `TerminalInstance.color_scheme` propagated to UI rendering
- `terminal_color_to_egui()` and `effective_colors()` accept scheme parameter
- ANSI 16 colors resolve from scheme instead of hardcoded `SYSTEM` array
- Cursor color uses scheme
- Tab strip and grid background derived from scheme
- 5 new unit tests for TerminalColorScheme
- 1 pre-existing `resize_stable` dead_code warning remains (unrelated)

**Breaking change:** `Cell` no longer derives `Copy` (due to `Option<String>`).
`Cell::BLANK` const replaced with `Cell::blank()` function.
All tests updated.

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
- **TESTS** — all workspace tests pass (102 terminal tests, 112 UI tests, etc.)

## Remaining roadmap items — pick next available

### Easy / self-contained (pick these first)

**Phase 7 (Terminal) — highest priority:**
- [x] Implement mouse reporting (DECSET 1000/1002/1003)
- [x] Implement content reflow on terminal resize
- [x] Implement OSC 8 hyperlinks — parse `\e]8;...;url\a...\e]8;;\a` → clickable links
- [x] Implement OSC 133 shell integration markers — prompt start/end detection
- [x] Add configurable color scheme / theme to TerminalProfile
- [ ] Add Unicode width proper crate to replace approximate `unicode_width()`
- [ ] Add more unit tests to crabide-terminal (currently 102, roadmap says "no unit tests")

**Phase 6 (Git):**
- [ ] Add branch listing (local + remote)
- [ ] Add branch deletion
- [ ] Add diff for staged changes (index vs HEAD)

**Phase 10 (App):**
- [ ] Window state persistence (size, position, maximized state)
- [ ] Session restore (reopen files from last session)

**Cross-cutting:**
- [ ] `docs/` directory (currently empty)

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
