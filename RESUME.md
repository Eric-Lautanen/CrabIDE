# Resume — crabide project

## Session summary

Implemented mouse reporting (DECSET 1000/1002/1003) for the terminal emulator. The core encoding logic and DECSET parsing is committed and working. The UI-side mouse event forwarding was written but has NOT been verified to compile yet — it needs `cargo check` and likely fixes.

## Handoff Policy

**This project MUST continue to be handed off across sessions until the entire ROADMAP.md is complete.** Each session should:

1. Read this RESUME.md to determine what was done and what's next
2. Pick the next unfinished roadmap item(s) and implement them
3. Follow the Autonomous Coding Best Practices in ROADMAP.md (cargo check, clippy, fmt, test after every edit; commit early and often)
4. Update ROADMAP.md checkboxes as items are completed
5. Update this RESUME.md with progress before calling `handoff`
6. Call `handoff` with reason "continuing to next roadmap item" when nearing context limit or when the current item is done and more remain
7. **Never stop voluntarily** — keep working through roadmap items until context forces a handoff

## What was done this session

1. **Added DECSET 1006 (SGR extended mouse mode)** — New `mouse_sgr` field on `Grid`, `TerminalGridDelta`, `TerminalInstance`. Parsing of `?1006h`/`?1006l` in `csi_dispatch`. Committed as `dfdebc1`.

2. **Added mouse encoding types and methods** — `MouseButton` enum (Left/Middle/Right/ScrollUp/ScrollDown), `ScrollDirection` enum, `Grid::encode_mouse_press()`, `encode_mouse_release()`, `encode_mouse_motion()`, `encode_mouse_scroll()`, `mouse_reporting_active()`. X10 and SGR encoding helpers. All committed in `dfdebc1`.

3. **Re-exported `MouseButton` and `ScrollDirection`** from `crabide_terminal::lib.rs`. Committed in `dfdebc1`.

4. **Wrote UI-side mouse event forwarding** in `crates/crabide-ui/src/panels/terminal_panel.rs` — Added ~60 lines of mouse event handling code that converts egui pointer events to terminal mouse escape sequences when a mouse reporting mode is active. **THIS HAS NOT BEEN VERIFIED TO COMPILE YET.** The next session must run `cargo check --workspace` and fix any issues.

## Build status
- **Last verified GREEN** — after commit `dfdebc1`, `cargo check`, `cargo clippy`, `cargo fmt`, `cargo test --workspace` all passed with zero warnings.
- **UNVERIFIED** — The terminal_panel.rs mouse event code added AFTER the commit has NOT been checked. It likely has compilation issues (egui API usage may be wrong, temporary Grid construction is wasteful).

## Critical: Fix terminal_panel.rs mouse code first

The mouse event code in `terminal_panel.rs` (lines ~281-370) needs:
1. `cargo check --workspace` to find compilation errors
2. Fix any egui API issues (e.g., `is_pointer_button_pressed` may not exist — check egui docs)
3. The temporary `Grid::new()` construction just to call encode methods is wasteful — refactor to use standalone encode functions or a helper struct that doesn't need a full Grid
4. Run `cargo clippy --workspace` and `cargo fmt --all`
5. Run `cargo test --workspace`
6. Commit once green

## Remaining roadmap items — pick next available

### Easy / self-contained (pick these first)

**Phase 7 (Terminal) — highest priority:**
- [ ] ~~Implement mouse reporting (DECSET 1000/1002/1003)~~ — NEARLY DONE, just need to fix UI-side code and add unit tests
- [ ] Implement content reflow on terminal resize
- [ ] Implement OSC 8 hyperlinks — parse `\e]8;...;url\a...\e]8;;\a` → clickable links
- [ ] Implement OSC 133 shell integration markers — prompt start/end detection
- [ ] Add configurable color scheme / theme to TerminalProfile
- [ ] Add Unicode width proper crate to replace approximate `unicode_width()`
- [ ] Add more unit tests to crabide-terminal (currently 72, roadmap says "no unit tests")

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
