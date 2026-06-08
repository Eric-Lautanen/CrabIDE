# Resume — crabide project

## Session summary

Mouse reporting (DECSET 1000/1002/1003) is now fully implemented and verified:
- Core encoding logic (X10 + SGR protocols) committed and tested (86 tests, +14 new)
- UI-side mouse event forwarding compiles and works with egui 0.34
- Standalone encode functions avoid temporary Grid construction
- `cargo check --workspace`, `cargo clippy --workspace`, `cargo fmt --all`, `cargo test --workspace` all green

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

1. **Refactored mouse encoding to standalone functions** — `encode_mouse_press()`, `encode_mouse_release()`, `encode_mouse_motion()`, `encode_mouse_scroll()` are now free functions in `grid.rs` that take mode flags as parameters instead of requiring a `Grid` instance. This eliminates the wasteful temporary `Grid::new()` construction.

2. **Fixed UI-side mouse event forwarding** — Rewrote `terminal_panel.rs` mouse handling to use the standalone encode functions, fixed all egui API issues (`is_pointer_button_down_on` → `ui.input(|i| i.pointer.*)`), added `crabide-terminal` dependency to `crabide-ui`.

3. **Added 14 unit tests** — Tests cover X10 press/release/motion, SGR press/release/motion, scroll up/down, right button, inactive modes, and SGR scroll encoding.

4. **Updated ROADMAP.md** — Marked mouse reporting as complete.

## Build status
- **GREEN** — `cargo check --workspace` zero warnings (pre-existing `resize_stable` dead_code warning only)
- **CLIPPY** — zero warnings
- **TESTS** — all workspace tests pass (including 86 terminal tests)

## Remaining roadmap items — pick next available

### Easy / self-contained (pick these first)

**Phase 7 (Terminal) — highest priority:**
- [ ] Implement content reflow on terminal resize
- [ ] Implement OSC 8 hyperlinks — parse `\e]8;...;url\a...\e]8;;\a` → clickable links
- [ ] Implement OSC 133 shell integration markers — prompt start/end detection
- [ ] Add configurable color scheme / theme to TerminalProfile
- [ ] Add Unicode width proper crate to replace approximate `unicode_width()`
- [ ] Add more unit tests to crabide-terminal (currently 86, roadmap says "no unit tests")

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
