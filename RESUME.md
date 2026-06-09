# Resume — crabide project

> **MANDATORY:** Every session MUST update RESUME.md, update ROADMAP.md checkboxes,
> and call `handoff` with reason `"continuing to next roadmap item"` before ending.
> Never stop voluntarily — always hand off to continue until ROADMAP.md is 100% complete.

## Session summary

**WASM engine resource limits + marketplace URL configured ✅**
- Added memory cap (64 MB per instance) via `Config::wasm_memory_maximum_size()`
- Added fuel metering (100,000 fuel per call, 80,000 warning threshold) via `Config::consume_fuel()` + `Store::add_fuel()`
- Added execution timeout via epoch-based interruption (`Config::epoch_interruption()` + `Store::set_epoch_deadline(1)`)
- Added `apply_limits()` / `check_fuel()` helper methods on `WasmExtension` to enforce limits before/after every WIT guest call
- Updated all 13 `NativeExtension` trait methods (activate, deactivate, document events, cursor events, execute_command, gutter, hover, completions, terminal_output) to call `apply_limits()` before and `check_fuel()` after each WASM invocation
- Added `ExtensionsSettings` struct with `marketplace_url` field to `crabide-config` settings (TOML serializable)
- Added `registry_client` field to `ExtensionHost`, with `set_marketplace_url()` method that propagates to `RegistryClient`
- Wired marketplace URL from config settings to both `RegistryClient` and `ExtensionHost` during app startup
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

### Phase 9 (Extensions) — COMPLETE ✅:
- [x] WASM editor/workspace/commands host implementations
- [x] Registry download with checksum verification (install_registry, download + verify + install flow)
- [x] Capability enforcement (check ExtensionCapabilities before granting resource access)
- [x] WASM engine resource limits (memory cap, fuel metering, execution timeout)
- [x] Marketplace URL configuration

### Phase 4 (UI) — next up:
- [ ] Minimap, split editor, context menu, drag-and-drop tabs, scrollbar annotations, peek view, output panel, settings UI, keybindings editor, theme picker, welcome screen, multi-cursor Alt+Click, column select mode

### Phase 12:
- [ ] Update checker, crash reporter, installers (Windows/macOS/Linux), CI workflows, performance pass, README/docs site

## Cross-cutting
- [ ] Feature flag matrix test
- [ ] `crabide-workspace` crate tracking in Phase 1/2
