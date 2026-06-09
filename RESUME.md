# Resume — crabide project

> **MANDATORY:** Every session MUST update RESUME.md, update ROADMAP.md checkboxes,
> and call `handoff` with reason `"continuing to next roadmap item"` before ending.
> Never stop voluntarily — always hand off to continue until ROADMAP.md is 100% complete.

## Session summary

**Git submodule support added ✅**
- Added `SubmoduleInfo` core type with path, url, branch, commit, status fields
- Added `GitEvent` variants: `SubmodulesListed`, `SubmoduleAdded`, `SubmoduleUpdated`, `SubmoduleSynced`
- Added `GitCommand` variants: `ListSubmodules`, `SubmoduleAdd`, `SubmoduleUpdate`, `SubmoduleSync`
- Added `GitService` methods: `list_submodules()`, `submodule_add()`, `submodule_update()`, `submodule_sync()`
- Implemented git2 submodule operations: list (with status flags), add (setup + clone + finalize), update (init + fetch), sync
- Added `submodules` field to `GitPanelState` with UI listing showing status icons and commit hashes
- Wired all new events in app.rs event handler
- All workspace tests pass, zero warnings (clippy + check)

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
- **TESTS** — all ~990+ workspace tests pass

## Remaining roadmap items — pick next available

### Phase 6 (Git):
- [x] tag management, remote management
- [x] submodule support
- [ ] conflict resolution

### Phase 9 (Extensions):
- [ ] WASM editor/workspace/commands host implementations
- [ ] Registry download with checksum verification

### Phase 4 (UI):
- [ ] Minimap, split editor, context menu, drag-and-drop tabs, scrollbar annotations, peek view, output panel, settings UI, keybindings editor, theme picker, welcome screen, multi-cursor Alt+Click, column select mode

### Phase 12:
- [ ] Update checker, crash reporter, installers (Windows/macOS/Linux), CI workflows, performance pass, README/docs site

## Cross-cutting
- [ ] Feature flag matrix test
- [ ] `crabide-workspace` crate tracking in Phase 1/2
