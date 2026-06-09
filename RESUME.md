# Resume — Remediation session (Session 3)

> **Session goal:** Fix all audit findings from Sessions 1-2, starting from Critical → High → Medium → Low.

---

## Session 3 completed (2026-06-xx)

**What was done:**

### 🔴 Critical
- **C-1**: Added `// SAFETY:` comments to all unsafe blocks:
  - CountingAlloc (main.rs)
  - `raw_lang!` macro (app.rs)
  - Normalised `// Safety:` → `// SAFETY:` in git lib.rs
- **C-2**: Added `#![deny(unsafe_op_in_unsafe_fn)]` to main.rs; wrapped all `MiMalloc` calls in inner `unsafe {}` blocks

### 🔴 High
- **H-1**: Added `kill_on_drop(true)` to DAP `Command`; added `shutdown()` method for graceful disconnect
- **H-2**: Stored PTY child handle in `PtyHandle`; added `Drop` impl that kills the child on cleanup; updated `TerminalManager::kill()` to terminate the process
- **H-3**: Spawned background thread for `Engine::increment_epoch()` at 100ms intervals
- **H-4**: Refactored `notification_loop` to pass message `id` to `handle_notification`; added `LspTransport::respond()` for proper JSON-RPC responses; fixed all server→client request handlers to use message-level `id` instead of `params.get("id")`
- **H-5**: Added `#[non_exhaustive]` to all 30+ public enums across all crates; added wildcard `_ =>` arms to all match statements affected
- **H-6**: Removed unused deps (`thiserror`, `log`, `parking_lot`, `serde`, `serde_json`) from `crabide-buffer`; removed `thiserror`, `anyhow` from `crabide-config`

### 🟡 Medium
- **M-4**: Already fixed (early return in `CursorSet::normalise` when `len <= 1`)
- **M-5**: Changed `SyntaxEngine::highlights()` to use `compute_highlights_with_injections` instead of `compute_highlights`

### 🟢 Low — Not yet started
- L-1: `#[must_use]` on pure functions
- L-2: `cloned()` → `copied()`
- L-3: Edition 2024 migration
- L-4: `mem::replace` → `mem::take`

### ⚪ Notes — Not yet started
- N-1: Unit tests for DAP, git, workspace, terminal
- N-2: Comment rot
- M-1/M-2/M-3: ConfigManager optimisation

### Build status
- `cargo check --workspace` — zero warnings ✅
- `cargo clippy --workspace` — zero warnings ✅

---

## Remaining items for next session

| # | Issue | Priority |
|---|-------|----------|
| M-1/M-2 | ConfigManager clone-heavy hot path | 🟡 Medium |
| M-3 | HashMap allocation without pre-sizing | 🟡 Medium |
| L-1 | Add `#[must_use]` to pure functions | 🟢 Low |
| L-2 | Fix `cloned()` → `copied()` | 🟢 Low |
| L-3 | Edition 2024 migration | 🟢 Low |
| L-4 | Replace `mem::replace` with `mem::take` | 🟢 Low |
| N-1 | Add unit tests | ⚪ Note |
| N-2 | Fix comment rot | ⚪ Note |

---

*Session 3: 9 of 15 checklist items completed (C-1, C-2, H-1 through H-6, M-4, M-5).*
