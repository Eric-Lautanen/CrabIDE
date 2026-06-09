//! Custom VT100/VT220 terminal grid state.
//!
//! We implement the grid state machine on top of the `vte` crate's parser,
//! rather than depending on `alacritty_terminal` which brings in winit, glutin,
//! OpenGL, a full config system, and font rasterisation. We don't need any of that.
//!
//! `vte` provides the escape sequence parser (the finite state machine that turns
//! raw bytes into typed events: `print`, `execute`, `csi_dispatch`, `osc_dispatch`,
//! etc.). We implement `vte::Perform` to update our `Grid` state.
//!
//! # Scope
//!
//! - 2D cell grid with `Vec<Row>` where each `Row = Vec<Cell>`
//! - Full 24-bit true color (SGR 38/48)
//! - All 256-color palette entries
//! - Cell attributes: bold, italic, underline, blink, reverse, strikeout, dim
//! - Cursor movement: all CUP, CUF, CUB, CUU, CUD, HVP variants
//! - Erase commands: EL, ED, ECH
//! - Scrollback buffer (configurable depth, implemented as `VecDeque<Row>`)
//! - Alternate screen buffer (for vim, htop, etc.)
//! - OSC 0/1/2: window title
//! - OSC 7: working directory (shell integration)
//! - OSC 8: hyperlinks
//! - SGR mouse reporting
//! - UTF-8 / wide characters (CJK full-width, emoji)

use crabide_core::event::{CellAttrs, ChangedRow, TerminalCell, TerminalColor, TerminalGridDelta};
use std::collections::VecDeque;
use vte::{Parser, Perform};

// --- Cell ---

/// A single terminal cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    /// Number of columns this character occupies (1 for ASCII, 2 for CJK).
    pub width: u8,
    pub fg: Color,
    pub bg: Color,
    pub attrs: Attrs,
    /// OSC 8 hyperlink URL if this cell is part of a clickable hyperlink.
    pub hyperlink: Option<String>,
}

impl Cell {
    pub fn blank() -> Self {
        Self {
            ch: ' ',
            width: 1,
            fg: Color::Default,
            bg: Color::Default,
            attrs: Attrs::empty(),
            hyperlink: None,
        }
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::blank()
    }
}

/// Terminal color representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Default,
    Named(NamedColor),
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NamedColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct Attrs: u8 {
        const BOLD      = 0b0000_0001;
        const ITALIC    = 0b0000_0010;
        const UNDERLINE = 0b0000_0100;
        const BLINK     = 0b0000_1000;
        const REVERSE   = 0b0001_0000;
        const STRIKEOUT = 0b0010_0000;
        const DIM       = 0b0100_0000;
    }
}

// --- Grid ---

const DEFAULT_SCROLLBACK: usize = 10_000;

/// The full terminal grid state.
pub struct Grid {
    /// Number of visible columns.
    pub cols: u16,
    /// Number of visible rows.
    pub rows: u16,
    /// The visible screen (row-major: `screen[row][col]`).
    screen: Vec<Vec<Cell>>,
    /// The alternate screen (used by full-screen apps like vim).
    alt_screen: Vec<Vec<Cell>>,
    /// Whether we're currently in the alternate screen.
    alt_active: bool,
    /// Scrollback buffer (rows scrolled off the top of the primary screen).
    scrollback: VecDeque<Vec<Cell>>,
    /// Max scrollback rows.
    scrollback_limit: usize,

    // Cursor state
    pub cursor_col: u16,
    pub cursor_row: u16,
    /// Saved cursor position (for DECSC/DECRC).
    saved_cursor: (u16, u16),

    // Current SGR attributes for new characters
    cur_fg: Color,
    cur_bg: Color,
    cur_attrs: Attrs,

    // Dirty tracking: rows that changed since last delta extraction
    dirty: Vec<bool>,

    /// Whether each screen row is a soft-wrapped continuation from the
    /// previous row (true) or began with a hard newline (false).
    /// Used for content reflow on terminal resize.
    wrapped: Vec<bool>,

    // Window metadata
    /// OSC 0/2 — window title.
    pub title: Option<String>,
    /// OSC 7 — shell working directory.
    pub cwd: Option<String>,

    // DECSET modes
    /// DECSET 25 — cursor visibility. When false, the terminal cursor should be hidden.
    pub cursor_visible: bool,
    /// DECSET 2004 — bracketed paste mode. When true, pasted text is wrapped in
    /// `\x1b[200~` … `\x1b[201~` delimiters.
    pub bracketed_paste: bool,

    // Scroll region (DECSTBM)
    /// Top row of the scroll region (0-based, inclusive). Default 0.
    scroll_top: u16,
    /// Bottom row of the scroll region (0-based, inclusive). Default rows-1.
    scroll_bottom: u16,

    // Mouse reporting modes
    /// DECSET 1000 — X10 mouse reporting (button press only, no release/motion).
    pub mouse_x10: bool,
    /// DECSET 1002 — normal mouse tracking (button press, release, motion while dragging).
    pub mouse_normal: bool,
    /// DECSET 1003 — button-event mouse tracking (all motion events).
    pub mouse_button_event: bool,
    /// DECSET 1006 — SGR extended mouse mode (used by most modern terminals).
    pub mouse_sgr: bool,

    // Hyperlink state (OSC 8)
    /// Current hyperlink URL while a hyperlink sequence is open.
    cur_hyperlink: Option<String>,

    // Shell integration (OSC 133)
    /// Set when OSC 133 C (command start) is received.
    pub command_started: Option<String>,
    /// Set when OSC 133 D (command finished) is received, with optional exit code.
    pub command_finished: Option<i32>,

    // vte parser
    // vte parser
    parser: Parser,
}

impl Grid {
    pub fn new(cols: u16, rows: u16) -> Self {
        let blank_row = vec![Cell::blank(); cols as usize];
        let screen = vec![blank_row.clone(); rows as usize];
        let alt_screen = vec![blank_row.clone(); rows as usize];
        let dirty = vec![true; rows as usize]; // Start fully dirty

        Self {
            cols,
            rows,
            screen,
            alt_screen,
            alt_active: false,
            scrollback: VecDeque::with_capacity(DEFAULT_SCROLLBACK),
            scrollback_limit: DEFAULT_SCROLLBACK,
            cursor_col: 0,
            cursor_row: 0,
            saved_cursor: (0, 0),
            cur_fg: Color::Default,
            cur_bg: Color::Default,
            cur_attrs: Attrs::empty(),
            dirty,
            wrapped: vec![false; rows as usize],
            title: None,
            cwd: None,
            cursor_visible: true,
            bracketed_paste: false,
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            mouse_x10: false,
            mouse_normal: false,
            mouse_button_event: false,
            mouse_sgr: false,
            cur_hyperlink: None,
            command_started: None,
            command_finished: None,
            parser: Parser::new(),
        }
    }
}

/// Encode a mouse button press event as an escape sequence.
/// Returns `None` if no mouse reporting mode is active.
pub fn encode_mouse_press(
    mouse_x10: bool,
    mouse_normal: bool,
    mouse_button_event: bool,
    mouse_sgr: bool,
    button: MouseButton,
    col: u16,
    row: u16,
) -> Option<Vec<u8>> {
    if !mouse_x10 && !mouse_normal && !mouse_button_event {
        return None;
    }
    Some(if mouse_sgr {
        sgr_mouse_encode(button, col, row, false)
    } else {
        x10_mouse_encode(button, col, row)
    })
}

/// Encode a mouse button release event as an escape sequence.
/// Returns `None` if the active mode does not report releases.
pub fn encode_mouse_release(
    mouse_normal: bool,
    mouse_button_event: bool,
    mouse_sgr: bool,
    button: MouseButton,
    col: u16,
    row: u16,
) -> Option<Vec<u8>> {
    if !mouse_normal && !mouse_button_event {
        return None;
    }
    Some(if mouse_sgr {
        sgr_mouse_encode(button, col, row, true)
    } else {
        x10_mouse_encode_release(button, col, row)
    })
}

/// Encode a mouse motion event (while a button is held) as an escape sequence.
/// Returns `None` if the active mode does not report motion.
pub fn encode_mouse_motion(
    mouse_normal: bool,
    mouse_button_event: bool,
    mouse_sgr: bool,
    button: MouseButton,
    col: u16,
    row: u16,
) -> Option<Vec<u8>> {
    if !mouse_normal && !mouse_button_event {
        return None;
    }
    Some(if mouse_sgr {
        sgr_mouse_encode(button, col, row, false)
    } else {
        x10_mouse_encode_motion(button, col, row)
    })
}

/// Encode a mouse scroll event as an escape sequence.
/// Returns `None` if no mouse reporting mode is active.
pub fn encode_mouse_scroll(
    mouse_x10: bool,
    mouse_normal: bool,
    mouse_button_event: bool,
    mouse_sgr: bool,
    direction: ScrollDirection,
    col: u16,
    row: u16,
) -> Option<Vec<u8>> {
    if !mouse_x10 && !mouse_normal && !mouse_button_event {
        return None;
    }
    let button = match direction {
        ScrollDirection::Up => MouseButton::ScrollUp,
        ScrollDirection::Down => MouseButton::ScrollDown,
    };
    Some(if mouse_sgr {
        sgr_mouse_encode(button, col, row, true)
    } else {
        x10_mouse_encode(button, col, row)
    })
}

impl Grid {
    /// Feed raw bytes from the PTY into the grid.
    pub fn feed(&mut self, bytes: &[u8]) {
        // vte 0.15: advance() takes a &[u8] slice, not individual bytes.
        let mut parser = std::mem::replace(&mut self.parser, Parser::new());
        parser.advance(self, bytes);
        self.parser = parser;
    }

    /// Resize the grid to new dimensions, reflowing soft-wrapped content.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.rows = rows;

        // Only attempt reflow on the primary screen (not alt screen).
        // Alt screen is used by full-screen apps (vim, htop) that handle their own layout.
        if self.alt_active {
            // Alt screen: simple resize without reflow
            let blank_row = || vec![Cell::blank(); cols as usize];
            for row in &mut self.screen {
                row.resize(cols as usize, Cell::blank());
            }
            self.screen.resize_with(rows as usize, blank_row);
            self.wrapped.resize(rows as usize, false);
        } else {
            // Build logical lines by merging consecutive wrapped rows
            let screen = std::mem::take(&mut self.screen);
            let wrapped = std::mem::take(&mut self.wrapped);
            let mut logical: Vec<Vec<Cell>> = Vec::new();

            let mut i = 0;
            while i < screen.len() {
                let mut merged = screen[i].clone();
                // Trim trailing blanks from this row so merging is clean
                while merged.last().is_some_and(|c| c.ch == ' ') {
                    merged.pop();
                }
                // If the NEXT row is a wrapped continuation, merge it
                while i + 1 < screen.len() && i + 1 < wrapped.len() && wrapped[i + 1] {
                    let next = &screen[i + 1];
                    let mut trimmed = next.clone();
                    while trimmed.last().is_some_and(|c| c.ch == ' ') {
                        trimmed.pop();
                    }
                    merged.extend(trimmed);
                    i += 1;
                }
                logical.push(merged);
                i += 1;
            }

            // Re-wrap logical lines at the new column width
            let cols_u = cols as usize;
            let rows_u = rows as usize;
            let mut new_screen: Vec<Vec<Cell>> = Vec::new();
            let mut new_wrapped: Vec<bool> = Vec::new();

            for line in logical {
                if cols_u == 0 {
                    break;
                }
                // Split into chunks of cols_u
                let mut offset = 0;
                while offset < line.len() {
                    let end = (offset + cols_u).min(line.len());
                    let mut row: Vec<Cell> = line[offset..end].to_vec();
                    // Pad with blanks if needed
                    row.resize(cols_u, Cell::blank());
                    if offset > 0 {
                        new_wrapped.push(true);
                    } else {
                        new_wrapped.push(false);
                    }
                    new_screen.push(row);
                    offset += cols_u;
                }
            }

            // Ensure at least `rows` rows
            while new_screen.len() < rows_u {
                new_screen.push(vec![Cell::blank(); cols_u]);
                new_wrapped.push(false);
            }
            // Truncate if too many rows
            new_screen.truncate(rows_u);
            new_wrapped.truncate(rows_u);

            self.screen = new_screen;
            self.wrapped = new_wrapped;
        }

        // Same simple resize for alt screen (stored separately)
        let blank_row = || vec![Cell::blank(); cols as usize];
        for row in &mut self.alt_screen {
            row.resize(cols as usize, Cell::blank());
        }
        self.alt_screen.resize_with(rows as usize, blank_row);

        self.dirty = vec![true; rows as usize];

        // Clamp cursor
        self.cursor_col = self.cursor_col.min(cols.saturating_sub(1));
        self.cursor_row = self.cursor_row.min(rows.saturating_sub(1));

