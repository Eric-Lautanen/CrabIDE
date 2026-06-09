//! Output panel — bottom strip showing text output from tasks, build, extensions.
//!
//! Supports multiple named output channels with a dropdown selector.
//! Each channel accumulates lines (capped at 5000) and auto-scrolls by default.

use crabide_config::Color;

use crate::state::{UiState, cfg_to_egui};

pub const MIN_HEIGHT: f32 = 80.0;

fn c(r: u8, g: u8, b: u8) -> Color {
    Color::rgb(r, g, b)
}

/// Render the Output panel inside the provided `ui`.
pub fn show(ui: &mut egui::Ui, state: &mut UiState) {
    let bg = cfg_to_egui(state.theme.ui_or("sideBar.background", c(0x25, 0x25, 0x26)));
    let fg = cfg_to_egui(state.theme.ui_or("editor.foreground", c(0xcc, 0xcc, 0xcc)));
    let muted = cfg_to_egui(
        state
            .theme
            .ui_or("tab.inactiveForeground", c(0x85, 0x85, 0x85)),
    );
    let hov_bg = cfg_to_egui(
        state
            .theme
            .ui_or("list.hoverBackground", c(0x2a, 0x2d, 0x2e)),
    );
    let border = cfg_to_egui(state.theme.ui_or("panel.border", c(0x3a, 0x3a, 0x3c)));

    ui.painter()
        .rect_filled(ui.available_rect_before_wrap(), 0.0, bg);

    let (_channel, lines) = {
        let panel = &state.output_panel;
        let channel = if panel.active_channel.is_empty() && !panel.channels.is_empty() {
            panel.channels[0].clone()
        } else {
            panel.active_channel.clone()
        };
        let lines = panel
            .channel_lines
            .get(&channel)
            .cloned()
            .unwrap_or_default();
        (channel, lines)
    };

    // ── Header row: channel dropdown + auto-scroll toggle ────────────────
    let header_h = 24.0;
    let header_rect = egui::Rect::from_min_size(
        ui.available_rect_before_wrap().min,
        egui::vec2(ui.available_width(), header_h),
    );
    ui.painter().rect_filled(header_rect, 0.0, bg);
    ui.painter().line(
        vec![
            egui::pos2(header_rect.min.x, header_rect.max.y),
            egui::pos2(header_rect.max.x, header_rect.max.y),
        ],
        (1.0, border),
    );

    ui.set_height(header_h);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;

        // ── Channel selector ──────────────────────────────────────────────
        let panel = &mut state.output_panel;
        let current = if panel.active_channel.is_empty() && !panel.channels.is_empty() {
            panel.channels[0].clone()
        } else {
            panel.active_channel.clone()
        };

        egui::ComboBox::from_id_salt("output_channel_selector")
            .selected_text(egui::RichText::new(&current).color(fg).size(12.0))
            .width(180.0)
            .show_ui(ui, |ui| {
                for ch in panel.channels.clone() {
                    let selected = ch == current;
                    let label_bg = if selected {
                        hov_bg
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    let (rect, resp) =
                        ui.allocate_exact_size(egui::vec2(180.0, 20.0), egui::Sense::click());
                    if !ui.is_rect_visible(rect) {
                        continue;
                    }
                    ui.painter().rect_filled(rect, 0.0, label_bg);
                    ui.painter().text(
                        egui::pos2(rect.left() + 4.0, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        &ch,
                        egui::FontId::proportional(12.0),
                        if selected { egui::Color32::WHITE } else { fg },
                    );
                    if resp.clicked() {
                        panel.active_channel = ch;
                    }
                }
            });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // ── Clear button ──────────────────────────────────────────────
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("Clear").color(fg).size(11.0))
                        .fill(bg)
                        .stroke(egui::Stroke::new(1.0, border))
                        .corner_radius(2.0),
                )
                .clicked()
            {
                let channel = if panel.active_channel.is_empty() && !panel.channels.is_empty() {
                    panel.channels[0].clone()
                } else {
                    panel.active_channel.clone()
                };
                panel.channel_lines.entry(channel).or_default().clear();
            }

            // ── Auto-scroll toggle ─────────────────────────────────────────
            let scroll_label = if panel.auto_scroll {
                "Auto-scroll: ON"
            } else {
                "Auto-scroll: OFF"
            };
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(scroll_label)
                            .color(if panel.auto_scroll {
                                egui::Color32::WHITE
                            } else {
                                muted
                            })
                            .size(11.0),
                    )
                    .fill(bg)
                    .stroke(egui::Stroke::new(1.0, border))
                    .corner_radius(2.0),
                )
                .clicked()
            {
                panel.auto_scroll = !panel.auto_scroll;
            }
        });
    });

    // ── Output lines ──────────────────────────────────────────────────────
    let scroll_to_bottom = state.output_panel.scroll_to_bottom;
    let lines_clone = lines.clone();
    let mut scroll_area = egui::ScrollArea::vertical()
        .id_salt("output_panel_scroll")
        .auto_shrink([false, false]);
    if scroll_to_bottom {
        // Set scroll offset to a very large value to force bottom.
        scroll_area = scroll_area.scroll_offset(egui::vec2(0.0, f32::MAX));
    }
    scroll_area.show(ui, |ui| {
        for line in &lines_clone {
            ui.add(egui::Label::new(
                egui::RichText::new(line)
                    .color(fg)
                    .size(12.0)
                    .family(egui::FontFamily::Monospace),
            ));
        }
    });

    if scroll_to_bottom {
        state.output_panel.scroll_to_bottom = false;
    }
}
