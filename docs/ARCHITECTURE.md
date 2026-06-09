# Architecture

crabide follows a layered architecture with 15 crates, each building on the phases below.

## Crate dependency graph

```
crabide (binary/app)
├── crabide-ui (UI rendering, panels, editor view)
│   ├── crabide-core (domain model, events, traits)
│   ├── crabide-config (settings, keybindings, themes)
│   ├── crabide-buffer (Document, EditHistory, CursorSet)
│   └── crabide-terminal (grid state machine, PTY)
├── crabide-app (event loop, wiring, background services)
│   ├── crabide-workspace (document lifecycle)
│   │   ├── crabide-vfs (file system abstraction)
│   │   ├── crabide-buffer
│   │   └── crabide-core
│   ├── crabide-lsp (language server protocol client)
│   ├── crabide-dap (debug adapter protocol client)
│   ├── crabide-git (git integration via libgit2)
│   ├── crabide-terminal (PTY manager)
│   ├── crabide-extensions (native + WASM extension host)
│   │   └── crabide-core
│   ├── crabide-search (fuzzy finder, grep)
│   ├── crabide-syntax (tree-sitter highlighting)
│   └── crabide-config
```

## Phase structure

| Phase | Crate | Status |
|-------|-------|--------|
| 1 | core, config, vfs, buffer | ✅ Complete |
| 2 | syntax | 🔶 Partial |
| 3 | lsp | 🔶 ~90% |
| 4 | ui | ✅ Complete |
| 5 | search | 🔶 Partial |
| 6 | git | 🔶 Partial |
| 7 | terminal | 🔶 ~85% |
| 8 | dap | 🔶 ~95% |
| 9 | extensions | 🔶 ~85% |
| 10 | app (polish) | 🔶 ~60% |

See `ROADMAP.md` at the repository root for detailed status per feature.
