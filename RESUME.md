# Resume — crabide project

> **MANDATORY:** Every session MUST update RESUME.md, update ROADMAP.md checkboxes,
> and call `handoff` with reason `"continuing to next roadmap item"` before ending.
> Never stop voluntarily — always hand off to continue until ROADMAP.md is 100% complete.

## Session summary

**Phase 4 UI: minimap, context menu, welcome screen interactive cards ✅**
- Wired minimap panel into editor (registered module, added `minimap_visible` state, wired `ToggleMinimap` action, renders as right-side panel)
- Implemented right-click context menu with built-in editor actions (Cut, Copy, Paste, Select All) at right-click position; added `ContextMenuState` to UiState; dismisses on Escape or outside click; supports extension contributions via `registered_context_menus`
- Made welcome screen cards interactive: changed from `Sense::hover()` to `Sense::click()`, mapped card labels to actions (New File, Open File, Command Palette, etc.), cards now respond to clicks with proper action dispatch
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
