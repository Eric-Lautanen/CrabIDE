# RESUME.md — CrabIDE Session Handoff

## Current State: BROKEN BUILD (intentional — mid-implementation)

The workspace does **not compile** right now. The LSP wiring in `crabide-app` is half-done.
The `crabide-lsp` crate itself compiles fine — all errors are in `crabide-app/src/app.rs`.

## What Was Done This Session

### Completed (committed & pushed):
1. **Notification handlers** in `crabide-lsp/src/client.rs`:
   - `telemetry/event`, `textDocument/willSave`, `textDocument/willSaveWaitUntil`
   - `will_save()` notification method added
   - `type_definition()` and `declaration()` request methods added
   - `willSave: true` and `willSaveWaitUntil: true` in initialize capabilities
   - `typeDefinition` and `declaration` capabilities added

2. **Per-document version tracking** in `crabide-lsp/src/client.rs`:
   - `doc_versions: RwLock<HashMap<DocumentUri, u32>>` in `LspClientInner`
   - `did_open()` stores initial version
   - `did_change()` auto-increments (signature changed: removed `version` param)
   - `did_close()` cleans up version entry
   - `doc_version()` getter added

3. **`with_env()` builder** in `crabide-lsp/src/config.rs`

4. **LSP action wiring** in `crabide-app/src/app.rs` (IN PROGRESS — broken):
   - Added `lsp_manager: LspServerManager` and `lsp_request_id: Arc<AtomicU32>` to `crabideApp`
   - Replaced stub action handlers with real LSP calls (GotoDefinition, Format, Rename, Hover, Completion, CodeActions)
   - Added LSP helper methods: `active_language()`, `active_uri_and_position()`, `lsp_goto()`, `lsp_format()`, `lsp_hover()`, `lsp_complete()`, `lsp_code_actions()`, `lsp_rename()`
   - Added LSP event handlers in `apply_lsp_event()` for: LocationsReady, HoverReady, CompletionReady, FormattingReady, RenameReady, CodeActionsReady, LogMessage, InlayHintsUpdated, SemanticTokensUpdated, CodeLensUpdated

## What Needs To Be Fixed (42 compile errors)

### Error Category 1: LSP helper methods placed inside `impl eframe::App for crabideApp` instead of `impl crabideApp`
The methods `active_language`, `active_uri_and_position`, `lsp_goto`, `lsp_format`, `lsp_hover`, `lsp_complete`, `lsp_code_actions`, `lsp_rename` were inserted before `fn on_exit()` which is inside the `eframe::App` trait impl block. They need to be in the inherent `impl crabideApp` block instead.

**Fix**: Move these methods from the `impl eframe::App` block to the `impl crabideApp` block. The `impl crabideApp` block starts at line ~111 and the `impl eframe::App` block starts later. The LSP methods were inserted around line 2986 (inside the eframe::App impl). They should be moved to before the eframe::App impl block starts.

### Error Category 2: Missing UiState fields
The `apply_lsp_event` handler references fields that don't exist on `UiState`:
- `hover_text: Option<String>` 
- `completion_items: Vec<crabide_core::event::CompletionItem>`
- `completion_visible: bool`
- `code_actions: Vec<crabide_core::event::CodeAction>`
- `code_actions_visible: bool`

**Fix**: Add these fields to `crabide-ui/src/state.rs` in the `UiState` struct and initialize them in `UiState::new()`.

### Error Category 3: Missing EditorTab fields
The `apply_lsp_event` handler references fields that don't exist on `EditorTab`:
- `inlay_hints: Vec<crabide_core::event::InlayHint>`
- `semantic_tokens: Vec<crabide_core::event::SemanticToken>`
- `code_lens: Vec<crabide_core::event::CodeLens>`

**Fix**: Add these fields to `crabide-ui/src/state.rs` in the `EditorTab` struct and initialize them in `EditorTab::new()`.

### Error Category 4: Missing `apply_workspace_edit()` method
Called in `apply_lsp_event` for FormattingReady and RenameReady but doesn't exist yet.

**Fix**: Add `fn apply_workspace_edit(&mut self, edit: WorkspaceEdit)` to `impl crabideApp` that iterates `edit.document_changes`, applies each `DocumentEdit` via `self.workspace.apply_edits()`, and syncs the tab.

### Error Category 5: Minor type issues
- `word_at_cursor()` returns `String`, not `Option<String>` — wrap in `Some()`
- `lsp_code_actions(&self)` needs `&mut self` because it calls `self.ui_state.set_status()`

## File Locations

- **crabide-lsp client**: `crates/crabide-lsp/src/client.rs` (compiles fine)
- **crabide-lsp config**: `crates/crabide-lsp/src/config.rs` (compiles fine)
- **crabide-lsp server_mgr**: `crates/crabide-lsp/src/server_mgr.rs` (compiles fine)
- **crabide-app main**: `crates/crabide-app/src/app.rs` (BROKEN — 42 errors)
- **crabide-ui state**: `crates/crabide-ui/src/state.rs` (needs new fields)
- **ROADMAP**: `ROADMAP.md` (updated with current progress)

## Priority Order for Next Session

1. Move LSP helper methods from `impl eframe::App` to `impl crabideApp`
2. Add missing fields to `UiState` and `EditorTab` in `crabide-ui/src/state.rs`
3. Add `apply_workspace_edit()` method
4. Fix `word_at_cursor` return type wrapping and `&mut self` on `lsp_code_actions`
5. Get `cargo check --workspace` passing with zero errors
6. Run `cargo clippy --workspace` and fix warnings
7. Commit and push
8. Add hover popup UI rendering in `crabide-ui`
9. Add completion popup UI rendering in `crabide-ui`
10. Add code actions popup UI rendering in `crabide-ui`
11. Wire `did_open`/`did_change`/`did_close`/`did_save` notifications when tabs open/edit/save/close
12. Auto-start LSP servers when a folder is opened (based on file extensions)

## Key Architecture Notes

- `LspServerManager` is in `crates/crabide-lsp/src/server_mgr.rs` — manages server processes
- `LspClient` is in `crates/crabide-lsp/src/client.rs` — one per running server, cheaply cloneable (Arc internally)
- `LspServerManager::get_client(&language)` returns `Option<Arc<LspClient>>`
- LSP requests are async (fire-and-forget from UI thread, responses arrive as `LspEvent` on the crossbeam channel)
- The `lsp_request_id` counter is `Arc<AtomicU32>` for generating unique request IDs
- `did_change()` signature changed: no longer takes `version` param (auto-incremented internally)
