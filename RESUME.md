# Resume — crabide project

> ⚠️ **MANDATORY POLICY (every session):**
> 1. Update this RESUME.md with what was done — never leave stale info.
> 2. Update ROADMAP.md — mark completed items `[x]`, add new gaps as `[ ]`.
> 3. `git add -A && git commit -m "TYPE: message"` after every green build.
> 4. Push periodically.
> 5. **Call `handoff` with `reason: "continuing to next roadmap item"`** when:
>    - Current roadmap item is done and more remain, OR
>    - Context is nearing the token limit.
> 6. **Never stop voluntarily** — always hand off to continue. The project MUST be completed end-to-end.

## Session summary

**Phase 12: crash reporter ✅**
- Panic hook writes crash details (timestamp, panic message, backtrace) to `~/.crabide/crash.log`
- Installed early in main() before any services start
- Falls back to stderr if crash.log is unwritable
- No new dependencies added

**Phase 4 UI: settings panel ✅**
- Settings editor overlay with grouped settings fields, editable controls (Ctrl+,)
- New `Action::ToggleSettingsPanel` action variant with keybinding `Ctrl+,`
- New `SettingsPanelState` / `SettingsField` / `SettingsFieldType` types for editable fields
- New `settings_panel` panel rendering section headers + form controls (bool checkbox, int/float drag, string text, enum combo)
- App layer populates common settings (font_size, tab_size, word_wrap, etc.) and persists changes
- All Phase 4 UI features now complete ✅

**Previous session: Phase 4 UI: keybindings editor ✅**
- Keybindings editor overlay showing all keyboard shortcuts in a searchable table (Ctrl+K Ctrl+S)
- New `Action::ToggleKeybindingsEditor` action variant with keybinding `Ctrl+K Ctrl+S`
- New `KeybindingsEditorState` struct with visible flag, bindings list, and search query
- New `keybindings_editor` panel rendering a centered modal window with search + scrollable table
- App layer populates bindings list from `KeybindingEngine::bindings()` + `all_actions()` labels
- Case-insensitive search across both command name and key combo

**Previous session: Phase 4 UI: theme picker ✅**
- Implemented column/box selection via `Shift+Alt+drag` (like VS Code column select)
- New `column_select` flag on `PointerEvent::Press` detected when both Shift+Alt are held
- `column_select_anchor` field on `EditorTab` stores the press position for column selection
- On drag with Shift+Alt held, creates a rectangular block of cursors spanning all lines in the vertical range, each with a selection covering the horizontal column range
- Reuses existing `CursorSet::set_multi_selection()` to apply the box selection
- Column select anchor is cleared on mouse release alongside `drag_anchor`

**Previous session: Phase 4 UI: peek view ✅**
- Added peek view overlay (like VS Code Peek) for inline definition/reference preview
- New `PeekState` / `PeekKind` types in `UiState` with open/close/next/prev/selected_location
- New `peek_view` panel rendering a split overlay: location list (left) + code preview (right)
- Added `PeekDefinition`, `PeekReferences`, `PeekImplementation`, `PeekTypeDefinition`, `PeekDeclaration`, `ClosePeek` actions
- Default keybindings: `Alt+F12` peek definition, `Shift+F12` / `Ctrl+Shift+F12` peek references
- Peek uses existing LSP `textDocument/definition` etc. methods but stores results in peek state
- Working keyboard navigation (Up/Down/Enter/Escape), mouse click/double-click, close button
- LSP `LocationsReady` handler checks `pending_peek_method` flag to decide peek vs navigate

## Build status
- **GREEN** — `cargo check --workspace` zero warnings (pre-existing `resize_stable` dead_code warning only)
- **CLIPPY** — zero warnings
- **TESTS** — all workspace tests pass

## Cross-cutting
- [ ] Feature flag matrix test
- [ ] `crabide-workspace` crate tracking in Phase 1/2
