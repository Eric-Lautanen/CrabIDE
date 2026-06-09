//! Debugger bottom panel — call stack, variables, watch, debug console.
//!
//! Rendered as a resizable bottom strip (like the git / terminal panels).
//! When no session is active the panel shows a launch-config picker.

use crabide_config::Action;
use crabide_core::event::OutputCategory;

use super::debug_toolbar;
use crate::state::{UiState, cfg_to_egui};

/// Minimum height of the debug panel in pixels.
pub const MIN_HEIGHT: f32 = 120.0;

/// Render the debug panel and return backend actions.
pub fn show(ui: &mut egui::Ui, state: &mut UiState) -> Vec<Action> {
    let mut actions: Vec<Action> = Vec::new();

    let panel_bg = cfg_to_egui(state.theme.ui_or(
        "sideBar.background",
        crabide_config::Color::rgb(0x25, 0x25, 0x26),
    ));
    let header_fg = cfg_to_egui(state.theme.ui_or(
        "sideBarTitle.foreground",
        crabide_config::Color::rgb(0xbb, 0xbb, 0xbb),
    ));
    let tab_active_fg = cfg_to_egui(state.theme.ui_or(
        "tab.activeForeground",
        crabide_config::Color::rgb(0xff, 0xff, 0xff),
    ));
    let tab_inactive_fg = cfg_to_egui(state.theme.ui_or(
        "tab.inactiveForeground",
        crabide_config::Color::rgb(0x88, 0x88, 0x88),
    ));
    let text_fg = cfg_to_egui(state.theme.ui_or(
        "editor.foreground",
        crabide_config::Color::rgb(0xcc, 0xcc, 0xcc),
    ));
    let accent = egui::Color32::from_rgb(0x4e, 0xc9, 0xb0);

    ui.set_min_height(MIN_HEIGHT);

    // ── Header row ────────────────────────────────────────────────────────────
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(8, 3))
        .fill(panel_bg)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("🐛 DEBUG")
                        .color(header_fg)
                        .size(11.0)
                        .strong(),
                );

                // Session status indicator.
                let dap = &state.dap_panel;
                if dap.session_active {
                    let (status_text, status_color) = if dap.paused {
                        (
                            format!(
                                "⏸ Paused — {}",
                                dap.stop_reason.as_deref().unwrap_or("stopped")
                            ),
                            egui::Color32::from_rgb(0xcc, 0xa7, 0x00),
                        )
                    } else {
                        (
                            "▶ Running".to_owned(),
                            egui::Color32::from_rgb(0x4e, 0xc9, 0xb0),
                        )
                    };
                    ui.label(
                        egui::RichText::new(status_text)
                            .color(status_color)
                            .size(11.0),
                    );
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Toolbar or launch bar.
                    if state.dap_panel.session_active {
                        let toolbar_actions = debug_toolbar::show(ui, state);
                        actions.extend(toolbar_actions);
                    } else {
                        let launch_actions = debug_toolbar::show_launch_bar(ui, state);
                        actions.extend(launch_actions);
                    }
                });
            });
        });

    ui.separator();

    // ── Sub-tab bar ───────────────────────────────────────────────────────────
    let tabs = ["📋 CALL STACK", "📦 VARIABLES", "🔍 WATCH", "💬 CONSOLE"];
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(8, 0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, label) in tabs.iter().enumerate() {
                    let active = state.dap_panel.active_tab == i;
                    let fg = if active {
                        tab_active_fg
                    } else {
                        tab_inactive_fg
                    };
                    let resp = ui.add(
                        egui::Button::new(egui::RichText::new(*label).color(fg).size(11.0))
                            .frame(false)
                            .min_size(egui::vec2(0.0, 20.0)),
                    );
                    if active {
                        // Active tab underline.
                        let r = resp.rect;
                        ui.painter().line_segment(
                            [
                                egui::pos2(r.left(), r.bottom()),
                                egui::pos2(r.right(), r.bottom()),
                            ],
                            egui::Stroke::new(2.0, accent),
                        );
                    }
                    if resp.clicked() {
                        state.dap_panel.active_tab = i;
                    }
                }
            });
        });

    ui.separator();

    // ── Content area ──────────────────────────────────────────────────────────
    let available = ui.available_size();
    egui::ScrollArea::vertical()
        .id_salt("dap_panel_scroll")
        .max_height(available.y)
        .show(ui, |ui| {
            ui.set_width(available.x);
            match state.dap_panel.active_tab {
                0 => show_call_stack(ui, state, text_fg, accent),
                1 => show_variables(ui, state, text_fg),
                2 => show_watch(ui, state, text_fg, &mut actions),
                3 => show_console(ui, state),
                _ => {}
            }
        });

    actions
}

// ── Call stack ────────────────────────────────────────────────────────────────

