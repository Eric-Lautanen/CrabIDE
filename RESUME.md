# Resume — crabide project

## Session summary

Started implementing **DECSET 25 (cursor visibility toggle)** and **DECSET 2004 (bracketed paste mode)** in `crabide-terminal`. The work is **partially complete but has a build-breaking bug** that must be fixed before continuing.

### What was done (partially — NOT committed, NOT building)

1. **`crates/crabide-core/src/event.rs`** — Added `cursor_visible: bool` and `bracketed_paste: bool` fields to `TerminalGridDelta` struct. Also updated the test at line ~1140 that constructs `TerminalGridDelta` to include the new fields. This part appears correct.

2. **`crates/crabide-terminal/src/grid.rs`** — **HAS A BUG: duplicate struct fields in `Grid::new()` constructor.** The changes made:
   - Added `cursor_visible: bool` and `bracketed_paste: bool` fields to the `Grid` struct (lines 137-142) — correct
   - Added DECSET 25 / DECSET 2004 handling in `csi_dispatch` (lines 562-580) — correct
   - Added `cursor_visible` and `bracketed_paste` to `take_delta()` return (lines 249-250) — correct
   - **BUG in `Grid::new()` (lines 155-185):** The struct literal has duplicate fields. Lines 170-174 are correct (`title`, `cwd`, `cursor_visible`, `bracketed_paste`, `parser`). But lines 176-184 are leftover duplicates (`title`, `cwd`, `cursor_visible`, `bracketed_paste`, `parser`, `cwd`, `cursor_visible`, `bracketed_paste`, `parser`) that must be deleted. The `}` on line 175 closes the struct but the duplicates after it make it invalid Rust.

### What needs to be fixed immediately

**`crates/crabide-terminal/src/grid.rs` lines 175-185** — Delete the duplicate field assignments. The `Grid::new()` constructor should end at line 175 with just:
```rust
            parser: Parser::new(),
        }
    }
```
Remove everything from line 176 through line 185 (the duplicate `title`, `cwd`, `cursor_visible`, `bracketed_paste`, `parser` entries and extra closing braces).

### Remaining work after the fix

- Verify `cargo check --workspace` passes
- Verify `cargo clippy --workspace` passes  
- Run `cargo fmt --all`
- Wire `cursor_visible` and `bracketed_paste` into `crabide-ui/src/state.rs`:
  - Add `cursor_visible: bool` and `bracketed_paste: bool` fields to `TerminalInstance`
  - Update `apply_delta()` to read `delta.cursor_visible` and `delta.bracketed_paste`
  - Update `TerminalInstance::new()` test construction of `TerminalGridDelta` (line ~429) to include new fields
- Add unit tests for DECSET 25 and DECSET 2004 in `grid.rs`
- Commit and update ROADMAP.md

### Build status
- **BROKEN** — `grid.rs` has duplicate struct fields, will not compile

### Remaining roadmap items (next session picks)

**Easy / self-contained tasks:**
- Phase 6 (Git): Add branch listing (local + remote), branch deletion, diff for staged changes
- Phase 7 (Terminal): DECSTBM (scroll regions), insert/delete line/char, mouse reporting, content reflow
- Phase 10 (App): Window state persistence, session restore
- Cross-cutting: `docs/` directory (currently empty)

**Medium tasks:**
- Phase 2 (Syntax): Add language support for HTML, CSS, YAML, Shell, SQL, Java, C#, etc.
- Phase 6 (Git): fetch/pull/push/merge/rebase, stash, log/history
- Phase 7 (Terminal): DECSTBM (scroll regions), insert/delete line/char, mouse reporting, content reflow
- Phase 8 (DAP): attach workflow, evaluate, setVariable, threads, function/exception breakpoints
- Phase 9 (Extensions): WASM editor/workspace/commands host implementations, registry download
