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

    // Terminal title
    pub title: Option<String>,

    // OSC 7 working directory
    pub cwd: Option<String>,

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
            if self.cursor_row >= self.rows {
                self.scroll_up(1);
                self.cursor_row = self.rows - 1;
            }
        }
    }

    fn scroll_up(&mut self, count: u16) {
        for _ in 0..count {
            let removed = self.screen.remove(0);
            if self.scrollback.len() >= self.scrollback_limit {
                self.scrollback.pop_front();
            }
            self.scrollback.push_back(removed);
            self.screen.push(vec![Cell::BLANK; self.cols as usize]);
        }
        // All rows are dirty after scroll
        self.dirty.iter_mut().for_each(|d| *d = true);
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
                if self.cursor_row >= self.rows {
                    self.scroll_up(1);
                    self.cursor_row = self.rows - 1;
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
