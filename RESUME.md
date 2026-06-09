# Resume — crabide project

> **MANDATORY:** Every session MUST update RESUME.md, update ROADMAP.md checkboxes,
> and call `handoff` with reason `"continuing to next roadmap item"` before ending.
> Never stop voluntarily — always hand off to continue until ROADMAP.md is 100% complete.

## Session summary

**Extension capability enforcement implemented ✅**
- Added `is_output_allowed()` public helper function that checks whether an `ExtensionOutput` variant is allowed given the extension's declared `ExtensionCapabilities`
- Added capability filtering in `ExtensionHost::poll_all()` — sensitive outputs (WriteFile, SendToTerminal, OpenTerminal, ApplyEdits, InsertAtCursor, SetCursorPosition) are filtered out if the extension lacks the required capability
- Added defense-in-depth checks in `wasm_ext.rs` WASM host functions:
  - `write_file()` now requires `file_write` capability
  - `read_file()` / `file_exists()` / `list_dir()` / `get_document_slice()` (cross-document) now require `file_read` capability
  - `send_to_terminal()` / `open_terminal()` now require `terminal` capability
  - `apply_edits()` / `insert_at_cursor()` / `set_cursor_position()` now require `file_write` capability
- Added `capabilities` field to `WasmExtension` struct and `HostState`, defaulting to all-denied (secure by default)
- Overrode `capabilities()` on `WasmExtension` to return stored capabilities
- Updated `notify_terminal_output()` to also filter outputs through `is_output_allowed()` for defense-in-depth
- Exported `is_output_allowed` from the crate for testing and external use
- Added 22 unit tests covering all capability-checked output variants and both allowed/denied scenarios
- All 960+ workspace tests pass, zero warnings, zero clippy issues

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
- **TESTS** — all ~960+ workspace tests pass

## Remaining roadmap items — pick next available

### Phase 9 (Extensions) — capability enforcement done:
- [x] WASM editor/workspace/commands host implementations
- [x] Registry download with checksum verification (install_registry, download + verify + install flow)
- [x] Capability enforcement (check ExtensionCapabilities before granting resource access)
- [ ] WASM engine resource limits (memory cap, fuel metering, execution timeout)
- [ ] Marketplace URL configuration

### Phase 4 (UI):
- [ ] Minimap, split editor, context menu, drag-and-drop tabs, scrollbar annotations, peek view, output panel, settings UI, keybindings editor, theme picker, welcome screen, multi-cursor Alt+Click, column select mode

### Phase 12:
- [ ] Update checker, crash reporter, installers (Windows/macOS/Linux), CI workflows, performance pass, README/docs site

## Cross-cutting
- [ ] Feature flag matrix test
- [ ] `crabide-workspace` crate tracking in Phase 1/2
