# Resume — crabide project

> **MANDATORY:** Every session MUST update RESUME.md, update ROADMAP.md checkboxes,
> and call `handoff` with reason `"continuing to next roadmap item"` before ending.
> Never stop voluntarily — always hand off to continue until ROADMAP.md is 100% complete.

## Session summary

**WASM extension host stubs implemented ✅**
- Expanded `HostState` with context fields: `ctx_full_text`, `ctx_workspace_roots`, `ctx_cursor_line`, `ctx_cursor_col`, `ctx_selection`, `ctx_terminals`, `ctx_visible_panels`
- Updated `WasmExtension::set_ctx()` to populate all new fields from `ExtensionContext`
- Implemented all previously-stubbed WIT host functions in `wasm_ext.rs`:
  - `editor::Host`: `get_document_slice()` (extract from active doc text or read from disk), `apply_edits()` (queues `ExtensionOutput::ApplyEdits`), `insert_at_cursor()` (queues `ExtensionOutput::InsertAtCursor`), `get_cursor_position()` (returns context cursor), `set_cursor_position()` (queues `ExtensionOutput::SetCursorPosition`), `get_selection_text()` (extracts from text + selection range)
  - `workspace::Host`: `get_workspace_roots()` (returns cached workspace roots), `find_files()` (simple recursive glob walk of workspace roots)
  - `commands::Host`: `execute_command()` (logs and returns error -- needs app wiring), `show_quick_pick()` and `show_input_box()` (queue notification since synchronous return impossible)
  - `status_bar::Host`: `set_visible()` (queues `ExtensionOutput::StatusBarVisible`)
  - `terminal::Host`: `list_terminals()` (returns cached terminal IDs)
  - `panels::Host`: `is_panel_visible()` (checks cached visible panel set)
- Added new types: `TextEdit` struct, `ExtensionOutput::SetCursorPosition`, `ExtensionOutput::ApplyEdits`, `ExtensionOutput::InsertAtCursor`, `ExtensionOutput::StatusBarVisible`
- Added stub handling in `app.rs::apply_extension_outputs()` for new variants (log TODO)
- Added `simple_glob_match()` helper (`*` and `?` wildcards) for `find_files()`
- Fixed `async: false` removal from bindgen! macro (incompatible with wit-bindgen 0.57)
- Fixed missing `current_theme_id` field in two `empty_ctx` constructors in `host.rs`
- All workspace tests pass, zero warnings (clippy + check), pre-existing `resize_stable` warning only

## Mandatory Policy (read every session)

1. **Update RESUME.md** — overwrite this section with what was done. Never leave stale info.
2. **Update ROADMAP.md** — mark completed items `[x]`, add new gaps as `[ ]`.
3. **Commit** `git add -A && git commit -m "TYPE: message"` after every green build.
4. **Push** periodically.
5. **Handoff** — call `handoff` with `reason: "continuing to next roadmap item"` when:
   - Current roadmap item is done and more remain, OR
   - Context is nearing the token limit
6. **Never stop** — always hand off to continue. The project MUST be completed end-to-end.

## Build status
- **GREEN** — `cargo check --workspace` zero warnings (pre-existing `resize_stable` dead_code warning only)
- **CLIPPY** — zero warnings
- **TESTS** — all ~930+ workspace tests pass

## Remaining roadmap items — pick next available

### Phase 9 (Extensions) — WASM stubs done, registry download next:
- [x] WASM editor/workspace/commands host implementations
- [ ] Registry download with checksum verification (install_registry, download + verify + install flow)
- [ ] Capability enforcement (check ExtensionCapabilities before granting resource access)
- [ ] WASM engine resource limits (memory cap, fuel metering, execution timeout)
- [ ] Marketplace URL configuration

### Phase 4 (UI):
- [ ] Minimap, split editor, context menu, drag-and-drop tabs, scrollbar annotations, peek view, output panel, settings UI, keybindings editor, theme picker, welcome screen, multi-cursor Alt+Click, column select mode

### Phase 12:
- [ ] Update checker, crash reporter, installers (Windows/macOS/Linux), CI workflows, performance pass, README/docs site

## Cross-cutting
- [ ] Feature flag matrix test
- [ ] `crabide-workspace` crate tracking in Phase 1/2
