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
mod icon_data;
mod window_state;

use anyhow::Result;
use eframe::NativeOptions;
use env_logger::Env;

// Global allocator — wraps mimalloc with allocation counting for profiling.
// mimalloc aggressively returns idle pages to the OS on Windows, which
// dramatically reduces idle RSS compared to the default CRT heap.

use std::sync::atomic::{AtomicU64, Ordering};

struct CountingAlloc;

unsafe impl std::alloc::GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let ptr = mimalloc::MiMalloc.alloc(layout);
        if !ptr.is_null() {
            ALLOCATED.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        ALLOCATED.fetch_sub(layout.size() as u64, Ordering::Relaxed);
        mimalloc::MiMalloc.dealloc(ptr, layout);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, new_size: usize) -> *mut u8 {
        let old_size = layout.size();
        let new_ptr = mimalloc::MiMalloc.realloc(ptr, layout, new_size);
        if !new_ptr.is_null() {
            if new_size > old_size {
                ALLOCATED.fetch_add((new_size - old_size) as u64, Ordering::Relaxed);
            } else {
                ALLOCATED.fetch_sub((old_size - new_size) as u64, Ordering::Relaxed);
            }
        }
        new_ptr
    }
}

/// Total bytes currently allocated via the global allocator.
pub(crate) static ALLOCATED: AtomicU64 = AtomicU64::new(0);

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

/// Parsed CLI arguments.
struct CliArgs {
    /// Files or directories to open on startup.
    paths: Vec<std::path::PathBuf>,
    /// Print version and exit.
    version: bool,
    /// Override the log level.
    log_level: Option<String>,
}

fn parse_args() -> CliArgs {
    let mut args = CliArgs {
        paths: Vec::new(),
        version: false,
        log_level: None,
    };
    let mut iter = std::env::args().skip(1).peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                eprintln!("Usage: crabide [OPTIONS] [PATHS...]");
                eprintln!();
                eprintln!("Options:");
                eprintln!("  -h, --help           Show this help message");
                eprintln!("  -V, --version        Print version and exit");
                eprintln!("  -l, --log <LEVEL>    Set log level (trace|debug|info|warn|error)");
                eprintln!();
                eprintln!("Paths: files or directories to open on startup");
                std::process::exit(0);
            }
            "-V" | "--version" => {
                args.version = true;
            }
            "-l" | "--log" => {
                if let Some(level) = iter.next() {
                    args.log_level = Some(level);
                } else {
                    eprintln!("error: --log requires a level argument");
                    std::process::exit(1);
                }
            }
            s if s.starts_with('-') => {
                eprintln!("error: unknown option {s}");
                eprintln!("Use --help for usage information");
                std::process::exit(1);
            }
            path => {
                args.paths.push(std::path::PathBuf::from(path));
            }
        }
    }
    args
}

fn main() -> Result<()> {
    // Parse CLI arguments
    let cli = parse_args();

    if cli.version {
        println!("crabide {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // Logging
    // Default log level: `warn` in release, `debug` in dev builds.
    // Override: crabide_LOG=trace cargo run, or --log trace
    let log_level = cli
        .log_level
        .as_deref()
        .unwrap_or_else(|| default_log_level());
    env_logger::Builder::from_env(Env::default().filter_or("crabide_LOG", log_level))
        .format_timestamp_millis()
        .init();

    log::info!(
        "crabide Editor {} starting on {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    );

    // ── Crash reporter (panic hook → file) ──────────────────────────────────────
    // Write panic messages to a crash log in the user config directory so users
    // (and bug reporters) can inspect the details after a crash.
    let panic_log_path =
        crabide_config::SettingsLoader::user_config_dir().map(|d| d.join("crash.log"));
    if let Some(ref path) = panic_log_path {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let log_path = path.clone();
        std::panic::set_hook(Box::new(move |info| {
            let msg = format!(
                "crabide panic at {}: {info}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
            );
            // Write to crash log (best-effort).
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
            {
                use std::io::Write;
                let _ = writeln!(f, "{msg}");
                // Include a backtrace if RUST_BACKTRACE is set.
                let bt = std::backtrace::Backtrace::capture();
                let _ = writeln!(f, "{bt}");
            }
            // Also emit to stderr so console users still see the crash.
            eprintln!("{msg}");
        }));
    }

    // Ctrl+C signal handler for graceful shutdown.
    // Sets a flag that the app checks each frame; eframe will close the window
    // on the next update cycle.
    let shutdown_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag_clone = shutdown_flag.clone();
    ctrlc::set_handler(move || {
        log::info!("Ctrl+C received; initiating graceful shutdown");
        flag_clone.store(true, std::sync::atomic::Ordering::Relaxed);
    })
    .unwrap_or_else(|e| log::warn!("Failed to set Ctrl+C handler: {e}"));

    // Filter initial paths to those that exist on disk.
    let initial_paths: Vec<std::path::PathBuf> =
        cli.paths.into_iter().filter(|p| p.exists()).collect();

    // Tokio runtime
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

    // Load persisted window state (size, position, maximized) from last session.
    let win = window_state::load_window_state();

    // eframe / egui (glow / OpenGL renderer)
    // glow replaces the wgpu renderer. OpenGL is sufficient for 2D text+UI and
    // avoids the D3D12/Vulkan/Metal GPU heap (~2050 MB) that wgpu allocates
    // even for a blank window. The entire wgpu dep subtree is dropped.
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("crabide")
        .with_inner_size([win.width.max(640.0), win.height.max(400.0)])
        .with_min_inner_size([640.0, 400.0])
        .with_icon(load_icon());
    if let (Some(x), Some(y)) = (win.x, win.y) {
        viewport = viewport.with_position([x, y]);
    }
    if win.maximized {
        viewport = viewport.with_maximized(true);
    }

    let native_options = NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    // Run the event loop this blocks until the window is closed.
    // eframe calls `crabideApp::update()` every frame.
    eframe::run_native(
        "crabide",
        native_options,
        Box::new(move |cc| {
            let mut app = Box::new(app::crabideApp::new(cc, rt_handle, initial_paths));
            app.set_shutdown_flag(shutdown_flag);
            Ok(app)
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;

    // Graceful shutdown: wait for all background tasks to finish.
    log::info!("Shutting down background runtime");
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

/// Returns the application icon from assets/icon-32.png.
/// Pre-decoded to raw RGBA at compile time via tools/gen_icon.py.
fn load_icon() -> egui::IconData {
    egui::IconData {
        rgba: icon_data::ICON_RGBA.to_vec(),
        width: icon_data::ICON_WIDTH,
        height: icon_data::ICON_HEIGHT,
    }
}
