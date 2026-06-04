# RESUME.md

## Session Summary
Executed major portions of the crabide roadmap across Phases 1-8, plus cross-cutting cleanup.

## What was done (10 commits)

### Cross-cutting
- **Removed dead dependencies** across 5 crates: `once_cell` (config, syntax), `regex-lite` (search), `tokio`/`rayon`/`thiserror`/`anyhow` (git), `serde`/`crossbeam-channel` (syntax), `uuid`/`serde`/`serde_json`/`thiserror`/`anyhow`/`crossbeam-channel` (workspace)
- Replaced `once_cell::sync::Lazy` with `std::sync::OnceLock` in syntax grammar registry
- **Removed `#![allow(dead_code)]`** from 4 LSP files (client.rs, config.rs, convert.rs, server_mgr.rs), fixed the one actual dead code item (`SYNC_NONE` constant)
- Fixed pre-existing UI bugs: duplicate extension panel builder chains, `ctx` vs `ui` borrow conflicts, deprecated egui API calls

### Phase 1 - Core
- Added `Display` impls on all event types (LspEvent, DapEvent, TerminalEvent, GitEvent, VfsEvent, ExtensionEvent, EditorEvent)
- Added `From<url::ParseError>` and `From<StripPrefixError>` impls for `crabideError`
- Added `Serialize`/`Deserialize` + `Display` on `DocumentId`
- Added missing LSP shared types: `DocumentSymbol`, `SymbolKind`, `SignatureHelp`, `SignatureInformation`, `ParameterInformation`, `ParameterLabel`, `FoldingRange`, `FoldingRangeKind`, `SelectionRange`, `InlineCompletionItem`
- Renamed `PositionOutOfBounds.col` → `character` for consistency

### Phase 1 - Buffer
- Added `CursorSet::remove()` and `CursorSet::iter()` methods
- Added `Document::clear()` and `Document::reload()` methods
- Fixed `SnippetEngine` Transform node: now references actual tabstop text instead of empty string
- Cached compiled regex in `Expander::apply_transform()` via `RefCell<BTreeMap>`

### Phase 1 - VFS
- Added `VfsResolver` factory that selects VFS impl by URI scheme (file://, memory://, untitled://)
- Implemented atomic writes in `LocalVfs` (write to .crabide-tmp then rename)
- Added `MemoryVfs` for testing (in-memory HashMap-based VFS)
- Added `ReadOnlyVfs<T>` wrapper that blocks all write operations

### Phase 2 - Syntax
- Added `.sort_by_key()` in `compute_highlights()` to sort spans by start position
- Updated `reparse_document()` to accept `Option<tree_sitter::InputEdit>` for true incremental parsing

### Phase 3 - LSP
- Added dedicated `LspEvent::FormattingReady` variant (format/format_range no longer reuse RenameReady)
- Added `LspTransport::request_with_timeout()` with optional Duration
- Exposed `LspClient::transport()` for graceful shutdown
- Fixed `From<Arc<LspClient>> for ServerEntry` panic (now creates no-op lifecycle handle)
- Added server→client request dispatch stubs (workspace/applyEdit, workspace/configuration, client/registerCapability)

### Phase 5 - Search
- Cached `nucleo::Matcher` instance in `FuzzyFileFinder` across search calls (was creating new one per call)

### Phase 8 - DAP
- Implemented `resolve_adapter()` with adapter-type registry (python/debugpy, node/js-debug, lldb, gdb, codelldb)
- Added `adapter_type` and `port` fields to `LaunchConfig`
- Added `DapTransport::request_with_timeout()` with optional Duration
- Properly discard capabilities from initialize response (log them instead of ignoring)

## What remains
- Phase 9 extensions: Implement WASM host stubs, capability enforcement, resource limits
- Phase 10 app: CLI arg parsing, Ctrl+C handler, window state persistence
- Many more items in each phase (see ROADMAP.md for full list)
- Unit tests across all crates (currently 0% coverage)
- Dep rationale comments on all `[dependencies]` entries

## Build status
All green: `cargo check --workspace`, `cargo clippy --workspace`, `cargo fmt --all` pass with zero warnings.
