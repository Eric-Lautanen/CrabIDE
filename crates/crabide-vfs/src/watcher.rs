//! Debounced filesystem watcher — converts `notify` events to `VfsEvent`s.

use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::RecommendedWatcher;
use notify::RecursiveMode;
use notify_debouncer_full::{DebounceEventResult, Debouncer, FileIdMap, new_debouncer};

use crabide_core::error::{Result, crabideError};
use crabide_core::event::VfsEvent;

/// A debounced file watcher that sends [`VfsEvent`]s to a channel.
pub struct VfsWatcher {
    debouncer: Debouncer<RecommendedWatcher, FileIdMap>,
}

impl VfsWatcher {
    /// Create a new watcher. Events are sent to `tx`.
    pub fn new(tx: crossbeam_channel::Sender<VfsEvent>) -> Result<Self> {
        let debouncer = new_debouncer(
            Duration::from_millis(100),
            None,
            move |result: DebounceEventResult| {
                let events = match result {
                    Ok(evs) => evs,
                    Err(errs) => {
                        for e in errs {
                            let _ = tx.try_send(VfsEvent::WatchError(format!("{e}")));
                        }
                        return;
                    }
                };
                for debounced in events {
                    translate_event(debounced.event.kind, debounced.event.paths, &tx);
                }
            },
        )
        .map_err(|e| crabideError::Other(format!("Failed to create file watcher: {e}")))?;

        Ok(Self { debouncer })
    }

    /// Start watching `path`. If `recursive` is true, subdirectories are included.
    pub fn watch(&mut self, path: &Path, recursive: bool) -> Result<()> {
        let mode = if recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        self.debouncer
            .watch(path, mode)
            .map_err(|e| crabideError::Other(format!("Cannot watch {}: {e}", path.display())))
    }

    /// Stop watching `path`.
    pub fn unwatch(&mut self, path: &Path) -> Result<()> {
        self.debouncer
            .unwatch(path)
            .map_err(|e| crabideError::Other(format!("Cannot unwatch {}: {e}", path.display())))
    }
}

// ── Event translation ─────────────────────────────────────────────────────────

fn translate_event(
    kind: notify::EventKind,
    paths: Vec<PathBuf>,
    tx: &crossbeam_channel::Sender<VfsEvent>,
) {
    use notify::event::{EventKind, ModifyKind, RenameMode};

    match kind {
        EventKind::Create(_) => {
            for p in paths {
                let _ = tx.try_send(VfsEvent::FileCreated(p));
            }
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if paths.len() >= 2 => {
            let _ = tx.try_send(VfsEvent::FileRenamed {
                from: paths[0].clone(),
                to: paths[1].clone(),
            });
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
            for p in paths {
                let _ = tx.try_send(VfsEvent::FileDeleted(p));
            }
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
            for p in paths {
                let _ = tx.try_send(VfsEvent::FileCreated(p));
            }
        }
        EventKind::Modify(_) | EventKind::Any => {
            for p in paths {
                let _ = tx.try_send(VfsEvent::FileModified(p));
            }
        }
        EventKind::Remove(_) => {
            for p in paths {
                let _ = tx.try_send(VfsEvent::FileDeleted(p));
            }
        }
        EventKind::Access(_) | EventKind::Other => {}
    }
}
