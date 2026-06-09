//! Tab bar: open document tabs with dirty indicator, close button,
//! and drag-and-drop reordering.

use egui::{Color32, Rect, Sense, vec2};

use crate::state::{UiState, cfg_to_egui};

pub enum TabBarAction {
    Activate(usize),
    Close(usize),
    /// Move tab at `from` to position `to` (drop target index).
    MoveTab {
        from: usize,
        to: usize,
    },
    None,
}

/// Drag-and-drop state stored in egui memory between frames.
#[derive(Clone)]
struct TabDragState {
    /// Index of the tab being dragged.
    src_idx: usize,
    /// Current screen X of the pointer.
    current_x: f32,
}

// SAFETY: TabDragState only contains primitive types (usize, f32).
unsafe impl Send for TabDragState {}
unsafe impl Sync for TabDragState {}

const DRAG_ID: &str = "tab_bar_drag";

fn load_drag(ctx: &egui::Context) -> Option<TabDragState> {
    ctx.data_mut(|d| d.get_temp(egui::Id::new(DRAG_ID)))
}

fn save_drag(ctx: &egui::Context, state: TabDragState) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(DRAG_ID), state));
}

fn clear_drag(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<TabDragState>(egui::Id::new(DRAG_ID)));
}

pub fn show(ui: &mut egui::Ui, state: &mut UiState) -> TabBarAction {
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
    let tab_bg_drag = cfg_to_egui(
        state
            .theme
            .ui_or("tab.hoverBackground", c(0x3a, 0x3a, 0x3c)),
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
    let drop_indicator = Color32::from_rgb(0x00, 0x7a, 0xcc);

    let bar_height = 35.0_f32;
    let ctx = ui.ctx().clone();

    let (bar_rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), bar_height), Sense::hover());
    ui.painter().rect_filled(bar_rect, 0.0, header_bg);

    let mut child_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(bar_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );

    // Track accumulated tab X-offsets to compute drop target.
    let mut tab_offsets: Vec<f32> = Vec::new();
    let mut total_width = 0.0_f32;

    let drag_state = load_drag(&ctx);

    egui::ScrollArea::horizontal()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
        .show(&mut child_ui, |ui| {
            for (idx, tab) in state.tabs().iter().enumerate() {
                let is_active = state.active_tab() == Some(idx);
                let is_dragging = drag_state
                    .as_ref()
                    .map(|d| d.src_idx == idx)
                    .unwrap_or(false);
                let tab_bg = if is_dragging {
                    tab_bg_drag
                } else if is_active {
                    tab_bg_active
                } else {
                    tab_bg_inactive
                };
                let fg = if is_active { fg_active } else { fg_inactive };

                // Render tab frame with drag-and-drop sense
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

                            // File name — clickable AND draggable.
                            let name_resp = ui
                                .add(
                                    egui::Label::new(
                                        egui::RichText::new(&tab.title).color(fg).size(13.0),
                                    )
                                    .sense(Sense::click_and_drag()),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand);

                            if name_resp.clicked() {
                                action = TabBarAction::Activate(idx);
                            }
                            if name_resp.middle_clicked() {
                                action = TabBarAction::Close(idx);
                            }

                            // Drag detection
                            if name_resp.drag_started() {
                                save_drag(
                                    &ctx,
                                    TabDragState {
                                        src_idx: idx,
                                        current_x: name_resp.rect.center().x,
                                    },
                                );
                            }

                            if is_dragging {
                                // Update current drag position
                                if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                                    if let Some(ref mut ds) = ctx.data_mut(|d| {
                                        d.get_temp::<TabDragState>(egui::Id::new(DRAG_ID))
                                    }) {
                                        ds.current_x = pos.x;
                                    }
                                }
                            }

                            // Dirty indicator OR close button
                            if tab.is_dirty {
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
                let tab_w = tab_rect.width();

                // Record offset for drop-target computation
                tab_offsets.push(total_width);
                total_width += tab_w;

                // Active tab: blue top accent bar + bottom line to merge with editor
                if is_active {
                    ui.painter().rect_filled(
                        Rect::from_min_size(tab_rect.min, vec2(tab_rect.width(), 2.0)),
                        0.0,
                        accent,
                    );
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

            // ── Drag-drop drop indicator and reorder logic ──────────────────────
            if let Some(ref ds) = drag_state {
                let pointer_x = ds.current_x;
                // Convert to local coordinates relative to bar_rect.
                let rel_x = pointer_x - bar_rect.min.x;
                let n_tabs = state.tab_count();

                // Determine target index from pointer position.
                let target_idx = if rel_x <= 0.0 || n_tabs == 0 {
                    0
                } else if let Some(&last_off) = tab_offsets.last() {
                    if rel_x >= last_off + total_width {
                        n_tabs
                    } else {
                        // Find which gap the pointer is in.
                        let mut t = n_tabs;
                        for (i, &off) in tab_offsets.iter().enumerate() {
                            let next_off = tab_offsets.get(i + 1).copied().unwrap_or(total_width);
                            let mid = (off + next_off) / 2.0;
                            if rel_x < mid {
                                t = i;
                                break;
                            }
                        }
                        t
                    }
                } else {
                    0
                };

                // Draw drop indicator (2px wide vertical bar)
                let indicator_rel_x = if target_idx < tab_offsets.len() {
                    let off = tab_offsets[target_idx];
                    // Draw at the left edge of the target tab.
                    off
                } else {
                    // After the last tab
                    total_width
                };

                let indicator_screen_x = bar_rect.min.x + indicator_rel_x;
                let indicator_rect = Rect::from_min_size(
                    egui::pos2(indicator_screen_x - 1.0, bar_rect.min.y + 4.0),
                    vec2(2.0, bar_rect.height() - 8.0),
                );
                ui.painter()
                    .rect_filled(indicator_rect, 0.0, drop_indicator);

                // Check for drop (drag released)
                let dropped = ui.input(|i| !i.pointer.any_down());
                if dropped {
                    // Compute final target: adjust for source removal if needed.
                    let final_target = if target_idx > ds.src_idx {
                        // When dropping to the right of source, shift left by 1
                        // because the source will be removed first.
                        target_idx - 1
                    } else {
                        target_idx
                    }
                    .min(n_tabs.saturating_sub(1));

                    if final_target != ds.src_idx {
                        action = TabBarAction::MoveTab {
                            from: ds.src_idx,
                            to: final_target,
                        };
                    }
                    clear_drag(&ctx);
                }
            } else {
                // No drag active: clear any stale state on click release.
                if !ui.input(|i| i.pointer.any_down()) {
                    clear_drag(&ctx);
                }
            }
        });

    action
}

fn c(r: u8, g: u8, b: u8) -> crabide_config::Color {
    crabide_config::Color::rgb(r, g, b)
}
