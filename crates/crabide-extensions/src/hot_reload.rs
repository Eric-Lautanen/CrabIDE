//! Hot-reload watcher for extension `.wasm` files.
//!
//! Uses `notify-debouncer-full` to watch the extensions directory and send
//! debounced change events to [`crate::host::ExtensionHost`] via a crossbeam channel.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, bounded};
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, FileIdMap, new_debouncer};

/// Starts a debounced file-system watcher on `dir`.
///
/// When a `.wasm` file within `dir` is created or modified, the path is sent
/// through the returned [`Receiver`] after a 500 ms quiet period.
///
/// The watcher runs on its own OS thread (managed by `notify`). The returned
/// `Receiver` is non-blocking — call `try_iter()` each frame.
///
/// # Errors
///
/// Returns an error string if the OS watcher cannot be started (e.g. inotify
/// limit reached). In that case hot-reload is silently disabled.
pub fn start_hot_reload_watcher(dir: &Path) -> Result<Receiver<PathBuf>, String> {
    let (tx, rx): (Sender<PathBuf>, Receiver<PathBuf>) = bounded(64);

    let mut debouncer: Debouncer<RecommendedWatcher, FileIdMap> = new_debouncer(
        Duration::from_millis(500),
        None,
        move |result: DebounceEventResult| match result {
            Ok(events) => {
                for evt in events {
                    for path in &evt.paths {
                        if path.extension().and_then(|s| s.to_str()) == Some("wasm") {
                            let _ = tx.try_send(path.to_path_buf());
                        }
                    }
                }
            }
            Err(errors) => {
                for e in errors {
                    log::warn!("hot-reload watcher error: {e}");
                }
            }
        },
    )
    .map_err(|e| format!("failed to create watcher: {e}"))?;

    debouncer
        .watch(dir, RecursiveMode::NonRecursive)
        .map_err(|e| format!("failed to watch {:?}: {e}", dir))?;

    // Leak the debouncer so the watcher keeps running for the lifetime of the app.
    // This is intentional — the watcher should run until the process exits.
    std::mem::forget(debouncer);

    Ok(rx)
}
