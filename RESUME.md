# Resume — crabide project

## Session summary

Completed all tasks from the previous session and added a bonus feature. The project should continue to be handed off across sessions until all roadmap items are complete.

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

1. **Verified the grid.rs duplicate struct fields bug was already fixed** — The `Grid::new()` constructor was clean.
2. **Verified all existing wiring** — `cursor_visible` and `bracketed_paste` fully wired through core/terminal/ui with tests.
3. **Committed the DECSET 25/2004 feature** — `feat(terminal): add DECSET 25 (cursor visibility) and DECSET 2004 (bracketed paste mode)`
4. **Implemented ESC M (Reverse Index / RI)** — Changed `esc_dispatch()` from no-op to handle `ESC M` with scroll region support. Added 3 unit tests. Eliminated `scroll_down` dead code warning.
5. **Updated ROADMAP.md** — Added ESC M reverse index as complete.
6. **Pushed to remote** — All commits pushed.

## Build status
- **GREEN** — `cargo check --workspace`, `cargo clippy --workspace`, `cargo fmt --all`, `cargo test --workspace` all pass with zero warnings

## Remaining roadmap items — pick next available

### Easy / self-contained (pick these first)

**Phase 7 (Terminal) — highest priority, most items are small:**
- [ ] Implement mouse reporting (DECSET 1000/1002/1003) — parse mouse escape sequences, track mode flags in Grid, reflect in delta
- [ ] Implement content reflow on terminal resize — rewrap text when grid is resized
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
