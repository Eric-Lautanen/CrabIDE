//! Right-click context menu for editor, file explorer, tab bar, and terminal.
//!
//! The menu shows built-in actions (Cut, Copy, Paste, Select All) plus
//! extension-contributed items from `registered_context_menus`.

use crate::state::{ContextMenuAction, UiState, cfg_to_egui};

/// Height of each context menu item row in pixels.
const ITEM_H: f32 = 24.0;
/// Maximum number of items before scrolling.
const MAX_VISIBLE_ITEMS: usize = 20;

/// Render the context menu popup at the stored position.
///
/// Handles activation of built-in actions and extension commands directly.
pub fn show(ui: &mut egui::Ui, state: &mut UiState) {
    if !state.context_menu.visible {
        return;
    }

    // Dismiss on Escape.
    let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
    if escape {
        state.context_menu.visible = false;
        state.context_menu.items.clear();
        return;
    }

    // Dismiss when clicking outside the menu area.
    let clicked = ui.input(|i| i.pointer.primary_pressed());
    if clicked {
        state.context_menu.visible = false;
        state.context_menu.items.clear();
        return;
    }

    if state.context_menu.items.is_empty() {
        state.context_menu.visible = false;
        return;
    }

    let items = state.context_menu.items.clone();
    let n_items = items.len();

    let max_vis = MAX_VISIBLE_ITEMS.min(n_items);
    let popup_h = max_vis as f32 * ITEM_H + 8.0;

    // Theme colors
    let bg = cfg_to_egui(state.theme.ui_or(
        "menu.background",
        crabide_config::Color::rgb(0x2d, 0x2d, 0x2d),
    ));
    let fg = cfg_to_egui(state.theme.ui_or(
        "menu.foreground",
        crabide_config::Color::rgb(0xcc, 0xcc, 0xcc),
    ));
    let sel_bg = cfg_to_egui(state.theme.ui_or(
        "menu.selectionBackground",
        crabide_config::Color::rgba(0x09, 0x4a, 0x71, 0x80),
    ));
    let border = cfg_to_egui(
        state
            .theme
            .ui_or("menu.border", crabide_config::Color::rgb(0x45, 0x45, 0x45)),
    );

    // Clamp position so the menu stays on screen.
    let screen = ui.ctx().content_rect();
    let menu_w = 200.0_f32.min(screen.width() - 20.0);
    let mut mx = state
        .context_menu
        .pos
        .x
        .min(screen.right() - menu_w - 4.0)
        .max(screen.left() + 4.0);
    let mut my = state
        .context_menu
        .pos
        .y
        .min(screen.bottom() - popup_h - 4.0)
        .max(screen.top() + 4.0);

    // Offset so the menu appears to the right of and slightly below the cursor.
    mx += 8.0;
    my += 4.0;

    let cm_pos = egui::pos2(mx, my);

    // Collect pending actions/commands outside the closure to avoid borrow conflicts.
    let mut activated_action: Option<crabide_config::Action> = None;
    let mut activated_command: Option<String> = None;

    egui::Area::new(egui::Id::new("context_menu_popup"))
        .fixed_pos(cm_pos)
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            egui::Frame::default()
                .fill(bg)
                .stroke(egui::Stroke::new(1.0, border))
                .corner_radius(egui::CornerRadius::same(4))
                .show(ui, |ui| {
                    ui.set_min_width(menu_w);
                    ui.set_max_width(menu_w);
                    ui.set_max_height(popup_h);

                    for item in &items {
                        let (rect, resp) = ui
                            .allocate_exact_size(egui::vec2(menu_w, ITEM_H), egui::Sense::click());
                        if !ui.is_rect_visible(rect) {
                            continue;
                        }

                        if resp.hovered() {
                            ui.painter().rect_filled(rect, 2.0, sel_bg);
                        }

                        ui.painter().text(
                            egui::pos2(rect.left() + 8.0, rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            &item.label,
                            egui::FontId::proportional(13.0),
                            fg,
                        );

                        if resp.clicked() {
                            match &item.action {
                                ContextMenuAction::Action(action) => {
                                    activated_action = Some(action.clone());
                                }
                                ContextMenuAction::Command(cmd) => {
                                    activated_command = Some(cmd.clone());
                                }
                            }
                        }
                    }
                });
        });

    // Handle activation (outside the closure).
    if let Some(action) = activated_action {
        state.context_menu.visible = false;
        state.context_menu.items.clear();
        // Handle it internally if possible.
        crate::handle_ui_action(action, state);
    }
    if let Some(cmd) = activated_command {
        state.context_menu.visible = false;
        state.context_menu.items.clear();
        // Enqueue for the app to execute.
        state.extensions_panel.pending_execute_command = Some((cmd, vec![]));
    }
}
