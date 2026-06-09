# crabide

**Resource-efficient, cross-platform code editor.** Rust + egui. VS Code feature parity at <60MB idle RAM.

Linux / macOS / Windows.

## Features

- **Lightweight** — <100MB idle RAM, <200MB on disk
- **LSP client** — code completion, hover, go-to-definition, references, rename, diagnostics, code actions, signature help, inlay hints, semantic tokens, code lens, folding, document symbols, formatting
- **DAP debugger** — launch/attach, breakpoints (function, exception, data), step, stack, variables, debug console, threads, run-to-cursor, cancellation
- **Integrated terminal** — full VT100/VT220 grid emulation with PTY support, scrollback, alt screen, bracketed paste, mouse reporting, hyperlinks (OSC 8), shell integration (OSC 133)
- **Git integration** — status, diff, blame, stage/unstage, commit, branch, fetch/pull/push, stash, log/history, tags, remotes, submodules, conflict resolution
- **Syntax highlighting** — tree-sitter for 22 languages with incremental parsing, injection languages, fold markers, outline, indent queries, locals/scope
- **Multi-cursor editing** — Alt+Click, column select (Shift+Alt+drag), Ctrl+D word select, snippet tabstop navigation
- **Workspace search** — fuzzy file finder (Ctrl+P), workspace grep (Ctrl+Shift+F), search in open buffers, go-to-symbol (Ctrl+Shift+O), go-to-line (Ctrl+G)
- **Extensions** — WASM extension runtime (wasmtime), 5 built-in extensions, marketplace registry
- **Code navigation** — peek view (Alt+F12), breadcrumbs, minimap, folding gutter, scrollbar annotations (diagnostics, search, git)
- **Editor UI** — split editor, tab bar with drag-and-drop reordering, file explorer, command palette, settings editor, keybindings editor, theme picker, welcome screen, context menu
- **Terminal multiplexer** — multiple terminals, profiles, color schemes
- **Session persistence** — window state, open files restored on restart
- **Auto-update checker** — background check for new releases
- **Crash reporter** — panic logs to `~/.crabide/crash.log`

## Quick Start

```bash
# Clone and build
git clone https://github.com/crabide-editor/crabide
cd crabide
cargo run --release
```

Opens an empty editor. Open files with `Ctrl+O` or pass paths as CLI args:

```bash
cargo run --release -- path/to/file.rs path/to/directory
```

### CLI Options

| Option | Description |
|--------|-------------|
| `-h`, `--help` | Show help message |
| `-V`, `--version` | Print version and exit |
| `-l`, `--log <LEVEL>` | Set log level (trace, debug, info, warn, error) |

## Installation

### Pre-built binaries

Download from [GitHub Releases](https://github.com/crabide-editor/crabide/releases).

| Platform | Architecture | Format |
|----------|-------------|--------|
| Windows | x86_64 | `.exe` (portable) or installer |
| macOS | ARM64 (M1+) | `.app` bundle or DMG |
| macOS | x86_64 | `.app` bundle or DMG |
| Linux | x86_64 | AppImage, `.deb`, or `.rpm` |

### Package managers

| OS | Command |
|----|---------|
| macOS (Homebrew) | `brew install crabide` |
| Linux (AUR) | `yay -S crabide` |

*(coming soon)*

### Build from source

**Prerequisites:**
- Rust 1.80+
- Linux: `libgtk-3-dev`, `libxcb-render0-dev`, `libxcb-shape0-dev`, `libxcb-xfixes0-dev`, `libxkbcommon-dev`, `libssl-dev`, `libgit2-dev`, `pkg-config`, `clang`

```bash
# Default build (git support included)
cargo build --release

# Minimal build (no git support)
cargo build --release --no-default-features

# All features
cargo build --release --all-features
```

## Feature Flags

| Flag | Adds | Size Impact |
|------|------|-------------|
| *(default)* | Core editor, LSP, terminal, git, extensions | ~200MB on disk |
| `wasm-extensions` | WASM extension runtime (wasmtime + cranelift) | +~50MB RSS |
| `webview` | WebView extension panels (wry) | +~30MB |
| `remote-ssh` | SSH remote development (russh) | +~15MB |
| `dev-containers` | Docker Dev Containers (bollard) | +~25MB |

## Architecture

```
crabide-app/        # eframe binary, event dispatch, action wiring
crabide-core/       # types, errors, events, traits — zero heavy deps
crabide-buffer/     # Document (ropey), EditHistory, CursorSet, SnippetEngine
crabide-syntax/     # tree-sitter grammar registry, highlighting, outline, folding
crabide-lsp/        # JSON-RPC transport, LSP client
crabide-dap/        # DAP types, transport, debug client
crabide-terminal/   # VT100/VT220 grid state machine, PTY spawn
crabide-git/        # git2 integration: status, diff, blame, stage, commit, branch
crabide-extensions/ # NativeExtension trait, WASM host, 5 built-in extensions
crabide-vfs/        # VirtualFileSystem trait, LocalVfs, debounced file watcher
crabide-config/     # TOML settings, keybinding engine, VS Code theme parser
crabide-search/     # Fuzzy file finder (nucleo), workspace grep (rayon)
crabide-workspace/  # Multi-root document lifecycle manager
crabide-ui/         # All egui panels: editor, gutter, tab bar, status bar, etc.
```

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+P` | Quick open file |
| `Ctrl+Shift+F` | Find in files |
| `Ctrl+D` | Add selection to next find match |
| `Ctrl+,` | Settings editor |
| `Ctrl+K Ctrl+S` | Keybindings editor |
| `Ctrl+K Ctrl+T` | Theme picker |
| `Ctrl+Shift+O` | Go to symbol |
| `Ctrl+G` | Go to line |
| `Alt+F12` | Peek definition |
| `Shift+F12` | Peek references |
| `Ctrl+Shift+P` | Command palette |
| `F5` | Start debugging |
| `Ctrl+\`` | Toggle terminal |

See the **Keybindings editor** (`Ctrl+K Ctrl+S`) inside crabide for the full list.

## Configuration

Settings are stored in `~/.config/crabide/settings.toml` (Linux/macOS) or
`%APPDATA%\crabide\settings.toml` (Windows).

Edit via the **Settings UI** (`Ctrl+,`) or directly in the file.

## Development

```bash
# Check all crates
cargo check --workspace

# Run all tests
cargo test --workspace

# Lint
cargo clippy --workspace --all-targets -- -D warnings

# Format
cargo fmt --all

# Build with specific features
cargo build --features wasm-extensions
```

### CI

The project uses GitHub Actions:

- **Lint** — rustfmt + clippy on ubuntu/macos/windows
- **Test** — full test suite on ubuntu/macos/windows
- **Build** — release builds for all platforms
- **Feature Matrix** — compiles and tests all feature flag combinations
- **Security Audit** — `cargo-audit` on dependencies

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development workflow and coding standards.

## License

MIT OR Apache-2.0 at your option.
