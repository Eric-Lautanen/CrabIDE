//! Minimap: scrollable code overview sidebar rendered to the right of the editor.
//!
//! Shows a zoomed-out view of the entire document with syntax-coloured lines.
//! The visible viewport is highlighted as a semi-transparent rectangle that
//! the user can drag to scroll the editor.

use crabide_config::Color;

use crate::state::{cfg_to_egui, UiState};

/// Width of the minimap in pixels.
const MINIMAP_WIDTH: f32 = 80.0;
/// Height of each minimap line in pixels.
const MINIMAP_LINE_H: f32 = 2.0;

/// Render the minimap to the right of the editor content area.
///
/// Returns `true` if the user clicked/dragged the minimap viewport handle
/// (the caller should scroll the editor accordingly).
pub fn show(ui: &mut egui::Ui, state: &mut UiState) -> bool {
    let Some(active_idx) = state.active_tab() else {
        return false;
    };
    let Some(tab) = state.tabs().get(active_idx) else {
        return false;
    };

    let _n_lines = tab.lines.len().max(1);

    let bg = cfg_to_egui(
        state
            .theme
            .ui_or("minimap.background", Color::rgb(0x1e, 0x1e, 0x1e)),
    );
    let viewport_fg = cfg_to_egui(state.theme.ui_or(
        "minimap.selectionHighlight",
        Color::rgba(0x40, 0x40, 0x40, 0x80),
    ));
    let line_fg = cfg_to_egui(
        state
            .theme
            .ui_or("editor.foreground", Color::rgb(0x80, 0x80, 0x80)),
    );

    // Allocate the minimap area.
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(MINIMAP_WIDTH, ui.available_height()),
        egui::Sense::click_and_drag(),
    );
    ui.painter().rect_filled(rect, 0.0, bg);

    // Clip to the minimap rect.
    ui.set_clip_rect(rect);

    // Draw each line as a tiny horizontal bar coloured by the first highlight span.
    let default_fg = line_fg;
    for (i, line_text) in tab.lines.iter().enumerate() {
        if line_text.trim().is_empty() {
            continue;
        }
        let y = rect.top() + i as f32 * MINIMAP_LINE_H;
        if y > rect.bottom() {
            break;
        }

        // Find the first highlight span on this line for colour.
        let line_u32 = i as u32;
        let fg = tab
            .highlight_spans
            .iter()
            .find(|sp| sp.range.start.line <= line_u32 && sp.range.end.line >= line_u32)
            .map(|sp| {
                let vscope = crabide_syntax::highlight::scope_to_vscode(sp.scope.as_str());
                let ts = state.theme.token_style(vscope);
                ts.foreground.map(cfg_to_egui).unwrap_or(default_fg)
            })
            .unwrap_or(default_fg);

        // Scale line width to minimap.
        let char_count = line_text.chars().count() as f32;
        let bar_w = (char_count / 100.0 * MINIMAP_WIDTH).clamp(2.0, MINIMAP_WIDTH - 4.0);
        let bar_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 2.0, y),
            egui::vec2(bar_w, MINIMAP_LINE_H),
        );
        ui.painter().rect_filled(bar_rect, 0.0, fg);
    }

    // Draw the viewport indicator.
    // Approximate visible lines from the minimap height vs editor line height.
    let visible_lines = (rect.height() / 16.0).round() as usize;
    // Use pending_scroll_line if set, otherwise default to 0.
    let first_vis = state.pending_scroll_line.unwrap_or(0);
    let vp_top = rect.top() + first_vis as f32 * MINIMAP_LINE_H;
    let vp_h = visible_lines as f32 * MINIMAP_LINE_H;
    let vp_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left(), vp_top),
        egui::vec2(MINIMAP_WIDTH, vp_h),
    );
    ui.painter().rect_filled(vp_rect, 2.0, viewport_fg);

    // Handle click/drag on the minimap to scroll.
    let mut scrolled = false;
    if resp.clicked() || resp.dragged() {
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            if rect.contains(pos) {
                let click_y = pos.y - rect.top();
                let target_line = (click_y / MINIMAP_LINE_H) as usize;
                state.pending_scroll_line = Some(target_line);
                scrolled = true;
            }
        }
    }

    scrolled
}
