# Resume — crabide project

> ⚠️ **MANDATORY POLICY (every session):**
> 1. Update this RESUME.md with what was done — never leave stale info.
> 2. Update ROADMAP.md — mark completed items `[x]`, add new gaps as `[ ]`.
> 3. `git add -A && git commit -m "TYPE: message"` after every green build.
> 4. Push periodically.
> 5. **Call `handoff` with `reason: "continuing to next roadmap item"`** when:
>    - Current roadmap item is done and more remain, OR
>    - Context is nearing the token limit.
> 6. **Never stop voluntarily** — always hand off to continue. The project MUST be completed end-to-end.

## Session summary

**Performance pass ✅**

Implemented three profiling subsystems for performance analysis:

### Frame time profiler (FPS counter)
- Added `FrameProfiler` struct in `crabide-ui/src/state.rs` with 120-sample ring buffer
- Tracks FPS, min/avg/max frame time, and P95 latency
- Render overlay window (toggle: `Ctrl+Shift+``) showing all frame stats
- Integrated into `crabideApp::ui()` to record frame start time

### LSP round-trip latency tracking
- Added `PendingRequest::sent_at` timestamp in `crabide-lsp/src/transport.rs`
- `LspTransport::spawn()` now accepts an optional latency callback channel
- Created latency bridge thread in `server_mgr.rs` forwarding to app event bus
- Added `LspEvent::LatencyRecord` variant carrying method name + duration
- Added `LspLatencyTracker` in `crabide-ui` state with ring buffer of recent latencies
- App `apply_lsp_event` handler updates the tracker
- Profiler overlay displays avg/max LSP latency with sample count

### Heap profiling
- Replaced direct `mimalloc::MiMalloc` global allocator with `CountingAlloc` wrapper
- `CountingAlloc` tracks total allocated bytes via `AtomicU64`
- App samples heap usage every 60 frames (~1 Hz)
- Profiler overlay displays current heap usage in MB

### New action & keybinding
- Added `Action::ToggleProfiler` to Action enum
- Default keybinding: `Ctrl+Shift+``
- Added to `all_actions()` and `handle_ui_action()`

## Build status
- **GREEN** — `cargo check --workspace` zero warnings
- **CLIPPY** — zero warnings
- **TESTS** — all 1051 workspace tests pass

## Cross-cutting
- [x] Feature flag matrix test (CI tests 9 combos)
- [x] Phase 12 polish: packaging scripts, release workflow, docs, clippy cleanup
- [x] Performance pass: egui frame time, LSP round-trip latency, heap profiling
- [ ] Push to origin

## What's next
- Push to remote repository
