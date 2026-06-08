# crabide roadmap

Build order follows dependency direction. Each phase builds on the phases before it.

**Legend:** ✅ = done (verified), 🔶 = partial, ❌ = not started

> **Audit date:** 2026-06-08. All statuses verified against actual source code. See git log for latest changes

## Crate & Dependency Minimization

Every external crate adds compile time, binary size, memory use, and supply-chain risk. Prioritize removing dependencies and consolidating crates wherever possible.

- **Prefer a 20-line local helper over pulling a crate.** Before adding any dependency, ask: can I write this in ≤50 lines? If yes, do that instead.
- **If a crate has only one use site, inline it.** One function from `once_cell`? Use `std::sync::OnceLock` (stable since Rust 1.70). One use of `regex-lite`? Fold the logic.
- **Granular crates are fine for separation of concerns but only when they reduce coupling.** If crate A and B always change together, merge them. The current 15 crates should be scrutinized: could some collapse?
- **Every `[dependencies]` entry must carry a brief rationale comment** explaining why a local helper wouldn't suffice. Uncommented deps will be removed.
- **Run `cargo-udeps` periodically** to find unused crates. Any crate declared but never imported gets deleted.
- **Feature flags are the right way to gate heavy dependencies** (wasmtime, bollard, russh, wry). Default build must stay lean.

## Autonomous Coding Best Practices

These rules exist because no human reviews intermediate steps. The agent must produce correct, review-ready code on the first pass across potentially hundreds of changes.

### Every Stop Must Compile Clean

After every compilable stop in any crate, run ALL of the following and fix every issue before continuing:

```powershell
cargo check --workspace 2>&1
cargo clippy --workspace 2>&1
cargo fmt --all 2>&1
```

- `cargo check` — zero warnings (deny all warnings in CI)
- `cargo clippy` — fix every lint, do not suppress
- `cargo fmt` — run before every commit
- **Never** add `#[allow(...)]`, `#[allow(dead_code)]`, or any suppression — fix the root cause
- **Never** leave `todo!()`, `unimplemented!()`, `dbg!()`, or `eprintln!()` in production code
- **Never** commit unused dependencies or dead code

### Incremental Correctness

- **One conceptual step per stop.** If a task requires adding a type, wiring it through a trait, and consuming it in the UI, that's 3+ stops. Do not batch them into one giant edit.
- **Commit early, commit often.** After every verified green build of a coherent unit, `git add -A && git commit -m "feature: what was done"`. This creates safe rollback points and a readable history.
- **If a commit would be >200 lines changed, split it.** Large diffs hide bugs.
- **Rebase locally to keep history linear** — no merge bubbles.

### Integration Awareness

- **Cross-crate changes must be validated together.** If you touch `crabide-config` and `crabide-ui` in the same logical change, build the full workspace (`cargo check --workspace`) before committing.
- **After wiring new event types**, verify the drain site in `crabide-app/src/app.rs` handles them (not just the `_ => {}` catch-all).
- **After adding a method to a trait** in `crabide-core`, update every implementation in dependent crates. The compile error will tell you where — fix all of them before moving on.

### Safe Edit Practices

- **Read the file first.** Never edit a file you haven't read in this session — you'll miss imports, naming conventions, and existing patterns.
- **Mimic existing patterns.** If neighboring code uses `thiserror` and `anyhow`, don't introduce a custom error enum. If the existing code uses `parking_lot::RwLock`, don't switch to `std::sync::RwLock`.
- **Prefer small, targeted edits over rewrites.** Changing 3 lines in 2 files is better than rewriting one file and deleting another.
- **When in doubt about architecture, ask.** Open a question instead of guessing the design intent.

### State Management

- **Never commit secrets, keys, or tokens.**
- **Never force-push to shared branches.**
- **Keep CI green.** If a stop breaks the build, fix it immediately or revert.
- **Document non-obvious decisions** in commit messages (e.g., "use RwLock over Mutex because reads dominate 10:1").
- **Keep ROADMAP.md in sync.** After every commit that completes a roadmap item, update the corresponding checkbox from `[ ]` to `[x]`. If a commit adds a new gap or feature not yet tracked, add it as a `[ ]` entry. The roadmap is the single source of truth for progress — stale checkboxes defeat its purpose.

---

## Phase 1 — Foundation ✓

### crabide-core — COMPLETE ✅
Core domain model, error hierarchy (24 variants), event bus (6 domains, 50+ variants), 3 core traits.

