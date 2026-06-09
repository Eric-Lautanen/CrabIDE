//! Settings editor panel — visual editor for `settings.toml`.
//!
//! Renders as a central overlay with grouped settings fields.

use crate::state::{SettingsFieldType, UiState, cfg_to_egui};

/// Render the settings editor overlay.
pub fn show(ui: &mut egui::Ui, state: &mut UiState) {
    if !state.settings_panel.visible {
        return;
    }

    let ctx = ui.ctx();

    // Close on Escape
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.settings_panel.visible = false;
        return;
    }

    let drop_bg = cfg_to_egui(state.theme.ui_or(
        "dropdown.background",
        crabide_config::Color::rgb(0x3c, 0x3c, 0x3c),
    ));
    let _input_bg = cfg_to_egui(state.theme.ui_or(
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
    let label_fg = cfg_to_egui(state.theme.ui_or(
        "editor.foreground",
        crabide_config::Color::rgb(0xaa, 0xaa, 0xaa),
    ));
    let section_bg = cfg_to_egui(state.theme.ui_or(
        "sideBarSectionHeader.background",
        crabide_config::Color::rgba(0x00, 0x00, 0x00, 0x33),
    ));

    let screen = ctx.content_rect();
    let win_w = 520.0_f32.min(screen.width() - 40.0);
    let win_h = 480.0_f32.min(screen.height() - 80.0);
    let win_left = screen.center().x - win_w / 2.0;
    let win_top = screen.top() + 40.0;

    egui::Window::new("##settings_panel")
        .id(egui::Id::new("settings_panel_window"))
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
                egui::RichText::new("Settings")
                    .color(text_fg)
                    .size(14.0)
                    .strong(),
            );
            ui.add_space(8.0);

            let mut changed = false;

            // ── Settings groups ─────────────────────────────────────────────────
            let fields = state.settings_panel.fields.clone();
            let mut groups: std::collections::BTreeMap<&str, Vec<usize>> =
                std::collections::BTreeMap::new();
            for (i, f) in fields.iter().enumerate() {
                groups.entry(f.group.as_str()).or_default().push(i);
            }

            egui::ScrollArea::vertical()
                .id_salt("settings_panel_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    for (group_name, indices) in &groups {
                        // ── Section header ──────────────────────────────────────
                        let header_h = 24.0;
                        let header_rect = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), header_h),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(header_rect.0, 0.0, section_bg);
                        ui.painter().text(
                            egui::pos2(header_rect.0.left() + 12.0, header_rect.0.center().y),
                            egui::Align2::LEFT_CENTER,
                            *group_name,
                            egui::FontId::proportional(12.0),
                            text_fg,
                        );
                        ui.add_space(4.0);

                        for &idx in indices {
                            let field = &fields[idx];
                            let row_h = 28.0;

                            ui.horizontal(|ui| {
                                ui.add_space(16.0);
                                ui.set_height(row_h);

                                // Label
                                let _label_w = 140.0_f32.min(ui.available_width() * 0.35);
                                ui.label(
                                    egui::RichText::new(&field.key).color(label_fg).size(12.0),
                                )
                                .on_hover_text(&field.key);

                                // Value control
                                let val_w = ui.available_width().max(60.0);
                                match field.field_type {
                                    SettingsFieldType::Bool => {
                                        let mut val = field.value == "true";
                                        if ui
                                            .add_sized(
                                                egui::vec2(80.0, 20.0),
                                                egui::Checkbox::without_text(&mut val),
                                            )
                                            .changed()
                                        {
                                            let new_val = if val { "true" } else { "false" };
                                            if let Some(f) =
                                                state.settings_panel.fields.iter_mut().find(|f| {
                                                    f.group == field.group && f.key == field.key
                                                })
                                            {
                                                f.value = new_val.to_owned();
                                                changed = true;
                                            }
                                        }
                                    }
                                    SettingsFieldType::Int => {
                                        let mut val = field.value.parse::<i64>().unwrap_or(0);
                                        if ui
                                            .add_sized(
                                                egui::vec2(80.0, 20.0),
                                                egui::DragValue::new(&mut val)
                                                    .range(i64::MIN..=i64::MAX)
                                                    .speed(1.0),
                                            )
                                            .changed()
                                        {
                                            if let Some(f) =
                                                state.settings_panel.fields.iter_mut().find(|f| {
                                                    f.group == field.group && f.key == field.key
                                                })
                                            {
                                                f.value = val.to_string();
                                                changed = true;
                                            }
                                        }
                                    }
                                    SettingsFieldType::Float => {
                                        let mut val = field.value.parse::<f64>().unwrap_or(0.0);
                                        if ui
                                            .add_sized(
                                                egui::vec2(80.0, 20.0),
                                                egui::DragValue::new(&mut val).speed(0.1),
                                            )
                                            .changed()
                                        {
                                            if let Some(f) =
                                                state.settings_panel.fields.iter_mut().find(|f| {
                                                    f.group == field.group && f.key == field.key
                                                })
                                            {
                                                f.value = format!("{val:.1}");
                                                changed = true;
                                            }
                                        }
                                    }
                                    SettingsFieldType::String => {
                                        let mut val = field.value.clone();
                                        let resp = ui.add_sized(
                                            egui::vec2(val_w, 20.0),
                                            egui::TextEdit::singleline(&mut val)
                                                .font(egui::TextStyle::Monospace)
                                                .text_color(input_fg)
                                                .frame(egui::Frame::NONE)
                                                .desired_width(f32::INFINITY),
                                        );
                                        if resp.changed() || resp.lost_focus() {
                                            if let Some(f) =
                                                state.settings_panel.fields.iter_mut().find(|f| {
                                                    f.group == field.group && f.key == field.key
                                                })
                                            {
                                                f.value = val;
                                                changed = true;
                                            }
                                        }
                                    }
                                    SettingsFieldType::Enum(ref options) => {
                                        let current = field.value.clone();
                                        let mut selected =
                                            options.iter().position(|o| o == &current).unwrap_or(0);
                                        egui::ComboBox::from_id_salt((
                                            "settings_enum",
                                            &field.group,
                                            &field.key,
                                        ))
                                        .selected_text(&current)
                                        .show_ui(
                                            ui,
                                            |ui| {
                                                for (ei, opt) in options.iter().enumerate() {
                                                    ui.selectable_value(
                                                        &mut selected,
                                                        ei,
                                                        opt.as_str(),
                                                    );
                                                }
                                            },
                                        );
                                        if options.get(selected).is_some_and(|o| o != &current) {
                                            if let Some(f) =
                                                state.settings_panel.fields.iter_mut().find(|f| {
                                                    f.group == field.group && f.key == field.key
                                                })
                                            {
                                                f.value = options[selected].clone();
                                                changed = true;
                                            }
                                        }
                                    }
                                }
                            });
                            ui.add_space(2.0);
                        }
                        ui.add_space(6.0);
                    }
                });

            // ── Save button ─────────────────────────────────────────────────
            if changed {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(ui.available_width() - 100.0);
                    if ui
                        .add_sized(
                            egui::vec2(100.0, 26.0),
                            egui::Button::new(
                                egui::RichText::new("Save").color(egui::Color32::WHITE),
                            )
                            .fill(cfg_to_egui(state.theme.ui_or(
                                "activityBarBadge.background",
                                crabide_config::Color::rgb(0x00, 0x7a, 0xcc),
                            ))),
                        )
                        .clicked()
                    {
                        state.settings_panel.visible = false;
                    }
                });
            }
        });
}
