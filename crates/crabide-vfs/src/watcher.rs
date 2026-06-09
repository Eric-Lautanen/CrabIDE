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

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, EventKind, ModifyKind, RemoveKind, RenameMode};

    /// Helper: create a crossbeam channel and translate a notify event through it.
    fn translate(kind: EventKind, paths: Vec<PathBuf>) -> Vec<VfsEvent> {
        let (tx, rx) = crossbeam_channel::unbounded();
        translate_event(kind, paths, &tx);
        rx.try_iter().collect()
    }

    #[test]
    fn translate_create_sends_file_created() {
        let events = translate(
            EventKind::Create(CreateKind::File),
            vec![PathBuf::from("/tmp/test.txt")],
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            VfsEvent::FileCreated(p) => assert_eq!(p, &PathBuf::from("/tmp/test.txt")),
            other => panic!("expected FileCreated, got {other:?}"),
        }
    }

    #[test]
    fn translate_modify_sends_file_modified() {
        let events = translate(
            EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Any)),
            vec![PathBuf::from("/tmp/test.txt")],
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            VfsEvent::FileModified(p) => assert_eq!(p, &PathBuf::from("/tmp/test.txt")),
            other => panic!("expected FileModified, got {other:?}"),
        }
    }

    #[test]
    fn translate_remove_sends_file_deleted() {
        let events = translate(
            EventKind::Remove(RemoveKind::File),
            vec![PathBuf::from("/tmp/test.txt")],
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            VfsEvent::FileDeleted(p) => assert_eq!(p, &PathBuf::from("/tmp/test.txt")),
            other => panic!("expected FileDeleted, got {other:?}"),
        }
    }

    #[test]
    fn translate_rename_both_sends_renamed() {
        let events = translate(
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            vec![
                PathBuf::from("/tmp/old.txt"),
                PathBuf::from("/tmp/new.txt"),
            ],
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            VfsEvent::FileRenamed { from, to } => {
                assert_eq!(from, &PathBuf::from("/tmp/old.txt"));
                assert_eq!(to, &PathBuf::from("/tmp/new.txt"));
            }
            other => panic!("expected FileRenamed, got {other:?}"),
        }
    }

    #[test]
    fn translate_rename_from_sends_deleted() {
        let events = translate(
            EventKind::Modify(ModifyKind::Name(RenameMode::From)),
            vec![PathBuf::from("/tmp/old.txt")],
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            VfsEvent::FileDeleted(p) => assert_eq!(p, &PathBuf::from("/tmp/old.txt")),
            other => panic!("expected FileDeleted, got {other:?}"),
        }
    }

    #[test]
    fn translate_rename_to_sends_created() {
        let events = translate(
            EventKind::Modify(ModifyKind::Name(RenameMode::To)),
            vec![PathBuf::from("/tmp/new.txt")],
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            VfsEvent::FileCreated(p) => assert_eq!(p, &PathBuf::from("/tmp/new.txt")),
            other => panic!("expected FileCreated, got {other:?}"),
        }
    }

    #[test]
    fn translate_access_sends_nothing() {
        let events = translate(
            EventKind::Access(notify::event::AccessKind::Any),
            vec![PathBuf::from("/tmp/test.txt")],
        );
        assert!(events.is_empty());
    }

    #[test]
    fn translate_other_sends_nothing() {
        let events = translate(
            EventKind::Other,
            vec![PathBuf::from("/tmp/test.txt")],
        );
        assert!(events.is_empty());
    }

    #[test]
    fn translate_modify_metadata_sends_modified() {
        let events = translate(
            EventKind::Modify(ModifyKind::Metadata(notify::event::MetadataKind::Any)),
            vec![PathBuf::from("/tmp/test.txt")],
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            VfsEvent::FileModified(p) => assert_eq!(p, &PathBuf::from("/tmp/test.txt")),
            other => panic!("expected FileModified, got {other:?}"),
        }
    }

    #[test]
    fn translate_rename_both_less_than_two_paths_sends_nothing() {
        // RenameMode::Both with only 1 path — should not match the pattern
        let events = translate(
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            vec![PathBuf::from("/tmp/only.txt")],
        );
        // Falls through to Modify(_) handler, sends FileModified
        assert_eq!(events.len(), 1);
        match &events[0] {
            VfsEvent::FileModified(_) => {} // OK — falls through
            other => panic!("expected FileModified, got {other:?}"),
        }
    }
}
