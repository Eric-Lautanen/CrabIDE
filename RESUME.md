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

**Phase 4 UI: peek view ✅**
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
