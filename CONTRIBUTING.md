# Contributing to crabide

## Development Workflow

1. **Read** — understand the codebase area before editing
2. **Edit** — implement the change (one conceptual step per stop)
3. **Verify** — `cargo check --workspace && cargo clippy --workspace && cargo fmt --all`
4. **Commit** — `git commit -m "type: concise summary"`

### Commit Types

| Prefix | When |
|--------|------|
| `feat` | New feature or capability |
| `fix` | Bug fix |
| `perf` | Performance improvement |
| `refactor` | Code restructuring with no behavior change |
| `test` | Adding or updating tests |
| `docs` | Documentation only |
| `chore` | Build, CI, dependencies, tooling |

### Commit Rules

- Subject ≤ 72 chars, no trailing period
- One commit per conceptual step
- Never batch unrelated changes
- Every commit compiles (`cargo check --workspace` passes)
- Imperative mood ("add", "fix", not "added", "fixed")

## Coding Standards

- **Zero warnings** — `cargo check` must pass with zero warnings
- **Zero clippy lints** — fix every lint, do not suppress
- **Zero `todo!()`, `unimplemented!()`, `dbg!()`, `eprintln!()`** in production code
- **Zero unused dependencies** — run `cargo-udeps` periodically
- **Prefer local helpers over external crates** — can you write it in ≤50 lines?
- **Every `[dependencies]` entry must carry a brief rationale comment**
- **Mimic existing patterns** — if neighboring code uses `thiserror` and `anyhow`, don't introduce a custom error enum

## Architecture

See [ARCHITECTURE.md](docs/ARCHITECTURE.md) for the crate dependency graph and phase structure.
See [ROADMAP.md](ROADMAP.md) for the full build plan.

## Running Tests

```bash
# Full workspace
cargo test --workspace

# Single crate
cargo test --package crabide-buffer

# With specific features
cargo test --features wasm-extensions
```

## Building

```bash
# Default (includes git support)
cargo build --release

# Minimal
cargo build --release --no-default-features

# All features
cargo build --release --all-features
```

## CI

The project uses GitHub Actions. See `.github/workflows/` for workflow definitions.