        // Clamp scroll region
        self.scroll_bottom = self.scroll_bottom.min(rows.saturating_sub(1));
        self.scroll_top = self.scroll_top.min(self.scroll_bottom);
    }

    /// Extract a delta of all dirty rows since the last call, and clear dirty flags.
    pub fn take_delta(&mut self) -> TerminalGridDelta {
        let mut changed_rows = Vec::new();
        let active = if self.alt_active {
            &self.alt_screen
        } else {
            &self.screen
        };

        for (row_idx, is_dirty) in self.dirty.iter_mut().enumerate() {
            if *is_dirty {
                let cells: Vec<TerminalCell> = active[row_idx]
                    .iter()
                    .map(|c| TerminalCell {
                        ch: c.ch,
                        fg: color_to_event(c.fg),
                        bg: color_to_event(c.bg),
                        attrs: cell_attrs_to_event(c.attrs),
                        hyperlink: c.hyperlink.clone(),
                    })
                    .collect();
                changed_rows.push(ChangedRow {
                    row: row_idx as u16,
                    cells,
                });
                *is_dirty = false;
            }
        }

        TerminalGridDelta {
            rows: changed_rows,
            cursor_col: self.cursor_col,
            cursor_row: self.cursor_row,
            scroll_top: self.scrollback.len() as u32,
            cursor_visible: self.cursor_visible,
            bracketed_paste: self.bracketed_paste,
            mouse_x10: self.mouse_x10,
            mouse_normal: self.mouse_normal,
            mouse_button_event: self.mouse_button_event,
            mouse_sgr: self.mouse_sgr,
        }
    }

    // --- Internal helpers ---

    fn active_screen(&mut self) -> &mut Vec<Vec<Cell>> {
        if self.alt_active {
            &mut self.alt_screen
        } else {
            &mut self.screen
        }
    }

    fn mark_dirty(&mut self, row: usize) {
        if row < self.dirty.len() {
            self.dirty[row] = true;
        }
    }

    fn put_char(&mut self, ch: char, width: u8) {
        let row = self.cursor_row as usize;
        let col = self.cursor_col as usize;
        // Cache before the mutable borrow of self via active_screen()
        let cols = self.cols as usize;
        let rows = self.rows as usize;
        let hyperlink = self.cur_hyperlink.clone();

        if row < rows && col < cols {
            let cell = Cell {
                ch,
                width,
                fg: self.cur_fg,
                bg: self.cur_bg,
                attrs: self.cur_attrs,
                hyperlink: hyperlink.clone(),
            };
            let screen = self.active_screen();
            screen[row][col] = cell;

            // If wide character, blank the next cell
            if width == 2 && col + 1 < cols {
                screen[row][col + 1] = Cell {
                    ch: ' ',
                    width: 0,
                    hyperlink,
                    ..Cell::blank()
                };
            }
            self.dirty[row] = true;
        }

        // Advance cursor
        let advance = if width == 0 { 1 } else { u16::from(width) };
        self.cursor_col += advance;

        // Line wrap
        if self.cursor_col >= self.cols {
            self.cursor_col = 0;
            self.cursor_row += 1;
            // Mark the new row as a soft-wrapped continuation
            if (self.cursor_row as usize) < self.wrapped.len() {
                self.wrapped[self.cursor_row as usize] = true;
            }
            if self.cursor_row > self.scroll_bottom {
                self.cursor_row = self.scroll_bottom;
                self.scroll_up(1);
            }
        }
    }

    fn scroll_up(&mut self, count: u16) {
        let top = self.scroll_top as usize;
        let bottom = self.scroll_bottom as usize;
        for _ in 0..count {
            // Remove the top row of the scroll region
            let removed = self.screen.remove(top);
            self.wrapped.remove(top);
            // Only push to scrollback if the scroll region starts at row 0
            if top == 0 {
                if self.scrollback.len() >= self.scrollback_limit {
                    self.scrollback.pop_front();
                }
                self.scrollback.push_back(removed);
            }
            // Insert a blank row at the bottom of the scroll region
            self.screen
                .insert(bottom, vec![Cell::blank(); self.cols as usize]);
            self.wrapped.insert(bottom, false);
        }
        // Mark scroll region rows as dirty
        for r in top..=bottom {
            self.mark_dirty(r);
        }
    }

    /// Scroll the scroll region down by `count` lines.
    /// New blank lines appear at the top of the region; lines at the bottom are discarded.
    fn scroll_down(&mut self, count: u16) {
        let top = self.scroll_top as usize;
        let bottom = self.scroll_bottom as usize;
        for _ in 0..count {
            // Remove the bottom row of the scroll region
            self.screen.remove(bottom);
            self.wrapped.remove(bottom);
            // Insert a blank row at the top of the scroll region
            self.screen
                .insert(top, vec![Cell::blank(); self.cols as usize]);
            self.wrapped.insert(top, false);
        }
        // Mark scroll region rows as dirty
        for r in top..=bottom {
            self.mark_dirty(r);
        }
    }

    /// Apply SGR (Select Graphic Rendition) parameters.
    fn apply_sgr(&mut self, params: &[u16]) {
        let mut i = 0;
        if params.is_empty() {
            // SGR 0: reset all
            self.cur_fg = Color::Default;
            self.cur_bg = Color::Default;
            self.cur_attrs = Attrs::empty();
            return;
        }

        while i < params.len() {
            match params[i] {
                0 => {
                    self.cur_fg = Color::Default;
                    self.cur_bg = Color::Default;
                    self.cur_attrs = Attrs::empty();
                }
                1 => self.cur_attrs.insert(Attrs::BOLD),
                2 => self.cur_attrs.insert(Attrs::DIM),
                3 => self.cur_attrs.insert(Attrs::ITALIC),
                4 => self.cur_attrs.insert(Attrs::UNDERLINE),
                5 | 6 => self.cur_attrs.insert(Attrs::BLINK),
                7 => self.cur_attrs.insert(Attrs::REVERSE),
                9 => self.cur_attrs.insert(Attrs::STRIKEOUT),
                22 => self.cur_attrs.remove(Attrs::BOLD | Attrs::DIM),
                23 => self.cur_attrs.remove(Attrs::ITALIC),
                24 => self.cur_attrs.remove(Attrs::UNDERLINE),
                25 => self.cur_attrs.remove(Attrs::BLINK),
                27 => self.cur_attrs.remove(Attrs::REVERSE),
                29 => self.cur_attrs.remove(Attrs::STRIKEOUT),

                // Standard 8 foreground colors
                30..=37 => self.cur_fg = Color::Named(named_fg(params[i] - 30)),
                38 => {
                    if i + 2 < params.len() && params[i + 1] == 5 {
                        self.cur_fg = Color::Indexed(params[i + 2] as u8);
                        i += 2;
                    } else if i + 4 < params.len() && params[i + 1] == 2 {
                        self.cur_fg = Color::Rgb(
                            params[i + 2] as u8,
                            params[i + 3] as u8,
                            params[i + 4] as u8,
                        );
                        i += 4;
                    }
                }
                39 => self.cur_fg = Color::Default,

                // Standard 8 background colors
                40..=47 => self.cur_bg = Color::Named(named_fg(params[i] - 40)),
                48 => {
                    if i + 2 < params.len() && params[i + 1] == 5 {
                        self.cur_bg = Color::Indexed(params[i + 2] as u8);
                        i += 2;
                    } else if i + 4 < params.len() && params[i + 1] == 2 {
                        self.cur_bg = Color::Rgb(
                            params[i + 2] as u8,
                            params[i + 3] as u8,
                            params[i + 4] as u8,
                        );
                        i += 4;
                    }
                }
                49 => self.cur_bg = Color::Default,

                // Bright foreground colors
                90..=97 => self.cur_fg = Color::Named(named_bright_fg(params[i] - 90)),
                // Bright background colors
                100..=107 => self.cur_bg = Color::Named(named_bright_fg(params[i] - 100)),

                _ => {} // Unknown SGR ignore
            }
            i += 1;
        }
    }
}

// --- vte::Perform implementation ---

