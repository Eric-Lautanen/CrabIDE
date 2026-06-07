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

    /// Remove the cursor at the given index, if it exists.
    /// If this would leave the set empty, the last cursor is reset to origin instead.
    pub fn remove(&mut self, index: usize) {
        if self.cursors.len() <= 1 {
            self.set_single(Position::ZERO);
            return;
        }
        if index < self.cursors.len() {
            self.cursors.remove(index);
        }
    }

    /// Iterate over all cursors by reference.
    pub fn iter(&self) -> impl Iterator<Item = &Cursor> {
        self.cursors.iter()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crabide_core::types::Position;

    #[test]
    fn test_cursor_set_new() {
        let cs = CursorSet::new();
        assert_eq!(cs.count(), 1);
        assert_eq!(cs.primary().pos(), Position::ZERO);
        assert_eq!(cs.mode(), SelectionMode::Normal);
    }

    #[test]
    fn test_cursor_set_with_cursor() {
        let pos = Position::new(5, 3);
        let cs = CursorSet::with_cursor(pos);
        assert_eq!(cs.count(), 1);
        assert_eq!(cs.primary().pos(), pos);
    }

    #[test]
    fn test_add_cursor() {
        let mut cs = CursorSet::new();
        cs.add(Position::new(10, 0));
        assert_eq!(cs.count(), 2);
        // Last added is primary
        assert_eq!(cs.primary().pos(), Position::new(10, 0));
    }

    #[test]
    fn test_add_duplicate_cursor_is_deduped() {
        let mut cs = CursorSet::new();
        cs.add(Position::ZERO); // same as existing
        assert_eq!(cs.count(), 1);
    }

    #[test]
    fn test_set_single() {
        let mut cs = CursorSet::new();
        cs.add(Position::new(5, 0));
        cs.add(Position::new(10, 0));
        assert_eq!(cs.count(), 3);
        cs.set_single(Position::new(2, 2));
        assert_eq!(cs.count(), 1);
        assert_eq!(cs.primary().pos(), Position::new(2, 2));
        assert_eq!(cs.mode(), SelectionMode::Normal);
    }

    #[test]
    fn test_remove_cursor() {
        let mut cs = CursorSet::with_cursor(Position::new(1, 0));
        cs.add(Position::new(2, 0));
        cs.add(Position::new(3, 0));
        assert_eq!(cs.count(), 3);
        cs.remove(1); // remove middle
        assert_eq!(cs.count(), 2);
    }

    #[test]
    fn test_remove_last_cursor_resets_to_origin() {
        let mut cs = CursorSet::with_cursor(Position::new(5, 5));
        // Only one cursor; removing it should reset to origin
        cs.remove(0);
        assert_eq!(cs.count(), 1);
        assert_eq!(cs.primary().pos(), Position::ZERO);
    }

    #[test]
    fn test_collapse_all() {
        let mut cs = CursorSet::new();
        cs.primary_mut().extend_to(Position::new(0, 5));
        assert!(cs.primary().has_selection());
        cs.collapse_all();
        assert!(!cs.primary().has_selection());
    }

    #[test]
    fn test_iter() {
        let mut cs = CursorSet::new();
        cs.add(Position::new(2, 0));
        cs.add(Position::new(1, 0));
        let positions: Vec<Position> = cs.iter().map(|c| c.pos()).collect();
        // Should be sorted
        assert_eq!(
            positions,
            vec![Position::ZERO, Position::new(1, 0), Position::new(2, 0),]
        );
    }

    #[test]
    fn test_all_returns_sorted() {
        let mut cs = CursorSet::new();
        cs.add(Position::new(3, 0));
        cs.add(Position::new(1, 0));
        cs.add(Position::new(2, 0));
        let all = cs.all();
        assert_eq!(all.len(), 4);
        // Check sorted order
        for w in all.windows(2) {
            assert!(w[0].pos() <= w[1].pos());
        }
    }

    #[test]
    fn test_set_multi_selection() {
        let mut cs = CursorSet::new();
        let ranges = vec![
            Range::new(Position::new(0, 0), Position::new(0, 5)),
            Range::new(Position::new(1, 0), Position::new(1, 10)),
        ];
        cs.set_multi_selection(&ranges);
        assert_eq!(cs.count(), 2);
        assert!(cs.primary().has_selection());
    }

    #[test]
    fn test_map_cursors() {
        let mut cs = CursorSet::new();
        cs.add(Position::new(1, 0));
        cs.map_cursors(|c| c.move_to(Position::new(c.pos().line, c.pos().character + 1)));
        assert_eq!(cs.primary().pos(), Position::new(1, 1));
    }

    #[test]
    fn test_cursor_operations() {
        let mut c = Cursor::at(Position::new(3, 7));
        assert_eq!(c.pos(), Position::new(3, 7));
        assert!(!c.has_selection());

        c.extend_to(Position::new(5, 2));
        assert!(c.has_selection());
        assert_eq!(
            c.range(),
            Range::new(Position::new(3, 7), Position::new(5, 2))
        );

        c.collapse();
        assert!(!c.has_selection());
        assert_eq!(c.pos(), Position::new(5, 2));

        c.extend_to(Position::new(1, 0));
        assert!(c.selection.is_reversed());
        assert_eq!(
            c.range(),
            Range::new(Position::new(1, 0), Position::new(5, 2))
        );

        c.collapse_to_start();
        assert_eq!(c.pos(), Position::new(1, 0));

        c.move_to(Position::new(0, 0));
        assert_eq!(c.pos(), Position::ZERO);
    }

    #[test]
    fn test_selection_mode() {
        let mut cs = CursorSet::new();
        assert_eq!(cs.mode(), SelectionMode::Normal);
        cs.set_mode(SelectionMode::Line);
        assert_eq!(cs.mode(), SelectionMode::Line);
        cs.set_mode(SelectionMode::Column);
        assert_eq!(cs.mode(), SelectionMode::Column);
    }
}