**Tidy-up:**
- [x] Add `Display` impls on event types for debug logging
- [x] Add `From<url::ParseError>` / `From<StripPrefixError>` impls
- [x] Add `DocumentId` Serialize/Deserialize + Display
- [x] Add missing LSP shared types: `DocumentSymbol`, `SignatureHelp`, `FoldingRange`, `SelectionRange`, `InlineCompletion`
- [x] Rename `PositionOutOfBounds.col` `character` for consistency

### crabide-config — PARTIAL 🔶
TOML settings (5 groups, 38 fields), keybinding engine (~80 default bindings), VS Code theme parser, 2 built-in themes, ConfigManager with file watcher.

**Gaps:**
- [x] Implement `KeybindingEngine::when` condition evaluation context system
- [x] Add per-language settings overlay (`[language.rust] tab_size = 4`)
- [x] Complete `all_actions()` with all ~80 Action variants (verified: 2-column enum + IndexMap)
- [x] Remove dead `once_cell` dependency
- [x] Add action registry API for extensions to register custom actions
- [x] Add 89 unit tests for keybindings (ActionRegistry, parse_chord, WhenCondition, WhenContext, KeybindingEngine, all_actions_with)
- [x] Add `keybindings.json` (VS Code format) import compatibility

### crabide-vfs — COMPLETE ✅
`LocalVfs` with full `VirtualFileSystem` impl, debounced `VfsWatcher`, URI↔path helpers.

**Gaps:**
- [x] Add VFS resolver/factory that selects impl by URI scheme
- [x] Add atomic writes (write to temp, rename)
- [x] Add MemoryVfs for testing
- [x] Add read-only VFS wrapper

### crabide-buffer — COMPLETE ✅
`Document` (ropey, BOM, line endings), `EditHistory` (500-entry, groups, checkpoints, undo/redo), `CursorSet` (multi-cursor, sorted, dedup), `SnippetEngine` (full VS Code syntax parser).

**Gaps:**
- [x] Fix `Transform` node expansion currently applies to `""` instead of referenced tabstop's text
- [x] Cache compiled regex in `apply_transform` (creates new `Regex` per call)
- [x] Add `CursorSet::remove()` / `iter()` methods
- [x] Add `Document::clear()` / `reload()` methods
- [x] No unit tests in crate

---

## Phase 2 — Syntax Highlighting ◐

### crabide-syntax — PARTIAL 🔶
Grammar registry (static + dynamic loading), highlight queries for 10 languages, highlight engine, outline for 8 languages, folding range extraction, SyntaxEngine with per-doc cache.

**Gaps:**
- [x] Fix `reparse_document()` to accept `InputEdit` for true incremental parsing
- [x] Implement indentation query runner (`IndentEngine` + `SyntaxEngine::indents()` active)
- [x] Implement locals/scope-aware queries (`LocalsEngine` + `SyntaxEngine::local_scopes()` active)
- [x] Implement `DocumentObserver` on `SyntaxEngine` to auto-parse on buffer changes
- [x] Dispatch parsing to Rayon thread pool (`rayon` dep declared, unused)
- [x] Add injection language support (embedded JS in HTML, Rust in Markdown)
- [x] Sort highlight spans in `compute_highlights()` (doc says sorted, never calls `.sort()`)
- [x] Add custom fold marker support (`// #region` / `// #endregion`)
- [ ] Add language support for: HTML, CSS/SCSS/LESS, YAML, Shell/Bash, SQL, Java, C#, Kotlin, Ruby, PHP
- [ ] No unit tests in crate (has 2 test modules in `indent.rs` and `locals.rs` — needs more coverage)

---

## Phase 3 — LSP Client ◐

### crabide-lsp — PARTIAL (~90%)

**Critical fixes:**
- [x] Fix crash detection: replace 30s polling stub with proper process-exit notification (child `wait()`)
- [x] Fix graceful shutdown: expose `LspTransport` from `LspClient` so `shutdown`/`exit` can actually be sent
- [x] Remove `#[allow(dead_code)]` from stubs throughout the crate (violates project convention)
- [x] Add server↔client request dispatch path (handle `workspace/applyEdit`, `workspace/configuration`, `client/registerCapability` in notification loop)
- [x] Fix `format()` / `format_range()` they reuse `RenameReady` event variant; add dedicated `FormattingReady`
- [x] Add request timeout or `request_with_timeout()` to transport

