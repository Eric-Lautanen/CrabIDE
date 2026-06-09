//! Keybindings editor panel — view and search all keyboard shortcuts.
//!
//! Renders as a central overlay with a searchable table of all actions
//! and their assigned key combinations.

use crate::state::{cfg_to_egui, UiState};

/// Render the keybindings editor overlay.
pub fn show(ui: &mut egui::Ui, state: &mut UiState) {
    if !state.keybindings_editor.visible {
        return;
    }

    let ctx = ui.ctx();

    // Close on Escape
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.keybindings_editor.visible = false;
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
    let text_fg = cfg_to_egui(state.theme.ui_or(
        "editor.foreground",
        crabide_config::Color::rgb(0xcc, 0xcc, 0xcc),
    ));
    let weak_fg = cfg_to_egui(state.theme.ui_or(
        "editor.foreground",
        crabide_config::Color::rgb(0x88, 0x88, 0x88),
    ));

    let screen = ctx.content_rect();
    let win_w = 520.0_f32.min(screen.width() - 40.0);
    let win_h = 450.0_f32.min(screen.height() - 80.0);
    let win_left = screen.center().x - win_w / 2.0;
    let win_top = screen.top() + 50.0;

    egui::Window::new("##keybindings_editor")
        .id(egui::Id::new("keybindings_editor_window"))
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
                egui::RichText::new("Keyboard Shortcuts")
                    .color(text_fg)
                    .size(13.0)
                    .strong(),
            );
            ui.add_space(6.0);

            // ── Search box ─────────────────────────────────────────────────────
            let mut query = state.keybindings_editor.query.clone();
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
                                egui::RichText::new("Search keybindings...")
                                    .color(egui::Color32::from_rgb(0x88, 0x88, 0x88)),
                            ),
                    );
                    resp.request_focus();
                });
            ui.add_space(4.0);

            // ── Bindings table ─────────────────────────────────────────────────
            let bindings = state.keybindings_editor.bindings.clone();
            let q = query.to_lowercase();
            let filtered: Vec<(String, String)> = if q.is_empty() {
                bindings
            } else {
                bindings
                    .into_iter()
                    .filter(|(label, key)| {
                        label.to_lowercase().contains(&q) || key.to_lowercase().contains(&q)
                    })
                    .collect()
            };

            // ── Header row ────────────────────────────────────────────────────
            let header_bg = cfg_to_egui(state.theme.ui_or(
                "editorLineNumber.foreground",
                crabide_config::Color::rgb(0x33, 0x33, 0x33),
            ));
            let header_h = 22.0;
            let header_rect = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), header_h),
                egui::Sense::hover(),
            );
            ui.painter().rect_filled(header_rect.0, 0.0, header_bg);
            let col_w = (ui.available_width() - 24.0) / 2.0;
            ui.painter().text(
                egui::pos2(header_rect.0.left() + 12.0, header_rect.0.center().y),
                egui::Align2::LEFT_CENTER,
                "Command",
                egui::FontId::proportional(11.0),
                weak_fg,
            );
            ui.painter().text(
                egui::pos2(
                    header_rect.0.left() + 12.0 + col_w,
                    header_rect.0.center().y,
                ),
                egui::Align2::LEFT_CENTER,
                "Keybinding",
                egui::FontId::proportional(11.0),
                weak_fg,
            );

            ui.add_space(2.0);

            // ── Scrollable list ───────────────────────────────────────────────
            egui::ScrollArea::vertical()
                .id_salt("keybindings_editor_list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for (label, key) in filtered.iter() {
                        let row_h = 22.0;
                        let row_rect = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), row_h),
                            egui::Sense::hover(),
                        );
                        let (_rect, resp) = row_rect;

                        if resp.hovered() {
                            let hover_bg = cfg_to_egui(state.theme.ui_or(
                                "list.hoverBackground",
                                crabide_config::Color::rgba(0x2a, 0x2d, 0x2e, 0xff),
                            ));
                            ui.painter().rect_filled(_rect, 0.0, hover_bg);
                        }

                        let col_w = (ui.available_width() - 24.0) / 2.0;
                        ui.painter().text(
                            egui::pos2(_rect.left() + 12.0, _rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            label,
                            egui::FontId::proportional(12.0),
                            text_fg,
                        );
                        if !key.is_empty() {
                            ui.painter().text(
                                egui::pos2(_rect.left() + 12.0 + col_w, _rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                key,
                                egui::FontId::monospace(11.0),
                                weak_fg,
                            );
                        }
                    }
                });

            // Commit query back to state so it persists while panel is open.
            state.keybindings_editor.query = query;
        });
}