impl Perform for Grid {
    fn print(&mut self, c: char) {
        let width = unicode_width(c);
        self.put_char(c, width as u8);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x08 if self.cursor_col > 0 => {
                // BS backspace
                self.cursor_col -= 1;
            }
            0x09 => {
                // HT horizontal tab
                let next_tab = ((self.cursor_col / 8) + 1) * 8;
                self.cursor_col = next_tab.min(self.cols - 1);
            }
            0x0A..=0x0C => {
                // LF, VT, FF line feed
                self.cursor_row += 1;
                if self.cursor_row > self.scroll_bottom {
                    self.cursor_row = self.scroll_bottom;
                    self.scroll_up(1);
                }
                self.mark_dirty(self.cursor_row as usize);
            }
            0x0D => {
                // CR carriage return
                self.cursor_col = 0;
            }
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        let p: Vec<u16> = params
            .iter()
            .map(|s| s.first().copied().unwrap_or(0))
            .collect();
        let p1 = p.first().copied().unwrap_or(0);
        let p2 = p.get(1).copied().unwrap_or(0);

        match action {
            // CUP Cursor Position
            'H' | 'f' => {
                self.cursor_row = p1.saturating_sub(1).min(self.rows - 1);
                self.cursor_col = p2.saturating_sub(1).min(self.cols - 1);
            }
            // CUU Cursor Up
            'A' => {
                self.cursor_row = self.cursor_row.saturating_sub(p1.max(1));
            }
            // CUD Cursor Down
            'B' => {
                self.cursor_row = (self.cursor_row + p1.max(1)).min(self.rows - 1);
            }
            // CUF Cursor Forward
            'C' => {
                self.cursor_col = (self.cursor_col + p1.max(1)).min(self.cols - 1);
            }
            // CUB Cursor Back
            'D' => {
                self.cursor_col = self.cursor_col.saturating_sub(p1.max(1));
            }
            // ED Erase in Display
            'J' => {
                let rows = self.rows as usize;
                let cols = self.cols as usize;
                match p1 {
                    0 => {
                        // Erase from cursor to end of screen
                        let row = self.cursor_row as usize;
                        let col = self.cursor_col as usize;
                        if row < rows {
                            for c in col..cols {
                                self.screen[row][c] = Cell::blank();
                            }
                            self.mark_dirty(row);
                            for r in (row + 1)..rows {
                                self.screen[r] = vec![Cell::blank(); cols];
                                self.mark_dirty(r);
                            }
                        }
                    }
                    1 => {
                        // Erase from start to cursor
                        let row = self.cursor_row as usize;
                        for r in 0..row {
                            self.screen[r] = vec![Cell::blank(); cols];
                            self.mark_dirty(r);
                        }
                        if row < rows {
                            let col = self.cursor_col as usize;
                            for c in 0..=col {
                                self.screen[row][c] = Cell::blank();
                            }
                            self.mark_dirty(row);
                        }
                    }
                    2 | 3 => {
                        // Erase entire screen
                        for r in 0..rows {
                            self.screen[r] = vec![Cell::blank(); cols];
                            self.mark_dirty(r);
                        }
                        self.cursor_row = 0;
                        self.cursor_col = 0;
                    }
                    _ => {}
                }
            }
            // EL Erase in Line
            'K' => {
                let row = self.cursor_row as usize;
                let col = self.cursor_col as usize;
                let cols = self.cols as usize;
                if row < self.rows as usize {
                    match p1 {
                        0 => {
                            for c in col..cols {
                                self.screen[row][c] = Cell::blank();
                            }
                        }
                        1 => {
                            for c in 0..=col {
                                self.screen[row][c] = Cell::blank();
                            }
                        }
                        2 => {
                            self.screen[row] = vec![Cell::blank(); cols];
                        }
                        _ => {}
                    }
                    self.mark_dirty(row);
                }
            }
            // SGR Set Graphic Rendition
            'm' => {
                self.apply_sgr(&p);
            }
            // DECSC Save cursor
            's' => {
                self.saved_cursor = (self.cursor_col, self.cursor_row);
            }
            // DECRC Restore cursor
            'u' => {
                self.cursor_col = self.saved_cursor.0;
                self.cursor_row = self.saved_cursor.1;
            }
            // DECSTBM — Set Scrolling Region
            'r' => {
                let top = if p1 == 0 { 0 } else { p1.saturating_sub(1) };
                let bottom = if p2 == 0 {
                    self.rows.saturating_sub(1)
                } else {
                    p2.saturating_sub(1).min(self.rows.saturating_sub(1))
                };
                if top < bottom {
                    self.scroll_top = top;
                    self.scroll_bottom = bottom;
                }
                // DECSTBM also moves cursor to home position
                self.cursor_col = 0;
                self.cursor_row = 0;
            }
            // Insert Lines (CSI L) — insert N blank lines at cursor row,
            // scrolling existing lines within the scroll region down.
            'L' => {
                let n = p1.max(1);
                let row = self.cursor_row as usize;
                let top = self.scroll_top as usize;
                let bottom = self.scroll_bottom as usize;
                if row >= top && row <= bottom {
                    let count = n.min((bottom - row + 1) as u16);
                    for _ in 0..count {
                        self.screen.remove(bottom);
                        self.screen
                            .insert(row, vec![Cell::blank(); self.cols as usize]);
                    }
                    for r in row..=bottom {
                        self.mark_dirty(r);
                    }
                }
            }
            // Delete Lines (CSI M) — delete N lines at cursor row,
            // scrolling lines below up within the scroll region.
            'M' => {
                let n = p1.max(1);
                let row = self.cursor_row as usize;
                let top = self.scroll_top as usize;
                let bottom = self.scroll_bottom as usize;
                if row >= top && row <= bottom {
                    let count = n.min((bottom - row + 1) as u16);
                    for _ in 0..count {
                        self.screen.remove(row);
                        self.screen
                            .insert(bottom, vec![Cell::blank(); self.cols as usize]);
                    }
                    for r in row..=bottom {
                        self.mark_dirty(r);
                    }
                }
            }
            // Insert Characters (CSI @) — insert N blank chars at cursor,
            // shifting existing chars right; chars past the right edge are lost.
            '@' => {
                let n = p1.max(1) as usize;
                let row = self.cursor_row as usize;
                let col = self.cursor_col as usize;
                let cols = self.cols as usize;
                if row < self.rows as usize && col < cols {
                    let count = n.min(cols - col);
                    // Shift characters right by `count`, discarding those past the edge
                    for c in (col..cols).rev() {
                        if c >= count {
                            self.screen[row][c] = self.screen[row][c - count].clone();
                        }
                    }
                    // Fill the inserted positions with blanks
                    for c in col..(col + count) {
                        if c < cols {
                            self.screen[row][c] = Cell::blank();
                        }
                    }
                    self.mark_dirty(row);
                }
            }
            // Delete Characters (CSI P) — delete N chars at cursor,
            // shifting remaining chars left; blanks fill from the right edge.
            'P' => {
                let n = p1.max(1) as usize;
                let row = self.cursor_row as usize;
                let col = self.cursor_col as usize;
                let cols = self.cols as usize;
                if row < self.rows as usize && col < cols {
                    let count = n.min(cols - col);
                    // Shift characters left by `count`
                    for c in col..cols {
                        if c + count < cols {
                            self.screen[row][c] = self.screen[row][c + count].clone();
                        } else {
                            self.screen[row][c] = Cell::blank();
                        }
                    }
                    self.mark_dirty(row);
                }
            }
            // DECSET / DECRST — handle ? prefix modes
            'h' if params.iter().any(|s| s.first().copied() == Some(25)) => {
                self.cursor_visible = true;
            }
            'l' if params.iter().any(|s| s.first().copied() == Some(25)) => {
                self.cursor_visible = false;
            }
            'h' if params.iter().any(|s| s.first().copied() == Some(2004)) => {
                self.bracketed_paste = true;
            }
            'l' if params.iter().any(|s| s.first().copied() == Some(2004)) => {
                self.bracketed_paste = false;
            }
            // Mouse reporting modes
            'h' if params.iter().any(|s| s.first().copied() == Some(1000)) => {
                self.mouse_x10 = true;
            }
            'l' if params.iter().any(|s| s.first().copied() == Some(1000)) => {
                self.mouse_x10 = false;
            }
            'h' if params.iter().any(|s| s.first().copied() == Some(1002)) => {
                self.mouse_normal = true;
            }
            'l' if params.iter().any(|s| s.first().copied() == Some(1002)) => {
                self.mouse_normal = false;
            }
            'h' if params.iter().any(|s| s.first().copied() == Some(1003)) => {
                self.mouse_button_event = true;
            }
            'l' if params.iter().any(|s| s.first().copied() == Some(1003)) => {
                self.mouse_button_event = false;
            }
            'h' if params.iter().any(|s| s.first().copied() == Some(1006)) => {
                self.mouse_sgr = true;
            }
            'l' if params.iter().any(|s| s.first().copied() == Some(1006)) => {
                self.mouse_sgr = false;
            }
            // Alternate screen: switch in (?1049h) / out (?1049l)
            'h' if params.iter().any(|s| s.first().copied() == Some(1049)) => {
                self.alt_active = true;
                self.dirty.iter_mut().for_each(|d| *d = true);
            }
            'l' if params.iter().any(|s| s.first().copied() == Some(1049)) => {
                self.alt_active = false;
                self.dirty.iter_mut().for_each(|d| *d = true);
            }
            _ => {} // Unknown CSI ignore
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if params.is_empty() {
            return;
        }
        let cmd = params[0];

        match cmd {
            b"0" | b"2" => {
                // Set title: OSC 0/2 ; title ST
                let title_data = params.get(1).copied().unwrap_or(&[]);
                self.title = std::str::from_utf8(title_data)
                    .ok()
                    .map(std::borrow::ToOwned::to_owned);
            }
            b"7" => {
                // Shell working directory: OSC 7 ; path ST
                let cwd_data = params.get(1).copied().unwrap_or(&[]);
                self.cwd = std::str::from_utf8(cwd_data)
                    .ok()
                    .map(std::borrow::ToOwned::to_owned);
            }
            // OSC 8: hyperlinks
            // Format: ESC ] 8 ; params ; url BEL
            // Close:  ESC ] 8 ; ; BEL  (empty URL)
            b"8" => {
                let url = params.get(2).copied().unwrap_or(&[]);
                if url.is_empty() {
                    self.cur_hyperlink = None;
                } else {
                    self.cur_hyperlink = std::str::from_utf8(url)
                        .ok()
                        .map(std::borrow::ToOwned::to_owned);
                }
            }
            // OSC 133: shell integration
            // Format: ESC ] 133 ; A/B/C/D/L [; data] ST
            b"133" => {
                let marker = params.get(1).copied().unwrap_or(&[]);
                match marker {
                    b"A" => {
                        // Prompt start
                    }
                    b"B" => {
                        // Prompt end
                    }
                    b"C" => {
                        // Command started — optional text in params[2]
                        let command = params
                            .get(2)
                            .and_then(|s| std::str::from_utf8(s).ok())
                            .unwrap_or("")
                            .to_owned();
                        self.command_started = Some(command);
                    }
                    b"D" => {
                        // Command finished — optional exit code in params[2]
                        let exit_code = params
                            .get(2)
                            .and_then(|s| std::str::from_utf8(s).ok())
                            .and_then(|s| s.trim().parse::<i32>().ok());
                        self.command_finished = Some(exit_code.unwrap_or(0));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn hook(&mut self, _: &vte::Params, _: &[u8], _: bool, _: char) {}
    fn put(&mut self, _: u8) {}
    fn unhook(&mut self) {}
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        // ESC M — Reverse Index (RI): move cursor up one line; if at top of
        // scroll region, scroll the region down by one line.
        if byte == b'M' {
            if self.cursor_row == self.scroll_top {
                self.scroll_down(1);
            } else if self.cursor_row > 0 {
                self.cursor_row -= 1;
            }
        }
    }
}

// --- Helpers ---

fn named_fg(n: u16) -> NamedColor {
    match n {
        0 => NamedColor::Black,
        1 => NamedColor::Red,
        2 => NamedColor::Green,
        3 => NamedColor::Yellow,
        4 => NamedColor::Blue,
        5 => NamedColor::Magenta,
        6 => NamedColor::Cyan,
        _ => NamedColor::White,
    }
}

fn named_bright_fg(n: u16) -> NamedColor {
    match n {
        0 => NamedColor::BrightBlack,
        1 => NamedColor::BrightRed,
        2 => NamedColor::BrightGreen,
        3 => NamedColor::BrightYellow,
        4 => NamedColor::BrightBlue,
        5 => NamedColor::BrightMagenta,
        6 => NamedColor::BrightCyan,
        _ => NamedColor::BrightWhite,
    }
}

fn color_to_event(c: Color) -> TerminalColor {
    match c {
        Color::Default => TerminalColor::Default,
        Color::Named(n) => TerminalColor::Indexed(named_to_index(n)),
        Color::Indexed(i) => TerminalColor::Indexed(i),
        Color::Rgb(r, g, b) => TerminalColor::Rgb(r, g, b),
    }
}

fn named_to_index(n: NamedColor) -> u8 {
    match n {
        NamedColor::Black => 0,
        NamedColor::Red => 1,
        NamedColor::Green => 2,
        NamedColor::Yellow => 3,
        NamedColor::Blue => 4,
        NamedColor::Magenta => 5,
        NamedColor::Cyan => 6,
        NamedColor::White => 7,
        NamedColor::BrightBlack => 8,
        NamedColor::BrightRed => 9,
        NamedColor::BrightGreen => 10,
        NamedColor::BrightYellow => 11,
        NamedColor::BrightBlue => 12,
        NamedColor::BrightMagenta => 13,
        NamedColor::BrightCyan => 14,
        NamedColor::BrightWhite => 15,
    }
}

fn cell_attrs_to_event(a: Attrs) -> CellAttrs {
    let mut out = CellAttrs::empty();
    if a.contains(Attrs::BOLD) {
        out.insert(CellAttrs::BOLD);
    }
    if a.contains(Attrs::ITALIC) {
        out.insert(CellAttrs::ITALIC);
    }
    if a.contains(Attrs::UNDERLINE) {
        out.insert(CellAttrs::UNDERLINE);
    }
    if a.contains(Attrs::BLINK) {
        out.insert(CellAttrs::BLINK);
    }
    if a.contains(Attrs::REVERSE) {
        out.insert(CellAttrs::REVERSE);
    }
    if a.contains(Attrs::STRIKEOUT) {
        out.insert(CellAttrs::STRIKEOUT);
    }
    if a.contains(Attrs::DIM) {
        out.insert(CellAttrs::DIM);
    }
    out
}

// ── Mouse encoding types ────────────────────────────────────────────────────

/// Mouse button for terminal mouse reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    ScrollUp,
    ScrollDown,
}

/// Scroll direction for mouse wheel events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScrollDirection {
    Up,
    Down,
}

// ── Mouse encoding helpers ──────────────────────────────────────────────────

/// Encode a mouse button press using X10 protocol: `\e[Mbxy`
/// where b = 32 + button_code, x = 32 + col + 1, y = 32 + row + 1.
fn x10_mouse_encode(button: MouseButton, col: u16, row: u16) -> Vec<u8> {
    let cb = mouse_button_code(button, false);
    let b = (32 + cb) as u8;
    let x = (32 + u32::from(col.saturating_add(1).min(223))) as u8;
    let y = (32 + u32::from(row.saturating_add(1).min(223))) as u8;
    vec![0x1b, b'[', b'M', b, x, y]
}

/// Encode a mouse button release using X10 protocol.
fn x10_mouse_encode_release(button: MouseButton, col: u16, row: u16) -> Vec<u8> {
    let cb = mouse_button_code(button, true);
    let b = (32 + cb) as u8;
    let x = (32 + u32::from(col.saturating_add(1).min(223))) as u8;
    let y = (32 + u32::from(row.saturating_add(1).min(223))) as u8;
    vec![0x1b, b'[', b'M', b, x, y]
}

/// Encode a mouse motion event using X10 protocol (button + 32 motion flag).
fn x10_mouse_encode_motion(button: MouseButton, col: u16, row: u16) -> Vec<u8> {
    let cb = mouse_button_code(button, false) + 32;
    let b = (32 + cb) as u8;
    let x = (32 + u32::from(col.saturating_add(1).min(223))) as u8;
    let y = (32 + u32::from(row.saturating_add(1).min(223))) as u8;
    vec![0x1b, b'[', b'M', b, x, y]
}

/// Encode a mouse event using SGR extended protocol: `\e[<b;x;yM` (press) or `\e[<b;x;ym` (release).
fn sgr_mouse_encode(button: MouseButton, col: u16, row: u16, release: bool) -> Vec<u8> {
    let cb = mouse_button_code(button, release);
    let suffix = if release { 'm' } else { 'M' };
    format!("\x1b[<{cb};{};{}{suffix}", col + 1, row + 1).into_bytes()
}

/// Map a `MouseButton` to the X10/SGR button code.
/// - Left=0, Middle=1, Right=2, Release=3, ScrollUp=4, ScrollDown=5
fn mouse_button_code(button: MouseButton, release: bool) -> u32 {
    match button {
        MouseButton::Left if release => 3,
        MouseButton::Left => 0,
        MouseButton::Middle if release => 3,
        MouseButton::Middle => 1,
        MouseButton::Right if release => 3,
        MouseButton::Right => 2,
        MouseButton::ScrollUp => 4 + 64,
        MouseButton::ScrollDown => 5 + 64,
    }
}

/// Unicode character display width using the `unicode-width` crate.
/// Returns 2 for CJK/full-width/emoji, 0 for combining/control, 1 otherwise.
fn unicode_width(c: char) -> usize {
    unicode_width::UnicodeWidthChar::width(c).unwrap_or(0)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn grid_24x80() -> Grid {
        Grid::new(80, 24)
    }

    // ── Grid construction ──────────────────────────────────────────────────

    #[test]
    fn grid_new_initial_state() {
        let g = grid_24x80();
        assert_eq!(g.cols, 80);
        assert_eq!(g.rows, 24);
        assert_eq!(g.cursor_col, 0);
        assert_eq!(g.cursor_row, 0);
        assert!(g.title.is_none());
        assert!(g.cwd.is_none());
        assert_eq!(g.scrollback.len(), 0);
    }

    #[test]
    fn grid_new_all_cells_blank() {
        let g = grid_24x80();
        for row in 0..g.rows {
            for col in 0..g.cols {
                let cell = &g.screen[row as usize][col as usize];
                assert_eq!(cell.ch, ' ', "cell at {row},{col} not blank");
                assert_eq!(cell.width, 1);
                assert_eq!(cell.fg, Color::Default);
                assert_eq!(cell.bg, Color::Default);
                assert_eq!(cell.attrs, Attrs::empty());
            }
        }
    }

    // ── feed / print ───────────────────────────────────────────────────────

    #[test]
    fn feed_ascii_text() {
        let mut g = grid_24x80();
        g.feed(b"Hello");
        assert_eq!(g.cursor_col, 5);
        assert_eq!(g.cursor_row, 0);
        assert_eq!(g.screen[0][0].ch, 'H');
        assert_eq!(g.screen[0][1].ch, 'e');
        assert_eq!(g.screen[0][2].ch, 'l');
        assert_eq!(g.screen[0][3].ch, 'l');
        assert_eq!(g.screen[0][4].ch, 'o');
    }

    #[test]
    fn feed_newline() {
        let mut g = grid_24x80();
        g.feed(b"ab\ncd");
        // After 'ab': cursor at (row=0, col=2), screen[0][0]='a', screen[0][1]='b'
        // After LF: cursor_row=1, cursor_col=2
        // After 'cd': cursor at (row=1, col=4), screen[1][2]='c', screen[1][3]='d'
        assert_eq!(g.cursor_row, 1);
        assert_eq!(g.cursor_col, 4);
        assert_eq!(g.screen[0][0].ch, 'a');
        assert_eq!(g.screen[0][1].ch, 'b');
        assert_eq!(g.screen[1][2].ch, 'c');
        assert_eq!(g.screen[1][3].ch, 'd');
    }

    #[test]
    fn feed_carriage_return() {
        let mut g = grid_24x80();
        g.feed(b"abc\rX");
        assert_eq!(g.cursor_col, 1);
        assert_eq!(g.screen[0][0].ch, 'X');
        assert_eq!(g.screen[0][1].ch, 'b');
    }

    #[test]
    fn feed_tab() {
        let mut g = grid_24x80();
        g.feed(b"\tX");
        // Tab moves to column 8 (next multiple of 8)
        assert_eq!(g.cursor_col, 9);
        assert_eq!(g.screen[0][8].ch, 'X');
    }

    #[test]
    fn feed_backspace() {
        let mut g = grid_24x80();
        g.feed(b"ab\x08X");
        // 'a' at col 0, 'b' at col 1, BS moves to col 1, 'X' overwrites 'b'
        assert_eq!(g.cursor_col, 2);
        assert_eq!(g.screen[0][0].ch, 'a');
        assert_eq!(g.screen[0][1].ch, 'X');
    }

    #[test]
    fn feed_backspace_at_col_zero() {
        let mut g = grid_24x80();
        g.feed(b"\x08");
        // Backspace at col 0 should be no-op
        assert_eq!(g.cursor_col, 0);
    }

    #[test]
    fn feed_line_wrap() {
        let mut g = Grid::new(5, 3);
        // Fill first line (cols 0-4)
        g.feed(b"12345");
        // After 5 chars on a 5-wide grid, cursor wraps to next line
        // because after advancing from col 4 to col 5, the wrap check triggers:
        // cursor_col (5) >= cols (5) → cursor_col=0, cursor_row=1
        assert_eq!(g.cursor_row, 1);
        assert_eq!(g.cursor_col, 0);
        // The 5th char '5' is at col 4 on row 0
        assert_eq!(g.screen[0][4].ch, '5');
    }

    #[test]
    fn feed_scroll_when_full() {
        let mut g = Grid::new(5, 2);
        g.feed(b"12345"); // fill line 0
        g.feed(b"67890"); // fill line 1, line 0 scrolls up
        // Cursor should be at row 1 (the last line after scroll)
        assert_eq!(g.cursor_row, 1);
        // Scrollback should have the first line
        assert_eq!(g.scrollback.len(), 1);
        // The visible screen now has line 1 content (67890) on row 0
        // and the new line is blank
        assert_eq!(g.screen[0][0].ch, '6');
        assert_eq!(g.screen[1][0].ch, ' '); // blank new line
    }

    // ── CSI cursor movement ────────────────────────────────────────────────

    #[test]
    fn csi_cursor_up() {
        let mut g = grid_24x80();
        g.cursor_row = 5;
        g.feed(b"\x1b[A"); // CUU
        assert_eq!(g.cursor_row, 4);
    }

    #[test]
    fn csi_cursor_up_clamped() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[A"); // CUU at top should clamp to 0
        assert_eq!(g.cursor_row, 0);
    }

    #[test]
    fn csi_cursor_down() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[B"); // CUD
        assert_eq!(g.cursor_row, 1);
    }

    #[test]
    fn csi_cursor_down_clamped() {
        let mut g = grid_24x80();
        g.cursor_row = 23;
        g.feed(b"\x1b[B"); // CUD at bottom should clamp
        assert_eq!(g.cursor_row, 23);
    }

    #[test]
    fn csi_cursor_forward() {
        let mut g = grid_24x80();
        g.cursor_col = 10;
        g.feed(b"\x1b[C"); // CUF
        assert_eq!(g.cursor_col, 11);
    }

    #[test]
    fn csi_cursor_back() {
        let mut g = grid_24x80();
        g.cursor_col = 10;
        g.feed(b"\x1b[D"); // CUB
        assert_eq!(g.cursor_col, 9);
    }

    #[test]
    fn csi_cursor_position() {
        let mut g = grid_24x80();
        // CUP row;col (1-based)
        g.feed(b"\x1b[5;10H");
        assert_eq!(g.cursor_row, 4);
        assert_eq!(g.cursor_col, 9);
    }

    #[test]
    fn csi_cursor_position_alt_form() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[3;6f"); // HVP (same as CUP)
        assert_eq!(g.cursor_row, 2);
        assert_eq!(g.cursor_col, 5);
    }

    // ── DECSC / DECRC ────────────────────────────────────────────────────

    #[test]
    fn decsc_decrc() {
        let mut g = grid_24x80();
        g.cursor_col = 30;
        g.cursor_row = 10;
        g.feed(b"\x1b[s"); // DECSC save
        g.cursor_col = 0;
        g.cursor_row = 0;
        g.feed(b"\x1b[u"); // DECRC restore
        assert_eq!(g.cursor_col, 30);
        assert_eq!(g.cursor_row, 10);
    }

    // ── SGR ────────────────────────────────────────────────────────────────

    #[test]
    fn sgr_bold() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[1mX");
        assert!(g.screen[0][0].attrs.contains(Attrs::BOLD));
    }

    #[test]
    fn sgr_italic() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[3mX");
        assert!(g.screen[0][0].attrs.contains(Attrs::ITALIC));
    }

    #[test]
    fn sgr_foreground_color() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[31mX"); // red foreground
        assert_eq!(g.screen[0][0].fg, Color::Named(NamedColor::Red));
    }

    #[test]
    fn sgr_background_color() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[44mX"); // blue background
        assert_eq!(g.screen[0][0].bg, Color::Named(NamedColor::Blue));
    }

    #[test]
    fn sgr_reset() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[1;31;44mX"); // bold + red fg + blue bg
        assert!(g.screen[0][0].attrs.contains(Attrs::BOLD));
        g.feed(b"\x1b[0mY"); // reset
        assert!(!g.screen[0][1].attrs.contains(Attrs::BOLD));
        assert_eq!(g.screen[0][1].fg, Color::Default);
        assert_eq!(g.screen[0][1].bg, Color::Default);
    }

    #[test]
    fn sgr_256_color() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[38;5;82mX"); // indexed color 82
        assert_eq!(g.screen[0][0].fg, Color::Indexed(82));
    }

    #[test]
    fn sgr_true_color() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[38;2;255;128;0mX"); // RGB
        assert_eq!(g.screen[0][0].fg, Color::Rgb(255, 128, 0));
    }

    #[test]
    fn sgr_bright_colors() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[91mX"); // bright red fg (90+1=91)
        assert_eq!(g.screen[0][0].fg, Color::Named(NamedColor::BrightRed));
        g.feed(b"\x1b[104mY"); // bright cyan bg (100+4=104 → BrightBlue? No...)
        // 104 - 100 = 4 → named_bright_fg(4) = BrightBlue
        assert_eq!(g.screen[0][1].bg, Color::Named(NamedColor::BrightBlue));
    }

    #[test]
    fn sgr_multiple_attributes() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[1;4;7mX"); // bold + underline + reverse
        let cell = g.screen[0][0].clone();
        assert!(cell.attrs.contains(Attrs::BOLD));
        assert!(cell.attrs.contains(Attrs::UNDERLINE));
        assert!(cell.attrs.contains(Attrs::REVERSE));
    }

    // ── Erase commands ───────────────────────────────────────────────────

    #[test]
    fn erase_in_line_to_end() {
        let mut g = Grid::new(10, 3);
        g.feed(b"ABCDE"); // 5 chars, cursor at col 5
        g.feed(b"\x1b[K"); // EL 0: erase from cursor (col 5) to end of line (col 9)
        let line = &g.screen[0];
        assert_eq!(line[0].ch, 'A');
        assert_eq!(line[4].ch, 'E');
        assert_eq!(line[5].ch, ' '); // erased
        assert_eq!(line[9].ch, ' ');
    }

    #[test]
    fn erase_in_line_to_start() {
        let mut g = Grid::new(10, 3);
        g.feed(b"ABCDE");
        g.cursor_col = 3; // position cursor at col 3 on row 0
        g.feed(b"\x1b[1K"); // EL 1: erase from start to cursor (col 3)
        let line = &g.screen[0];
        assert_eq!(line[0].ch, ' ');
        assert_eq!(line[3].ch, ' ');
        assert_eq!(line[4].ch, 'E'); // not erased
    }

    #[test]
    fn erase_in_line_all() {
        let mut g = Grid::new(10, 3);
        g.feed(b"ABCDE");
        g.feed(b"\x1b[2K"); // EL 2: erase entire line
        for cell in &g.screen[0] {
            assert_eq!(cell.ch, ' ');
        }
    }

    #[test]
    fn erase_in_display_to_end() {
        let mut g = Grid::new(10, 5);
        // Fill rows with distinct content
        g.feed(b"Row0\nRow1\nRow2\nRow3");
        // cursor is now at row 3 (after Row3), col 4
        g.feed(b"\x1b[2H"); // CUP to row 2 (1-based) → row=1, col=0
        g.feed(b"\x1b[J"); // ED 0: erase from cursor to end
        // Row 0 should be untouched
        assert_eq!(g.screen[0][0].ch, 'R');
        assert_eq!(g.screen[0][1].ch, 'o');
        // Rows 1-4 should be erased (blank)
        assert_eq!(g.screen[1][0].ch, ' ', "row 1 should be erased");
        assert_eq!(g.screen[4][0].ch, ' ', "row 4 should be erased");
    }

    #[test]
    fn erase_in_display_all() {
        let mut g = Grid::new(10, 5);
        g.feed(b"Line1\nLine2\nLine3");
        g.feed(b"\x1b[2J"); // ED 2: erase entire display
        for row in 0..5 {
            for col in 0..10 {
                assert_eq!(
                    g.screen[row][col].ch, ' ',
                    "cell {row},{col} should be blank"
                );
            }
        }
        assert_eq!(g.cursor_row, 0);
        assert_eq!(g.cursor_col, 0);
    }

    // ── Alternate screen ──────────────────────────────────────────────────

    #[test]
    fn alternate_screen_switch() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[?1049h"); // enter alt screen
        assert!(g.alt_active);
        g.feed(b"\x1b[?1049l"); // exit alt screen
        assert!(!g.alt_active);
    }

    // ── DECSET 25 (cursor visibility) ────────────────────────────────────

    #[test]
    fn decset_25_hide_cursor() {
        let mut g = grid_24x80();
        assert!(g.cursor_visible, "cursor should start visible");
        g.feed(b"\x1b[?25l"); // DECRST 25 — hide cursor
        assert!(!g.cursor_visible, "cursor should be hidden after ?25l");
    }

    #[test]
    fn decset_25_show_cursor() {
        let mut g = grid_24x80();
        g.cursor_visible = false;
        g.feed(b"\x1b[?25h"); // DECSET 25 — show cursor
        assert!(g.cursor_visible, "cursor should be visible after ?25h");
    }

    #[test]
    fn decset_25_toggle_round_trip() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[?25l");
        assert!(!g.cursor_visible);
        g.feed(b"\x1b[?25h");
        assert!(g.cursor_visible);
    }

    #[test]
    fn decset_25_reflected_in_delta() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[?25l");
        let delta = g.take_delta();
        assert!(!delta.cursor_visible);
    }

    // ── DECSET 2004 (bracketed paste mode) ───────────────────────────────

    #[test]
    fn decset_2004_enable() {
        let mut g = grid_24x80();
        assert!(!g.bracketed_paste, "bracketed paste should start disabled");
        g.feed(b"\x1b[?2004h"); // DECSET 2004 — enable bracketed paste
        assert!(
            g.bracketed_paste,
            "bracketed paste should be enabled after ?2004h"
        );
    }

    #[test]
    fn decset_2004_disable() {
        let mut g = grid_24x80();
        g.bracketed_paste = true;
        g.feed(b"\x1b[?2004l"); // DECRST 2004 — disable bracketed paste
        assert!(
            !g.bracketed_paste,
            "bracketed paste should be disabled after ?2004l"
        );
    }

    #[test]
    fn decset_2004_toggle_round_trip() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[?2004h");
        assert!(g.bracketed_paste);
        g.feed(b"\x1b[?2004l");
        assert!(!g.bracketed_paste);
    }

    #[test]
    fn decset_2004_reflected_in_delta() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[?2004h");
        let delta = g.take_delta();
        assert!(delta.bracketed_paste);
    }

    #[test]
    fn alternate_screen_independent_buffer() {
        let mut g = grid_24x80();
        g.feed(b"Normal");
        // Both screens start blank; after "Normal", primary has content
        assert_eq!(g.screen[0][0].ch, 'N');
        // Enter alt screen via direct flag (CSI-based switch tested separately)
        g.alt_active = true;
        // Write to alt screen (should not affect primary)
        g.feed(b"AltText");
        // After switching back, primary should still have "Normal"
        g.alt_active = false;
        assert_eq!(g.screen[0][0].ch, 'N');
    }

    // ── take_delta ────────────────────────────────────────────────────────

    #[test]
    fn take_delta_initial_all_dirty() {
        let mut g = grid_24x80();
        let delta = g.take_delta();
        assert_eq!(delta.rows.len() as u16, g.rows);
        assert_eq!(delta.scroll_top, 0);
    }

    #[test]
    fn take_delta_clears_dirty() {
        let mut g = grid_24x80();
        let _ = g.take_delta();
        let delta = g.take_delta();
        assert!(delta.rows.is_empty());
    }

    #[test]
    fn take_delta_after_write() {
        let mut g = grid_24x80();
        let _ = g.take_delta(); // clear initial dirty
        g.feed(b"X");
        let delta = g.take_delta();
        assert_eq!(delta.rows.len(), 1);
        assert_eq!(delta.rows[0].row, 0);
        assert_eq!(delta.rows[0].cells[0].ch, 'X');
    }

    #[test]
    fn take_delta_cursor_position() {
        let mut g = grid_24x80();
        g.feed(b"Hello");
        let delta = g.take_delta();
        assert_eq!(delta.cursor_col, 5);
        assert_eq!(delta.cursor_row, 0);
    }

    // ── Resize ────────────────────────────────────────────────────────────

    #[test]
    fn resize_smaller() {
        let mut g = Grid::new(20, 10);
        g.resize(10, 5);
        assert_eq!(g.cols, 10);
        assert_eq!(g.rows, 5);
        assert_eq!(g.screen.len(), 5);
        assert_eq!(g.screen[0].len(), 10);
    }

    #[test]
    fn resize_larger() {
        let mut g = Grid::new(10, 5);
        g.resize(20, 10);
        assert_eq!(g.cols, 20);
        assert_eq!(g.rows, 10);
        assert_eq!(g.screen.len(), 10);
        assert_eq!(g.screen[0].len(), 20);
    }

    #[test]
    fn resize_clamps_cursor() {
        let mut g = Grid::new(80, 24);
        g.cursor_col = 70;
        g.cursor_row = 20;
        g.resize(40, 10);
        assert!(g.cursor_col < g.cols);
        assert!(g.cursor_row < g.rows);
        assert_eq!(g.cursor_col, 39);
        assert_eq!(g.cursor_row, 9);
    }

    // ── OSC ───────────────────────────────────────────────────────────────

    #[test]
    fn osc_set_title() {
        let mut g = grid_24x80();
        g.feed(b"\x1b]0;My Title\x07");
        assert_eq!(g.title.as_deref(), Some("My Title"));
    }

    #[test]
    fn osc_set_title_osc2() {
        let mut g = grid_24x80();
        g.feed(b"\x1b]2;Another Title\x07");
        assert_eq!(g.title.as_deref(), Some("Another Title"));
    }

    #[test]
    fn osc_set_cwd() {
        let mut g = grid_24x80();
        g.feed(b"\x1b]7;file:///home/user/project\x07");
        assert_eq!(g.cwd.as_deref(), Some("file:///home/user/project"));
    }

    // ── Unicode width ─────────────────────────────────────────────────────

    #[test]
    fn unicode_width_ascii() {
        assert_eq!(unicode_width('a'), 1);
        assert_eq!(unicode_width(' '), 1);
        assert_eq!(unicode_width('1'), 1);
    }

    #[test]
    fn unicode_width_cjk() {
        assert_eq!(unicode_width('\u{4e2d}'), 2); // 中
        assert_eq!(unicode_width('\u{ff21}'), 2); // Ａ fullwidth
    }

    #[test]
    fn unicode_width_emoji() {
        assert_eq!(unicode_width('\u{1f600}'), 2); // 😀
    }

    // ── Color helpers ─────────────────────────────────────────────────────

    #[test]
    fn named_fg_mapping() {
        assert_eq!(named_fg(0), NamedColor::Black);
        assert_eq!(named_fg(1), NamedColor::Red);
        assert_eq!(named_fg(7), NamedColor::White);
    }

    #[test]
    fn named_bright_fg_mapping() {
        assert_eq!(named_bright_fg(0), NamedColor::BrightBlack);
        assert_eq!(named_bright_fg(7), NamedColor::BrightWhite);
    }

    #[test]
    fn named_to_index_mapping() {
        assert_eq!(named_to_index(NamedColor::Black), 0);
        assert_eq!(named_to_index(NamedColor::BrightWhite), 15);
    }

    // ── Cell ──────────────────────────────────────────────────────────────

    #[test]
    fn cell_default_is_blank() {
        let c = Cell::default();
        assert_eq!(c, Cell::blank());
    }

    #[test]
    fn cell_new_fields() {
        let c = Cell {
            ch: 'A',
            width: 1,
            fg: Color::Named(NamedColor::Red),
            bg: Color::Default,
            attrs: Attrs::BOLD,
            hyperlink: None,
        };
        assert_eq!(c.ch, 'A');
        assert_eq!(c.fg, Color::Named(NamedColor::Red));
    }

    // ── DECSTBM (scroll regions) ────────────────────────────────────────

    #[test]
    fn decstbm_set_region() {
        let mut g = grid_24x80();
        // Default: full screen
        assert_eq!(g.scroll_top, 0);
        assert_eq!(g.scroll_bottom, 23);
        // Set region rows 5-20 (1-based → 4-19 0-based)
        g.feed(b"\x1b[5;20r");
        assert_eq!(g.scroll_top, 4);
        assert_eq!(g.scroll_bottom, 19);
    }

    #[test]
    fn decstbm_reset_to_full() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[5;20r");
        assert_eq!(g.scroll_top, 4);
        assert_eq!(g.scroll_bottom, 19);
        // CSI r with no params resets to full screen
        g.feed(b"\x1b[r");
        assert_eq!(g.scroll_top, 0);
        assert_eq!(g.scroll_bottom, 23);
    }

    #[test]
    fn decstbm_moves_cursor_home() {
        let mut g = grid_24x80();
        g.cursor_row = 10;
        g.cursor_col = 30;
        g.feed(b"\x1b[5;20r");
        assert_eq!(g.cursor_row, 0);
        assert_eq!(g.cursor_col, 0);
    }

    #[test]
    fn decstbm_scroll_within_region() {
        let mut g = Grid::new(10, 5);
        // Fill all rows with distinct letters
        for row in 0..5 {
            for col in 0..10 {
                g.screen[row][col] = Cell {
                    ch: (b'A' + row as u8) as char,
                    width: 1,
                    fg: Color::Default,
                    bg: Color::Default,
                    attrs: Attrs::empty(),
                    hyperlink: None,
                };
            }
        }
        // Set scroll region to rows 1-3 (0-based)
        g.scroll_top = 1;
        g.scroll_bottom = 3;
        // Position cursor inside the region and trigger a scroll
        g.cursor_row = 3;
        g.cursor_col = 0;
        g.feed(b"\n"); // LF at bottom of region should scroll region up
        // Row 1 (was 'B') should be gone, replaced by row 2 ('C')
        assert_eq!(g.screen[1][0].ch, 'C', "row 1 should now be 'C' (was 'B')");
        // Row 2 (was 'C') should now be 'D'
        assert_eq!(g.screen[2][0].ch, 'D', "row 2 should now be 'D' (was 'C')");
        // Row 3 should be blank (new line)
        assert_eq!(g.screen[3][0].ch, ' ', "row 3 should be blank");
        // Row 0 ('A') and row 4 ('E') should be untouched
        assert_eq!(g.screen[0][0].ch, 'A', "row 0 should be untouched");
        assert_eq!(g.screen[4][0].ch, 'E', "row 4 should be untouched");
    }

    // ── Insert/Delete Line ───────────────────────────────────────────────

    #[test]
    fn insert_line_at_cursor() {
        let mut g = Grid::new(10, 5);
        // Fill rows with distinct letters
        for row in 0..5 {
            for col in 0..10 {
                g.screen[row][col] = Cell {
                    ch: (b'A' + row as u8) as char,
                    width: 1,
                    fg: Color::Default,
                    bg: Color::Default,
                    attrs: Attrs::empty(),
                    hyperlink: None,
                };
            }
        }
        g.cursor_row = 1;
        g.cursor_col = 0;
        g.feed(b"\x1b[L"); // Insert 1 line at row 1
        // Row 1 should now be blank
        assert_eq!(g.screen[1][0].ch, ' ', "row 1 should be blank (inserted)");
        // Row 2 should now be 'B' (was row 1)
        assert_eq!(g.screen[2][0].ch, 'B', "row 2 should be 'B' (shifted down)");
        // Row 3 should now be 'C'
        assert_eq!(g.screen[3][0].ch, 'C', "row 3 should be 'C' (shifted down)");
        // Row 4 should now be 'D' (was row 3, 'E' scrolled off bottom)
        assert_eq!(g.screen[4][0].ch, 'D', "row 4 should be 'D' (shifted down)");
        // Row 0 should be untouched
        assert_eq!(g.screen[0][0].ch, 'A', "row 0 should be untouched");
    }

    #[test]
    fn delete_line_at_cursor() {
        let mut g = Grid::new(10, 5);
        for row in 0..5 {
            for col in 0..10 {
                g.screen[row][col] = Cell {
                    ch: (b'A' + row as u8) as char,
                    width: 1,
                    fg: Color::Default,
                    bg: Color::Default,
                    attrs: Attrs::empty(),
                    hyperlink: None,
                };
            }
        }
        g.cursor_row = 1;
        g.cursor_col = 0;
        g.feed(b"\x1b[M"); // Delete 1 line at row 1
        // Row 1 should now be 'C' (was row 2)
        assert_eq!(g.screen[1][0].ch, 'C', "row 1 should be 'C' (shifted up)");
        // Row 2 should now be 'D'
        assert_eq!(g.screen[2][0].ch, 'D', "row 2 should be 'D' (shifted up)");
        // Row 3 should now be 'E'
        assert_eq!(g.screen[3][0].ch, 'E', "row 3 should be 'E' (shifted up)");
        // Row 4 should be blank (new line at bottom)
        assert_eq!(g.screen[4][0].ch, ' ', "row 4 should be blank");
        // Row 0 should be untouched
        assert_eq!(g.screen[0][0].ch, 'A', "row 0 should be untouched");
    }

    // ── Insert/Delete Character ──────────────────────────────────────────

    #[test]
    fn insert_char_at_cursor() {
        let mut g = Grid::new(10, 3);
        g.feed(b"ABCDEFGHIJ"); // fills row 0 with A-J
        g.cursor_col = 3; // cursor at 'D'
        g.cursor_row = 0;
        g.feed(b"\x1b[@"); // Insert 1 blank char at cursor
        assert_eq!(g.screen[0][3].ch, ' ', "inserted position should be blank");
        assert_eq!(g.screen[0][4].ch, 'D', "'D' should shift right to col 4");
        assert_eq!(g.screen[0][5].ch, 'E', "'E' should shift right to col 5");
        // 'J' at col 9 should be pushed off the edge
        assert_eq!(g.screen[0][9].ch, 'I', "col 9 should be 'I' (J pushed off)");
    }

    #[test]
    fn delete_char_at_cursor() {
        let mut g = Grid::new(10, 3);
        g.feed(b"ABCDEFGHIJ"); // fills row 0 with A-J
        g.cursor_col = 3; // cursor at 'D'
        g.cursor_row = 0;
        g.feed(b"\x1b[P"); // Delete 1 char at cursor
        assert_eq!(g.screen[0][3].ch, 'E', "'E' should shift left to col 3");
        assert_eq!(g.screen[0][4].ch, 'F', "'F' should shift left to col 4");
        // Right edge should be blank
        assert_eq!(g.screen[0][9].ch, ' ', "col 9 should be blank (shifted in)");
    }

    // ── ESC M (Reverse Index / RI) ──────────────────────────────────────

    #[test]
    fn esc_m_reverse_index_moves_up() {
        let mut g = grid_24x80();
        g.cursor_row = 5;
        g.feed(b"\x1bM");
        assert_eq!(g.cursor_row, 4, "ESC M should move cursor up one line");
    }

    #[test]
    fn esc_m_at_top_of_scroll_region_scrolls_down() {
        let mut g = grid_24x80();
        // Write 'X' on row 0
        g.feed(b"X");
        assert_eq!(g.screen[0][0].ch, 'X');
        // ESC M at row 0 (top of default scroll region) should scroll down
        g.cursor_col = 0;
        g.feed(b"\x1bM");
        // Row 0 should now be blank (scrolled down), X moved to row 1
        assert_eq!(
            g.screen[0][0].ch, ' ',
            "row 0 should be blank after RI scroll"
        );
        assert_eq!(g.screen[1][0].ch, 'X', "X should have moved to row 1");
    }

    #[test]
    fn esc_m_within_scroll_region() {
        let mut g = grid_24x80();
        // Set scroll region to rows 5-10 (1-based → 0-based: 4-10)
        g.feed(b"\x1b[5;11r");
        // DECSTBM moves cursor to home (0,0); move cursor to row 4 (top of scroll region)
        g.cursor_row = 4;
        g.cursor_col = 0;
        // Write something on row 4
        g.screen[4][0].ch = 'A';
        // Verify scroll region is set correctly
        assert_eq!(
            g.scroll_top, 4,
            "scroll_top should be 4 (0-based for row 5)"
        );
        assert_eq!(
            g.scroll_bottom, 10,
            "scroll_bottom should be 10 (0-based for row 11)"
        );
        // ESC M at top of scroll region should scroll region down
        g.feed(b"\x1bM");
        assert_eq!(
            g.screen[4][0].ch, ' ',
            "row 4 should be blank after RI in scroll region"
        );
        assert_eq!(g.screen[5][0].ch, 'A', "A should have moved to row 5");
    }

    // ── Mouse encoding ─────────────────────────────────────────────────────
    #[test]
    fn encode_mouse_press_x10() {
        let result = encode_mouse_press(true, false, false, false, MouseButton::Left, 5, 3);
        assert!(result.is_some());
        let bytes = result.unwrap();
        // X10: ESC [ M b x y  with b=32+0=32 (space), x=32+5+1=38 '&', y=32+3+1=36 '$'
        assert_eq!(bytes, b"\x1b[M &$");
    }

    #[test]
    fn encode_mouse_press_sgr() {
        let result = encode_mouse_press(true, false, false, true, MouseButton::Left, 5, 3);
        assert!(result.is_some());
        let bytes = result.unwrap();
        // SGR: ESC [ < 0 ; 6 ; 4 M
        assert_eq!(bytes, b"\x1b[<0;6;4M");
    }

    #[test]
    fn encode_mouse_press_inactive() {
        let result = encode_mouse_press(false, false, false, false, MouseButton::Left, 5, 3);
        assert!(result.is_none());
    }

    #[test]
    fn encode_mouse_release_x10() {
        let result = encode_mouse_release(true, false, false, MouseButton::Left, 5, 3);
        assert!(result.is_some());
        let bytes = result.unwrap();
        // X10 release: ESC [ M  b x y  where b=32+3=35 '#', x=38, y=36
        assert_eq!(bytes, b"\x1b[M#&$");
    }

    #[test]
    fn encode_mouse_release_sgr() {
        let result = encode_mouse_release(true, false, true, MouseButton::Left, 5, 3);
        assert!(result.is_some());
        let bytes = result.unwrap();
        // SGR release: ESC [ < 3 ; 6 ; 4 m
        assert_eq!(bytes, b"\x1b[<3;6;4m");
    }

    #[test]
    fn encode_mouse_release_inactive() {
        let result = encode_mouse_release(false, false, false, MouseButton::Left, 5, 3);
        assert!(result.is_none());
    }

    #[test]
    fn encode_mouse_motion_x10() {
        let result = encode_mouse_motion(true, false, false, MouseButton::Left, 5, 3);
        assert!(result.is_some());
        let bytes = result.unwrap();
        // X10 motion: button code (0) + 32 = 32, so b=32+32=64 '@'
        assert_eq!(bytes, b"\x1b[M@&$");
    }

    #[test]
    fn encode_mouse_motion_inactive() {
        let result = encode_mouse_motion(false, false, false, MouseButton::Left, 5, 3);
        assert!(result.is_none());
    }

    #[test]
    fn encode_mouse_scroll_x10_up() {
        let result = encode_mouse_scroll(true, false, false, false, ScrollDirection::Up, 5, 3);
        assert!(result.is_some());
        let bytes = result.unwrap();
        // ScrollUp button code = 4 + 64 = 68, b = 32 + 68 = 100 'd'
        assert_eq!(bytes, b"\x1b[Md&$");
    }

    #[test]
    fn encode_mouse_scroll_x10_down() {
        let result = encode_mouse_scroll(true, false, false, false, ScrollDirection::Down, 5, 3);
        assert!(result.is_some());
        let bytes = result.unwrap();
        // ScrollDown button code = 5 + 64 = 69, b = 32 + 69 = 101 'e'
        assert_eq!(bytes, b"\x1b[Me&$");
    }

    #[test]
    fn encode_mouse_scroll_inactive() {
        let result = encode_mouse_scroll(false, false, false, false, ScrollDirection::Up, 5, 3);
        assert!(result.is_none());
    }

    #[test]
    fn encode_mouse_right_button_x10() {
        let result = encode_mouse_press(true, false, false, false, MouseButton::Right, 10, 5);
        assert!(result.is_some());
        let bytes = result.unwrap();
        // Right button code = 2, b = 32+2=34 '"', x = 32+10+1=43 '+', y = 32+5+1=38 '&'
        assert_eq!(bytes, b"\x1b[M\"+&");
    }

    #[test]
    fn encode_mouse_right_button_sgr() {
        let result = encode_mouse_press(true, false, false, true, MouseButton::Right, 10, 5);
        assert!(result.is_some());
        let bytes = result.unwrap();
        // SGR: ESC [ < 2 ; 11 ; 6 M
        assert_eq!(bytes, b"\x1b[<2;11;6M");
    }

    #[test]
    fn encode_mouse_scroll_sgr() {
        let result = encode_mouse_scroll(true, true, false, true, ScrollDirection::Down, 5, 3);
        assert!(result.is_some());
        let bytes = result.unwrap();
        // ScrollDown = 5+64=69, SGR: ESC [ < 69 ; 6 ; 4 m (scroll is always release)
        assert_eq!(bytes, b"\x1b[<69;6;4m");
    }

    // ── Content reflow on resize ─────────────────────────────────────────

    #[test]
    fn resize_wider_unwraps_lines() {
        let mut g = Grid::new(5, 10);
        g.feed(b"ABCDEFGHIJ");
        // After feeding 10 chars into 5-wide grid:
        // Row 0: ABCDE, wrapped=true at row 1
        // Row 1: FGHIJ, wrapped=true at row 2
        // Row 2: (blank)
        assert!(
            g.wrapped[1],
            "row 1 should be marked as wrapped continuation"
        );
        assert!(
            g.wrapped[2],
            "row 2 should be marked as wrapped continuation"
        );
        // Widen to 10 cols
        g.resize(10, 10);
        // "ABCDEFGHIJ" should now fit on a single row
        assert_eq!(g.screen[0][0].ch, 'A');
        assert_eq!(g.screen[0][9].ch, 'J');
        assert!(!g.wrapped[0], "row 0 should not be wrapped");
    }

    #[test]
    fn resize_narrower_rewraps_content() {
        let mut g = Grid::new(10, 10);
        g.feed(b"ABCDEFGHIJ"); // fits on one row at 10 cols
        assert_eq!(g.screen[0][9].ch, 'J');
        // Narrow to 5 cols
        g.resize(5, 10);
        assert_eq!(g.screen[0][0].ch, 'A');
        assert_eq!(g.screen[0][4].ch, 'E');
        assert_eq!(g.screen[1][4].ch, 'J');
        assert!(g.wrapped[1], "row 1 should be wrapped continuation");
    }

    #[test]
    fn resize_preserves_non_wrapped_content() {
        let mut g = Grid::new(10, 10);
        g.feed(b"Hello\r\nWorld");
        // "\r\n" gives a hard newline (CR resets col, LF moves down)
        // Verify pre-resize state
        assert!(!g.wrapped[0]);
        assert!(!g.wrapped[1]);
        assert_eq!(g.screen[0][0].ch, 'H', "pre: row 0 should start with H");
        assert_eq!(g.screen[1][0].ch, 'W', "pre: row 1 should start with W");
        // Widen
        g.resize(20, 10);
        // Both lines should still be on their own rows
        assert_eq!(g.screen[0][0].ch, 'H', "row 0 should start with H");
        assert_eq!(
            g.screen[1][0].ch, 'W',
            "row 1 should start with W after resize"
        );
    }

    #[test]
    fn resize_too_many_rows_truncates() {
        let mut g = Grid::new(5, 20);
        g.feed(b"ABCDEFGHIJ"); // wraps across rows 0, 1, 2
        g.resize(5, 2);
        assert_eq!(g.screen.len(), 2);
        assert_eq!(g.wrapped.len(), 2);
        assert_eq!(g.screen[0][0].ch, 'A');
        assert_eq!(g.screen[0][4].ch, 'E');
    }

    #[test]
    fn resize_alt_screen_no_reflow() {
        let mut g = Grid::new(10, 10);
        g.feed(b"Hello");
        g.alt_active = true;
        g.feed(b"AltContent");
        g.resize(20, 10);
        // Primary screen content should not have been reflowed
        g.alt_active = false;
        assert_eq!(g.screen[0][0].ch, 'H');
    }

    // ── OSC 8 Hyperlinks ─────────────────────────────────────────────────
    #[test]
    fn osc_8_hyperlink_open_and_close() {
        let mut g = grid_24x80();
        // Open hyperlink to https://example.com
        g.feed(b"\x1b]8;;https://example.com\x07");
        g.feed(b"Click me");
        // Close hyperlink
        g.feed(b"\x1b]8;;\x07");
        // Cells should have the hyperlink URL
        for col in 0..8 {
            assert_eq!(
                g.screen[0][col].hyperlink.as_deref(),
                Some("https://example.com"),
                "cell at col {col} should have hyperlink"
            );
        }
        // After close, new cells should not have hyperlink
        g.feed(b"X");
        assert!(g.screen[0][8].hyperlink.is_none());
    }

    #[test]
    fn osc_8_hyperlink_no_url_closes() {
        let mut g = grid_24x80();
        g.feed(b"\x1b]8;;https://example.com\x07");
        g.feed(b"A");
        assert!(g.screen[0][0].hyperlink.is_some());
        // Close with empty params
        g.feed(b"\x1b]8;;\x07");
        g.feed(b"B");
        assert!(g.screen[0][1].hyperlink.is_none());
    }

    #[test]
    fn osc_8_hyperlink_no_text_between() {
        let mut g = grid_24x80();
        // Open and immediately close
        g.feed(b"\x1b]8;;https://example.com\x07\x1b]8;;\x07");
        g.feed(b"NoLink");
        for col in 0..6 {
            assert!(
                g.screen[0][col].hyperlink.is_none(),
                "cell at col {col} should not have hyperlink"
            );
        }
    }

    #[test]
    fn osc_8_hyperlink_in_delta() {
        let mut g = grid_24x80();
        let _ = g.take_delta(); // Clear initial dirty
        g.feed(b"\x1b]8;;https://example.com\x07H\x1b]8;;\x07");
        let delta = g.take_delta();
        assert_eq!(delta.rows.len(), 1);
        assert_eq!(
            delta.rows[0].cells[0].hyperlink.as_deref(),
            Some("https://example.com")
        );
    }

    // ── OSC 133 Shell Integration ─────────────────────────────────────────
    #[test]
    fn osc_133_command_started() {
        let mut g = grid_24x80();
        assert!(g.command_started.is_none());
        g.feed(b"\x1b]133;C\x07");
        assert_eq!(g.command_started.as_deref(), Some(""));
    }

    #[test]
    fn osc_133_command_started_with_text() {
        let mut g = grid_24x80();
        g.feed(b"\x1b]133;C;ls -la\x07");
        assert_eq!(g.command_started.as_deref(), Some("ls -la"));
    }

    #[test]
    fn osc_133_command_finished_no_code() {
        let mut g = grid_24x80();
        assert!(g.command_finished.is_none());
        g.feed(b"\x1b]133;D\x07");
        assert_eq!(g.command_finished, Some(0));
    }

    #[test]
    fn osc_133_command_finished_with_code() {
        let mut g = grid_24x80();
        g.feed(b"\x1b]133;D;42\x07");
        assert_eq!(g.command_finished, Some(42));
    }

    #[test]
    fn osc_133_command_finished_with_negative_code() {
        let mut g = grid_24x80();
        g.feed(b"\x1b]133;D;-1\x07");
        assert_eq!(g.command_finished, Some(-1));
    }

    #[test]
    fn osc_133_prompt_markers_dont_set_command() {
        let mut g = grid_24x80();
        g.feed(b"\x1b]133;A\x07"); // Prompt start
        g.feed(b"\x1b]133;B\x07"); // Prompt end
        assert!(g.command_started.is_none());
        assert!(g.command_finished.is_none());
    }

    #[test]
    fn osc_133_command_finished_is_one_shot() {
        let mut g = grid_24x80();
        g.feed(b"\x1b]133;D;0\x07");
        assert_eq!(g.command_finished, Some(0));
        // After reading the value, the app should clear it
        g.command_finished = None;
        assert!(g.command_finished.is_none());
    }

    // ── SGR attribute gaps ─────────────────────────────────────────────────

    #[test]
    fn sgr_dim() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[2mX");
        assert!(g.screen[0][0].attrs.contains(Attrs::DIM));
    }

    #[test]
    fn sgr_strikethrough() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[9mX");
        assert!(g.screen[0][0].attrs.contains(Attrs::STRIKEOUT));
    }

    #[test]
    fn sgr_no_bold_dim() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[1;2mX"); // bold + dim
        assert!(g.screen[0][0].attrs.contains(Attrs::BOLD));
        assert!(g.screen[0][0].attrs.contains(Attrs::DIM));
        g.feed(b"\x1b[22mY"); // SGR 22 removes both bold and dim
        assert!(!g.screen[0][1].attrs.contains(Attrs::BOLD));
        assert!(!g.screen[0][1].attrs.contains(Attrs::DIM));
    }

    #[test]
    fn sgr_no_italic() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[3mX");
        assert!(g.screen[0][0].attrs.contains(Attrs::ITALIC));
        g.feed(b"\x1b[23mY");
        assert!(!g.screen[0][1].attrs.contains(Attrs::ITALIC));
    }

    #[test]
    fn sgr_no_underline() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[4mX");
        assert!(g.screen[0][0].attrs.contains(Attrs::UNDERLINE));
        g.feed(b"\x1b[24mY");
        assert!(!g.screen[0][1].attrs.contains(Attrs::UNDERLINE));
    }

    #[test]
    fn sgr_no_blink() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[5mX");
        assert!(g.screen[0][0].attrs.contains(Attrs::BLINK));
        g.feed(b"\x1b[25mY");
        assert!(!g.screen[0][1].attrs.contains(Attrs::BLINK));
    }

    #[test]
    fn sgr_no_reverse() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[7mX");
        assert!(g.screen[0][0].attrs.contains(Attrs::REVERSE));
        g.feed(b"\x1b[27mY");
        assert!(!g.screen[0][1].attrs.contains(Attrs::REVERSE));
    }

    #[test]
    fn sgr_no_strikethrough() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[9mX");
        assert!(g.screen[0][0].attrs.contains(Attrs::STRIKEOUT));
        g.feed(b"\x1b[29mY");
        assert!(!g.screen[0][1].attrs.contains(Attrs::STRIKEOUT));
    }

    #[test]
    fn sgr_all_attributes() {
        // Set all attributes at once
        let mut g = grid_24x80();
        g.feed(b"\x1b[1;2;3;4;5;7;9mX");
        let cell = g.screen[0][0].clone();
        assert!(cell.attrs.contains(Attrs::BOLD));
        assert!(cell.attrs.contains(Attrs::DIM));
        assert!(cell.attrs.contains(Attrs::ITALIC));
        assert!(cell.attrs.contains(Attrs::UNDERLINE));
        assert!(cell.attrs.contains(Attrs::BLINK));
        assert!(cell.attrs.contains(Attrs::REVERSE));
        assert!(cell.attrs.contains(Attrs::STRIKEOUT));
    }

    #[test]
    fn sgr_reset_clears_all_attributes() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[1;4;7;9mX"); // bold + underline + reverse + strikeout
        assert!(g.screen[0][0].attrs.contains(Attrs::BOLD));
        g.feed(b"\x1b[0mY"); // SGR 0 reset
        assert!(g.screen[0][1].attrs.is_empty());
    }

    #[test]
    fn sgr_unknown_parameters_ignored() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[65;73;81mX"); // non-standard SGR codes
        // Should not crash, cell should be default
        let cell = g.screen[0][0].clone();
        assert_eq!(cell.attrs, Attrs::empty());
        assert_eq!(cell.fg, Color::Default);
        assert_eq!(cell.bg, Color::Default);
    }

    // ── ED 1 (Erase to Start of Display) ──────────────────────────────────

    #[test]
    fn erase_in_display_to_start() {
        let mut g = Grid::new(5, 5);
        // Fill rows with distinct content
        for row in 0..5 {
            for col in 0..5 {
                g.screen[row][col] = Cell {
                    ch: (b'A' + row as u8) as char,
                    width: 1,
                    ..Cell::blank()
                };
            }
        }
        g.cursor_row = 3;
        g.cursor_col = 2;
        g.feed(b"\x1b[1J"); // ED 1: erase from start to cursor
        // Row 0 and 1 should be entirely blank
        for row in 0..2 {
            for col in 0..5 {
                assert_eq!(
                    g.screen[row][col].ch, ' ',
                    "row {row} col {col} should be blank"
                );
            }
        }
        // Row 3 should have cols 0..=2 blank, cols 3..4 untouched ('D')
        assert_eq!(g.screen[3][0].ch, ' ', "row 3 col 0 should be blank");
        assert_eq!(g.screen[3][1].ch, ' ', "row 3 col 1 should be blank");
        assert_eq!(g.screen[3][2].ch, ' ', "row 3 col 2 should be blank");
        assert_eq!(g.screen[3][3].ch, 'D', "row 3 col 3 should be 'D'");
        // Row 4 should be untouched
        assert_eq!(g.screen[4][0].ch, 'E', "row 4 should be untouched");
    }

    #[test]
    fn erase_in_display_scrollback_same_as_all() {
        // ED 3 is treated identically to ED 2 (erase entire screen)
        let mut g = Grid::new(5, 5);
        g.feed(b"ABCDE\nFGHIJ\nKLMNO");
        g.feed(b"\x1b[3J"); // ED 3
        for row in 0..5 {
            for col in 0..5 {
                assert_eq!(
                    g.screen[row][col].ch, ' ',
                    "cell {row},{col} should be blank after ED 3"
                );
            }
        }
        assert_eq!(g.cursor_row, 0);
        assert_eq!(g.cursor_col, 0);
    }

    // ── DECSET mouse modes via feed ────────────────────────────────────────

    #[test]
    fn decset_1000_mouse_x10_via_feed() {
        let mut g = grid_24x80();
        assert!(!g.mouse_x10);
        g.feed(b"\x1b[?1000h"); // DECSET 1000
        assert!(g.mouse_x10);
    }

    #[test]
    fn decset_1000_mouse_x10_disable_via_feed() {
        let mut g = grid_24x80();
        g.mouse_x10 = true;
        g.feed(b"\x1b[?1000l"); // DECRST 1000
        assert!(!g.mouse_x10);
    }

    #[test]
    fn decset_1002_mouse_normal_via_feed() {
        let mut g = grid_24x80();
        assert!(!g.mouse_normal);
        g.feed(b"\x1b[?1002h");
        assert!(g.mouse_normal);
    }

    #[test]
    fn decset_1002_mouse_normal_disable_via_feed() {
        let mut g = grid_24x80();
        g.mouse_normal = true;
        g.feed(b"\x1b[?1002l");
        assert!(!g.mouse_normal);
    }

    #[test]
    fn decset_1003_mouse_button_event_via_feed() {
        let mut g = grid_24x80();
        assert!(!g.mouse_button_event);
        g.feed(b"\x1b[?1003h");
        assert!(g.mouse_button_event);
    }

    #[test]
    fn decset_1003_mouse_button_event_disable_via_feed() {
        let mut g = grid_24x80();
        g.mouse_button_event = true;
        g.feed(b"\x1b[?1003l");
        assert!(!g.mouse_button_event);
    }

    #[test]
    fn decset_1006_mouse_sgr_via_feed() {
        let mut g = grid_24x80();
        assert!(!g.mouse_sgr);
        g.feed(b"\x1b[?1006h");
        assert!(g.mouse_sgr);
    }

    #[test]
    fn decset_1006_mouse_sgr_disable_via_feed() {
        let mut g = grid_24x80();
        g.mouse_sgr = true;
        g.feed(b"\x1b[?1006l");
        assert!(!g.mouse_sgr);
    }

    #[test]
    fn decset_mouse_reflected_in_delta() {
        let mut g = grid_24x80();
        let _ = g.take_delta(); // clear initial dirty
        g.feed(b"\x1b[?1000h\x1b[?1002h\x1b[?1006h");
        let delta = g.take_delta();
        assert!(delta.mouse_x10);
        assert!(delta.mouse_normal);
        assert!(delta.mouse_sgr);
        assert!(!delta.mouse_button_event);
    }

    // ── Scroll region edge cases ───────────────────────────────────────────

    #[test]
    fn decstbm_invalid_region_ignored() {
        let mut g = grid_24x80();
        // Default: full screen
        assert_eq!(g.scroll_top, 0);
        assert_eq!(g.scroll_bottom, 23);
        // Invalid region (top >= bottom) should be ignored
        g.feed(b"\x1b[10;5r");
        assert_eq!(
            g.scroll_top, 0,
            "invalid region should not change scroll_top"
        );
        assert_eq!(
            g.scroll_bottom, 23,
            "invalid region should not change scroll_bottom"
        );
    }

    #[test]
    fn decstbm_region_clamped_to_screen() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[1;99r"); // bottom > rows
        assert_eq!(g.scroll_top, 0);
        assert_eq!(g.scroll_bottom, 23); // clamped to rows-1
    }

    #[test]
    fn scroll_up_outside_region_noop() {
        let mut g = Grid::new(5, 5);
        // Set scroll region to rows 2-4 (1-based: 2;4) → 0-based: 1-3
        g.scroll_top = 1;
        g.scroll_bottom = 3;
        // Fill content
        for row in 0..5 {
            g.screen[row][0] = Cell {
                ch: (b'A' + row as u8) as char,
                width: 1,
                ..Cell::blank()
            };
        }
        // LF at row 0 (above region) — should move cursor to row 1 but NOT scroll
        g.cursor_row = 0;
        g.cursor_col = 0;
        g.feed(b"\n");
        assert_eq!(g.screen[0][0].ch, 'A', "row 0 should be untouched");
        assert_eq!(
            g.screen[1][0].ch, 'B',
            "row 1 should be untouched since cursor moved into region without scrolling"
        );
    }

    // ── Insert/Delete line outside scroll region ──────────────────────────

    #[test]
    fn insert_line_outside_scroll_region_noop() {
        let mut g = Grid::new(5, 5);
        for row in 0..5 {
            for col in 0..5 {
                g.screen[row][col] = Cell {
                    ch: (b'A' + row as u8) as char,
                    width: 1,
                    ..Cell::blank()
                };
            }
        }
        g.feed(b"\x1b[2;4r"); // scroll region rows 2-4 (0-based: 1-3)
        g.cursor_row = 0; // outside region (above)
        g.cursor_col = 0;
        g.feed(b"\x1b[L"); // IL 1 — should be no-op outside scroll region
        assert_eq!(g.screen[0][0].ch, 'A', "row 0 should be untouched");
        assert_eq!(g.screen[1][0].ch, 'B', "row 1 should be untouched");
    }

    #[test]
    fn delete_line_outside_scroll_region_noop() {
        let mut g = Grid::new(5, 5);
        for row in 0..5 {
            for col in 0..5 {
                g.screen[row][col] = Cell {
                    ch: (b'A' + row as u8) as char,
                    width: 1,
                    ..Cell::blank()
                };
            }
        }
        g.feed(b"\x1b[2;4r"); // scroll region rows 2-4 (0-based: 1-3)
        g.cursor_row = 4; // outside region (below)
        g.cursor_col = 0;
        g.feed(b"\x1b[M"); // DL 1 — should be no-op outside scroll region
        assert_eq!(g.screen[3][0].ch, 'D', "row 3 should be untouched");
        assert_eq!(g.screen[4][0].ch, 'E', "row 4 should be untouched");
    }

    // ── Cursor edge cases ──────────────────────────────────────────────────

    #[test]
    fn csi_cursor_position_zero_clamped() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[0;0H"); // CUP with 0 — should clamp to row=0, col=0
        assert_eq!(g.cursor_row, 0);
        assert_eq!(g.cursor_col, 0);
    }

    #[test]
    fn csi_cursor_position_beyond_screen_clamped() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[999;999H"); // CUP far beyond bounds
        assert_eq!(g.cursor_row, 23); // clamped to rows-1
        assert_eq!(g.cursor_col, 79); // clamped to cols-1
    }

    #[test]
    fn csi_cursor_forward_with_param() {
        let mut g = grid_24x80();
        g.cursor_col = 5;
        g.feed(b"\x1b[10C"); // CUF 10
        assert_eq!(g.cursor_col, 15);
    }

    #[test]
    fn csi_cursor_back_with_param() {
        let mut g = grid_24x80();
        g.cursor_col = 20;
        g.feed(b"\x1b[10D"); // CUB 10
        assert_eq!(g.cursor_col, 10);
    }

    #[test]
    fn csi_cursor_up_with_param() {
        let mut g = grid_24x80();
        g.cursor_row = 15;
        g.feed(b"\x1b[5A"); // CUU 5
        assert_eq!(g.cursor_row, 10);
    }

    #[test]
    fn csi_cursor_down_with_param() {
        let mut g = grid_24x80();
        g.cursor_row = 5;
        g.feed(b"\x1b[10B"); // CUD 10
        assert_eq!(g.cursor_row, 15);
    }

    // ── Tab operations ─────────────────────────────────────────────────────

    #[test]
    fn tab_stops_at_multiple_of_8() {
        let mut g = grid_24x80();
        g.feed(b"\t");
        assert_eq!(g.cursor_col, 8);
        g.feed(b"\t");
        assert_eq!(g.cursor_col, 16);
    }

    #[test]
    fn tab_at_edge_clamped() {
        let mut g = Grid::new(10, 3);
        g.cursor_col = 9; // last column
        g.feed(b"\t"); // should clamp to cols-1 = 9 (no wrap)
        assert_eq!(g.cursor_col, 9);
    }

    #[test]
    fn tab_after_multiple_tabs() {
        let mut g = Grid::new(30, 3);
        g.feed(b"\t\t\t"); // col 8, 16, 24
        assert_eq!(g.cursor_col, 24);
    }

    // ── Resize edge cases ──────────────────────────────────────────────────

    #[test]
    fn resize_noop_same_size() {
        let mut g = Grid::new(10, 5);
        g.feed(b"Hello");
        let content_before = g.screen[0][0].ch;
        g.resize(10, 5);
        assert_eq!(g.cols, 10);
        assert_eq!(g.rows, 5);
        assert_eq!(
            g.screen[0][0].ch, content_before,
            "content unchanged after noop resize"
        );
    }

    #[test]
    fn resize_zero_cols_no_panic() {
        let mut g = Grid::new(10, 5);
        g.resize(0, 5); // cols=0 shouldn't panic
        assert_eq!(g.cols, 0);
        assert_eq!(g.rows, 5);
    }

    // ── Feed edge cases ────────────────────────────────────────────────────

    #[test]
    fn feed_empty_bytes() {
        let mut g = grid_24x80();
        g.feed(b""); // no-op should not crash
        assert_eq!(g.cursor_col, 0);
        assert_eq!(g.cursor_row, 0);
    }

    #[test]
    fn feed_multiline_does_not_truncate() {
        let mut g = Grid::new(5, 10);
        // Feed 100 chars into a 5-wide grid = 20 lines
        let input: Vec<u8> = (0..100).map(|i| b'A' + (i % 26) as u8).collect();
        g.feed(&input);
        // Should have scrolled several lines
        assert!(!g.scrollback.is_empty());
    }

    // ── Reverse index edge cases ──────────────────────────────────────────

    #[test]
    fn esc_m_at_row_zero_with_scroll_region() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[5;10r"); // scroll region rows 5-10
        g.cursor_row = 4; // top of scroll region
        g.cursor_col = 0;
        g.feed(b"\x1bM"); // RI at top of scroll region should scroll region down
        assert_eq!(g.cursor_row, 4, "cursor should stay at row 4");
    }

    #[test]
    fn esc_m_at_top_of_screen_no_scroll() {
        let mut g = grid_24x80();
        g.feed(b"\x1bM"); // RI at row 0, top of default scroll region
        // Should scroll the default region down
        assert_eq!(g.cursor_row, 0);
    }

    // ── Character width edge cases ─────────────────────────────────────────

    #[test]
    fn unicode_width_combining() {
        // Combining characters have width 0
        assert_eq!(unicode_width('\u{0301}'), 0); // combining acute accent
        assert_eq!(unicode_width('\u{20DD}'), 0); // combining enclosing circle
    }

    #[test]
    fn unicode_width_control() {
        // Most control chars have width 0 or None
        assert_eq!(unicode_width('\0'), 0);
        assert_eq!(unicode_width('\x01'), 0);
    }

    // ── Put char edge cases ────────────────────────────────────────────────

    #[test]
    fn put_char_at_last_column_wraps() {
        let mut g = Grid::new(5, 3);
        g.cursor_col = 4; // last column
        g.cursor_row = 0;
        g.feed(b"XY"); // Put 'X' at col 4, then 'Y' should wrap to row 1 col 0
        assert_eq!(g.screen[0][4].ch, 'X', "X at col 4");
        assert_eq!(g.screen[1][0].ch, 'Y', "Y wrapped to row 1 col 0");
        assert!(g.wrapped[1], "row 1 should be marked wrapped");
    }

    #[test]
    fn put_char_out_of_bounds_no_panic() {
        let mut g = Grid::new(5, 3);
        g.cursor_col = 10; // out of bounds
        g.cursor_row = 5; // out of bounds
        g.feed(b"X"); // should not panic
    }

    // ── Delta consistency ──────────────────────────────────────────────────

    #[test]
    fn take_delta_twice_empty() {
        let mut g = grid_24x80();
        let _ = g.take_delta();
        // No changes
        let delta = g.take_delta();
        assert!(delta.rows.is_empty());
    }

    #[test]
    fn take_delta_after_resize() {
        let mut g = grid_24x80();
        let _ = g.take_delta();
        g.resize(40, 12);
        let delta = g.take_delta();
        // After resize, rows should be marked dirty
        assert_eq!(delta.rows.len(), 12);
    }

    #[test]
    fn take_delta_after_alt_screen_switch() {
        let mut g = grid_24x80();
        let _ = g.take_delta();
        g.feed(b"\x1b[?1049h"); // enter alt screen
        let delta = g.take_delta();
        assert!(
            !delta.rows.is_empty(),
            "alt screen switch should mark all rows dirty"
        );
    }

    // ── OSC 8 hyperlinks with params ───────────────────────────────────────

    #[test]
    fn osc_8_hyperlink_with_id_param() {
        let mut g = grid_24x80();
        // OSC 8 with id parameter
        g.feed(b"\x1b]8;id=myid;https://example.com\x07");
        g.feed(b"Link");
        for col in 0..4 {
            assert_eq!(
                g.screen[0][col].hyperlink.as_deref(),
                Some("https://example.com"),
                "cell at col {col} should have hyperlink"
            );
        }
    }

    #[test]
    fn osc_8_hyperlink_empty_params() {
        let mut g = grid_24x80();
        // OSC 8 open with empty params (just semicolons)
        g.feed(b"\x1b]8;;\x07"); // empty URL = close (no-op if no open)
        g.feed(b"NoLink");
        for col in 0..6 {
            assert!(g.screen[0][col].hyperlink.is_none());
        }
    }

    #[test]
    fn osc_8_hyperlink_overwrite_previous() {
        let mut g = grid_24x80();
        g.feed(b"\x1b]8;;http://first\x07");
        g.feed(b"AA");
        g.feed(b"\x1b]8;;http://second\x07");
        g.feed(b"BB");
        assert_eq!(g.screen[0][0].hyperlink.as_deref(), Some("http://first"));
        assert_eq!(g.screen[0][2].hyperlink.as_deref(), Some("http://second"));
    }

    // ── OSC 133 additional edge cases ──────────────────────────────────────

    #[test]
    fn osc_133_command_finished_with_large_exit_code() {
        let mut g = grid_24x80();
        g.feed(b"\x1b]133;D;255\x07");
        assert_eq!(g.command_finished, Some(255));
    }

    #[test]
    fn osc_133_unknown_marker_ignored() {
        let mut g = grid_24x80();
        g.feed(b"\x1b]133;Z\x07"); // unknown marker
        assert!(g.command_started.is_none());
        assert!(g.command_finished.is_none());
    }

    // ── Empty / unknown escapes ───────────────────────────────────────────

    #[test]
    fn unknown_csi_ignored() {
        let mut g = grid_24x80();
        g.feed(b"\x1b[?9999h"); // unknown DECSET mode
        // Should not crash or affect known modes
        assert!(!g.mouse_x10);
        assert!(!g.mouse_sgr);
    }

    #[test]
    fn unknown_esc_sequence_ignored() {
        let mut g = grid_24x80();
        g.feed(b"\x1b(0"); // unknown SCS sequence (ESC ( )
        // Should not crash
        assert_eq!(g.cursor_col, 0);
    }

    #[test]
    fn csi_without_params_uses_default() {
        let mut g = grid_24x80();
        g.cursor_row = 10;
        g.cursor_col = 20;
        g.feed(b"\x1b[A"); // CUU with default param = 1
        assert_eq!(g.cursor_row, 9);
        g.feed(b"\x1b[B"); // CUD default = 1
        assert_eq!(g.cursor_row, 10);
        g.feed(b"\x1b[C"); // CUF default = 1
        assert_eq!(g.cursor_col, 21);
        g.feed(b"\x1b[D"); // CUB default = 1
        assert_eq!(g.cursor_col, 20);
    }

    // ── Sequential operations ──────────────────────────────────────────────

    #[test]
    fn feed_then_resize_then_feed() {
        let mut g = Grid::new(10, 5);
        g.feed(b"Hello");
        g.resize(20, 10);
        g.feed(b" World");
        assert_eq!(g.screen[0][0].ch, 'H');
        assert_eq!(g.screen[0][5].ch, ' ');
        assert_eq!(g.screen[0][6].ch, 'W');
    }

    // ── Property-based / fuzz tests ─────────────────────────────────────────

    /// Simple Xorshift PRNG for deterministic random byte generation.
    #[allow(dead_code)]
    struct FuzzRng(u64);

    impl FuzzRng {
        #[allow(dead_code)]
        fn new(seed: u64) -> Self {
            Self(seed)
        }
        #[allow(dead_code)]
        fn next_u8(&mut self) -> u8 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0 as u8
        }
        #[allow(dead_code)]
        fn next_bytes(&mut self, buf: &mut [u8]) {
            for b in buf.iter_mut() {
                *b = self.next_u8();
            }
        }
    }

    #[test]
    fn fuzz_random_byte_sequences_no_crash() {
        // Feed random byte sequences at various grid sizes and ensure no panic.
        let sizes = [(80, 24), (40, 10), (10, 5), (200, 100), (1, 1)];
        for &(cols, rows) in &sizes {
            let mut rng = FuzzRng(42);
            for _seq in 0..200 {
                let mut buf = vec![0u8; 50 + (rng.next_u8() as usize % 200)];
                rng.next_bytes(&mut buf);
                let mut g = Grid::new(cols, rows);
                g.feed(&buf);
                // Invariants after feed:
                assert!(g.cursor_col < cols, "col={} >= cols={}", g.cursor_col, cols);
                assert!(g.cursor_row < rows, "row={} >= rows={}", g.cursor_row, rows);
                assert_eq!(g.screen.len(), rows as usize);
                assert_eq!(g.alt_screen.len(), rows as usize);
                assert_eq!(g.dirty.len(), rows as usize);
                for r in 0..rows as usize {
                    assert_eq!(g.screen[r].len(), cols as usize);
                    assert_eq!(g.alt_screen[r].len(), cols as usize);
                }
            }
        }
    }

    #[test]
    fn fuzz_random_bytes_with_resize_no_crash() {
        let mut rng = FuzzRng(12345);
        for _seq in 0..100 {
            let start_cols = 10 + (u16::from(rng.next_u8()) % 100);
            let start_rows = 5 + (u16::from(rng.next_u8()) % 50);
            let mut g = Grid::new(start_cols, start_rows);

            // Feed some random data
            let mut buf = vec![0u8; (rng.next_u8() as usize) * 2];
            rng.next_bytes(&mut buf);
            g.feed(&buf);

            // Resize to new random size
            let new_cols = 5 + (u16::from(rng.next_u8()) % 150);
            let new_rows = 3 + (u16::from(rng.next_u8()) % 60);
            g.resize(new_cols, new_rows);

            // Invariants after resize
            assert_eq!(g.cols, new_cols);
            assert_eq!(g.rows, new_rows);
            assert!(g.cursor_col < new_cols);
            assert!(g.cursor_row < new_rows);
            assert_eq!(g.screen.len(), new_rows as usize);
            assert_eq!(g.alt_screen.len(), new_rows as usize);
            assert_eq!(g.dirty.len(), new_rows as usize);
            for r in 0..new_rows as usize {
                assert_eq!(g.screen[r].len(), new_cols as usize);
                assert_eq!(g.alt_screen[r].len(), new_cols as usize);
            }
        }
    }

    #[test]
    fn fuzz_alternate_screen_switch_no_crash() {
        let mut rng = FuzzRng(9999);
        let mut g = Grid::new(80, 24);
        for _ in 0..50 {
            // Feed random bytes
            let mut buf = vec![0u8; (rng.next_u8() as usize) * 3];
            rng.next_bytes(&mut buf);
            g.feed(&buf);

            // Toggle alternate screen
            g.feed(b"\x1b[?1049h"); // enter alt screen
            let mut buf2 = vec![0u8; (rng.next_u8() as usize) * 2];
            rng.next_bytes(&mut buf2);
            g.feed(&buf2);
            g.feed(b"\x1b[?1049l"); // exit alt screen

            // Invariants
            assert_eq!(g.screen.len(), 24);
            assert_eq!(g.alt_screen.len(), 24);
            assert!(!g.alt_active);
            assert!(g.cursor_col < 80);
            assert!(g.cursor_row < 24);
        }
    }

    #[test]
    fn fuzz_take_delta_after_random_bytes_non_empty() {
        let mut rng = FuzzRng(7777);
        let mut g = Grid::new(80, 24);
        let mut total_rows = 0;
        for _ in 0..30 {
            let mut buf = vec![0u8; (rng.next_u8() as usize) * 4];
            rng.next_bytes(&mut buf);
            g.feed(&buf);
            let delta = g.take_delta();
            // Delta may be empty if only control chars were processed,
            // but it should never panic or return out-of-bounds rows.
            for changed in &delta.rows {
                assert!(changed.row < 24, "row {} >= 24", changed.row);
                assert_eq!(changed.cells.len(), 80);
            }
            total_rows += delta.rows.len();
        }
        // At least some content was produced (very likely with random data)
        assert!(total_rows > 0, "no delta rows produced from random bytes");
    }
}
