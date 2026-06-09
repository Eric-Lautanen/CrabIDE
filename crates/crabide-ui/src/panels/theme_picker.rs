//! Theme picker panel — browse and select color themes.
//!
//! Renders as a central overlay with a searchable list of available themes.
//! When the user clicks a theme, the selection is communicated back to the
//! app via `state.theme_picker.pending_theme_id`.

use crate::state::{cfg_to_egui, UiState};

/// Render the theme picker overlay.
pub fn show(ui: &mut egui::Ui, state: &mut UiState) {
    if !state.theme_picker.visible {
        return;
    }

    let ctx = ui.ctx();

    // Close on Escape
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.theme_picker.visible = false;
        return;
    }

    let drop_bg = cfg_to_egui(state.theme.ui_or(
        "dropdown.background",
        crabide_config::Color::rgb(0x3c, 0x3c, 0x3c),
    ));
    let input_bg = cfg_to_egui(state.theme.ui_or(
        "input.background",
        crabide_config::Color::rgb(0x3c, 0x3c, 0x3c),
    ));
    let input_fg = cfg_to_egui(state.theme.ui_or(
        "input.foreground",
        crabide_config::Color::rgb(0xcc, 0xcc, 0xcc),
    ));
    let list_bg = cfg_to_egui(state.theme.ui_or(
        "list.activeSelectionBackground",
        crabide_config::Color::rgb(0x09, 0x4a, 0x7b),
    ));
    let list_fg = cfg_to_egui(state.theme.ui_or(
        "list.activeSelectionForeground",
        crabide_config::Color::rgb(0xff, 0xff, 0xff),
    ));
    let row_hover = cfg_to_egui(state.theme.ui_or(
        "list.hoverBackground",
        crabide_config::Color::rgba(0x2a, 0x2d, 0x2e, 0xff),
    ));
    let text_fg = cfg_to_egui(state.theme.ui_or(
        "editor.foreground",
        crabide_config::Color::rgb(0xcc, 0xcc, 0xcc),
    ));

    let screen = ctx.content_rect();
    let win_w = 360.0_f32.min(screen.width() - 40.0);
    let win_h = 400.0_f32.min(screen.height() - 80.0);
    let win_left = screen.center().x - win_w / 2.0;
    let win_top = screen.top() + 60.0;

    let mut theme_changed = false;

    egui::Window::new("##theme_picker")
        .id(egui::Id::new("theme_picker_window"))
        .title_bar(false)
        .resizable(false)
        .movable(false)
        .frame(
            egui::Frame::default()
                .fill(drop_bg)
                .corner_radius(egui::CornerRadius::same(4)),
        )
        .fixed_pos(egui::pos2(win_left, win_top))
        .fixed_size(egui::vec2(win_w, win_h))
        .show(ctx, |ui| {
            ui.set_width(win_w);

            // ── Title ──────────────────────────────────────────────────────────
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Select Theme")
                    .color(text_fg)
                    .size(13.0)
                    .strong(),
            );
            ui.add_space(6.0);

            // ── Search box ─────────────────────────────────────────────────────
            let mut query = String::new();
            egui::Frame::default()
                .fill(input_bg)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut query)
                            .font(egui::TextStyle::Monospace)
                            .text_color(input_fg)
                            .frame(egui::Frame::NONE)
                            .desired_width(f32::INFINITY)
                            .hint_text(
                                egui::RichText::new("Search themes...")
                                    .color(egui::Color32::from_rgb(0x88, 0x88, 0x88)),
                            ),
                    );
                    resp.request_focus();
                });
            ui.add_space(4.0);

            // ── Theme list ─────────────────────────────────────────────────────
            let current_id = state.theme.id.as_str();
            let themes = state.theme_picker.themes.clone();
            let filtered: Vec<(String, String)> = if query.is_empty() {
                themes
            } else {
                let q = query.to_lowercase();
                themes
                    .into_iter()
                    .filter(|(_, name)| name.to_lowercase().contains(&q))
                    .collect()
            };

            egui::ScrollArea::vertical()
                .id_salt("theme_picker_list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for (i, (id, name)) in filtered.iter().enumerate() {
                        let is_selected = id == current_id;
                        let is_hovered = state.theme_picker.selected_idx == i;
                        let bg = if is_selected {
                            list_bg
                        } else if is_hovered {
                            row_hover
                        } else {
                            egui::Color32::TRANSPARENT
                        };
                        let fg = if is_selected { list_fg } else { text_fg };

                        let (rect, resp) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 24.0),
                            egui::Sense::click(),
                        );
                        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);

                        if resp.hovered() {
                            state.theme_picker.selected_idx = i;
                        }

                        if bg != egui::Color32::TRANSPARENT {
                            ui.painter().rect_filled(rect, 2.0, bg);
                        }
                        ui.painter().text(
                            rect.left_center() + egui::vec2(12.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            name,
                            egui::FontId::proportional(13.0),
                            fg,
                        );
                        if is_selected {
                            ui.painter().text(
                                rect.right_center() - egui::vec2(12.0, 0.0),
                                egui::Align2::RIGHT_CENTER,
                                "✓",
                                egui::FontId::proportional(13.0),
                                list_fg,
                            );
                        }

                        if resp.clicked() {
                            state.theme_picker.pending_theme_id = Some(id.clone());
                            theme_changed = true;
                        }
                    }
                });
        });

    // Close after selection.
    if theme_changed {
        state.theme_picker.visible = false;
    }
}
