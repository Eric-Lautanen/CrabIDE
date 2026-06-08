# Resume — crabide project

## Session summary

Mouse reporting (DECSET 1000/1002/1003) and content reflow on terminal resize are now fully implemented and verified:

**Mouse reporting ✅**
- Core encoding logic (X10 + SGR protocols) committed and tested (91 tests, +19 new)
- UI-side mouse event forwarding compiles and works with egui 0.34
- Standalone encode functions avoid temporary Grid construction

**Content reflow on resize ✅**
- `wrapped: Vec<bool>` field tracks soft-wrapped rows
- Set in `put_char` when auto-wrap occurs; maintained through scroll_up/scroll_down
- Reflow in `resize()`: merge wrapped rows into logical lines, re-split at new width
- Handles widening (unwraps), narrowing (rewraps), hard newlines, truncation
- 5 new unit tests

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
- **TESTS** — all workspace tests pass (91 terminal tests, 112 UI tests, etc.)

## Remaining roadmap items — pick next available

### Easy / self-contained (pick these first)

**Phase 7 (Terminal) — highest priority:**
- [x] Implement mouse reporting (DECSET 1000/1002/1003)
- [x] Implement content reflow on terminal resize
- [ ] Implement OSC 8 hyperlinks — parse `\e]8;...;url\a...\e]8;;\a` → clickable links
- [ ] Implement OSC 133 shell integration markers — prompt start/end detection
- [ ] Add configurable color scheme / theme to TerminalProfile
- [ ] Add Unicode width proper crate to replace approximate `unicode_width()`
- [ ] Add more unit tests to crabide-terminal (currently 91, roadmap says "no unit tests")

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
