//! Peek view — inline definition/reference preview overlay.
//!
//! Renders a split overlay within the editor: a list of locations on the left
//! and a preview of the selected location on the right.  The user can navigate
//! between locations with up/down arrows and open the selected file with Enter.

use crate::state::{UiState, cfg_to_egui};

/// Render the peek overlay if it is visible.
/// Must be called from within the editor area so it paints on top.
pub fn show(ui: &mut egui::Ui, state: &mut UiState, actions: &mut Vec<crabide_config::Action>) {
    if !state.peek.visible || state.peek.locations.is_empty() {
        return;
    }

    let bg = cfg_to_egui(state.theme.ui_or(
        "editorWidget.background",
        crabide_config::Color::rgb(0x25, 0x25, 0x26),
    ));
    let border = cfg_to_egui(state.theme.ui_or(
        "editorWidget.border",
        crabide_config::Color::rgb(0x45, 0x45, 0x45),
    ));
    let fg = cfg_to_egui(state.theme.ui_or(
        "editor.foreground",
        crabide_config::Color::rgb(0xcc, 0xcc, 0xcc),
    ));
    let muted = cfg_to_egui(state.theme.ui_or(
        "editorWidget.foreground",
        crabide_config::Color::rgb(0x99, 0x99, 0x99),
    ));
    let selected_bg = cfg_to_egui(state.theme.ui_or(
        "list.activeSelectionBackground",
        crabide_config::Color::rgb(0x09, 0x47, 0x71),
    ));

    let area = ui.available_rect_before_wrap();
    // Peek overlay height: ~40% of available height, min 120px, max 400px.
    let peek_height = (area.height() * 0.4).clamp(120.0, 400.0);
    let peek_rect = egui::Rect::from_min_size(
        egui::pos2(area.left(), area.bottom() - peek_height),
        egui::vec2(area.width(), peek_height),
    );

    // Draw background and border.
    ui.painter().rect_filled(peek_rect, 0.0, bg);
    ui.painter().rect_stroke(
        peek_rect,
        0.0,
        egui::Stroke::new(1.0, border),
        egui::StrokeKind::Outside,
    );

    // Title bar: kind label + count + close button.
    let title_bar_height = 26.0;
    let title_rect = egui::Rect::from_min_size(
        peek_rect.min,
        egui::vec2(peek_rect.width(), title_bar_height),
    );
    let title_bg = cfg_to_egui(state.theme.ui_or(
        "sideBarSectionHeader.background",
        crabide_config::Color::rgb(0x2a, 0x2a, 0x2a),
    ));
    ui.painter().rect_filled(title_rect, 0.0, title_bg);

    let kind_label = match state.peek.kind {
        Some(crate::state::PeekKind::Definition) => "Peek: Definition",
        Some(crate::state::PeekKind::Declaration) => "Peek: Declaration",
        Some(crate::state::PeekKind::Implementation) => "Peek: Implementation",
        Some(crate::state::PeekKind::TypeDefinition) => "Peek: Type Definition",
        Some(crate::state::PeekKind::References) => "Peek: References",
        None => "Peek",
    };
    let title_text = format!(
        "{}  —  {} location(s)",
        kind_label,
        state.peek.locations.len()
    );
    ui.painter().text(
        egui::pos2(title_rect.left() + 8.0, title_rect.center().y),
        egui::Align2::LEFT_CENTER,
        title_text,
        egui::FontId::proportional(12.0),
        fg,
    );

    // Close button.
    let close_rect = egui::Rect::from_min_size(
        egui::pos2(title_rect.right() - 20.0, title_rect.top()),
        egui::vec2(20.0, title_bar_height),
    );
    let close_resp = ui.allocate_rect(close_rect, egui::Sense::click());
    ui.painter().text(
        close_rect.center(),
        egui::Align2::CENTER_CENTER,
        "×",
        egui::FontId::proportional(14.0),
        muted,
    );
    if close_resp.clicked() {
        state.peek.close();
        return;
    }

    // Split into left list (35%) and right preview (65%).
    let content_rect = egui::Rect::from_min_size(
        egui::pos2(peek_rect.left(), title_rect.bottom()),
        egui::vec2(peek_rect.width(), peek_rect.height() - title_bar_height),
    );
    let list_width = (content_rect.width() * 0.35).max(160.0);
    let list_rect = egui::Rect::from_min_size(
        content_rect.min,
        egui::vec2(list_width, content_rect.height()),
    );
    let preview_rect = egui::Rect::from_min_size(
        egui::pos2(list_rect.right(), content_rect.top()),
        egui::vec2(content_rect.width() - list_width, content_rect.height()),
    );

    // ── Left: location list ────────────────────────────────────────────────
    ui.painter().rect_filled(list_rect, 0.0, bg);
    // Divider line between list and preview.
    ui.painter().line_segment(
        [
            egui::pos2(list_rect.right(), list_rect.top()),
            egui::pos2(list_rect.right(), list_rect.bottom()),
        ],
        egui::Stroke::new(1.0, border),
    );

    // Keyboard navigation: up/down/enter/escape.
    let ctx = ui.ctx();
    if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
        state.peek.prev();
        ctx.request_repaint();
    }
    if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
        state.peek.next();
        ctx.request_repaint();
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
        navigate_to_selected(ui, state, actions);
        return;
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.peek.close();
        return;
    }

    // Render location list items inside a scrollable area.
    let item_height = 22.0;
    let total_items = state.peek.locations.len();
    egui::ScrollArea::vertical()
        .id_salt("peek_list_scroll")
        .auto_shrink([false; 2])
        .show_viewport(ui, |ui, viewport| {
            ui.set_width(list_rect.width());

            let start_line = (viewport.top() / item_height).floor() as usize;
            let end_line = (viewport.bottom() / item_height).ceil() as usize;
            let start_line = start_line.min(total_items.saturating_sub(1));
            let end_line = end_line.min(total_items);

            for i in start_line..end_line {
                let loc = match state.peek.locations.get(i) {
                    Some(l) => l,
                    None => continue,
                };

                let y = list_rect.top() + i as f32 * item_height;
                let item_rect = egui::Rect::from_min_size(
                    egui::pos2(list_rect.left(), y),
                    egui::vec2(list_rect.width(), item_height),
                );
                let is_selected = i == state.peek.selected_idx;
                let item_bg = if is_selected {
                    selected_bg
                } else if ui.rect_contains_pointer(item_rect) {
                    cfg_to_egui(state.theme.ui_or(
                        "list.hoverBackground",
                        crabide_config::Color::rgb(0x2a, 0x2d, 0x2e),
                    ))
                } else {
                    egui::Color32::TRANSPARENT
                };

                if ui.is_rect_visible(item_rect) {
                    ui.painter().rect_filled(item_rect, 0.0, item_bg);

                    let file_name = loc
                        .uri
                        .as_url()
                        .to_file_path()
                        .ok()
                        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
                        .unwrap_or_else(|| loc.uri.to_string());
                    let line = loc.range.start.line + 1;
                    let label = format!("{file_name}:{line}");

                    ui.painter().text(
                        egui::pos2(item_rect.left() + 6.0, item_rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        label,
                        egui::FontId::monospace(11.0),
                        if is_selected { fg } else { muted },
                    );

                    let click_resp = ui.allocate_rect(item_rect, egui::Sense::click());
                    if click_resp.clicked() {
                        state.peek.selected_idx = i;
                    }
                    if click_resp.double_clicked() {
                        navigate_to_selected(ui, state, actions);
                        return;
                    }
                }
            }

            // Reserve full scrollable height.
            ui.allocate_exact_size(
                egui::vec2(list_rect.width(), total_items as f32 * item_height),
                egui::Sense::hover(),
            );
        });

    // ── Right: code preview ────────────────────────────────────────────────
    ui.painter().rect_filled(preview_rect, 0.0, bg);

    if let Some(loc) = state.peek.selected_location() {
        // Draw the file path at the top of the preview.
        let file_path = loc
            .uri
            .as_url()
            .to_file_path()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| loc.uri.to_string());
        ui.painter().text(
            egui::pos2(preview_rect.left() + 8.0, preview_rect.top() + 4.0),
            egui::Align2::LEFT_TOP,
            &file_path,
            egui::FontId::proportional(11.0),
            muted,
        );

        // Try to get the lines from the open tab, or just show a message.
        let preview_lines = state
            .tabs()
            .iter()
            .find(|t| t.uri == loc.uri)
            .map(|t| t.lines.clone())
            .unwrap_or_default();

        if !preview_lines.is_empty() {
            let start_line = loc.range.start.line as usize;
            let end_line = (loc.range.end.line as usize + 5).min(preview_lines.len());
            let context_start = start_line.saturating_sub(2);

            let preview_text: String = preview_lines[context_start..end_line]
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    let line_no = context_start + i + 1;
                    format!("{:>4}  {}\n", line_no, l)
                })
                .collect();

            let preview_text_rect = egui::Rect::from_min_size(
                egui::pos2(preview_rect.left() + 4.0, preview_rect.top() + 18.0),
                egui::vec2(preview_rect.width() - 8.0, preview_rect.height() - 20.0),
            );

            let preview_resp = ui.allocate_rect(preview_text_rect, egui::Sense::hover());
            ui.painter().text(
                preview_text_rect.left_top(),
                egui::Align2::LEFT_TOP,
                preview_text,
                egui::FontId::monospace(11.0),
                fg,
            );
            preview_resp.on_hover_text("Enter to navigate, Esc to close");
        } else {
            ui.painter().text(
                egui::pos2(preview_rect.center().x, preview_rect.center().y),
                egui::Align2::CENTER_CENTER,
                "Open this file to see preview",
                egui::FontId::proportional(11.0),
                muted,
            );
        }
    }
}

/// Navigate to the currently selected peek location.
fn navigate_to_selected(
    ui: &egui::Ui,
    state: &mut UiState,
    actions: &mut Vec<crabide_config::Action>,
) {
    let loc = match state.peek.selected_location() {
        Some(l) => l.clone(),
        None => return,
    };
    actions.push(crabide_config::Action::OpenFile);
    if let Ok(path) = loc.uri.as_url().to_file_path() {
        state.pending_open_path = Some(path);
    }
    if let Some(idx) = state.tabs().iter().position(|t| t.uri == loc.uri) {
        state.active_group_mut().active_tab = Some(idx);
    }
    state.pending_scroll_line = Some(loc.range.start.line as usize);
    if let Some(tab) = state.active_tab_mut() {
        tab.cursors.set_single(loc.range.start);
    }
    state.peek.close();
    ui.ctx().request_repaint();
}
