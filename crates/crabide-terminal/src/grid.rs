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
//! - Scrollback buffer (configurable depth, implemented as VecDeque<Row>)
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    /// Number of columns this character occupies (1 for ASCII, 2 for CJK).
    pub width: u8,
    pub fg: Color,
    pub bg: Color,
    pub attrs: Attrs,
}

impl Cell {
    pub const BLANK: Self = Self {
        ch: ' ',
        width: 1,
        fg: Color::Default,
        bg: Color::Default,
        attrs: Attrs::empty(),
    };
}

impl Default for Cell {
    fn default() -> Self {
        Self::BLANK
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

    // vte parser
    parser: Parser,
}

impl Grid {
    pub fn new(cols: u16, rows: u16) -> Self {
        let blank_row = vec![Cell::BLANK; cols as usize];
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
            title: None,
            cwd: None,
            cursor_visible: true,
            bracketed_paste: false,
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            parser: Parser::new(),
        }
    }

    /// Feed raw bytes from the PTY into the grid.
    pub fn feed(&mut self, bytes: &[u8]) {
        // vte 0.15: advance() takes a &[u8] slice, not individual bytes.
        let mut parser = std::mem::replace(&mut self.parser, Parser::new());
        parser.advance(self, bytes);
        self.parser = parser;
    }

    /// Resize the grid to new dimensions.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.rows = rows;

        let blank_row = || vec![Cell::BLANK; cols as usize];

        // Resize existing rows
        for row in &mut self.screen {
            row.resize(cols as usize, Cell::BLANK);
        }
        // Add or remove rows
        self.screen.resize_with(rows as usize, blank_row);
        self.dirty.resize(rows as usize, true);

        // Same for alt screen
        for row in &mut self.alt_screen {
            row.resize(cols as usize, Cell::BLANK);
        }
        self.alt_screen.resize_with(rows as usize, blank_row);

        // Clamp cursor
        self.cursor_col = self.cursor_col.min(cols.saturating_sub(1));
        self.cursor_row = self.cursor_row.min(rows.saturating_sub(1));
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

        if row < rows && col < cols {
            let cell = Cell {
                ch,
                width,
                fg: self.cur_fg,
                bg: self.cur_bg,
                attrs: self.cur_attrs,
            };
            let screen = self.active_screen();
            screen[row][col] = cell;

            // If wide character, blank the next cell
            if width == 2 && col + 1 < cols {
                screen[row][col + 1] = Cell {
                    ch: ' ',
                    width: 0,
                    ..cell
                };
            }
            self.dirty[row] = true;
        }

        // Advance cursor
        let advance = if width == 0 { 1 } else { width as u16 };
        self.cursor_col += advance;

        // Line wrap
        if self.cursor_col >= self.cols {
            self.cursor_col = 0;
            self.cursor_row += 1;
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
            // Only push to scrollback if the scroll region starts at row 0
            if top == 0 {
                if self.scrollback.len() >= self.scrollback_limit {
                    self.scrollback.pop_front();
                }
                self.scrollback.push_back(removed);
            }
            // Insert a blank row at the bottom of the scroll region
            self.screen
                .insert(bottom, vec![Cell::BLANK; self.cols as usize]);
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
            // Insert a blank row at the top of the scroll region
            self.screen
                .insert(top, vec![Cell::BLANK; self.cols as usize]);
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
                                self.screen[row][c] = Cell::BLANK;
                            }
                            self.mark_dirty(row);
                            for r in (row + 1)..rows {
                                self.screen[r] = vec![Cell::BLANK; cols];
                                self.mark_dirty(r);
                            }
                        }
                    }
                    1 => {
                        // Erase from start to cursor
                        let row = self.cursor_row as usize;
                        for r in 0..row {
                            self.screen[r] = vec![Cell::BLANK; cols];
                            self.mark_dirty(r);
                        }
                        if row < rows {
                            let col = self.cursor_col as usize;
                            for c in 0..=col {
                                self.screen[row][c] = Cell::BLANK;
                            }
                            self.mark_dirty(row);
                        }
                    }
                    2 | 3 => {
                        // Erase entire screen
                        for r in 0..rows {
                            self.screen[r] = vec![Cell::BLANK; cols];
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
                                self.screen[row][c] = Cell::BLANK;
                            }
                        }
                        1 => {
                            for c in 0..=col {
                                self.screen[row][c] = Cell::BLANK;
                            }
                        }
                        2 => {
                            self.screen[row] = vec![Cell::BLANK; cols];
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
                            .insert(row, vec![Cell::BLANK; self.cols as usize]);
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
                            .insert(bottom, vec![Cell::BLANK; self.cols as usize]);
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
                            self.screen[row][c] = self.screen[row][c - count];
                        }
                    }
                    // Fill the inserted positions with blanks
                    for c in col..(col + count) {
                        if c < cols {
                            self.screen[row][c] = Cell::BLANK;
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
                            self.screen[row][c] = self.screen[row][c + count];
                        } else {
                            self.screen[row][c] = Cell::BLANK;
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
        let data = params.get(1).copied().unwrap_or(&[]);

        match cmd {
            b"0" | b"2" => {
                // Set title
                self.title = std::str::from_utf8(data).ok().map(|s| s.to_owned());
            }
            b"7" => {
                // Shell working directory
                self.cwd = std::str::from_utf8(data).ok().map(|s| s.to_owned());
            }
            _ => {}
        }
    }

    fn hook(&mut self, _: &vte::Params, _: &[u8], _: bool, _: char) {}
    fn put(&mut self, _: u8) {}
    fn unhook(&mut self) {}
    fn esc_dispatch(&mut self, _: &[u8], _: bool, _: u8) {}
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

/// Approximate Unicode character display width.
/// Returns 2 for CJK/full-width, 0 for combining, 1 for everything else.
fn unicode_width(c: char) -> usize {
    // Fast path for ASCII
    if c.is_ascii() {
        return 1;
    }

    let cp = c as u32;

    // CJK Unified Ideographs and common wide ranges
    if matches!(
        cp,
        0x1100..=0x115F
            | // Hangul Jamo
          0x2E80..=0x303E
            | // CJK Radicals
          0x3040..=0x33FF
            | // Japanese
          0x3400..=0x4DBF
            | // CJK Extension A
          0x4E00..=0x9FFF
            | // CJK Unified Ideographs
          0xA000..=0xA4CF
            | // Yi
          0xA960..=0xA97F
            | // Hangul
          0xAC00..=0xD7AF
            | // Hangul Syllables
          0xF900..=0xFAFF
            | // CJK Compatibility
          0xFE10..=0xFE19
            | // Vertical Forms
          0xFE30..=0xFE4F
            | // CJK Compatibility Forms
          0xFF00..=0xFF60
            | // Fullwidth Latin
          0xFFE0..=0xFFE6
            | // Fullwidth Signs
          0x1B000..=0x1B0FF
            | // Kana supplement
          0x1F300..=0x1F9FF // Emoji
    ) {
        return 2;
    }

    1
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
        let cell = g.screen[0][0];
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
        assert_eq!(c, Cell::BLANK);
    }

    #[test]
    fn cell_new_fields() {
        let c = Cell {
            ch: 'A',
            width: 1,
            fg: Color::Named(NamedColor::Red),
            bg: Color::Default,
            attrs: Attrs::BOLD,
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
}
