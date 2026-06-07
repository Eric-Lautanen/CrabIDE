# RESUME — Roadmap Audit Complete

## What was done
- Read `ROADMAP.md` and audited all 14 crates against actual source code
- Updated phase statuses to match reality (many were marked COMPLETE/DONE but had significant gaps)
- Marked 3 newly-completed LSP wiring items from the most recent commit
- Identified lingering unused deps (`regex-lite` in workspace, `crossbeam-channel` in syntax)
- Committed: `778786f roadmap: update completion statuses after codebase audit`

## Key findings
| Area | Old Status | New Status | Notes |
|---|---|---|---|
| crabide-config | COMPLETE | PARTIAL | 5 gaps remain |
| crabide-syntax | COMPLETE | PARTIAL | 6 gaps remain |
| crabide-search | COMPLETE | PARTIAL | 5 gaps remain (regex-lite still in workspace) |
| crabide-git | COMPLETE | PARTIAL | 10 gaps remain |
| crabide-extensions | COMPLETE | PARTIAL | ~60% — WASM host stubs, registry download |
| Phase 3 LSP wiring | various | 3 more done | UI fields, apply_workspace_edit, EditorTab fields |

## Next recommended priorities
1. **Remove `regex-lite`** from workspace `Cargo.toml` (line 72) — unused dep
2. **Remove `crossbeam-channel`** from `crates/crabide-syntax/Cargo.toml` (line 27) — unused dep
3. **Implement real app icon loading** in `crates/crabide-app/src/main.rs` (`load_icon()` function) — assets/ has real icons but placeholder used
4. **Add unit tests** to at least one crate (e.g., crabide-core or crabide-buffer)
5. **Wire ShowSignatureHelp → LSP** (last unwired LSP action)
6. **Add hover/completion/code_actions popup UI rendering** in crabide-ui

## Context usage
~99% of 1M tokens consumed. Handing off.
