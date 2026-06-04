//! crabide Editor — entry point.
//!
//! # Architecture overview
//!
//! ```text
//! main()
//!   ├─ init logging
//!   ├─ start Tokio runtime (background I/O)
//!   ├─ load workspace / config
//!   ├─ spawn background services:
//!   │    ├─ VFS + file watcher
//!   │    ├─ LSP manager
//!   │    ├─ DAP client
//!   │    ├─ Git service
//!   │    ├─ Terminal manager
//!   │    └─ Extension host
//!   └─ run eframe (blocks until window closes)
//!        └─ crabideApp::update() — called at display refresh rate
//!             └─ drain all event channels
//!             └─ render all panels via crabide-ui
//! ```
//!
//! The UI thread **never blocks**. All background work uses Tokio tasks or
//! Rayon threads, results delivered via bounded crossbeam channels.
//!
//! # Renderer choice
//! We use the **glow (OpenGL)** eframe renderer instead of wgpu:
//! - Eliminates D3D12/Vulkan/Metal GPU heap allocations (~20–50 MB)
//! - A single lightweight OpenGL context is more than sufficient for 2D UI
//! - Removes the entire wgpu dependency subtree from the binary

mod app;

use anyhow::Result;
use eframe::NativeOptions;
use env_logger::Env;

// ── Global allocator ──────────────────────────────────────────────────────────
// mimalloc aggressively returns idle pages to the OS on Windows, which
// dramatically reduces idle RSS compared to the default CRT heap.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> Result<()> {
    // ── Logging ───────────────────────────────────────────────────────────────
    // Default log level: `warn` in release, `debug` in dev builds.
    // Override: crabide_LOG=trace cargo run
    env_logger::Builder::from_env(Env::default().filter_or("crabide_LOG", default_log_level()))
        .format_timestamp_millis()
        .init();

    log::info!(
        "crabide Editor {} starting on {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    );

    // ── Parse CLI args ────────────────────────────────────────────────────────
    let args: Vec<String> = std::env::args().skip(1).collect();
    let initial_paths: Vec<std::path::PathBuf> = args
        .iter()
        .map(std::path::PathBuf::from)
        .filter(|p| p.exists())
        .collect();

    // ── Tokio runtime ─────────────────────────────────────────────────────────
    // Multi-threaded runtime for all background I/O (LSP, DAP, VFS, git, etc.)
    // This is a separate runtime from the UI thread. We start it here and pass
    // a handle into the app so background tasks can be spawned from the UI.
    //
    // Capped at 2 threads: a code editor's async I/O is latency-bound not
    // throughput-bound. Extra threads just consume stack memory (~8 MB each on
    // Windows) and compete with Rayon for CPU cache.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("crabide-bg")
        .enable_all()
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to start Tokio runtime: {e}"))?;

    let rt_handle = rt.handle().clone();

    // ── eframe / egui (glow / OpenGL renderer) ────────────────────────────────
    // glow replaces the wgpu renderer. OpenGL is sufficient for 2D text+UI and
    // avoids the D3D12/Vulkan/Metal GPU heap (~20–50 MB) that wgpu allocates
    // even for a blank window. The entire wgpu dep subtree is dropped.
    let native_options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("crabide")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([640.0, 400.0])
            .with_icon(load_icon()),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    // Run the event loop — this blocks until the window is closed.
    // eframe calls `crabideApp::update()` every frame.
    eframe::run_native(
        "crabide",
        native_options,
        Box::new(move |cc| Ok(Box::new(app::crabideApp::new(cc, rt_handle, initial_paths)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;

    // Graceful shutdown: wait for all background tasks to finish.
    log::info!("Shutting down background runtime…");
    rt.shutdown_timeout(std::time::Duration::from_secs(5));
    log::info!("crabide exited cleanly");

    Ok(())
}

fn default_log_level() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "warn"
    }
}

/// Returns a default application icon.
/// Replace with actual icon bytes before shipping a release build.
fn load_icon() -> egui::IconData {
    // A 2×2 amber pixel icon — placeholder until real icon assets are added.
    // Format: RGBA, row-major.
    egui::IconData {
        rgba: vec![
            232, 197, 71, 255, 232, 197, 71, 255, 232, 197, 71, 255, 232, 197, 71, 255,
        ],
        width: 2,
        height: 2,
    }
}