**Feature additions:**
- [x] Add `textDocument/semanticTokens/full` request method + event handling
- [x] Add `textDocument/codeLens` request method + event handling
- [x] Add notification handlers for: `willSave`/`willSaveWaitUntil`, `telemetry/event`, `textDocument/typeDefinition`
- [x] Add per-document version tracking in `LspClient`
- [x] Add `with_env()` builder method to `LspServerConfig`
- [x] Fix `From<Arc<LspClient>> for ServerEntry` panic use proper placeholder instead
- [x] Add `type_definition()` and `declaration()` request methods to LspClient
- [x] Add `will_save()` notification method to LspClient
- [x] Enable willSave/willSaveWaitUntil capabilities in initialize params
- [x] Add typeDefinition/declaration capabilities in initialize params

**Wiring in crabide-app:**
- [x] Wire GotoDefinition / References / Implementation / Declaration / TypeDefinition → LSP
- [x] Wire FormatDocument / FormatSelection → LSP
- [x] Wire RenameSymbol → LSP
- [x] Wire ShowHover → LSP
- [x] Wire TriggerCompletion → LSP
- [x] Wire ApplyCodeAction → LSP
- [x] Wire ShowSignatureHelp → LSP
- [x] Add hover popup UI rendering
- [x] Add completion popup UI rendering
- [x] Add code actions popup UI rendering
- [x] Add signature help popup UI rendering
- [x] Add `apply_workspace_edit()` helper in crabide-app
- [x] Add UI state fields for hover/completion/code_actions (hover_text, completion_items, completion_visible, code_actions, code_actions_visible)
- [x] Add inlay_hints/semantic_tokens/code_lens fields to EditorTab

---

## Phase 4 — UI & Editor Core ◐

### crabide-ui — PARTIAL (~65%)
Editor view, cursor, gutter, scrolling, panel layout, file explorer, tab bar, status bar, keyboard routing, command palette, find/replace — all implemented. Terminal panel, git panel, problems panel, extensions panel, debug panel, debug toolbar, workspace search — all implemented.

**Missing features:**
- [x] **Code folding gutter UI**: fold markers + expand/collapse controls
- [x] **Breadcrumbs**: path bar above editor showing symbol hierarchy
- [x] **Inlay hints**: render LSP inlay hints (parameter names, type hints) inline
- [ ] **Minimap**: scrollable code overview in sidebar
- [ ] **Split editor**: side-by-side file comparison / multi-pane layouts
- [ ] **Context menu**: right-click with editor/file-explorer/tab actions + extension contributions
- [ ] **Drag-and-drop tab reordering** in tab bar
- [ ] **Scrollbar annotations**: diagnostic markers, search result highlights, git diff markers
- [ ] **Peek view**: inline definition/reference preview (like VS Code peek)
- [ ] **Output panel**: wire `ToggleOutputPanel` to actual panel
- [ ] **Settings UI panel**: visual editor for `settings.toml`
- [ ] **Keybindings editor UI**
- [ ] **Theme picker UI**
- [ ] **Welcome screen**: interactive cards (not just decorative)
- [ ] **Multi-cursor Alt+Click**: wire Alt+Click to add cursor (currently only keyboard Ctrl+D)
- [ ] **Column select mode**: wire Shift+Alt+drag

### crabide-buffer → crabide-ui wiring
- [x] Wire `SnippetEngine` tabstop UI: active tabstop highlight, Tab/Shift+Tab cycling
- [x] Fix `SnippetEngine::expand()` Transform node to reference actual tabstop text
- [x] Add incremental placeholder update during typing

---

## Phase 5 — Search ◐

### crabide-search — PARTIAL 🔶
Fuzzy file finder (nucleo), workspace grep (rayon), Go-to-line.

**Gaps:**
- [x] Wire auto-reindex on VFS file change events
- [x] Add cancellation support for grep (AbortHandle)
- [x] Add incremental search (debounce + streaming results via background thread)
- [x] Add search-in-open-buffers support (search unsaved `Document` contents)
- [x] Remove dead `regex-lite` dependency from workspace Cargo.toml (still declared but unused in any crate)
- [x] Cache `nucleo::Matcher` instance across search calls
- [x] Implement Go-to-symbol (Ctrl+Shift+O) uses crabide-syntax outline

---

## Phase 6 — Git ◐

### crabide-git — PARTIAL 🔶
Status, diff hunks, blame, stage/unstage, commit, branch, discard.

**Gaps:**
- [ ] Add fetch / pull / push / merge / rebase
- [x] Add branch listing (local + remote)
- [x] Add branch deletion
- [ ] Add stash (push, pop, list, drop)
- [ ] Add log / history / graph view
- [ ] Add tag management
- [ ] Add remote management (add, remove, list)
- [ ] Add submodule support
- [x] Add diff for staged changes (index vs HEAD)
- [ ] Add conflict resolution helpers
- [x] Remove dead `tokio`, `rayon`, `thiserror`, `anyhow` dependencies (unused)

