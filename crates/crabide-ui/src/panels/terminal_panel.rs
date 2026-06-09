//! Terminal panel UI — renders the bottom-strip integrated terminal.
//!
//! # Layout
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │ [Terminal 1] [Terminal 2]  [+]  [×]                      │  ← tab strip
//! ├──────────────────────────────────────────────────────────┤
//! │  $ cargo build                                           │
//! │  Compiling crabide v0.1.0 ...                            │  ← cell grid
//! │  ▌                                                       │  ← cursor
//! └──────────────────────────────────────────────────────────┘
//! ```
//!
//! # Keyboard handling
//! When `terminal.has_focus` is true the outer keyboard router skips
//! its normal editor bindings and calls `encode_key` here to convert
//! egui key events into PTY escape sequences.

use egui::{Color32, FontId, Key, Modifiers, Rect, Ui, Vec2, pos2, vec2};

use crabide_config::Action;
use crabide_core::event::{CellAttrs, TerminalColor, TerminalColorScheme};
use crabide_terminal::{
    MouseButton, ScrollDirection, encode_mouse_motion, encode_mouse_press, encode_mouse_release,
    encode_mouse_scroll,
};

use crate::state::UiState;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Height of the terminal tab strip in logical pixels.
const TAB_HEIGHT: f32 = 28.0;
/// Minimum/default panel height (resizable via egui's bottom-panel handle).
pub const MIN_HEIGHT: f32 = 120.0;

// ── show ──────────────────────────────────────────────────────────────────────

