# Changelog

All notable changes to crabide are documented here.

## [0.1.0] - 2026-06-08

### Added

- **Core editor** with multi-cursor, snippets, undo/redo, BOM handling
- **Syntax highlighting** for 22 languages via tree-sitter
- **LSP client** with completion, hover, go-to-definition, references, rename, diagnostics, code actions, signature help, inlay hints, semantic tokens, code lens, folding, formatting
- **DAP debugger** with launch/attach, breakpoints (function, exception, data), step, stack, variables, debug console, threads, run-to-cursor
- **Integrated terminal** with full VT100/VT220 emulation, PTY support, scrollback, alt screen, bracketed paste, mouse reporting, hyperlinks, shell integration
- **Git integration** with status, diff, blame, stage/unstage, commit, branch, fetch/pull/push, stash, log/history, tags, remotes, submodules, conflict resolution
- **Extension system** with WASM runtime (wasmtime), 5 built-in extensions, marketplace registry
- **Workspace search** with fuzzy file finder (nucleo), workspace grep (rayon), search in open buffers
- **Split editor** with side-by-side file comparison
- **Peek view** for inline definition/reference preview
- **Settings UI** with grouped form controls (Ctrl+,)
- **Keybindings editor** with searchable table (Ctrl+K Ctrl+S)
- **Theme picker** with searchable overlay (Ctrl+K Ctrl+T)
- **Breadcrumbs**, **minimap**, **folding gutter**, **scrollbar annotations**
- **Drag-and-drop tab reordering**
- **Column select mode** (Shift+Alt+drag)
- **Context menu** with editor actions
- **Interactive welcome screen**
- **Command palette** (Ctrl+Shift+P)
- **Output panel** with channel selector
- **Multi-root workspace** support
- **Update checker** background thread
- **Crash reporter** panic hook
- **Window state persistence** and session restore
- **34 CLI flags** for settings customization
- **Ctrl+C graceful shutdown**

### Performance

- Sub-60MB idle RAM usage
- Egui (glow/OpenGL) renderer for minimal GPU footprint
- Rayon-based parallel syntax highlighting and grep
- Bounded channels for all background communication
- mimalloc global allocator for aggressive page return to OS
- Tokio runtime capped at 2 background threads

### Platform Support

- **Windows** — x86_64 (MSVC), portable binary + NSIS installer
- **macOS** — ARM64 (M1+) + x86_64, .app bundle + DMG
- **Linux** — x86_64, AppImage + .deb + .rpm
