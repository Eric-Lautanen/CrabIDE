//! Tab bar: open document tabs with dirty indicator and close button.

use egui::{vec2, Color32, Rect, Sense};

use crate::state::{cfg_to_egui, UiState};

pub enum TabBarAction {
    Activate(usize),
    Close(usize),
    None,
}

pub fn show(ui: &mut egui::Ui, state: &UiState) -> TabBarAction {
    let mut action = TabBarAction::None;

    let tab_bg_active = cfg_to_egui(
        state
            .theme
            .ui_or("tab.activeBackground", c(0x1e, 0x1e, 0x1e)),
    );
    let tab_bg_inactive = cfg_to_egui(
        state
            .theme
            .ui_or("tab.inactiveBackground", c(0x2d, 0x2d, 0x2d)),
    );
    let fg_active = cfg_to_egui(
        state
            .theme
            .ui_or("tab.activeForeground", c(0xff, 0xff, 0xff)),
    );
    let fg_inactive = cfg_to_egui(
        state
            .theme
            .ui_or("tab.inactiveForeground", c(0x99, 0x99, 0x99)),
    );
    let accent = cfg_to_egui(
        state
            .theme
            .ui_or("tab.activeBorderTop", c(0x00, 0x7a, 0xcc)),
    );
    let header_bg = cfg_to_egui(
        state
            .theme
            .ui_or("editorGroupHeader.tabsBackground", c(0x25, 0x25, 0x26)),
    );
    let divider = Color32::from_rgb(0x3a, 0x3a, 0x3c);

    let bar_height = 35.0_f32;

    let (bar_rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), bar_height), Sense::hover());
    ui.painter().rect_filled(bar_rect, 0.0, header_bg);

    let mut child_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(bar_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );

    egui::ScrollArea::horizontal()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
        .show(&mut child_ui, |ui| {
            for (idx, tab) in state.tabs().iter().enumerate() {
                let is_active = state.active_tab() == Some(idx);
                let tab_bg = if is_active {
                    tab_bg_active
                } else {
                    tab_bg_inactive
                };
                let fg = if is_active { fg_active } else { fg_inactive };

                // Render tab frame
                let resp = egui::Frame::NONE
                    .inner_margin(egui::Margin {
                        left: 14,
                        right: 10,
                        top: 0,
                        bottom: 0,
                    })
                    .fill(tab_bg)
                    .show(ui, |ui| {
                        ui.set_height(bar_height);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 5.0;

                            // File name
                            let name_resp = ui
                                .add(
                                    egui::Label::new(
                                        egui::RichText::new(&tab.title).color(fg).size(13.0),
                                    )
                                    .sense(Sense::click()),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if name_resp.clicked() {
                                action = TabBarAction::Activate(idx);
                            }
                            if name_resp.middle_clicked() {
                                action = TabBarAction::Close(idx);
                            }

                            // Dirty indicator OR close button
                            if tab.is_dirty {
                                // Dot instead of × when dirty
                                let dot_resp = ui
                                    .add(
                                        egui::Label::new(
                                            egui::RichText::new("●").color(accent).size(10.0),
                                        )
                                        .sense(Sense::click()),
                                    )
                                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                                if dot_resp.clicked() {
                                    action = TabBarAction::Close(idx);
                                }
                            } else {
                                // Faint x — only shown on active or hovered tab via alpha
                                let close_color = if is_active {
                                    fg.gamma_multiply(0.5)
                                } else {
                                    Color32::TRANSPARENT
                                };
                                let close_resp = ui
                                    .add(
                                        egui::Label::new(
                                            egui::RichText::new("✕").color(close_color).size(11.0),
                                        )
                                        .sense(Sense::click()),
                                    )
                                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                                if close_resp.clicked() {
                                    action = TabBarAction::Close(idx);
                                }
                                if close_resp.hovered() || name_resp.hovered() {
                                    // Repaint ✕ brighter on hover via second painter call
                                    let r = close_resp.rect;
                                    ui.painter().text(
                                        r.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "✕",
                                        egui::FontId::proportional(11.0),
                                        fg.gamma_multiply(0.7),
                                    );
                                }
                            }
                        });
                    });

                let tab_rect = resp.response.rect;

                // Active tab: blue top accent bar + bottom line to merge with editor
                if is_active {
                    ui.painter().rect_filled(
                        Rect::from_min_size(tab_rect.min, vec2(tab_rect.width(), 2.0)),
                        0.0,
                        accent,
                    );
                    // Erase the bottom border so active tab "connects" to editor
                    ui.painter().rect_filled(
                        Rect::from_min_size(
                            egui::pos2(tab_rect.min.x + 1.0, tab_rect.max.y - 1.0),
                            vec2(tab_rect.width() - 2.0, 1.0),
                        ),
                        0.0,
                        tab_bg_active,
                    );
                }

                // Thin right-edge divider between tabs (not after the last one)
                if idx + 1 < state.tab_count() {
                    ui.painter().rect_filled(
                        Rect::from_min_size(
                            egui::pos2(tab_rect.max.x - 1.0, tab_rect.min.y + 4.0),
                            vec2(1.0, tab_rect.height() - 8.0),
                        ),
                        0.0,
                        divider,
                    );
                }
            }
        });

    action
}

fn c(r: u8, g: u8, b: u8) -> crabide_config::Color {
    crabide_config::Color::rgb(r, g, b)
}
