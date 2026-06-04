//! Edit history — branching undo/redo using cheap `ropey::Rope` snapshots.
//!
//! Because `Rope::clone()` is O(1) (Arc-shared internal nodes), we can store
//! the full buffer state at each history entry without significant memory cost.
//! Memory only grows when clones diverge through additional edits.
//!
//! # Branching undo
//!
//! When the user undoes several steps and then makes a new edit, the old
//! "redo" branch is preserved as an alternate history. This matches the
//! vim `undotree` model. The current implementation stores a flat timeline
//! with a cursor; full tree branching is a future enhancement.

use ropey::Rope;
use std::collections::VecDeque;

/// Maximum entries kept in the undo stack before oldest entries are evicted.
const MAX_HISTORY: usize = 500;

/// A single point in edit history.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    /// A cheap O(1) clone of the rope at this point in time.
    pub rope: Rope,
    /// Human-readable label for this checkpoint (shown in undo tree UI).
    pub label: String,
    /// The cursor positions at this history point (serialised for restoration).
    pub cursor_data: Vec<u8>,
}

/// Named checkpoint, created by user or editor for significant operations
/// (e.g. "Before refactor", "Before paste").
#[derive(Debug, Clone)]
pub struct HistoryCheckpoint {
    pub label: String,
    pub index: usize,
}

/// Manages undo/redo for a single `Document`.
///
/// All methods are called with exclusive access to the document — the caller
/// holds the document's write lock for the duration of any undo/redo operation.
pub struct EditHistory {
    /// The history stack. `entries[cursor]` is the current state.
    entries: VecDeque<HistoryEntry>,
    /// Points to the "current" entry (the one the document is at).
    cursor: usize,
    /// Named checkpoints in the history.
    checkpoints: Vec<HistoryCheckpoint>,
    /// Whether we're currently inside a compound edit group.
    /// While `group_depth > 0`, pushes coalesce into the current top entry.
    group_depth: u32,
}

impl EditHistory {
    pub fn new(initial_rope: Rope) -> Self {
        let mut entries = VecDeque::new();
        entries.push_back(HistoryEntry {
            rope: initial_rope,
            label: String::from("Initial"),
            cursor_data: vec![],
        });

        Self {
            entries,
            cursor: 0,
            checkpoints: vec![],
            group_depth: 0,
        }
    }

    /// Push a new state onto the history stack.
    ///
    /// Any entries after the current cursor (redo states) are discarded.
    /// If `MAX_HISTORY` is reached, the oldest entry is evicted.
    pub fn push(&mut self, rope: Rope, label: impl Into<String>, cursor_data: Vec<u8>) {
        if self.group_depth > 0 {
            // Inside a compound group — overwrite the current top entry
            if let Some(entry) = self.entries.get_mut(self.cursor) {
                entry.rope = rope;
                entry.cursor_data = cursor_data;
            }
            return;
        }

        // Discard redo states
        let remove_from = self.cursor + 1;
        while self.entries.len() > remove_from {
            self.entries.pop_back();
        }

        // Evict oldest if at capacity
        if self.entries.len() >= MAX_HISTORY {
            self.entries.pop_front();
            // Adjust checkpoint indices
            for cp in &mut self.checkpoints {
                cp.index = cp.index.saturating_sub(1);
            }
            self.cursor = self.cursor.saturating_sub(1);
        }

        self.entries.push_back(HistoryEntry {
            rope,
            label: label.into(),
            cursor_data,
        });
        self.cursor = self.entries.len() - 1;
    }

    /// Start a compound edit group. Multiple `push` calls within a group
    /// are merged into a single undo step.
    ///
    /// Groups are reference-counted — must call `end_group()` the same number
    /// of times as `begin_group()`.
    pub fn begin_group(&mut self) {
        self.group_depth += 1;
    }

    /// End a compound edit group.
    pub fn end_group(&mut self) {
        self.group_depth = self.group_depth.saturating_sub(1);
    }

    /// Mark a named checkpoint at the current position.
    pub fn checkpoint(&mut self, label: impl Into<String>) {
        self.checkpoints.push(HistoryCheckpoint {
            label: label.into(),
            index: self.cursor,
        });
    }

    // ── Undo / Redo ───────────────────────────────────────────────────────────

    /// Undo one step. Returns the rope snapshot and cursor data to restore,
    /// or `None` if there is nothing to undo.
    pub fn undo(&mut self) -> Option<&HistoryEntry> {
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        self.entries.get(self.cursor)
    }

    /// Redo one step. Returns the rope and cursor data to restore,
    /// or `None` if there is nothing to redo.
    pub fn redo(&mut self) -> Option<&HistoryEntry> {
        if self.cursor + 1 >= self.entries.len() {
            return None;
        }
        self.cursor += 1;
        self.entries.get(self.cursor)
    }

    /// Jump to a named checkpoint.
    pub fn jump_to_checkpoint(&mut self, label: &str) -> Option<&HistoryEntry> {
        let idx = self
            .checkpoints
            .iter()
            .rev()
            .find(|c| c.label == label)?
            .index;
        self.cursor = idx;
        self.entries.get(self.cursor)
    }

    // ── Queries ───────────────────────────────────────────────────────────────

    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_redo(&self) -> bool {
        self.cursor + 1 < self.entries.len()
    }

    pub fn undo_label(&self) -> Option<&str> {
        if self.cursor == 0 {
            return None;
        }
        self.entries.get(self.cursor).map(|e| e.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.entries.get(self.cursor + 1).map(|e| e.label.as_str())
    }

    pub fn history_len(&self) -> usize {
        self.entries.len()
    }

    pub fn current_cursor(&self) -> usize {
        self.cursor
    }

    pub fn checkpoints(&self) -> &[HistoryCheckpoint] {
        &self.checkpoints
    }
}