/// Render the terminal panel body (tab strip + cell grid).
///
/// Called from `crabide_ui::render` inside a `TopBottomPanel::bottom`.
/// Returns any backend `Action`s generated (NewTerminal, KillTerminal).
pub fn show(ui: &mut Ui, state: &mut UiState) -> Vec<Action> {
    let mut actions = Vec::new();

    // Determine active color scheme (first instance or default).
    let active_scheme = state
        .terminal
        .active()
        .map(|i| &i.color_scheme)
        .unwrap_or_else(|| {
            // Use a static default for the empty state.
            static DEFAULT_SCHEME: TerminalColorScheme = TerminalColorScheme::dark_const();
            &DEFAULT_SCHEME
        });
    let default_bg = Color32::from_rgb(
        active_scheme.background.0,
        active_scheme.background.1,
        active_scheme.background.2,
    );

    let tab_bg = default_bg.linear_multiply(1.2); // slightly lighter
    let tab_fg = Color32::from_rgb(
        active_scheme.foreground.0,
        active_scheme.foreground.1,
        active_scheme.foreground.2,
    );
    let tab_active_bg = default_bg;
    let tab_active_fg = Color32::WHITE;

    // ── Tab strip ─────────────────────────────────────────────────────────────
    {
        let full_rect = ui.available_rect_before_wrap();
        let tab_rect = Rect::from_min_size(full_rect.min, Vec2::new(full_rect.width(), TAB_HEIGHT));
        let painter = ui.painter();
        painter.rect_filled(tab_rect, 0.0, tab_bg);

        let mut x = tab_rect.min.x;
        let y_center = tab_rect.center().y;

        // Instance tabs
        let mut kill_id: Option<u32> = None;
        let mut activate_idx: Option<usize> = None;

        for (idx, inst) in state.terminal.instances.iter().enumerate() {
            let is_active = idx == state.terminal.active_idx;
            let bg = if is_active { tab_active_bg } else { tab_bg };
            let fg = if is_active { tab_active_fg } else { tab_fg };

            // Measure tab label width (approximate)
            let label = format!("  {}  ", &inst.title);
            let tab_w = label.len() as f32 * 7.0 + 20.0;
            let close_w = 20.0;
            let total_w = tab_w + close_w;

            let tab_area = Rect::from_min_size(pos2(x, tab_rect.min.y), vec2(total_w, TAB_HEIGHT));

            // Background
            painter.rect_filled(tab_area, 0.0, bg);

            // Title text
            painter.text(
                pos2(x + 8.0, y_center),
                egui::Align2::LEFT_CENTER,
                &inst.title,
                FontId::proportional(12.0),
                fg,
            );

            // Close button
            let close_x = x + tab_w;
            let close_rect =
                Rect::from_min_size(pos2(close_x, tab_rect.min.y), vec2(close_w, TAB_HEIGHT));
            painter.text(
                close_rect.center(),
                egui::Align2::CENTER_CENTER,
                "✕",
                FontId::proportional(12.0),
                tab_fg.gamma_multiply(0.6),
            );

            // Interaction
            let resp = ui.interact(
                tab_area,
                egui::Id::new(("term_tab", idx)),
                egui::Sense::click(),
            );
            if resp.clicked() {
                activate_idx = Some(idx);
            }

            let close_resp = ui.interact(
                close_rect,
                egui::Id::new(("term_tab_close", idx)),
                egui::Sense::click(),
            );
            if close_resp.clicked() {
                kill_id = Some(inst.id);
            }

            x += total_w + 1.0;
        }

        if let Some(idx) = activate_idx {
            state.terminal.active_idx = idx;
            state.terminal.has_focus = true;
        }
        if let Some(id) = kill_id {
            state.terminal.pending_kill = Some(id);
        }

        // [+] New terminal button
        let new_rect = Rect::from_min_size(pos2(x, tab_rect.min.y), vec2(28.0, TAB_HEIGHT));
        painter.rect_filled(new_rect, 0.0, tab_bg);
        painter.text(
            new_rect.center(),
            egui::Align2::CENTER_CENTER,
            "⊕",
            FontId::proportional(14.0),
            tab_fg.gamma_multiply(0.8),
        );
        let new_resp = ui.interact(
            new_rect,
            egui::Id::new("term_new_btn"),
            egui::Sense::click(),
        );
        if new_resp.clicked() {
            state.terminal.pending_new = true;
            actions.push(Action::NewTerminal);
        }

        // Allocate space for the tab strip.
        ui.allocate_rect(tab_rect, egui::Sense::hover());
    }

    // ── Cell grid ─────────────────────────────────────────────────────────────
    let grid_rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(grid_rect, 0.0, default_bg);

    if state.terminal.instances.is_empty() {
        ui.painter().text(
            grid_rect.center(),
            egui::Align2::CENTER_CENTER,
            "Press ⊕ to open a terminal",
            FontId::monospace(13.0),
            tab_fg.gamma_multiply(0.4),
        );
        let click_resp = ui.allocate_rect(grid_rect, egui::Sense::click());
        if click_resp.clicked() {
            state.terminal.has_focus = true;
            state.terminal.pending_new = true;
            actions.push(Action::NewTerminal);
        }
        return actions;
    }

    let font_size = state.font_size;
    let font_id = FontId::monospace(font_size);
    // Use exact font metrics so cursor and text columns align perfectly.
    let cell_w = ui.fonts_mut(|f| f.glyph_width(&font_id, ' '));
    let cell_h = ui.fonts_mut(|f| f.row_height(&font_id));

    // Click in grid → grab focus
    let grid_resp = ui.allocate_rect(grid_rect, egui::Sense::click());
    if grid_resp.clicked() {
        state.terminal.has_focus = true;
    }

    let inst = match state.terminal.instances.get(state.terminal.active_idx) {
        Some(i) => i,
        None => return actions,
    };

    let painter = ui.painter();
    let visible_rows = ((grid_rect.height() / cell_h) as usize).max(1);
    let visible_cols = ((grid_rect.width() / cell_w) as usize).max(1);
    let total_rows = inst.rows.len();

    // Scroll viewport to always keep cursor visible.
    // When cursor is near the top (fresh shell), start from row 0.
    // When cursor moves lower, scroll down to keep it in view.
    let cursor_row = inst.cursor_row as usize;
    let cursor_col = inst.cursor_col as usize;
    let start_row = {
        let bottom_start = cursor_row.saturating_sub(visible_rows.saturating_sub(1));
        bottom_start.min(total_rows.saturating_sub(visible_rows))
    };
    let end_row = (start_row + visible_rows).min(total_rows);

    for (screen_row, grid_row) in (start_row..end_row).enumerate() {
        let row_cells = &inst.rows[grid_row];
        let row_col_count = row_cells.len().min(visible_cols);
        let y = grid_rect.min.y + screen_row as f32 * cell_h;

        // Paint non-default backgrounds first (including REVERSE cells)
        for (col, cell) in row_cells[..row_col_count].iter().enumerate() {
            let (_, bg) = effective_colors(cell, &inst.color_scheme);
            if bg != default_bg {
                let rect = Rect::from_min_size(
                    pos2(grid_rect.min.x + col as f32 * cell_w, y),
                    vec2(cell_w, cell_h),
                );
                painter.rect_filled(rect, 0.0, bg);
            }
        }

        // Paint text as color-runs (consecutive cells with same effective fg)
        let mut run_start = 0;
        while run_start < row_col_count {
            let (run_fg, _) = effective_colors(&row_cells[run_start], &inst.color_scheme);
            let mut run_end = run_start + 1;
            while run_end < row_col_count {
                let (c, _) = effective_colors(&row_cells[run_end], &inst.color_scheme);
                if c != run_fg {
                    break;
                }
                run_end += 1;
            }

            // Skip pure-space runs with default fg (invisible and wasteful)
            let text: String = row_cells[run_start..run_end]
                .iter()
                .map(|c| if c.ch < ' ' { ' ' } else { c.ch })
                .collect();

            if text.chars().any(|c| c != ' ') {
                let x = grid_rect.min.x + run_start as f32 * cell_w;
                painter.text(
                    pos2(x, y),
                    egui::Align2::LEFT_TOP,
                    &text,
                    FontId::monospace(font_size),
                    run_fg,
                );
            }

            run_start = run_end;
        }
    }

    // ── Cursor ────────────────────────────────────────────────────────────────
    if state.terminal.has_focus && state.caret_visible && inst.cursor_visible {
        // cur_screen_row is relative to start_row (the top of the visible viewport)
        if cursor_row >= start_row {
            let cur_screen_row = cursor_row - start_row;
            if cur_screen_row < visible_rows && cursor_col < visible_cols {
                let cx = grid_rect.min.x + cursor_col as f32 * cell_w;
                let cy = grid_rect.min.y + cur_screen_row as f32 * cell_h;
                let cursor_color = Color32::from_rgb(
                    inst.color_scheme.cursor.0,
                    inst.color_scheme.cursor.1,
                    inst.color_scheme.cursor.2,
                );
                painter.rect_filled(
                    Rect::from_min_size(pos2(cx, cy), vec2(cell_w, cell_h)),
                    0.0,
                    cursor_color.linear_multiply(0.3),
                );
                painter.rect_stroke(
                    Rect::from_min_size(pos2(cx, cy), vec2(cell_w, cell_h)),
                    0.0,
                    egui::Stroke::new(1.0, cursor_color.linear_multiply(0.7)),
                    egui::StrokeKind::Inside,
                );
            }
        }
    }

    // ── Mouse reporting ─────────────────────────────────────────────────────
    // When a mouse reporting mode is active, translate mouse events into
    // escape sequences and queue them as pending input for the PTY.
    if inst.mouse_x10 || inst.mouse_normal || inst.mouse_button_event {
        let grid_resp = ui.allocate_rect(grid_rect, egui::Sense::click_and_drag());

        // Convert pointer position to cell coordinates
        if let Some(pos) = grid_resp.hover_pos() {
            let mx = ((pos.x - grid_rect.min.x) / cell_w) as u16;
            let my = ((pos.y - grid_rect.min.y) / cell_h) as u16;

            // Button press — only on initial press, not every frame while held
            // We use `interact` with `Sense::click` to detect clicks
            if ui.input(|i| i.pointer.primary_pressed()) {
                let button = MouseButton::Left;
                if let Some(bytes) = encode_mouse_press(
                    inst.mouse_x10,
                    inst.mouse_normal,
                    inst.mouse_button_event,
                    inst.mouse_sgr,
                    button,
                    mx,
                    my,
                ) {
                    state.terminal.pending_input.push((inst.id, bytes));
                }
            }
            if ui.input(|i| i.pointer.secondary_pressed()) {
                let button = MouseButton::Right;
                if let Some(bytes) = encode_mouse_press(
                    inst.mouse_x10,
                    inst.mouse_normal,
                    inst.mouse_button_event,
                    inst.mouse_sgr,
                    button,
                    mx,
                    my,
                ) {
                    state.terminal.pending_input.push((inst.id, bytes));
                }
            }

            // Button release
            if ui.input(|i| i.pointer.any_released()) {
                // Determine which button was released
                let button = if ui.input(|i| i.pointer.primary_released()) {
                    MouseButton::Left
                } else {
                    MouseButton::Right
                };
                if let Some(bytes) = encode_mouse_release(
                    inst.mouse_normal,
                    inst.mouse_button_event,
                    inst.mouse_sgr,
                    button,
                    mx,
                    my,
                ) {
                    state.terminal.pending_input.push((inst.id, bytes));
                }
            }

            // Mouse motion while dragging (only for normal/button-event modes)
            if (inst.mouse_normal || inst.mouse_button_event)
                && ui.input(|i| i.pointer.any_down())
                && grid_resp.hovered()
            {
                let button = if ui.input(|i| i.pointer.primary_down()) {
                    MouseButton::Left
                } else {
                    MouseButton::Right
                };
                if let Some(bytes) = encode_mouse_motion(
                    inst.mouse_normal,
                    inst.mouse_button_event,
                    inst.mouse_sgr,
                    button,
                    mx,
                    my,
                ) {
                    state.terminal.pending_input.push((inst.id, bytes));
                }
            }

            // Scroll wheel
            let scroll = ui.input(|i| i.smooth_scroll_delta);
            if scroll.y != 0.0 {
                let direction = if scroll.y > 0.0 {
                    ScrollDirection::Up
                } else {
                    ScrollDirection::Down
                };
                if let Some(bytes) = encode_mouse_scroll(
                    inst.mouse_x10,
                    inst.mouse_normal,
                    inst.mouse_button_event,
                    inst.mouse_sgr,
                    direction,
                    mx,
                    my,
                ) {
                    state.terminal.pending_input.push((inst.id, bytes));
                }
            }
        }
    }
    actions
}