fn show_call_stack(
    ui: &mut egui::Ui,
    state: &mut UiState,
    text_fg: egui::Color32,
    accent: egui::Color32,
) {
    if !state.dap_panel.session_active {
        ui.label(
            egui::RichText::new("No active debug session")
                .color(egui::Color32::from_rgb(0x66, 0x66, 0x66))
                .size(12.0),
        );
        return;
    }

    if state.dap_panel.call_stack.is_empty() {
        ui.label(
            egui::RichText::new(if state.dap_panel.paused {
                "No frames available"
            } else {
                "Program running…"
            })
            .color(egui::Color32::from_rgb(0x66, 0x66, 0x66))
            .size(12.0),
        );
        return;
    }

    // Collect click results outside the loop to avoid borrow conflicts.
    let mut clicked_frame_id: Option<u64> = None;
    let mut clicked_source_path: Option<std::path::PathBuf> = None;
    let mut clicked_line: Option<usize> = None;

    // Snapshot the parts of dap_panel needed for rendering.
    let frame_data: Vec<(u64, String, Option<std::path::PathBuf>, u32)> = state
        .dap_panel
        .call_stack
        .iter()
        .map(|f| (f.id, f.name.clone(), f.source_path.clone(), f.line))
        .collect();
    let active_frame_id = state.dap_panel.active_frame_id;

    for (frame_id, frame_name, source_path, frame_line) in &frame_data {
        let is_active = active_frame_id == Some(*frame_id);

        let bg = if is_active {
            egui::Color32::from_rgba_unmultiplied(0x4e, 0xc9, 0xb0, 0x22)
        } else {
            egui::Color32::TRANSPARENT
        };

        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 20.0), egui::Sense::click());

        if ui.is_rect_visible(rect) {
            ui.painter().rect_filled(rect, 0.0, bg);
            if resp.hovered() {
                ui.painter().rect_filled(
                    rect,
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(0xff, 0xff, 0xff, 0x0a),
                );
            }

            ui.painter().text(
                rect.left_center() + egui::vec2(8.0, 0.0),
                egui::Align2::LEFT_CENTER,
                frame_name.as_str(),
                egui::FontId::monospace(12.0),
                if is_active { accent } else { text_fg },
            );

            // Source location on the right.
            if let Some(path) = source_path {
                let loc = format!(
                    "{}:{frame_line}",
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                );
                ui.painter().text(
                    rect.right_center() - egui::vec2(8.0, 0.0),
                    egui::Align2::RIGHT_CENTER,
                    loc,
                    egui::FontId::proportional(11.0),
                    egui::Color32::from_rgb(0x66, 0x88, 0xaa),
                );
            }
        }

        if resp.clicked() {
            clicked_frame_id = Some(*frame_id);
            clicked_source_path = source_path.clone();
            clicked_line = Some(frame_line.saturating_sub(1) as usize);
            let _ = frame_line;
        }
    }

    // Apply click results after the loop (borrows released).
    if let Some(frame_id) = clicked_frame_id {
        state.dap_panel.active_frame_id = Some(frame_id);
        state.dap_panel.variables.clear();
        state.dap_panel.pending_expand_var = None;
        state.pending_open_path = clicked_source_path;
        if let Some(line) = clicked_line {
            state.pending_scroll_line = Some(line);
        }
    }
}

// ── Variables ─────────────────────────────────────────────────────────────────

