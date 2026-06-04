# crabide — AI Context

## Goal
Resource-efficient, cross-platform code editor in **Rust + egui**. VS Code feature parity at <60MB idle RAM. Linux / macOS / Windows.

Binary name: `crabide`. 

## Stack
| Layer | Crate/Lib | Version |
|---|---|---|
| UI | egui + eframe (wgpu backend) | 0.33 |
| Layout | egui_tiles | 0.14 |
| Text buffer | ropey | 1.6 |
| Syntax | tree-sitter | 0.24 |
| LSP | custom JSON-RPC transport (no tower) | — |
| DAP | in-house types + transport | — |
| Terminal | vte parser + custom grid | 0.14 |
| PTY | portable-pty | 0.8 |
| Git | git2 (vendored-openssl) | 0.20 |
| Extensions | wasmtime (cranelift+component-model) | 34 |
| WIT | wit-bindgen (macros) | 0.34 |
| Async | tokio (rt-multi-thread) | 1 |
| CPU work | rayon | 1.11 |
| Fuzzy search | nucleo | 0.5 |
| Channels | crossbeam-channel | 0.5 |
| Concurrent map | dashmap | 5 |
| Bitflags | bitflags | 2 |
| Async traits | async-trait | 0.1 |
| File watch | notify + notify-debouncer-full | 6 / 0.3 |
| Error | thiserror 2 + anyhow 1 | — |
| Small strings | smol_str (+ serde feature) | 0.3 |

## Workspace Layout
```
Cargo.toml                    # workspace root — all versions declared once
.cargo/config.toml            # mold linker (Linux), zld (macOS), TREE_SITTER_DIR env
wit/crabide-extension.wit    # WIT extension API (complete — all interfaces defined)
assets/                       # icon-256.png placeholder (only README exists)
crates/
  crabide-app/               # binary: main.rs (Tokio+eframe), app.rs (VeloApp)
  crabide-core/              # types, errors, events, traits — no heavy deps
  crabide-buffer/            # Document, EditHistory, CursorSet, SnippetEngine
  crabide-syntax/            # tree-sitter grammars + highlight queries (stub)
  crabide-lsp/               # transport.rs (complete), client state (stub)
  crabide-dap/               # DAP debug client (stub)
  crabide-terminal/          # grid.rs (complete VT state machine), PTY manager (stub)
  crabide-git/               # git2 integration (stub)
  crabide-extensions/        # wasmtime host (stub), src/api/ (empty dir)
  crabide-vfs/               # VFS trait + local impl + file watcher (stub)
  crabide-config/            # TOML settings, keybindings, theme parser (stub)
  crabide-search/            # nucleo fuzzy + workspace grep (stub)
  crabide-workspace/         # document lifecycle, multi-root (stub)
  crabide-ui/                # all egui panels (stub), src/panels/ (empty dir)
```

## Threading Model
- **UI thread** — egui render loop, `try_recv()` channel drain per frame, never blocks
- **Tokio pool** — all async I/O: LSP, DAP, VFS, git, SSH, PTY bridge (`crabide-bg` threads)
- **Rayon pool** — CPU work: syntax highlighting, tree-sitter, grep, fuzzy indexing
- **WASM threads** — isolated wasmtime instances per extension, crash-isolated

Event bus: `crossbeam_channel::bounded(4096)` — all services → UI thread via `EditorEvent`.

## What Is Done

**crabide-core**
- `types.rs` — `Position`, `Range`, `Selection`, `TextEdit`, `BufferId`, `DocumentUri`, `DocumentId`, `ExtensionId`, `Language`, `language_from_extension()`
- `error.rs` — `VeloError` enum (18 variants) + `Result<T>` alias
- `event.rs` — full typed bus: `LspEvent`, `DapEvent`, `TerminalEvent`, `GitEvent`, `VfsEvent`, `ExtensionEvent`, `EditorEvent`; all shared protocol types incl. `CellAttrs` bitflags
- `traits.rs` — `TextBuffer`, `DocumentObserver`, `VirtualFileSystem` (async_trait)

**crabide-buffer**
- `buffer.rs` — `Document` backed by ropey, full `TextBuffer` impl, BOM/line-ending detection, `apply_edit/apply_edits`, O(1) `rope_snapshot()` for undo
- `history.rs` — `EditHistory`: 500-entry VecDeque, Rope snapshots, `begin_group/end_group`, named checkpoints, `undo/redo/jump_to_checkpoint`
- `cursor.rs` — `Cursor`, `CursorSet` (sorted, deduplicated multi-cursor), `SelectionMode`
- `snippet.rs` — `SnippetEngine`/`Snippet`/`SnippetTabstop` types; naive expand stub, tabstop cycling API shape

**crabide-lsp**
- `transport.rs` — complete JSON-RPC transport (~290 LOC): Content-Length framing, writer/reader Tokio tasks, `request()` (async oneshot), `notify()`, DashMap pending registry

**crabide-terminal**
- `grid.rs` — complete VT100/VT220 state machine (~590 LOC): SGR (24-bit, 256-color, all attrs), cursor movement, erase, 10k-line scrollback, alternate screen, OSC 0/2 (title), OSC 7 (cwd). **OSC 8 hyperlinks NOT yet implemented** (in header comment only).

**crabide-app**
- `main.rs` — env_logger, Tokio runtime, eframe NativeOptions (wgpu), 5s graceful shutdown
- `app.rs` — `VeloApp`/`EditorState`, full event dispatch skeleton, placeholder egui UI

**wit/crabide-extension.wit** — complete WIT definition for all extension API surfaces.

## Feature Flags
| Flag | Adds |
|---|---|
| *(default)* | core editor, LSP, terminal, git, extensions |
| `webview` | wry 0.49 — WebView panels |
| `remote-ssh` | russh 0.50 — SSH remote development |
| `dev-containers` | bollard 0.18 — Docker Dev Containers |

## Conventions
- All versions in root `Cargo.toml [workspace.dependencies]` — never duplicate in sub-crates
- Sub-crates: `workspace = "../.."` in `[package]`, deps as `foo.workspace = true`
- Optional deps (`wry`, `russh`, `bollard`) use `optional = true` in the **sub-crate's own** `[dependencies]`, NOT in `[workspace.dependencies]`
- `#[allow(dead_code)]` on intentional stubs awaiting wiring
- Never set `default-features = false` unless the stripped feature is confirmed unwanted — `tree-sitter` burned us (stripped `std` linkage)
- `smol_str` **requires** `features = ["serde"]` — not a default
- `DashMap` is internally Arc'd — clone it directly, never wrap in outer `Arc`
- vte 0.14 `advance()` takes `&[u8]` slice — not the per-byte `u8` API of 0.13
- UI thread must never block — only `try_recv()`, never `recv()`
= Never allow dead code.  Fix all errors and warnings properly by removing them if possible.
- Never take simplest solution.  Always take the right solution!