---

## Phase 7 — Terminal ◐

### crabide-terminal — PARTIAL (~85%)
Grid state machine (SGR, cursor, erase, scrollback, alt screen, OSC 0/2/7), PTY spawn/read/write/resize, manager with profiles.

**Gaps:**
- [x] Implement OSC 8 hyperlinks (parse `\e]8;...;url\a...\e]8;;\a` → clickable links)
- [x] Implement DECSTBM (scroll regions) — needed for `less`/`vim`/`tmux`
- [x] Implement Insert/Delete Line (CSI L / CSI M)
- [x] Implement Insert/Delete Character (CSI @ / CSI P)
- [x] Implement mouse reporting (DECSET 1000/1002/1003)
- [x] Implement bracketed paste mode (DECSET 2004)
- [x] Implement cursor visibility toggle (DECSET 25)
- [x] Implement ESC M reverse index (RI) with scroll region support
- [x] Implement content reflow on terminal resize
- [x] Add configurable color scheme / theme to TerminalProfile
- [ ] Add Unicode width proper crate to replace approximate `unicode_width()`
- [x] Implement OSC 133 shell integration markers (prompt start/end)
- [x] No unit tests in crate (now has 102 tests)

---

## Phase 8 — DAP Debugger ◐

### crabide-dap — PARTIAL (~70%)
All DAP types defined, Content-Length transport complete, DapClient with launch/breakpoints/continue/step/stack/variables.

**Critical fixes:**
- [x] Fix `resolve_adapter()` stub add adapter-type registry (pythondebugpy, nodejs-debug, lldbcodelldb, etc.)
- [x] Add request timeout to `DapTransport::request()`
- [x] Properly discard capabilities from initialize response (currently `Ok(_)` ignored)

**Feature additions:**
- [ ] Implement `attach` workflow (connect to running process)
- [ ] Implement `evaluate` request (debug console REPL)
- [ ] Implement `setVariable` / `setExpression` (write variable values)
- [ ] Implement `gotoTargets` / `goto` (run to cursor)
- [ ] Implement `threads` request (list all threads)
- [ ] Implement `exceptionInfo` request (exception details on stop)
- [ ] Implement function breakpoints (`setFunctionBreakpoints`)
- [ ] Implement exception breakpoints (`setExceptionBreakpoints`)
- [ ] Implement data breakpoints / watchpoints
- [ ] Implement `runInTerminal` reverse-request handler
- [ ] Implement progress events (ProgressStart/Update/End)
- [ ] Implement `InvalidatedEvent` handler
- [ ] Implement `completions` request (tab-completion in debug console)
- [ ] Implement cancellation support (`CancelParams`)
- [ ] Add backpressure to writer (unbounded channel → bounded + semaphore)

---

## Phase 9 — Extensions ◐

### crabide-extensions — PARTIAL 🔶 (~60%)
NativeExtension trait, ExtensionHost, 5 built-in extensions, registry client, hot-reload, WASM extension loader with full WIT binding.

**WASM extension host (wasm_ext.rs):**
- [x] WASM component loading, compilation, instantiation (wasmtime `CrabideExtension::load`)
- [x] Host implementations for: diagnostics, status bar, terminal I/O, gutter markers, panel show/hide
- [x] WIT bindgen integration with full crabide-extension.wit world
- [ ] Implement `editor::Host::get_document_slice()` — enable cross-document access for WASM extensions
- [ ] Implement `editor::Host::apply_edits()` — enable WASM extensions to modify documents
- [ ] Implement `editor::Host::insert_at_cursor()` 
- [ ] Implement `editor::Host::get_cursor_position()` / `set_cursor_position()`
- [ ] Implement `editor::Host::get_selection_text()`
- [ ] Implement `workspace::Host::get_workspace_roots()`
- [ ] Implement `workspace::Host::find_files()`
- [ ] Implement `commands::Host::execute_command()` / `show_quick_pick()` / `show_input_box()`
- [ ] Implement `status_bar::Host::set_visible()`
- [ ] Implement `terminal::Host::list_terminals()`
- [ ] Implement `panels::Host::is_panel_visible()`

**Registry & lifecycle:**
- [ ] Implement registry download (actual ureq HTTP download with checksum verification)
- [ ] Implement `ExtensionHost::install_registry()` — actual download + verify + install flow
- [ ] Add capability enforcement (check `ExtensionCapabilities` before granting resource access)
- [ ] Add WASM engine resource limits (memory cap, fuel metering, execution timeout)
- [ ] Add marketplace URL configuration

---