// ── Keyboard encoding ─────────────────────────────────────────────────────────

/// Convert an egui key event into PTY escape bytes.
///
/// `text` is the printable text from `egui::Event::Text`, if any.
/// Returns `None` if the key has no PTY representation.
pub fn encode_key(key: Key, mods: Modifiers, text: Option<&str>) -> Option<Vec<u8>> {
    // Plain text input — highest priority (covers all printable chars + IME).
    if let Some(t) = text {
        if !t.is_empty() {
            return Some(t.as_bytes().to_vec());
        }
    }

    // Ctrl+letter special control characters
    if mods.ctrl && !mods.shift && !mods.alt {
        let byte: Option<u8> = match key {
            Key::C => Some(0x03), // ETX — interrupt
            Key::D => Some(0x04), // EOT — end-of-file
            Key::Z => Some(0x1a), // SUB — suspend
            Key::L => Some(0x0c), // FF  — clear screen
            Key::U => Some(0x15), // NAK — kill to line start
            Key::K => Some(0x0b), // VT  — kill to line end
            Key::A => Some(0x01), // SOH — move to line start
            Key::E => Some(0x05), // ENQ — move to line end
            Key::R => Some(0x12), // DC2 — reverse history search
            Key::W => Some(0x17), // ETB — delete word backwards
            _ => None,
        };
        if let Some(b) = byte {
            return Some(vec![b]);
        }
    }

    match key {
        Key::Enter => Some(b"\r".to_vec()),
        Key::Backspace => Some(vec![0x7f]),
        Key::Escape => Some(vec![0x1b]),
        Key::Tab => {
            if mods.shift {
                Some(b"\x1b[Z".to_vec())
            } else {
                Some(b"\t".to_vec())
            }
        }
        Key::ArrowUp => {
            if mods.ctrl {
                Some(b"\x1b[1;5A".to_vec())
            } else {
                Some(b"\x1b[A".to_vec())
            }
        }
        Key::ArrowDown => {
            if mods.ctrl {
                Some(b"\x1b[1;5B".to_vec())
            } else {
                Some(b"\x1b[B".to_vec())
            }
        }
        Key::ArrowRight => {
            if mods.ctrl {
                Some(b"\x1b[1;5C".to_vec())
            } else {
                Some(b"\x1b[C".to_vec())
            }
        }
        Key::ArrowLeft => {
            if mods.ctrl {
                Some(b"\x1b[1;5D".to_vec())
            } else {
                Some(b"\x1b[D".to_vec())
            }
        }
        Key::Home => Some(b"\x1b[H".to_vec()),
        Key::End => Some(b"\x1b[F".to_vec()),
        Key::PageUp => Some(b"\x1b[5~".to_vec()),
        Key::PageDown => Some(b"\x1b[6~".to_vec()),
        Key::Delete => Some(b"\x1b[3~".to_vec()),
        Key::F1 => Some(b"\x1bOP".to_vec()),
        Key::F2 => Some(b"\x1bOQ".to_vec()),
        Key::F3 => Some(b"\x1bOR".to_vec()),
        Key::F4 => Some(b"\x1bOS".to_vec()),
        Key::F5 => Some(b"\x1b[15~".to_vec()),
        Key::F6 => Some(b"\x1b[17~".to_vec()),
        Key::F7 => Some(b"\x1b[18~".to_vec()),
        Key::F8 => Some(b"\x1b[19~".to_vec()),
        Key::F9 => Some(b"\x1b[20~".to_vec()),
        Key::F10 => Some(b"\x1b[21~".to_vec()),
        Key::F11 => Some(b"\x1b[23~".to_vec()),
        Key::F12 => Some(b"\x1b[24~".to_vec()),
        _ => None,
    }
}

