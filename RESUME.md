# Resume — crabide project

> **MANDATORY:** Every session MUST update RESUME.md, update ROADMAP.md checkboxes,
> and call `handoff` with reason `"continuing to next roadmap item"` before ending.
> Never stop voluntarily — always hand off to continue until ROADMAP.md is 100% complete.

## Session summary

**Phase 4 UI: split editor (multi-pane layouts) ✅**
- Added `EditorGroup` struct with its own tabs/active_tab, replaced single `tabs`/`active_tab` with `editor_groups: Vec<EditorGroup>` + `active_group: usize`
- Added backward-compatible helper methods (`tabs()`, `tabs_mut()`, `active_tab()`, etc.)
- Updated `PaneKind` to `EditorGroup(usize)` for identifying which group a tile displays
- Wired `SplitEditorRight`, `SplitEditorDown`, `CloseEditor` actions in `handle_ui_action`
- Split creates a new editor group, moves the active tab, and rebuilds the layout
- CloseEditor merges tabs back to group 0 and restores single-editor layout
- Editor panel `show_for_group()` renders per-pane content correctly
- Updated all UI panels and app layer to use new accessor methods
- All workspace builds clean, zero clippy warnings (pre-existing `resize_stable` only)
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

### Phase 4 (UI) — in progress:
- [x] Minimap, context menu, welcome screen
- [ ] Split editor: side-by-side file comparison / multi-pane layouts
- [ ] Drag-and-drop tab reordering in tab bar
- [ ] Scrollbar annotations: diagnostic markers, search result highlights, git diff markers
- [ ] Peek view: inline definition/reference preview (like VS Code peek)
- [ ] Output panel: wire `ToggleOutputPanel` to actual panel
- [ ] Settings UI panel: visual editor for `settings.toml`
- [ ] Keybindings editor UI
- [ ] Theme picker UI
- [ ] Multi-cursor Alt+Click: wire Alt+Click to add cursor
- [ ] Column select mode: wire Shift+Alt+drag

### Phase 12:
- [ ] Update checker, crash reporter, installers (Windows/macOS/Linux), CI workflows, performance pass, README/docs site

## Cross-cutting
- [ ] Feature flag matrix test
- [ ] `crabide-workspace` crate tracking in Phase 1/2
