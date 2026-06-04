# crabide

Resource-efficient, cross-platform code editor. Rust + egui. VS Code feature parity at <60MB idle RAM.

Linux / macOS / Windows.

## Quick Start

```bash
cargo run --release
```

Opens an empty editor. Open files with `Ctrl+O` or pass paths as CLI args.

## Status

See [ROADMAP.md](ROADMAP.md) for completion state of all 14 crates across 10 phases.

## Architecture

```
crabide-app/        # eframe binary, event dispatch, action wiring
crabide-core/       # types, errors, events, traits — zero heavy deps
crabide-buffer/     # Document (ropey), EditHistory, CursorSet, SnippetEngine
crabide-syntax/     # tree-sitter grammar registry, highlighting, outline, folding
crabide-lsp/        # JSON-RPC transport, LSP client (initialize, didChange, completions, etc.)
crabide-dap/        # DAP types, transport, debug client (launch, breakpoints, step, stack)
crabide-terminal/   # VT100/VT220 grid state machine, PTY spawn via portable-pty
crabide-git/        # git2 integration: status, diff, blame, stage, commit, branch
crabide-extensions/ # NativeExtension trait, WASM host (wasmtime), 5 built-in extensions
crabide-vfs/        # VirtualFileSystem trait, LocalVfs, debounced file watcher
crabide-config/     # TOML settings, keybinding engine, VS Code theme parser
crabide-search/     # Fuzzy file finder (nucleo), workspace grep (rayon)
crabide-workspace/  # Multi-root document lifecycle manager
crabide-ui/         # All egui panels: editor, gutter, tab bar, status bar, file explorer,
                    # find/replace, command palette, terminal, git, debug, extensions, search
```

## Feature Flags

| Flag | Adds |
|---|---|
| *(default)* | Core editor, LSP, terminal, git, extensions |
| `wasm-extensions` | WASM extension runtime (wasmtime + cranelift) |
| `webview` | WebView extension panels (wry) |
| `remote-ssh` | SSH remote development (russh) |
| `dev-containers` | Docker Dev Containers (bollard) |

## License

MIT or Apache-2.0 at your option.
