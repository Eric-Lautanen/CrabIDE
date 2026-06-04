//! Cursor and selection management.
//!
//! crabide supports full multi-cursor editing: multiple independent cursors
//! that all receive the same edits simultaneously. Cursors are stored sorted
//! by position and de-duplicated after each operation.

use crabide_core::types::{Position, Range, Selection};
use std::cmp::Ordering;

/// How the cursor was placed (affects delete/word behaviour).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    /// Normal cursor/selection.
    Normal,
    /// Line-wise selection (full lines).
    Line,
    /// Column/block selection across multiple lines.
    Column,
}

/// A single cursor with an optional selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub selection: Selection,
    /// The "preferred" column for vertical navigation. When moving up/down,
    /// we try to stay at this column even if shorter lines cause snapping.
    pub preferred_col: u32,
}

impl Cursor {
    /// Create a cursor at the given position with no selection.
    pub fn at(pos: Position) -> Self {
        Self {
            selection: Selection::cursor(pos),
            preferred_col: pos.character,
        }
    }

    /// Current cursor position (the "active" end of the selection).
    pub fn pos(&self) -> Position {
        self.selection.active
    }

    /// The normalised selection range (start <= end).
    pub fn range(&self) -> Range {
        self.selection.as_range()
    }

    pub fn has_selection(&self) -> bool {
        !self.selection.is_empty()
    }

    /// Collapse the selection to the cursor position (clears selection).
    pub fn collapse(&mut self) {
        self.selection = Selection::cursor(self.selection.active);
    }

    /// Collapse to start of selection.
    pub fn collapse_to_start(&mut self) {
        let start = self.range().start;
        self.selection = Selection::cursor(start);
        self.preferred_col = start.character;
    }

    /// Collapse to end of selection.
    pub fn collapse_to_end(&mut self) {
        let end = self.range().end;
        self.selection = Selection::cursor(end);
        self.preferred_col = end.character;
    }

    /// Extend the selection to `pos`, keeping the anchor fixed.
    pub fn extend_to(&mut self, pos: Position) {
        self.selection.active = pos;
        self.preferred_col = pos.character;
    }

    /// Move cursor to `pos`, clearing any selection.
    pub fn move_to(&mut self, pos: Position) {
        self.selection = Selection::cursor(pos);
        self.preferred_col = pos.character;
    }
}

impl PartialOrd for Cursor {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Cursor {
    fn cmp(&self, other: &Self) -> Ordering {
        self.selection.active.cmp(&other.selection.active)
    }
}

/// The set of all active cursors in an editor view.
///
/// Maintains the invariant that cursors are sorted by position and
/// non-overlapping (overlapping cursors are merged on normalisation).
pub struct CursorSet {
    cursors: Vec<Cursor>,
    /// Mode applies to all cursors simultaneously.
    mode: SelectionMode,
}

impl CursorSet {
    /// Create a new CursorSet with a single cursor at the origin.
    pub fn new() -> Self {
        Self {
            cursors: vec![Cursor::at(Position::ZERO)],
            mode: SelectionMode::Normal,
        }
    }

    pub fn with_cursor(pos: Position) -> Self {
        Self {
            cursors: vec![Cursor::at(pos)],
            mode: SelectionMode::Normal,
        }
    }

    /// The "primary" cursor — the last one added, which controls scroll
    /// position and status bar display.
    pub fn primary(&self) -> &Cursor {
        self.cursors
            .last()
            .expect("CursorSet always has at least one cursor")
    }

    pub fn primary_mut(&mut self) -> &mut Cursor {
        self.cursors
            .last_mut()
            .expect("CursorSet always has at least one cursor")
    }

    pub fn all(&self) -> &[Cursor] {
        &self.cursors
    }

    pub fn all_mut(&mut self) -> &mut Vec<Cursor> {
        &mut self.cursors
    }

    pub fn count(&self) -> usize {
        self.cursors.len()
    }

    pub fn mode(&self) -> SelectionMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: SelectionMode) {
        self.mode = mode;
    }

    /// Add a new cursor at `pos` without removing existing ones.
    /// Triggers normalisation (sort + merge overlapping).
    pub fn add(&mut self, pos: Position) {
        self.cursors.push(Cursor::at(pos));
        self.normalise();
    }

    /// Replace all cursors with a single cursor at `pos`.
    pub fn set_single(&mut self, pos: Position) {
        self.cursors.clear();
        self.cursors.push(Cursor::at(pos));
        self.mode = SelectionMode::Normal;
    }

    /// Clear all selections (collapse cursors) without moving them.
    pub fn collapse_all(&mut self) {
        for c in &mut self.cursors {
            c.collapse();
        }
    }

    /// Add a new cursor covering `range` without removing existing ones.
    pub fn add_cursor_at_range(&mut self, range: Range) {
        let mut c = Cursor::at(range.start);
        c.extend_to(range.end);
        self.cursors.push(c);
        self.normalise();
    }

    /// Replace all cursors with multi-selections covering each given range.
    pub fn set_multi_selection(&mut self, ranges: &[Range]) {
        self.cursors.clear();
        for &r in ranges {
            let mut c = Cursor::at(r.start);
            c.extend_to(r.end);
            self.cursors.push(c);
        }
        if self.cursors.is_empty() {
            self.cursors.push(Cursor::at(Position::new(0, 0)));
        }
        self.mode = SelectionMode::Normal;
        self.normalise();
    }

    /// Move the primary cursor to `pos`, collapsing its selection.
    pub fn move_primary_to(&mut self, pos: Position) {
        if let Some(c) = self.cursors.last_mut() {
            c.move_to(pos);
        }
    }

    /// Apply `f` to every cursor, then normalise.
    pub fn map_cursors(&mut self, mut f: impl FnMut(&mut Cursor)) {
        for c in &mut self.cursors {
            f(c);
        }
        self.normalise();
    }

    /// Sort cursors by position and merge those with overlapping ranges.
    fn normalise(&mut self) {
        if self.cursors.len() <= 1 {
            return;
        }

        self.cursors.sort_unstable();
        self.cursors.dedup_by(|a, b| {
            // Merge cursors at the same position
            if a.pos() == b.pos() {
                // Keep `b` (which is lower index after sort_unstable, i.e. earlier)
                true // returning true removes `a`
            } else {
                false
            }
        });
    }
}

impl Default for CursorSet {
    fn default() -> Self {
        Self::new()
    }
}
