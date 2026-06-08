# Resume — crabide project

## Session summary

**Language support for 12 new languages ✅**
- Added 16 tree-sitter grammar crate dependencies (all compile on Windows)
- Wrote highlight queries for: HTML, CSS, SCSS, LESS, YAML, Shell/Bash, SQL, Java, C#, Kotlin, Ruby, PHP
- Registered TypeScript, C++, TOML, Markdown grammars (had queries, never registered)
- Used raw FFI helper macro for older grammars with incompatible tree-sitter versions
- All 22 languages now registered in `register_grammars()`

**Syntax crate test coverage increased ✅**
- Added 17 new unit tests across engine.rs, outline.rs, fold.rs
- Total tests in crabide-syntax: 86 (up from 69)
- Covers: SyntaxEngine creation, outline dispatch, symbol kind variants, fold kind mapping for new languages, node helper functions

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
- **TESTS** — all 903 workspace tests pass (86 syntax, 160 terminal, 112 UI, etc.)

## Remaining roadmap items — pick next available

### Medium tasks (all easy items are done)

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