fn show_variables(ui: &mut egui::Ui, state: &mut UiState, text_fg: egui::Color32) {
    let dap = &mut state.dap_panel;

    if dap.variables.is_empty() {
        ui.label(
            egui::RichText::new(if dap.session_active && dap.paused {
                "No variables (select a stack frame)"
            } else if dap.session_active {
                "Program running…"
            } else {
                "No active debug session"
            })
            .color(egui::Color32::from_rgb(0x66, 0x66, 0x66))
            .size(12.0),
        );
        return;
    }

    let mut pending_expand: Option<u64> = None;

    for (scope_ref, vars) in &dap.variables {
        // Section header per scope.
        let scope_label = format!("Scope #{scope_ref}");
        ui.label(
            egui::RichText::new(&scope_label)
                .color(egui::Color32::from_rgb(0x88, 0x88, 0xaa))
                .size(11.0)
                .strong(),
        );

        for var in vars {
            let has_children = var.variables_reference > 0;
            let is_expanded = dap.expanded_var_refs.contains(&var.variables_reference);
            let indent = 8.0;

            let (rect, resp) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 18.0), egui::Sense::click());

            if ui.is_rect_visible(rect) {
                if resp.hovered() {
                    ui.painter().rect_filled(
                        rect,
                        0.0,
                        egui::Color32::from_rgba_unmultiplied(0xff, 0xff, 0xff, 0x0a),
                    );
                }

                let mut x = rect.left() + indent;

                // Expand arrow.
                if has_children {
                    let arrow = if is_expanded { "▾" } else { "▸" };
                    ui.painter().text(
                        egui::pos2(x, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        arrow,
                        egui::FontId::proportional(10.0),
                        egui::Color32::from_rgb(0x88, 0x88, 0x88),
                    );
                }
                x += 12.0;

                // Name.
                let name_width = 140.0_f32.min(rect.width() * 0.35);
                ui.painter().text(
                    egui::pos2(x, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    &var.name,
                    egui::FontId::monospace(12.0),
                    egui::Color32::from_rgb(0x9c, 0xdc, 0xfe),
                );
                x += name_width;

                // Type (optional, dim).
                if let Some(ty) = &var.type_name {
                    ui.painter().text(
                        egui::pos2(x, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        format!("{ty} "),
                        egui::FontId::proportional(11.0),
                        egui::Color32::from_rgb(0x55, 0x88, 0x99),
                    );
                    x += 60.0_f32.min(rect.right() - x - 60.0).max(0.0);
                }

                // Value.
                let value_str = if var.value.len() > 80 {
                    format!("{}…", &var.value[..80])
                } else {
                    var.value.clone()
                };
                ui.painter().text(
                    egui::pos2(x, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    value_str,
                    egui::FontId::monospace(12.0),
                    text_fg,
                );
            }

            if resp.clicked() && has_children {
                if is_expanded {
                    dap.expanded_var_refs.remove(&var.variables_reference);
                } else {
                    dap.expanded_var_refs.insert(var.variables_reference);
                    pending_expand = Some(var.variables_reference);
                }
            }
        }
    }

    if let Some(var_ref) = pending_expand {
        dap.pending_expand_var = Some(var_ref);
    }
}

// ── Watch ─────────────────────────────────────────────────────────────────────

fn show_watch(
    ui: &mut egui::Ui,
    state: &mut UiState,
    text_fg: egui::Color32,
    _actions: &mut Vec<Action>,
) {
    let dap = &mut state.dap_panel;

    // Existing watch expressions.
    let mut to_remove: Option<usize> = None;
    for (i, expr) in dap.watch_expressions.iter().enumerate() {
        ui.horizontal(|ui| {
            let (rect, resp) = ui.allocate_exact_size(
                egui::vec2(ui.available_width() - 24.0, 18.0),
                egui::Sense::hover(),
            );
            if ui.is_rect_visible(rect) {
                ui.painter().text(
                    rect.left_center() + egui::vec2(4.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    expr,
                    egui::FontId::monospace(12.0),
                    text_fg,
                );
            }
            let _ = resp;

            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("×")
                            .color(egui::Color32::from_rgb(0x88, 0x88, 0x88))
                            .size(11.0),
                    )
                    .frame(false)
                    .min_size(egui::vec2(20.0, 18.0)),
                )
                .clicked()
            {
                to_remove = Some(i);
            }
        });
    }
    if let Some(i) = to_remove {
        dap.watch_expressions.remove(i);
    }

    // Input row.
    ui.horizontal(|ui| {
        let input_bg = egui::Color32::from_rgba_unmultiplied(0xff, 0xff, 0xff, 0x0a);
        let resp = egui::Frame::NONE.fill(input_bg).show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut dap.watch_input)
                    .font(egui::TextStyle::Monospace)
                    .hint_text(
                        egui::RichText::new("Add watch expression…")
                            .color(egui::Color32::from_rgb(0x55, 0x55, 0x55)),
                    )
                    .desired_width(ui.available_width() - 40.0)
                    .frame(egui::Frame::NONE),
            )
        });
        let enter = resp.inner.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let add_clicked = ui
            .add(
                egui::Button::new(
                    egui::RichText::new("+")
                        .color(egui::Color32::from_rgb(0x4e, 0xc9, 0xb0))
                        .size(13.0),
                )
                .frame(false)
                .min_size(egui::vec2(20.0, 20.0)),
            )
            .clicked();

        if (enter || add_clicked) && !dap.watch_input.trim().is_empty() {
            let expr = dap.watch_input.trim().to_owned();
            dap.watch_expressions.push(expr);
            dap.watch_input.clear();
        }
    });
}

// ── Debug console ─────────────────────────────────────────────────────────────

fn show_console(ui: &mut egui::Ui, state: &mut UiState) {
    let dap = &mut state.dap_panel;

    if dap.console_lines.is_empty() {
        ui.label(
            egui::RichText::new("Debug console output will appear here.")
                .color(egui::Color32::from_rgb(0x55, 0x55, 0x55))
                .size(12.0),
        );
        return;
    }

    let scroll = egui::ScrollArea::vertical()
        .id_salt("dap_console_scroll")
        .stick_to_bottom(dap.console_scroll_to_bottom)
        .show(ui, |ui| {
            for (category, line) in &dap.console_lines {
                let color = match category {
                    OutputCategory::Stderr => egui::Color32::from_rgb(0xf4, 0x88, 0x88),
                    OutputCategory::Important => egui::Color32::from_rgb(0xff, 0xcc, 0x00),
                    OutputCategory::Stdout => egui::Color32::from_rgb(0xcc, 0xcc, 0xcc),
                    _ => egui::Color32::from_rgb(0x99, 0xbb, 0xcc),
                };
                ui.label(
                    egui::RichText::new(line)
                        .color(color)
                        .size(12.0)
                        .monospace(),
                );
            }
        });
    // Once the user scrolls up, stop auto-scrolling.
    if scroll.inner_rect.top() > 4.0 {
        dap.console_scroll_to_bottom = false;
    }
}