## Phase 10 — Polish & Release ◐

### crabide-app — PARTIAL (~60%)

**Remaining items:**
- [x] Wire folding ranges from SyntaxEngine into EditorTab.folding_ranges
- [x] Wire FindInFiles to search open buffers (grep_buffers) in addition to disk files
- [x] Use real application icon from `assets/` (icons exist but `main.rs` still uses 2×2 amber placeholder)
- [x] CLI argument parsing (manual parser with `-h`/`-V`/`--log` support — NOT using clap to minimize deps)
- [x] `Ctrl+C` signal handler for graceful shutdown
- [ ] Window state persistence (size, position, maximized state)
- [ ] Session restore (reopen files from last session)

### Phase 12 items:
- [ ] Settings UI panel (visual editor for settings.toml)
- [ ] Keybindings editor UI
- [ ] Theme picker UI
- [ ] Welcome / splash screen (interactive)
- [ ] Update checker (ureq → GitHub releases)
- [ ] Crash reporter (panic hook → file)
- [ ] Windows installer (NSIS or WiX)
- [ ] macOS `.app` bundle + code signing + notarization
- [ ] Linux `.AppImage` + `.deb` / `.rpm`
- [ ] CI release artifacts (`.github/workflows/` dir exists but no workflow file — needs scaffolding)
- [ ] Performance pass: egui frame time, LSP round-trip latency, heap profiling
- [ ] README, docs site

---

## Git Commit Pipeline

Every task in the roadmap is completed through the following automated pipeline:

1. **Read** understand the codebase area
2. **Edit** implement the change (one conceptual step per stop)
3. **Verify** `cargo check --workspace && cargo clippy --workspace && cargo fmt --all`
4. **Update ROADMAP.md** mark completed items `[x]`, add new items `[ ]` if needed
5. **Stage** `git add -A`
6. **Commit** `git commit -m "TYPE: concise summary"`

Commit message types:
| Prefix | When |
|---|---|
| `feat` | New feature or capability |
| `fix` | Bug fix |
| `perf` | Performance improvement |
| `refactor` | Code restructuring with no behavior change |
| `test` | Adding or updating tests |
| `docs` | Documentation only |
| `chore` | Build, CI, dependencies, tooling |
| `roadmap` | Roadmap file updates |

Commit rules:
- **Subject ≤ 72 chars.** No trailing period.
- **One commit per conceptual step.** If it takes 5 stops to implement a feature, that's 5 commits.
- **Never batch unrelated changes.** A refactor and a feature go in separate commits.
- **Every commit compiles.** `cargo check --workspace` passes before every commit.
- **Commit messages use imperative mood:** "add", "fix", "wire", "remove", not "added", "fixed".

Example commit sequence for a hypothetical feature:
```
feat: add SshVfs stub with feature gate
feat: implement SshVfs::read_file with russh channel
feat: wire SshVfs into VfsResolver by URI scheme
fix: handle SshVfs auth failure gracefully
test: add integration test for SshVfs round-trip
```

---

## Cross-cutting Concerns

These aren't tied to any single phase:

- [x] **Dead dependency cleanup**: Removed unused deps from individual crate `Cargo.toml`s (`once_cell` from config, `tokio`/`rayon`/`thiserror`/`anyhow` from git, `serde` from syntax, `uuid` from workspace)
- [x] **Workspace-level dep cleanup**: `regex-lite` removed from workspace (commit `76cbbf0`). `crossbeam-channel` removed from `crabide-syntax` (same commit). Verified — no traces remain.
- [x] `#[allow(dead_code)]` removal: Fix or remove all dead-code suppressions (verified — none remain in production code)
- [x] **Unit test coverage**: `crabide-core` (140), `crabide-buffer` (47), `crabide-config` (105), `crabide-ui` (112), `crabide-app` (43), `crabide-vfs` (43), `crabide-terminal` (102), `crabide-extensions` (54), `crabide-dap` (43), `crabide-search` (38), `crabide-workspace` (25), `crabide-lsp` (19), `crabide-git` (3), `crabide-syntax` (57). Minimum coverage targets: 30% by v0.1
- [ ] **`docs/` directory**: Currently empty
- [ ] **Feature flag matrix test**: CI should test all feature flag combinations (`wasm-extensions`, `webview`, `remote-ssh`, `dev-containers`)
- [ ] **`crabide-workspace` crate**: Exists at `crates/crabide-workspace` (workspace/document lifecycle management). Implemented as a central hub connecting VFS, buffers, and observers. Should be tracked as part of Phase 1/2 since it depends on core, buffer, vfs and is consumed by app.