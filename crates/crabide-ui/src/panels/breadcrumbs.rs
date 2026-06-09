//! Breadcrumb bar rendered above the editor content area.
//!
//! Shows the symbol hierarchy at the cursor position as clickable segments
//! separated by `>` chevrons, similar to VS Code's breadcrumb bar.
//!
//! # Layout
//! ```text
//! ┌─ breadcrumb bar (22 px) ──────────────────────────────────────────┐
//! │ src > MyStruct > method > …                                       │
//! └───────────────────────────────────────────────────────────────────┘
//! ```
//!
//! Clicking a segment scrolls the editor to that symbol's selection range.

use crabide_config::Color;

use crate::state::{cfg_to_egui, UiState};

/// Render the breadcrumb bar above the editor content.
///
/// Returns `true` if a breadcrumb segment was clicked (the caller should
/// check `state.pending_scroll_line`).
pub fn show(ui: &mut egui::Ui, state: &mut UiState) -> bool {
    let Some(active_idx) = state.active_tab() else {
        return false;
    };
    let Some(tab) = state.tabs().get(active_idx) else {
        return false;
    };

    let breadcrumbs = tab.breadcrumbs.clone();
    if breadcrumbs.is_empty() {
        return false;
    }

    let bar_bg = cfg_to_egui(
        state
            .theme
            .ui_or("breadcrumbPicker.background", Color::rgb(0x25, 0x25, 0x26)),
    );
    let fg = cfg_to_egui(
        state
            .theme
            .ui_or("breadcrumb.foreground", Color::rgb(0xcc, 0xcc, 0xcc)),
    );
    let fg_muted = cfg_to_egui(
        state
            .theme
            .ui_or("breadcrumb.background", Color::rgb(0x88, 0x88, 0x88)),
    );
    let sep_fg = cfg_to_egui(
        state
            .theme
            .ui_or("breadcrumb.separatorColor", Color::rgb(0x66, 0x66, 0x66)),
    );
    let hov_bg = cfg_to_egui(
        state
            .theme
            .ui_or("list.hoverBackground", Color::rgba(0x2a, 0x2d, 0x2e, 0xff)),
    );

    let bar_height = 22.0;
    let (bar_rect, _bar_resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), bar_height),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(bar_rect, 0.0, bar_bg);

    let file_name = tab.title.clone();
    let mut segments: Vec<(String, Option<u32>)> = vec![(file_name, None)];
    for seg in &breadcrumbs {
        segments.push((seg.name.clone(), Some(seg.line)));
    }

    let font_id = egui::FontId::proportional(12.0);
    let sep_font = egui::FontId::proportional(11.0);
    let sep_text = " > ";

    let mut x = bar_rect.min.x + 8.0;
    let y_center = bar_rect.center().y;
    let mut clicked_line: Option<usize> = None;

    for (i, (label, line)) in segments.iter().enumerate() {
        let is_last = i == segments.len() - 1;
        let color = if is_last { fg } else { fg_muted };

        // Measure text width via layout (needs mutable fonts).
        let galley =
            ui.fonts_mut(|f| f.layout(label.clone(), font_id.clone(), color, f32::INFINITY));
        let text_width = galley.size().x;
        let text_rect =
            egui::Rect::from_min_size(egui::pos2(x, y_center - 8.0), egui::vec2(text_width, 16.0));

        // Draw text.
        ui.painter()
            .galley(egui::pos2(x, y_center - 8.0), galley, color);

        // Clickable area for navigation.
        if line.is_some() {
            let resp = ui.allocate_rect(text_rect, egui::Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(text_rect, 2.0, hov_bg);
                // Re-draw text on top of hover bg.
                let hov_galley =
                    ui.fonts_mut(|f| f.layout(label.clone(), font_id.clone(), fg, f32::INFINITY));
                ui.painter()
                    .galley(egui::pos2(x, y_center - 8.0), hov_galley, fg);
            }
            if resp.clicked() {
                clicked_line = Some(line.unwrap() as usize);
            }
        }

        x += text_width;

        // Separator.
        if i < segments.len() - 1 {
            let sep_galley = ui.fonts_mut(|f| {
                f.layout(sep_text.to_owned(), sep_font.clone(), sep_fg, f32::INFINITY)
            });
            let sep_width = sep_galley.size().x;
            ui.painter()
                .galley(egui::pos2(x, y_center - 8.0), sep_galley, sep_fg);
            x += sep_width;
        }
    }

    if let Some(line) = clicked_line {
        state.pending_scroll_line = Some(line);
        return true;
    }

    false
}