// ── Color helpers ─────────────────────────────────────────────────────────────

/// Return the effective (fg, bg) colors for a cell, handling REVERSE video.
fn effective_colors(
    cell: &crate::state::DisplayCell,
    scheme: &TerminalColorScheme,
) -> (Color32, Color32) {
    let fg = terminal_color_to_egui(cell.fg, false, &cell.attrs, scheme);
    let bg = terminal_color_to_egui(cell.bg, true, &cell.attrs, scheme);
    if cell.attrs.contains(CellAttrs::REVERSE) {
        (bg, fg)
    } else {
        (fg, bg)
    }
}

/// Convert a `TerminalColor` to an egui `Color32`.
pub fn terminal_color_to_egui(
    color: TerminalColor,
    is_bg: bool,
    attrs: &CellAttrs,
    scheme: &TerminalColorScheme,
) -> Color32 {
    match color {
        TerminalColor::Default => {
            if is_bg {
                Color32::from_rgb(
                    scheme.background.0,
                    scheme.background.1,
                    scheme.background.2,
                )
            } else if attrs.contains(CellAttrs::DIM) {
                // Dimmed foreground: reduce intensity of the foreground color
                let (r, g, b) = (
                    scheme.foreground.0 / 2,
                    scheme.foreground.1 / 2,
                    scheme.foreground.2 / 2,
                );
                Color32::from_rgb(r, g, b)
            } else {
                Color32::from_rgb(
                    scheme.foreground.0,
                    scheme.foreground.1,
                    scheme.foreground.2,
                )
            }
        }
        TerminalColor::Rgb(r, g, b) => {
            if attrs.contains(CellAttrs::DIM) {
                Color32::from_rgb(r / 2, g / 2, b / 2)
            } else {
                Color32::from_rgb(r, g, b)
            }
        }
        TerminalColor::Indexed(idx) => {
            xterm_256_to_egui(idx, attrs.contains(CellAttrs::DIM), scheme)
        }
    }
}

fn xterm_256_to_egui(idx: u8, dim: bool, scheme: &TerminalColorScheme) -> Color32 {
    let (r, g, b) = xterm_256_rgb(idx, scheme);
    let (r, g, b) = if dim {
        (r / 2, g / 2, b / 2)
    } else {
        (r, g, b)
    };
    Color32::from_rgb(r, g, b)
}

fn xterm_256_rgb(idx: u8, scheme: &TerminalColorScheme) -> (u8, u8, u8) {
    // Use the scheme's ANSI colors for the first 16 colors.
    if (idx as usize) < scheme.ansi.len() {
        return scheme.ansi[idx as usize];
    }
    if idx >= 232 {
        let v = (8 + (idx - 232) as u32 * 10).min(255) as u8;
        return (v, v, v);
    }
    let n = idx - 16;
    let b = n % 6;
    let g = (n / 6) % 6;
    let r = n / 36;
    let c = |v: u8| if v == 0 { 0u8 } else { 55 + v * 40 };
    (c(r), c(g), c(b))
}
