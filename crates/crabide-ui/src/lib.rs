#![warn(clippy::pedantic)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::doc_markdown,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::struct_excessive_bools,
    clippy::similar_names,
    clippy::assigning_clones,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    clippy::collapsible_else_if,
    clippy::default_trait_access,
    clippy::explicit_iter_loop,
    clippy::float_cmp,
    clippy::fn_params_excessive_bools,
    clippy::format_collect,
    clippy::format_push_string,
    clippy::if_not_else,
    clippy::items_after_statements,
    clippy::manual_let_else,
    clippy::many_single_char_names,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::match_wildcard_for_single_variants,
    clippy::needless_continue,
    clippy::needless_pass_by_value,
    clippy::redundant_closure,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_else,
    clippy::return_self_not_must_use,
    clippy::semicolon_if_nothing_returned,
    clippy::too_many_lines,
    clippy::trivially_copy_pass_by_ref,
    clippy::uninlined_format_args,
    clippy::unnecessary_debug_formatting,
    clippy::unnecessary_map_or,
    clippy::unnecessary_wraps,
    clippy::unused_self,
    clippy::used_underscore_binding,
    clippy::wildcard_imports
)]
//! `crabide-ui` — all egui panels, widgets, and UI state for crabide Editor.
//!
//! # Entry point
//!
//! The app crate calls [`render`] once per egui frame:
//! ```ignore
//! let actions = crabide_ui::render(&ctx, &mut ui_state);
//! for action in actions { app.handle_action(action); }
//! ```
//!
//! # Modules
//! - [`state`]   — `UiState`, `EditorTab`, `FileExplorerState`, …
//! - [`layout`]  — `PaneKind`, `UiBehavior`, egui_tiles docking layout
//! - [`palette`] — Ctrl+Shift+P command palette (nucleo fuzzy search)
//! - [`panels`]  — per-surface rendering (`editor`, `gutter`, `tab_bar`, …)

pub mod layout;
pub mod palette;
pub mod panels;
pub mod state;

pub use layout::PaneKind;
pub use state::{
    BreadcrumbSegment, ContextMenuAction, ContextMenuContext, ContextMenuItem, ContextMenuState,
    DapPanelState, DisplayCell, EditorTab, ExtensionPanelUiState, ExtensionsPanelState,
    ExtensionsPanelTab, FileExplorerState, FileNode, FrameProfiler, GitDecoration, GitPanelState,
    KeybindingsEditorState, LspLatencyTracker, LspStatus, OutputPanelState, PeekKind, PeekState,
    SettingsField, SettingsFieldType, SettingsPanelState, SidebarPaneUiState, SidebarTab,
    SymbolOutlineEntry, SymbolOutlineState, TerminalInstance, TerminalPanelState, ThemePickerState,
    UiState, cfg_to_egui,
};

use std::sync::Arc;

use crabide_config::{Action, Key, KeyChord, Modifiers};
use crabide_core::types::Position;

// ── render ────────────────────────────────────────────────────────────────────

/// Render the complete editor UI for one egui frame.
///
/// Must be called from the egui `App::update` method. The returned `Vec`
/// contains **backend actions** that require app-level work (e.g. saving a
/// file, opening a URI, triggering LSP). UI-internal actions (palette toggle,
/// sidebar toggle, tab navigation, zoom, cursor movement) are handled inside
/// this function.
pub fn render(ui: &mut egui::Ui, state: &mut UiState) -> Vec<Action> {
    let ctx = ui.ctx().clone();
    let mut actions: Vec<Action> = Vec::new();

    // Caret blink
    // Only schedule repaints for the caret when a document is open. When idle
    // (no open tabs) the app should sleep until the user interacts.
    let now = ctx.input(|i| i.time);
    let terminal_active =
        state.terminal.visible && state.terminal.has_focus && !state.terminal.instances.is_empty();
    if state.active_tab().is_some() || terminal_active {
        state.tick_caret(now);
        ctx.request_repaint_after(std::time::Duration::from_millis(530));
    }

    // ── Terminal focus management ─────────────────────────────────────────────
    // If the user clicks somewhere that isn't the terminal, lose terminal focus.
    // The terminal panel itself sets has_focus = true when clicked.
    // Terminal focus is set by clicking inside the panel; lost by clicking the
    // editor area (the editor panel clears it when it receives a click).

    // ── Expire timed status messages ──────────────────────────────────────────
    state.expire_status();

    // ── Global keyboard routing ───────────────────────────────────────────────
    let key_actions = process_keyboard(&ctx, state);
    for action in key_actions {
        if !handle_ui_action(action.clone(), state) {
            actions.push(action);
        }
    }

    // ── Command palette ───────────────────────────────────────────────────────
    let registry = state.action_registry.clone();
    if let Some(confirmed) = palette::show(&ctx, state, &registry) {
        if !handle_ui_action(confirmed.clone(), state) {
            actions.push(confirmed);
        }
    }

    // ── Fuzzy file finder (Ctrl+P) ────────────────────────────────────────────
    if let Some(open_action) = panels::fuzzy_finder::show(&ctx, state) {
        actions.push(open_action);
    }

    // Go-to-line dialog (Ctrl+G)
    if let Some(goto_action) = show_goto_line(ui, state) {
        actions.push(goto_action);
    }

    // ── Symbol outline (Ctrl+Shift+O) ─────────────────────────────────────────
    if let Some(sym_action) = panels::symbol_outline::show(&ctx, state) {
        actions.push(sym_action);
    }

    // ── Find / replace floating window ────────────────────────────────────────
    panels::find_replace::show(&ctx, state, &mut actions);

    // ── Status bar (bottom — must register before git panel) ─────────────────
    actions.extend(panels::status_bar::show(ui, state));

    // Terminal panel (bottom strip, above git panel and status bar)
    if state.terminal.visible {
        egui::Panel::bottom("terminal_panel")
            .min_size(panels::terminal_panel::MIN_HEIGHT)
            .max_size(600.0)
            .resizable(true)
            .frame(egui::Frame::NONE.fill(cfg_to_egui(state.theme.ui_or(
                "terminal.background",
                crabide_config::Color::rgb(0x1a, 0x1a, 0x1a),
            ))))
            .show_inside(ui, |ui| {
                let term_actions = panels::terminal_panel::show(ui, state);
                for a in term_actions {
                    actions.push(a);
                }
            });
        // Request repaint at ~30 fps while terminal is visible to animate cursor
        // blink and show new output.
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }

    // ── Git panel (bottom strip above status bar) ─────────────────────────────
    if state.git_panel.visible {
        let panel_bg = cfg_to_egui(state.theme.ui_or(
            "sideBar.background",
            crabide_config::Color::rgb(0x25, 0x25, 0x26),
        ));
        egui::Panel::bottom("git_panel")
            .min_size(120.0)
            .max_size(400.0)
            .resizable(true)
            .frame(egui::Frame::NONE.fill(panel_bg))
            .show_inside(ui, |ui| {
                panels::git_panel::show(ui, state);
            });
    }

    // ── Problems panel (bottom strip, above git panel) ───────────────────────
    if state.problems_panel_open {
        let panel_bg = cfg_to_egui(state.theme.ui_or(
            "sideBar.background",
            crabide_config::Color::rgb(0x25, 0x25, 0x26),
        ));
        egui::Panel::bottom("problems_panel")
            .min_size(panels::problems_panel::MIN_HEIGHT)
            .max_size(350.0)
            .resizable(true)
            .frame(egui::Frame::NONE.fill(panel_bg))
            .show_inside(ui, |ui| {
                panels::problems_panel::show(ui, state);
            });
    }

    // ── Output panel (bottom strip, shows task/extension output) ──────────────
    if state.output_panel.visible {
        let panel_bg = cfg_to_egui(state.theme.ui_or(
            "sideBar.background",
            crabide_config::Color::rgb(0x25, 0x25, 0x26),
        ));
        egui::Panel::bottom("output_panel")
            .min_size(panels::output_panel::MIN_HEIGHT)
            .max_size(350.0)
            .resizable(true)
            .frame(egui::Frame::NONE.fill(panel_bg))
            .show_inside(ui, |ui| {
                panels::output_panel::show(ui, state);
            });
    }

    // ── Dynamic extension bottom panels ──────────────────────────────────────
    // Extensions register panels via PanelRegistration; the editor renders them
    // here without any hardcoded coupling to specific extension types.
    {
        let panel_bg = cfg_to_egui(state.theme.ui_or(
            "sideBar.background",
            crabide_config::Color::rgb(0x25, 0x25, 0x26),
        ));
        // Collect ids to avoid borrow issues.
        let bottom_ids: Vec<String> = state
            .extension_panels
            .values()
            .filter(|p| {
                p.open
                    && matches!(
                        p.registration.location,
                        crabide_extensions::PanelLocation::Bottom
                    )
            })
            .map(|p| p.registration.id.clone())
            .collect();
        for panel_id in bottom_ids {
            if let Some(panel) = state.extension_panels.get(&panel_id) {
                let min_h = panel.registration.min_size;
                let def_h = panel.registration.default_size;
                let egui_id = egui::Id::new(("ext_panel_bottom", panel_id.clone()));
                let title = panel.registration.title.clone();
                let content: Vec<crabide_extensions::ContentBlock> = panel.content.clone();
                egui::Panel::bottom(egui_id)
                    .min_size(min_h)
                    .default_size(def_h)
                    .max_size(500.0)
                    .resizable(true)
                    .frame(egui::Frame::NONE.fill(panel_bg))
                    .show_inside(ui, |ui| {
                        if let Some(nav) =
                            render_extension_panel(ui, state, &title, &panel_id, &content)
                        {
                            state.pending_navigate = Some(nav);
                        }
                    });
            }
        }
    }

    // ── Debug panel (bottom strip, above git panel) ───────────────────────────
    if state.dap_panel.visible && state.dap_panel.enabled {
        let panel_bg = cfg_to_egui(state.theme.ui_or(
            "sideBar.background",
            crabide_config::Color::rgb(0x25, 0x25, 0x26),
        ));
        egui::Panel::bottom("dap_panel")
            .min_size(panels::debug_panel::MIN_HEIGHT)
            .max_size(500.0)
            .resizable(true)
            .frame(egui::Frame::NONE.fill(panel_bg))
            .show_inside(ui, |ui| {
                let dap_actions = panels::debug_panel::show(ui, state);
                for a in dap_actions {
                    if !handle_ui_action(a.clone(), state) {
                        actions.push(a);
                    }
                }
            });
        // Repaint frequently while a session is active so the UI stays responsive.
        if state.dap_panel.session_active {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    // ── Menu bar (top) ────────────────────────────────────────────────────────
    // Collect menu actions into a temporary vec and route them through
    // handle_ui_action, just like keyboard actions.  Without this, any action
    // that is handled internally by the UI layer (Find, SelectAll, GotoLine,
    // ToggleWordWrap, FuzzyFindFile, …) is silently discarded or logged as
    // "unhandled action" by the app.
    let mut menu_actions: Vec<Action> = Vec::new();
    show_menu_bar(ui, state, &mut menu_actions);
    for action in menu_actions {
        if !handle_ui_action(action.clone(), state) {
            actions.push(action);
        }
    }

    // ── Dynamic extension right panels ───────────────────────────────────────
    {
        let panel_bg = cfg_to_egui(state.theme.ui_or(
            "sideBar.background",
            crabide_config::Color::rgb(0x25, 0x25, 0x26),
        ));
        let right_ids: Vec<String> = state
            .extension_panels
            .values()
            .filter(|p| {
                p.open
                    && matches!(
                        p.registration.location,
                        crabide_extensions::PanelLocation::Right
                    )
            })
            .map(|p| p.registration.id.clone())
            .collect();
        for panel_id in right_ids {
            if let Some(panel) = state.extension_panels.get(&panel_id) {
                let min_w = panel.registration.min_size;
                let def_w = panel.registration.default_size;
                let egui_id = egui::Id::new(("ext_panel_right", panel_id.clone()));
                let title = panel.registration.title.clone();
                let content: Vec<crabide_extensions::ContentBlock> = panel.content.clone();
                egui::Panel::right(egui_id)
                    .min_size(min_w)
                    .default_size(def_w)
                    .resizable(true)
                    .frame(egui::Frame::NONE.fill(panel_bg))
                    .show_inside(ui, |ui| {
                        if let Some(nav) =
                            render_extension_panel(ui, state, &title, &panel_id, &content)
                        {
                            state.pending_navigate = Some(nav);
                        }
                    });
            }
        }
    }

    // Main area: egui_tiles docking layout
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(cfg_to_egui(state.theme.ui_or(
            "editor.background",
            crabide_config::Color::rgb(0x1e, 0x1e, 0x1e),
        ))))
        .show_inside(ui, |ui| {
            if state.sidebar_visible {
                let mut behavior = layout::UiBehavior {
                    state,
                    actions: &mut actions,
                };
                let mut layout = egui_tiles::Tree::<PaneKind>::empty("_swap");
                std::mem::swap(&mut layout, &mut behavior.state.layout);
                layout.ui(&mut behavior, ui);
                std::mem::swap(&mut layout, &mut behavior.state.layout);
            } else {
                panels::editor::show(ui, state, &mut actions);
            }
        });

    // ── Theme picker overlay ─────────────────────────────────────────────
    panels::theme_picker::show(ui, state);

    // ── Keybindings editor overlay ───────────────────────────────────────
    panels::keybindings_editor::show(ui, state);

    // ── Settings panel overlay ───────────────────────────────────────────
    panels::settings_panel::show(ui, state);

    // ── Performance profiler overlay (Ctrl+Shift+`) toggle ──────────────
    if state.profiler.visible {
        show_profiler_overlay(ui, state);
    }

    actions
}

// ── Go-to-line dialog ─────────────────────────────────────────────────────────

/// Render the Ctrl+G go-to-line floating dialog.
///
/// Returns `Some(Action::GotoLine)` when the user confirms, which the app
/// handles by moving the cursor and scrolling.
fn show_goto_line(ui: &mut egui::Ui, state: &mut UiState) -> Option<Action> {
    if !state.goto_line.visible {
        return None;
    }

    let ctx = ui.ctx();
    let (enter, escape) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::Enter),
            i.key_pressed(egui::Key::Escape),
        )
    });

    if escape {
        state.goto_line.visible = false;
        state.goto_line.query.clear();
        ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
        return None;
    }

    let input_bg = cfg_to_egui(state.theme.ui_or(
        "input.background",
        crabide_config::Color::rgb(0x3c, 0x3c, 0x3c),
    ));
    let input_fg = cfg_to_egui(state.theme.ui_or(
        "input.foreground",
        crabide_config::Color::rgb(0xcc, 0xcc, 0xcc),
    ));
    let drop_bg = cfg_to_egui(state.theme.ui_or(
        "dropdown.background",
        crabide_config::Color::rgb(0x3c, 0x3c, 0x3c),
    ));

    let mut query = state.goto_line.query.clone();
    let max_lines = state.active_tab_ref().map_or(0, |t| t.lines.len());
    let hint = format!("Line number (1-{max_lines})");

    let screen = ctx.content_rect();
    let win_width = 280.0_f32.min(screen.width() - 40.0);
    let win_left = screen.center().x - win_width / 2.0;
    let win_top = screen.top() + 60.0;

    egui::Window::new("##goto_line")
        .id(egui::Id::new("goto_line_window"))
        .title_bar(false)
        .resizable(false)
        .movable(false)
        .frame(
            egui::Frame::default()
                .fill(drop_bg)
                .corner_radius(egui::CornerRadius::same(4)),
        )
        .fixed_pos(egui::pos2(win_left, win_top))
        .fixed_size(egui::vec2(win_width, 0.0))
        .show(ctx, |ui| {
            ui.set_width(win_width);
            egui::Frame::default()
                .fill(input_bg)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.set_width(win_width - 20.0);
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut query)
                            .font(egui::TextStyle::Monospace)
                            .text_color(input_fg)
                            .frame(egui::Frame::NONE)
                            .desired_width(f32::INFINITY)
                            .hint_text(
                                egui::RichText::new(hint)
                                    .color(egui::Color32::from_rgb(0x88, 0x88, 0x88)),
                            ),
                    );
                    resp.request_focus();
                });
        });

    state.goto_line.query = query;

    if enter {
        let action = Some(Action::GotoLine);
        state.goto_line.visible = false;
        ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
        return action;
    }

    None
}

// ── Menu bar ──────────────────────────────────────────────────────────────────

/// Render a two-column menu item: label left, shortcut right — no button frame.
/// Returns `true` if clicked.
fn menu_row(ui: &mut egui::Ui, label: &str, shortcut: &str) -> bool {
    // Measure label and shortcut to compute a tight row width.
    let label_font = egui::FontId::proportional(13.0);
    let shortcut_font = egui::FontId::proportional(11.0);
    let label_w = ui
        .painter()
        .layout_no_wrap(label.to_owned(), label_font.clone(), egui::Color32::WHITE)
        .rect
        .width();
    let key_w = if shortcut.is_empty() {
        0.0
    } else {
        ui.painter()
            .layout_no_wrap(
                shortcut.to_owned(),
                shortcut_font.clone(),
                egui::Color32::WHITE,
            )
            .rect
            .width()
    };
    // Row is wide enough for label + gap + shortcut, with side padding.
    let gap = if shortcut.is_empty() { 0.0 } else { 24.0 };
    let min_w = label_w + gap + key_w + 24.0; // 12px each side
    // All rows in the same menu share the available width so the shortcut column
    // aligns. The menu popup width is capped by set_max_width in each menu closure.
    let w = ui.available_width().max(min_w);

    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 22.0), egui::Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);

    if ui.is_rect_visible(rect) {
        if resp.hovered() {
            ui.painter()
                .rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
        }
        let fg = ui.visuals().text_color();
        let key_fg = ui.visuals().weak_text_color();

        ui.painter().text(
            rect.left_center() + egui::vec2(12.0, 0.0),
            egui::Align2::LEFT_CENTER,
            label,
            label_font,
            fg,
        );
        if !shortcut.is_empty() {
            ui.painter().text(
                rect.right_center() - egui::vec2(12.0, 0.0),
                egui::Align2::RIGHT_CENTER,
                shortcut,
                shortcut_font,
                key_fg,
            );
        }
    }
    resp.clicked()
}

fn show_menu_bar(ui: &mut egui::Ui, state: &mut UiState, actions: &mut Vec<Action>) {
    let menu_bg = cfg_to_egui(state.theme.ui_or(
        "sideBar.background",
        crabide_config::Color::rgb(0x25, 0x25, 0x26),
    ));
    let menu_fg = cfg_to_egui(state.theme.ui_or(
        "editor.foreground",
        crabide_config::Color::rgb(0xcc, 0xcc, 0xcc),
    ));

    egui::Panel::top("menu_bar")
        .frame(
            egui::Frame::NONE
                .fill(menu_bg)
                .inner_margin(egui::Margin::symmetric(4, 2)),
        )
        .show_inside(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                // Make top-level menu trigger buttons transparent so they blend
                // with the menu bar background; hover shows a subtle highlight.
                ui.visuals_mut().widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                ui.visuals_mut().widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
                ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::NONE;
                ui.visuals_mut().widgets.open.weak_bg_fill = cfg_to_egui(state.theme.ui_or(
                    "list.hoverBackground",
                    crabide_config::Color::rgba(0x2a, 0x2d, 0x2e, 0xff),
                ));
                ui.visuals_mut().widgets.open.bg_fill = ui.visuals().widgets.open.weak_bg_fill;
                ui.visuals_mut().override_text_color = Some(menu_fg);

                ui.menu_button("File", |ui| {
                    ui.set_max_width(230.0);
                    ui.visuals_mut().override_text_color = None;
                    ui.spacing_mut().item_spacing.y = 1.0;
                    if menu_row(ui, "New File", "Ctrl+N") {
                        actions.push(Action::NewFile);
                        ui.close();
                    }
                    if menu_row(ui, "Open File...", "Ctrl+O") {
                        actions.push(Action::OpenFile);
                        ui.close();
                    }
                    if menu_row(ui, "Open Folder...", "") {
                        actions.push(Action::OpenFolder);
                        ui.close();
                    }
                    ui.separator();
                    if menu_row(ui, "Save", "Ctrl+S") {
                        actions.push(Action::SaveFile);
                        ui.close();
                    }
                    if menu_row(ui, "Save As...", "Ctrl+Shift+S") {
                        actions.push(Action::SaveFileAs);
                        ui.close();
                    }
                    if menu_row(ui, "Save All", "Ctrl+K S") {
                        actions.push(Action::SaveAll);
                        ui.close();
                    }
                    ui.separator();
                    if menu_row(ui, "Close Tab", "Ctrl+W") {
                        actions.push(Action::CloseTab);
                        ui.close();
                    }
                    if menu_row(ui, "Close All Tabs", "") {
                        actions.push(Action::CloseAllTabs);
                        ui.close();
                    }
                    if !state.file_explorer.roots.is_empty() && menu_row(ui, "Close Folder", "") {
                        actions.push(Action::CloseFolder);
                        ui.close();
                    }
                    ui.separator();
                    if menu_row(ui, "Quit", "Alt+F4") {
                        actions.push(Action::Quit);
                        ui.close();
                    }
                });

                ui.menu_button("Edit", |ui| {
                    ui.set_max_width(230.0);
                    ui.visuals_mut().override_text_color = None;
                    ui.spacing_mut().item_spacing.y = 1.0;
                    if menu_row(ui, "Undo", "Ctrl+Z") {
                        actions.push(Action::Undo);
                        ui.close();
                    }
                    if menu_row(ui, "Redo", "Ctrl+Y") {
                        actions.push(Action::Redo);
                        ui.close();
                    }
                    ui.separator();
                    if menu_row(ui, "Cut", "Ctrl+X") {
                        actions.push(Action::Cut);
                        ui.close();
                    }
                    if menu_row(ui, "Copy", "Ctrl+C") {
                        actions.push(Action::Copy);
                        ui.close();
                    }
                    if menu_row(ui, "Paste", "Ctrl+V") {
                        actions.push(Action::Paste);
                        ui.close();
                    }
                    if menu_row(ui, "Select All", "Ctrl+A") {
                        actions.push(Action::SelectAll);
                        ui.close();
                    }
                    ui.separator();
                    if menu_row(ui, "Duplicate Line", "Ctrl+D") {
                        actions.push(Action::DuplicateLine);
                        ui.close();
                    }
                    if menu_row(ui, "Delete Line", "Ctrl+Shift+K") {
                        actions.push(Action::DeleteLine);
                        ui.close();
                    }
                    if menu_row(ui, "Move Line Up", "Alt+Up") {
                        actions.push(Action::MoveLineUp);
                        ui.close();
                    }
                    if menu_row(ui, "Move Line Down", "Alt+Down") {
                        actions.push(Action::MoveLineDown);
                        ui.close();
                    }
                    ui.separator();
                    if menu_row(ui, "Toggle Comment", "Ctrl+/") {
                        actions.push(Action::ToggleLineComment);
                        ui.close();
                    }
                    if menu_row(ui, "Indent Line", "Tab") {
                        actions.push(Action::IndentLine);
                        ui.close();
                    }
                    if menu_row(ui, "Outdent Line", "Shift+Tab") {
                        actions.push(Action::OutdentLine);
                        ui.close();
                    }
                    ui.separator();
                    if menu_row(ui, "Find", "Ctrl+F") {
                        actions.push(Action::Find);
                        ui.close();
                    }
                    if menu_row(ui, "Find & Replace", "Ctrl+H") {
                        actions.push(Action::FindReplace);
                        ui.close();
                    }
                    if menu_row(ui, "Find in Files", "Ctrl+Shift+F") {
                        actions.push(Action::FindInFiles);
                        ui.close();
                    }
                    ui.separator();
                    if menu_row(ui, "Word Wrap", "Alt+Z") {
                        actions.push(Action::ToggleWordWrap);
                        ui.close();
                    }
                });

                ui.menu_button("View", |ui| {
                    ui.set_max_width(230.0);
                    ui.visuals_mut().override_text_color = None;
                    ui.spacing_mut().item_spacing.y = 1.0;
                    let sidebar_label = if state.sidebar_visible {
                        "Hide Explorer"
                    } else {
                        "Show Explorer"
                    };
                    if menu_row(ui, sidebar_label, "Ctrl+B") {
                        state.sidebar_visible = !state.sidebar_visible;
                        ui.close();
                    }
                    let term_label = if state.terminal.visible {
                        "Hide Terminal"
                    } else {
                        "Show Terminal"
                    };
                    if menu_row(ui, term_label, "Ctrl+`") {
                        actions.push(Action::ToggleTerminal);
                        ui.close();
                    }
                    if menu_row(ui, "New Terminal", "Ctrl+Shift+`") {
                        actions.push(Action::NewTerminal);
                        ui.close();
                    }
                    ui.separator();
                    if menu_row(ui, "Command Palette", "Ctrl+Shift+P") {
                        state.command_palette.visible = true;
                        ui.close();
                    }
                    if menu_row(ui, "Open File...", "Ctrl+P") {
                        actions.push(Action::FuzzyFindFile);
                        ui.close();
                    }
                    if menu_row(ui, "Find in Files", "Ctrl+Shift+F") {
                        actions.push(Action::FindInFiles);
                        ui.close();
                    }
                    ui.separator();
                    // ── Core panels ──────────────────────────────────────────
                    let prob_label = if state.problems_panel_open {
                        "Hide Problems"
                    } else {
                        "Show Problems"
                    };
                    if menu_row(ui, prob_label, "Ctrl+Shift+M") {
                        actions.push(Action::ToggleProblemsPanel);
                        ui.close();
                    }
                    // Extension panels auto-generated from registered commands.
                    // Collect panel data first to avoid borrow conflicts.
                    let ext_panel_items: Vec<(String, String, bool, Option<String>)> = state
                        .extension_panels
                        .iter()
                        .map(|(id, panel)| {
                            (
                                id.clone(),
                                panel.registration.title.clone(),
                                panel.open,
                                panel.registration.toggle_command.clone(),
                            )
                        })
                        .collect();
                    for (pid, title, open, toggle_cmd) in ext_panel_items {
                        let label = if open {
                            format!("Close {title}")
                        } else {
                            format!("Open {title}")
                        };
                        let shortcut = toggle_cmd
                            .as_deref()
                            .and_then(|cmd| {
                                if state.action_registry.has(cmd) {
                                    Some("")
                                } else {
                                    None
                                }
                            })
                            .unwrap_or("");
                        if menu_row(ui, &label, shortcut) {
                            if let Some(cmd) = toggle_cmd {
                                actions.push(Action::Custom(cmd));
                            } else if let Some(p) = state.extension_panels.get_mut(&pid) {
                                p.open = !p.open;
                            }
                            ui.close();
                        }
                    }
                    ui.separator();
                    if menu_row(ui, "Zoom In", "Ctrl++") {
                        state.font_size = (state.font_size + 1.0).min(28.0);
                        ui.close();
                    }
                    if menu_row(ui, "Zoom Out", "Ctrl+-") {
                        state.font_size = (state.font_size - 1.0).max(8.0);
                        ui.close();
                    }
                    if menu_row(ui, "Reset Zoom", "Ctrl+0") {
                        state.font_size = 14.0;
                        ui.close();
                    }
                });

                ui.menu_button("Git", |ui| {
                    ui.set_max_width(230.0);
                    ui.visuals_mut().override_text_color = None;
                    ui.spacing_mut().item_spacing.y = 1.0;
                    let toggle_label = if state.git_enabled {
                        "Disable Git"
                    } else {
                        "Enable Git"
                    };
                    if menu_row(ui, toggle_label, "") {
                        actions.push(Action::ToggleGit);
                        ui.close();
                    }

                    if state.git_enabled {
                        ui.separator();
                        let panel_label = if state.git_panel.visible {
                            "Hide Source Control"
                        } else {
                            "Show Source Control"
                        };
                        if menu_row(ui, panel_label, "Ctrl+Shift+G") {
                            state.git_panel.visible = !state.git_panel.visible;
                            ui.close();
                        }
                        ui.separator();
                        if menu_row(ui, "Stage All", "") {
                            state.git_panel.pending_stage_all = true;
                            ui.close();
                        }
                        if menu_row(ui, "Unstage All", "") {
                            state.git_panel.pending_unstage_all = true;
                            ui.close();
                        }
                        ui.separator();
                        if menu_row(ui, "Commit", "") {
                            actions.push(Action::GitCommit);
                            ui.close();
                        }
                        if menu_row(ui, "Discard All Changes", "") {
                            actions.push(Action::GitDiscardChanges);
                            ui.close();
                        }
                    }
                });

                ui.menu_button("Run", |ui| {
                    ui.set_max_width(240.0);
                    ui.visuals_mut().override_text_color = None;
                    ui.spacing_mut().item_spacing.y = 1.0;

                    let enable_label = if state.dap_panel.enabled {
                        "Disable Debugger"
                    } else {
                        "Enable Debugger"
                    };
                    if menu_row(ui, enable_label, "") {
                        actions.push(Action::ToggleDebug);
                        ui.close();
                    }

                    if state.dap_panel.enabled {
                        ui.separator();
                        let panel_label = if state.dap_panel.visible {
                            "Hide Debug Panel"
                        } else {
                            "Show Debug Panel"
                        };
                        if menu_row(ui, panel_label, "Ctrl+Shift+D") {
                            state.dap_panel.visible = !state.dap_panel.visible;
                            ui.close();
                        }
                        ui.separator();
                        if menu_row(ui, "Start Debugging", "F5") {
                            actions.push(Action::StartDebug);
                            ui.close();
                        }
                        if menu_row(ui, "Stop Debugging", "Shift+F5") {
                            actions.push(Action::StopDebug);
                            ui.close();
                        }
                        if menu_row(ui, "Restart", "") {
                            actions.push(Action::RestartDebug);
                            ui.close();
                        }
                        ui.separator();
                        let continue_label = if state.dap_panel.paused {
                            "Continue"
                        } else {
                            "Pause"
                        };
                        if menu_row(ui, continue_label, "F5") {
                            actions.push(Action::ContinueDebug);
                            ui.close();
                        }
                        if menu_row(ui, "Step Over", "F10") {
                            actions.push(Action::StepOver);
                            ui.close();
                        }
                        if menu_row(ui, "Step Into", "F11") {
                            actions.push(Action::StepInto);
                            ui.close();
                        }
                        if menu_row(ui, "Step Out", "Shift+F11") {
                            actions.push(Action::StepOut);
                            ui.close();
                        }
                        ui.separator();
                        if menu_row(ui, "Toggle Breakpoint", "F9") {
                            actions.push(Action::ToggleBreakpoint);
                            ui.close();
                        }
                    }
                });

                ui.menu_button("Go", |ui| {
                    ui.set_max_width(230.0);
                    ui.visuals_mut().override_text_color = None;
                    ui.spacing_mut().item_spacing.y = 1.0;
                    if menu_row(ui, "Go to Definition", "F12") {
                        actions.push(Action::GotoDefinition);
                        ui.close();
                    }
                    if menu_row(ui, "Go to References", "Alt+F12") {
                        actions.push(Action::GotoReferences);
                        ui.close();
                    }
                    if menu_row(ui, "Go to Symbol...", "Ctrl+Shift+O") {
                        actions.push(Action::GotoSymbol);
                        ui.close();
                    }
                    if menu_row(ui, "Go to Line...", "Ctrl+G") {
                        actions.push(Action::GotoLine);
                        ui.close();
                    }
                    if menu_row(ui, "Go Back", "Alt+Left") {
                        actions.push(Action::GoBack);
                        ui.close();
                    }
                    if menu_row(ui, "Go Forward", "Alt+Right") {
                        actions.push(Action::GoForward);
                        ui.close();
                    }
                    ui.separator();
                    if menu_row(ui, "Next Diagnostic", "F8") {
                        actions.push(Action::NextDiagnostic);
                        ui.close();
                    }
                    if menu_row(ui, "Prev Diagnostic", "Shift+F8") {
                        actions.push(Action::PreviousDiagnostic);
                        ui.close();
                    }
                });
            });
        });
}

// ── Keyboard routing ──────────────────────────────────────────────────────────

/// Read all key/text events from egui this frame, run them through the
/// keybinding engine (for key events) and emit InsertText (for text events).
fn process_keyboard(ctx: &egui::Context, state: &mut UiState) -> Vec<Action> {
    let mut actions = Vec::new();

    // Don't steal keystrokes while the command palette text box is focused.
    if state.command_palette.visible {
        return actions;
    }

    // While the fuzzy finder or goto-line dialogs are open, only process
    // Escape (already handled in those panels via ctx.input) — let their
    // TextEdit widgets consume all other key events.
    if state.fuzzy_finder.visible || state.goto_line.visible {
        return actions;
    }

    // ── Terminal keyboard forwarding ──────────────────────────────────────────
    // When the terminal has focus, route all key events to the PTY.
    // The only exceptions are Ctrl+` (toggle terminal) and Ctrl+Shift+` (new
    // terminal), which are always processed by the keybinding engine.
    if state.terminal.has_focus && state.terminal.visible && !state.terminal.instances.is_empty() {
        let Some(inst_id) = state.terminal.active().map(|i| i.id) else {
            // No active instance — fall through to normal editor handling.
            return actions;
        };

        ctx.input(|i| {
            for event in &i.events {
                match event {
                    egui::Event::Key {
                        key,
                        modifiers,
                        pressed: true,
                        ..
                    } => {
                        // Always handle Ctrl+` to toggle terminal (unfocus).
                        if *key == egui::Key::Backtick && modifiers.ctrl {
                            actions.push(Action::ToggleTerminal);
                            continue;
                        }
                        // Encode everything else as PTY bytes.
                        if let Some(bytes) =
                            panels::terminal_panel::encode_key(*key, *modifiers, None)
                        {
                            state.terminal.pending_input.push((inst_id, bytes));
                        }
                    }
                    egui::Event::Text(text) if !text.is_empty() => {
                        state
                            .terminal
                            .pending_input
                            .push((inst_id, text.as_bytes().to_vec()));
                    }
                    egui::Event::Paste(text) if !text.is_empty() => {
                        // Bracketed paste: wrap in \x1b[200~ … \x1b[201~ if the
                        // shell has enabled DECSET 2004, otherwise send raw text.
                        let bracketed = state.terminal.active().is_some_and(|i| i.bracketed_paste);
                        if bracketed {
                            let mut bytes = b"\x1b[200~".to_vec();
                            bytes.extend(text.as_bytes());
                            bytes.extend(b"\x1b[201~");
                            state.terminal.pending_input.push((inst_id, bytes));
                        } else {
                            state
                                .terminal
                                .pending_input
                                .push((inst_id, text.as_bytes().to_vec()));
                        }
                    }
                    _ => {}
                }
            }
        });

        return actions;
    }

    // If a TextEdit widget has focus (find bar, etc.), skip plain text insertion
    // but still process key-chord actions (e.g. Escape to close the bar).
    let no_widget_focused = ctx.memory(|m| m.focused().is_none());

    // Track whether a dialog was closed this frame so we can surrender stale
    // focus afterward (egui retains the focused-widget ID even after the widget
    // is no longer rendered, which keeps no_widget_focused = false and silently
    // blocks all plain-key actions on subsequent frames).
    let mut surrender_focus = false;

    ctx.input(|i| {
        for event in &i.events {
            match event {
                egui::Event::Key {
                    key,
                    modifiers,
                    pressed: true,
                    ..
                } => {
                    // ── Bare Enter → newline with auto-indent ─────────────────
                    // Enter does not fire as Event::Text on any platform, so we
                    // must handle it here before the keybinding lookup.
                    // Guard: when the find bar is visible it consumes Enter itself
                    // (via consume_key) but as a safety net we also skip it here.
                    if *key == egui::Key::Enter
                        && no_widget_focused
                        && !state.find_replace.visible
                        && !modifiers.ctrl
                        && !modifiers.alt
                        && !modifiers.mac_cmd
                        && !modifiers.command
                    {
                        actions.push(Action::InsertText("\n".to_owned()));
                        continue;
                    }

                    // ── Tab / Shift+Tab → indent or snippet tabstop ───────────
                    if *key == egui::Key::Tab
                        && no_widget_focused
                        && !modifiers.ctrl
                        && !modifiers.alt
                        && !modifiers.mac_cmd
                        && !modifiers.command
                    {
                        let has_snippet = state
                            .active_tab()
                            .and_then(|i| state.active_group_ref().tabs.get(i))
                            .is_some_and(|t| t.snippet_engine.current_tabstop().is_some());
                        if modifiers.shift {
                            actions.push(if has_snippet {
                                Action::PreviousTabstop
                            } else {
                                Action::OutdentLine
                            });
                        } else {
                            actions.push(if has_snippet {
                                Action::NextTabstop
                            } else {
                                Action::InsertText("    ".to_owned())
                            });
                        }
                        continue;
                    }

                    // ── Normal keybinding lookup ──────────────────────────────
                    // When a TextEdit widget (e.g. find bar) has keyboard focus,
                    // only process Escape and modifier+key combos.
                    let should_process = no_widget_focused
                        || *key == egui::Key::Escape
                        || modifiers.ctrl
                        || modifiers.alt
                        || modifiers.mac_cmd
                        || modifiers.command;

                    if should_process {
                        if *key == egui::Key::Escape && state.find_replace.visible {
                            state.find_replace.visible = false;
                            state.find_replace.match_ranges = Arc::new(Vec::new());
                            surrender_focus = true;
                        } else if *key == egui::Key::Escape && state.workspace_search.visible {
                            state.workspace_search.visible = false;
                            surrender_focus = true;
                        } else {
                            let chord = egui_key_to_chord(*key, *modifiers);
                            if let Some(action) =
                                state.keybindings.press(chord, Some(&state.when_context))
                            {
                                actions.push(action);
                            }
                        }
                    }
                }
                egui::Event::Text(text) if no_widget_focused && !text.is_empty() => {
                    // Filter out control characters that arrive as Text on some platforms
                    // (e.g. Enter → '\n', Return → '\r'). Those are handled above as Key events.
                    let filtered: String =
                        text.chars().filter(|&c| c != '\n' && c != '\r').collect();
                    if !filtered.is_empty() {
                        actions.push(Action::InsertText(filtered));
                    }
                }
                // OS-level paste (Ctrl+V on most platforms emits this event).
                egui::Event::Paste(text) if no_widget_focused && !text.is_empty() => {
                    actions.push(Action::InsertText(text.clone()));
                }
                _ => {}
            }
        }
    });

    if surrender_focus {
        ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
    }
    actions
}

/// Convert an egui key event to a crabide `KeyChord`.
fn egui_key_to_chord(key: egui::Key, mods: egui::Modifiers) -> KeyChord {
    let mut modifiers = Modifiers::empty();
    // On Windows/Linux: mods.ctrl is the Ctrl key.
    // On macOS: mods.mac_cmd is the Command key, which maps to CTRL for VS Code-style bindings.
    // IMPORTANT: mods.command is an *alias* — it equals mods.ctrl on Win/Linux and mods.mac_cmd on
    // macOS. Do NOT map mods.command to META; on Windows that would produce CTRL|META for every
    // Ctrl+key press, causing all Ctrl bindings to fail (CTRL|META ≠ CTRL).
    if mods.ctrl {
        modifiers |= Modifiers::CTRL;
    }
    if mods.mac_cmd {
        modifiers |= Modifiers::CTRL;
    } // macOS Cmd acts as CTRL for editor shortcuts
    if mods.shift {
        modifiers |= Modifiers::SHIFT;
    }
    if mods.alt {
        modifiers |= Modifiers::ALT;
    }

    let k = match key {
        egui::Key::ArrowUp => Key::Up,
        egui::Key::ArrowDown => Key::Down,
        egui::Key::ArrowLeft => Key::Left,
        egui::Key::ArrowRight => Key::Right,
        egui::Key::Home => Key::Home,
        egui::Key::End => Key::End,
        egui::Key::PageUp => Key::PageUp,
        egui::Key::PageDown => Key::PageDown,
        egui::Key::Enter => Key::Enter,
        egui::Key::Backspace => Key::Backspace,
        egui::Key::Delete => Key::Delete,
        egui::Key::Tab => Key::Tab,
        egui::Key::Escape => Key::Escape,
        egui::Key::Space => Key::Space,
        egui::Key::F1 => Key::F(1),
        egui::Key::F2 => Key::F(2),
        egui::Key::F3 => Key::F(3),
        egui::Key::F4 => Key::F(4),
        egui::Key::F5 => Key::F(5),
        egui::Key::F6 => Key::F(6),
        egui::Key::F7 => Key::F(7),
        egui::Key::F8 => Key::F(8),
        egui::Key::F9 => Key::F(9),
        egui::Key::F10 => Key::F(10),
        egui::Key::F11 => Key::F(11),
        egui::Key::F12 => Key::F(12),
        egui::Key::A => Key::Char('a'),
        egui::Key::B => Key::Char('b'),
        egui::Key::C => Key::Char('c'),
        egui::Key::D => Key::Char('d'),
        egui::Key::E => Key::Char('e'),
        egui::Key::F => Key::Char('f'),
        egui::Key::G => Key::Char('g'),
        egui::Key::H => Key::Char('h'),
        egui::Key::I => Key::Char('i'),
        egui::Key::J => Key::Char('j'),
        egui::Key::K => Key::Char('k'),
        egui::Key::L => Key::Char('l'),
        egui::Key::M => Key::Char('m'),
        egui::Key::N => Key::Char('n'),
        egui::Key::O => Key::Char('o'),
        egui::Key::P => Key::Char('p'),
        egui::Key::Q => Key::Char('q'),
        egui::Key::R => Key::Char('r'),
        egui::Key::S => Key::Char('s'),
        egui::Key::T => Key::Char('t'),
        egui::Key::U => Key::Char('u'),
        egui::Key::V => Key::Char('v'),
        egui::Key::W => Key::Char('w'),
        egui::Key::X => Key::Char('x'),
        egui::Key::Y => Key::Char('y'),
        egui::Key::Z => Key::Char('z'),
        egui::Key::Num0 => Key::Char('0'),
        egui::Key::Num1 => Key::Char('1'),
        egui::Key::Num2 => Key::Char('2'),
        egui::Key::Num3 => Key::Char('3'),
        egui::Key::Num4 => Key::Char('4'),
        egui::Key::Num5 => Key::Char('5'),
        egui::Key::Num6 => Key::Char('6'),
        egui::Key::Num7 => Key::Char('7'),
        egui::Key::Num8 => Key::Char('8'),
        egui::Key::Num9 => Key::Char('9'),
        egui::Key::Slash => Key::Char('/'),
        egui::Key::Backslash => Key::Char('\\'),
        egui::Key::Minus => Key::Char('-'),
        egui::Key::Plus => Key::Char('+'),
        egui::Key::Equals => Key::Char('='),
        egui::Key::OpenBracket => Key::Char('['),
        egui::Key::CloseBracket => Key::Char(']'),
        egui::Key::Backtick => Key::Char('`'),
        egui::Key::Semicolon => Key::Char(';'),
        egui::Key::Comma => Key::Char(','),
        egui::Key::Period => Key::Char('.'),
        egui::Key::Quote => Key::Char('\''),
        egui::Key::Colon => Key::Char(':'),
        egui::Key::Pipe => Key::Char('|'),
        egui::Key::Questionmark => Key::Char('?'),
        egui::Key::Exclamationmark => Key::Char('!'),
        egui::Key::OpenCurlyBracket => Key::Char('{'),
        egui::Key::CloseCurlyBracket => Key::Char('}'),
        _ => Key::Unknown(format!("{key:?}")),
    };

    KeyChord::new(modifiers, k)
}

// ── UI-internal action handler ────────────────────────────────────────────────

/// Handle actions that are purely UI-internal.
/// Returns `true` if the action was handled (do NOT forward to the app),
/// `false` if the app must also handle it.
pub(crate) fn handle_ui_action(action: Action, state: &mut UiState) -> bool {
    match action {
        // ── Command palette / sidebar / zoom ──────────────────────────────────
        Action::CommandPalette => {
            state.command_palette.visible = !state.command_palette.visible;
            true
        }
        Action::ToggleSidebar => {
            state.sidebar_visible = !state.sidebar_visible;
            true
        }
        Action::ZoomIn => {
            state.font_size = (state.font_size + 1.0).min(28.0);
            true
        }
        Action::ZoomOut => {
            state.font_size = (state.font_size - 1.0).max(8.0);
            true
        }
        Action::ZoomReset => {
            state.font_size = 14.0;
            true
        }

        // ── Fuzzy finder ──────────────────────────────────────────────────────
        Action::FuzzyFindFile => {
            // Open the overlay; forward to app so it can populate file_index.
            state.fuzzy_finder.open();
            false
        }

        // Go-to-line
        Action::GotoLine if !state.goto_line.visible => {
            state.goto_line.visible = true;
            state.goto_line.query.clear();
            true
        }
        Action::GotoLine => false,

        // ── Go-to-symbol (Ctrl+Shift+O) ───────────────────────────────────────
        Action::GotoSymbol if !state.symbol_outline.visible => {
            // Open overlay; forward to app so it can populate entries from syntax.
            state.symbol_outline.visible = true;
            state.symbol_outline.query.clear();
            state.symbol_outline.selected_idx = 0;
            state.symbol_outline.entries.clear();
            false
        }
        Action::GotoSymbol => {
            // Already open — confirmation, forwarded to app for scroll.
            false
        }

        // ── Tab navigation ────────────────────────────────────────────────────
        Action::FindInFiles => {
            if !state.workspace_search.visible {
                state.workspace_search.just_opened = true;
            }
            state.workspace_search.visible = true;
            // Forward to app to run the actual grep.
            false
        }
        // ── Tab navigation ────────────────────────────────────────────────────
        Action::NextTab => {
            let group = state.active_group_mut();
            if let Some(idx) = group.active_tab {
                if idx + 1 < group.tabs.len() {
                    group.active_tab = Some(idx + 1);
                }
            }
            true
        }
        Action::PreviousTab => {
            let group = state.active_group_mut();
            if let Some(idx) = group.active_tab {
                if idx > 0 {
                    group.active_tab = Some(idx - 1);
                }
            }
            true
        }
        Action::MoveTabRight => {
            let group = state.active_group_mut();
            if let Some(idx) = group.active_tab {
                let n = group.tabs.len();
                if idx + 1 < n {
                    group.tabs.swap(idx, idx + 1);
                    group.active_tab = Some(idx + 1);
                }
            }
            true
        }
        Action::MoveTabLeft => {
            let group = state.active_group_mut();
            if let Some(idx) = group.active_tab {
                if idx > 0 {
                    group.tabs.swap(idx, idx - 1);
                    group.active_tab = Some(idx - 1);
                }
            }
            true
        }

        // ── Word wrap toggle ──────────────────────────────────────────────────
        Action::ToggleWordWrap => {
            state.word_wrap = !state.word_wrap;
            true
        }

        // ── Terminal panel toggle (Ctrl+`) ────────────────────────────────────
        Action::ToggleTerminal => {
            state.terminal.visible = !state.terminal.visible;
            if state.terminal.visible {
                state.terminal.has_focus = true;
                // Open a first terminal if none exist yet.
                if state.terminal.instances.is_empty() {
                    state.terminal.pending_new = true;
                }
            } else {
                state.terminal.has_focus = false;
            }
            true
        }

        // ── NewTerminal / KillTerminal — forwarded to app ──────────────────────
        Action::NewTerminal => {
            state.terminal.visible = true;
            state.terminal.pending_new = true;
            false // app must call TerminalManager::new_terminal
        }
        Action::KillTerminal => {
            if let Some(inst) = state.terminal.active() {
                state.terminal.pending_kill = Some(inst.id);
            }
            false
        }

        // ── Git panel toggle (Ctrl+Shift+G) ───────────────────────────────────
        Action::ToggleGitPanel => {
            if state.git_enabled {
                state.git_panel.visible = !state.git_panel.visible;
            }
            true
        }

        // ── Debug panel toggle (Ctrl+Shift+D) ─────────────────────────────────
        Action::ToggleDebugPanel => {
            if state.dap_panel.enabled {
                state.dap_panel.visible = !state.dap_panel.visible;
            }
            true
        }

        // ── Extensions panel toggle (Ctrl+Shift+X) ────────────────────────────
        Action::ToggleExtensionsPanel => {
            use crate::state::SidebarTab;
            if !state.sidebar_visible {
                state.sidebar_visible = true;
                state.sidebar_tab = SidebarTab::Extensions;
            } else if state.sidebar_tab == SidebarTab::Extensions {
                // Already on Extensions tab — close the sidebar.
                state.sidebar_visible = false;
            } else {
                state.sidebar_tab = SidebarTab::Extensions;
            }
            true
        }

        // ── Problems panel toggle (Ctrl+Shift+M) ──────────────────────────────
        Action::ToggleProblemsPanel => {
            state.problems_panel_open = !state.problems_panel_open;
            true
        }

        // ── Performance profiler toggle (Ctrl+Shift+`) ──────────────────────
        Action::ToggleProfiler => {
            state.profiler.visible = !state.profiler.visible;
            true
        }

        // ── Output panel toggle ────────────────────────────────────────────────
        Action::ToggleOutputPanel => {
            state.output_panel.visible = !state.output_panel.visible;
            true
        }

        // ── Theme picker ──────────────────────────────────────────────────────
        Action::ToggleThemePicker => {
            state.theme_picker.visible = !state.theme_picker.visible;
            true
        }

        // ── Keybindings editor ─────────────────────────────────────────────────
        Action::ToggleKeybindingsEditor => {
            state.keybindings_editor.visible = !state.keybindings_editor.visible;
            true
        }

        // ── Settings panel ──────────────────────────────────────────────────────
        Action::ToggleSettingsPanel => {
            state.settings_panel.visible = !state.settings_panel.visible;
            true
        }

        // ── Minimap ───────────────────────────────────────────────────────────
        Action::ToggleMinimap => {
            state.minimap_visible = !state.minimap_visible;
            true
        }

        // ── Split editor ─────────────────────────────────────────────────────
        Action::SplitEditorRight => {
            let new_group_idx = state.editor_groups.len();
            let mut new_group = crate::state::EditorGroup::new();
            // Move the active tab from the current group to the new group.
            if let Some(active_idx) = state.active_tab() {
                let group = state.active_group_mut();
                let tab = group.tabs.remove(active_idx);
                group.active_tab = if group.tabs.is_empty() {
                    None
                } else {
                    Some(active_idx.saturating_sub(1).min(group.tabs.len() - 1))
                };
                new_group.tabs.push(tab);
                new_group.active_tab = Some(0);
            }
            state.editor_groups.push(new_group);

            // Rebuild layout: Sidebar | Editor(0) | Editor(new)
            let mut tiles = egui_tiles::Tiles::default();
            let explorer_id = tiles.insert_pane(PaneKind::FileExplorer);
            let editor0_id = tiles.insert_pane(PaneKind::EditorGroup(0));
            let editor1_id = tiles.insert_pane(PaneKind::EditorGroup(new_group_idx));
            let mut linear = egui_tiles::Linear::new(
                egui_tiles::LinearDir::Horizontal,
                vec![explorer_id, editor0_id, editor1_id],
            );
            linear.shares.set_share(explorer_id, 0.15);
            linear.shares.set_share(editor0_id, 0.425);
            linear.shares.set_share(editor1_id, 0.425);
            let root = tiles.insert_container(egui_tiles::Container::Linear(linear));
            state.layout = egui_tiles::Tree::new("crabide_layout", root, tiles);
            true
        }
        Action::SplitEditorDown => {
            let new_group_idx = state.editor_groups.len();
            let mut new_group = crate::state::EditorGroup::new();
            if let Some(active_idx) = state.active_tab() {
                let group = state.active_group_mut();
                let tab = group.tabs.remove(active_idx);
                group.active_tab = if group.tabs.is_empty() {
                    None
                } else {
                    Some(active_idx.saturating_sub(1).min(group.tabs.len() - 1))
                };
                new_group.tabs.push(tab);
                new_group.active_tab = Some(0);
            }
            state.editor_groups.push(new_group);

            // Rebuild layout: Sidebar | Vertical(Editor(0), Editor(new))
            let mut tiles = egui_tiles::Tiles::default();
            let explorer_id = tiles.insert_pane(PaneKind::FileExplorer);
            let editor0_id = tiles.insert_pane(PaneKind::EditorGroup(0));
            let editor1_id = tiles.insert_pane(PaneKind::EditorGroup(new_group_idx));
            let mut vert = egui_tiles::Linear::new(
                egui_tiles::LinearDir::Vertical,
                vec![editor0_id, editor1_id],
            );
            vert.shares.set_share(editor0_id, 0.5);
            vert.shares.set_share(editor1_id, 0.5);
            let vert_id = tiles.insert_container(egui_tiles::Container::Linear(vert));
            let mut linear = egui_tiles::Linear::new(
                egui_tiles::LinearDir::Horizontal,
                vec![explorer_id, vert_id],
            );
            linear.shares.set_share(explorer_id, 0.2);
            linear.shares.set_share(vert_id, 0.8);
            let root = tiles.insert_container(egui_tiles::Container::Linear(linear));
            state.layout = egui_tiles::Tree::new("crabide_layout", root, tiles);
            true
        }
        Action::CloseEditor => {
            // Close the split: remove the active editor group (if more than one).
            if state.editor_groups.len() > 1 {
                let closed_group = state.active_group;
                // Move all tabs from the closing group to group 0.
                let tabs_len = state.editor_groups[closed_group].tabs.len();
                if tabs_len > 0 {
                    let tabs = state.editor_groups[closed_group].tabs.split_off(0);
                    state.editor_groups[0].tabs.extend(tabs);
                }
                state.editor_groups.remove(closed_group);
                if state.active_group >= state.editor_groups.len() {
                    state.active_group = state.editor_groups.len() - 1;
                }
                // Rebuild with single editor layout.
                let mut tiles = egui_tiles::Tiles::default();
                let explorer_id = tiles.insert_pane(PaneKind::FileExplorer);
                let editor_id = tiles.insert_pane(PaneKind::EditorGroup(0));
                let mut linear = egui_tiles::Linear::new(
                    egui_tiles::LinearDir::Horizontal,
                    vec![explorer_id, editor_id],
                );
                linear.shares.set_share(explorer_id, 0.2);
                linear.shares.set_share(editor_id, 0.8);
                let root = tiles.insert_container(egui_tiles::Container::Linear(linear));
                state.layout = egui_tiles::Tree::new("crabide_layout", root, tiles);
            }
            true
        }

        // ── Debugger enable / disable (like ToggleGit) ────────────────────────
        Action::ToggleDebug => {
            // Forward to app so it can start/stop DapClient.
            false
        }

        // ── Debug session control — forward pending flags to app ───────────────
        Action::ContinueDebug => {
            state.dap_panel.pending_continue = true;
            false
        }
        Action::StepOver => {
            state.dap_panel.pending_step_over = true;
            false
        }
        Action::StepInto => {
            state.dap_panel.pending_step_in = true;
            false
        }
        Action::StepOut => {
            state.dap_panel.pending_step_out = true;
            false
        }
        Action::RestartDebug => {
            state.dap_panel.pending_restart = true;
            false
        }
        Action::StartDebug => {
            state.dap_panel.pending_launch = true;
            // Open the debug panel.
            if state.dap_panel.enabled {
                state.dap_panel.visible = true;
            }
            false
        }
        Action::StopDebug => {
            state.dap_panel.pending_stop = true;
            false
        }
        // ToggleBreakpoint is handled by gutter click; also forward via app.
        Action::ToggleBreakpoint => false,

        // ── Find / replace panel ──────────────────────────────────────────────
        Action::Find => {
            state.find_replace.just_opened = !state.find_replace.visible;
            state.find_replace.visible = true;
            state.find_replace.replace_visible = false;
            if !state.find_replace.query.is_empty() {
                panels::find_replace::recompute_matches(state);
            }
            true
        }
        Action::FindReplace => {
            if state.find_replace.visible && state.find_replace.replace_visible {
                // Panel already open: pass through so the app performs the replacement.
                false
            } else {
                // Open the find+replace panel.
                state.find_replace.just_opened = !state.find_replace.visible;
                state.find_replace.visible = true;
                state.find_replace.replace_visible = true;
                if !state.find_replace.query.is_empty() {
                    panels::find_replace::recompute_matches(state);
                }
                true
            }
        }
        Action::FindNext => {
            if state.find_replace.has_matches() {
                state.find_replace.next_match();
            }
            // Forward to app so it can scroll the editor to the match.
            false
        }
        Action::FindPrevious => {
            if state.find_replace.has_matches() {
                state.find_replace.prev_match();
            }
            false
        }

        // ── Snippet tabstop cycling ───────────────────────────────────────────
        Action::NextTabstop => {
            if let Some(idx) = state.active_tab() {
                if let Some(tab) = state.tabs_mut().get_mut(idx) {
                    if let Some(ts) = tab.snippet_engine.next_tabstop() {
                        let pos = ts.range.start;
                        tab.cursors.set_single(pos);
                    }
                }
            }
            true
        }
        Action::PreviousTabstop => {
            if let Some(idx) = state.active_tab() {
                if let Some(tab) = state.tabs_mut().get_mut(idx) {
                    if let Some(ts) = tab.snippet_engine.prev_tabstop() {
                        let pos = ts.range.start;
                        tab.cursors.set_single(pos);
                    }
                }
            }
            true
        }

        // ── Select all ────────────────────────────────────────────────────────
        Action::SelectAll => {
            if let Some(idx) = state.active_tab() {
                if let Some(tab) = state.tabs_mut().get_mut(idx) {
                    let last_line = tab.lines.len().saturating_sub(1);
                    let last_col = tab.lines.last().map_or(0, |l| l.chars().count()) as u32;
                    use crabide_core::types::Selection;
                    tab.cursors.primary_mut().selection = Selection {
                        anchor: Position::ZERO,
                        active: Position::new(last_line as u32, last_col),
                    };
                }
            }
            true
        }

        // ── Cursor movement (no selection) ────────────────────────────────────
        Action::CursorUp => {
            move_cursors(state, MoveKind::Up(1), false);
            scroll_to_primary(state);
            true
        }
        Action::CursorDown => {
            move_cursors(state, MoveKind::Down(1), false);
            scroll_to_primary(state);
            true
        }
        Action::CursorLeft => {
            move_cursors(state, MoveKind::Left, false);
            scroll_to_primary(state);
            true
        }
        Action::CursorRight => {
            move_cursors(state, MoveKind::Right, false);
            scroll_to_primary(state);
            true
        }
        Action::CursorWordLeft => {
            move_cursors(state, MoveKind::WordLeft, false);
            scroll_to_primary(state);
            true
        }
        Action::CursorWordRight => {
            move_cursors(state, MoveKind::WordRight, false);
            scroll_to_primary(state);
            true
        }
        Action::CursorLineStart => {
            move_cursors(state, MoveKind::LineStart, false);
            scroll_to_primary(state);
            true
        }
        Action::CursorLineEnd => {
            move_cursors(state, MoveKind::LineEnd, false);
            scroll_to_primary(state);
            true
        }
        Action::CursorFileStart => {
            move_cursors(state, MoveKind::FileStart, false);
            scroll_to_primary(state);
            true
        }
        Action::CursorFileEnd => {
            move_cursors(state, MoveKind::FileEnd, false);
            scroll_to_primary(state);
            true
        }
        Action::CursorPageUp => {
            move_cursors(state, MoveKind::Up(25), false);
            scroll_to_primary(state);
            true
        }
        Action::CursorPageDown => {
            move_cursors(state, MoveKind::Down(25), false);
            scroll_to_primary(state);
            true
        }

        // ── Scroll lines (move cursor + editor follows) ───────────────────────
        Action::ScrollLineUp => {
            move_cursors(state, MoveKind::Up(1), false);
            scroll_to_primary(state);
            true
        }
        Action::ScrollLineDown => {
            move_cursors(state, MoveKind::Down(1), false);
            scroll_to_primary(state);
            true
        }

        // ── Selection (extend) ────────────────────────────────────────────────
        Action::SelectUp => {
            move_cursors(state, MoveKind::Up(1), true);
            scroll_to_primary(state);
            true
        }
        Action::SelectDown => {
            move_cursors(state, MoveKind::Down(1), true);
            scroll_to_primary(state);
            true
        }
        Action::SelectLeft => {
            move_cursors(state, MoveKind::Left, true);
            scroll_to_primary(state);
            true
        }
        Action::SelectRight => {
            move_cursors(state, MoveKind::Right, true);
            scroll_to_primary(state);
            true
        }
        Action::SelectWordLeft => {
            move_cursors(state, MoveKind::WordLeft, true);
            scroll_to_primary(state);
            true
        }
        Action::SelectWordRight => {
            move_cursors(state, MoveKind::WordRight, true);
            scroll_to_primary(state);
            true
        }
        Action::SelectLineStart => {
            move_cursors(state, MoveKind::LineStart, true);
            scroll_to_primary(state);
            true
        }
        Action::SelectLineEnd => {
            move_cursors(state, MoveKind::LineEnd, true);
            scroll_to_primary(state);
            true
        }
        Action::SelectFileStart => {
            move_cursors(state, MoveKind::FileStart, true);
            scroll_to_primary(state);
            true
        }
        Action::SelectFileEnd => {
            move_cursors(state, MoveKind::FileEnd, true);
            scroll_to_primary(state);
            true
        }

        // ── Select current line ───────────────────────────────────────────────
        Action::SelectLine => {
            if let Some(idx) = state.active_tab() {
                if let Some(tab) = state.tabs_mut().get_mut(idx) {
                    use crabide_core::types::Selection;
                    let line = tab.cursors.primary().pos().line;
                    let len = tab
                        .lines
                        .get(line as usize)
                        .map_or(0, |l| l.chars().count()) as u32;
                    tab.cursors.primary_mut().selection = Selection {
                        anchor: Position::new(line, 0),
                        active: Position::new(line, len),
                    };
                }
            }
            true
        }

        // ── Add cursor above / below (Alt+Ctrl+Up/Down) ───────────────────────
        Action::AddCursorAbove => {
            if let Some(idx) = state.active_tab() {
                if let Some(tab) = state.tabs_mut().get_mut(idx) {
                    let pos = tab.cursors.primary().pos();
                    if pos.line > 0 {
                        let new_line = pos.line - 1;
                        let new_col = pos.character.min(
                            tab.lines
                                .get(new_line as usize)
                                .map_or(0, |l| l.chars().count() as u32),
                        );
                        tab.cursors.add(Position::new(new_line, new_col));
                    }
                }
            }
            true
        }
        Action::AddCursorBelow => {
            if let Some(idx) = state.active_tab() {
                if let Some(tab) = state.tabs_mut().get_mut(idx) {
                    let pos = tab.cursors.primary().pos();
                    let n_lines = tab.lines.len() as u32;
                    if pos.line + 1 < n_lines {
                        let new_line = pos.line + 1;
                        let new_col = pos.character.min(
                            tab.lines
                                .get(new_line as usize)
                                .map_or(0, |l| l.chars().count() as u32),
                        );
                        tab.cursors.add(Position::new(new_line, new_col));
                    }
                }
            }
            true
        }

        // ── Column select up / down ────────────────────────────────────────────
        Action::ColumnSelectUp => {
            if let Some(idx) = state.active_tab() {
                if let Some(tab) = state.tabs_mut().get_mut(idx) {
                    let pos = tab.cursors.primary().pos();
                    if pos.line > 0 {
                        let new_line = pos.line - 1;
                        let new_col = pos.character.min(
                            tab.lines
                                .get(new_line as usize)
                                .map_or(0, |l| l.chars().count() as u32),
                        );
                        tab.cursors.add(Position::new(new_line, new_col));
                    }
                }
            }
            true
        }
        Action::ColumnSelectDown => {
            if let Some(idx) = state.active_tab() {
                if let Some(tab) = state.tabs_mut().get_mut(idx) {
                    let pos = tab.cursors.primary().pos();
                    let n_lines = tab.lines.len() as u32;
                    if pos.line + 1 < n_lines {
                        let new_line = pos.line + 1;
                        let new_col = pos.character.min(
                            tab.lines
                                .get(new_line as usize)
                                .map_or(0, |l| l.chars().count() as u32),
                        );
                        tab.cursors.add(Position::new(new_line, new_col));
                    }
                }
            }
            true
        }

        // ── Expand/shrink selection ───────────────────────────────────────────
        Action::ExpandSelection => {
            if let Some(idx) = state.active_tab() {
                if let Some(tab) = state.tabs_mut().get_mut(idx) {
                    let pos = tab.cursors.primary().pos();
                    let line_str = tab.lines.get(pos.line as usize).map_or("", String::as_str);
                    let chars: Vec<char> = line_str.chars().collect();
                    let len = chars.len();
                    let col = pos.character as usize;
                    let mut start = col;
                    let mut end = col;
                    if col < len && (chars[col].is_alphanumeric() || chars[col] == '_') {
                        while start > 0
                            && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_')
                        {
                            start -= 1;
                        }
                        while end < len && (chars[end].is_alphanumeric() || chars[end] == '_') {
                            end += 1;
                        }
                    } else if col < len {
                        end = col + 1;
                    }
                    use crabide_core::types::Selection;
                    tab.cursors.primary_mut().selection = Selection {
                        anchor: Position::new(pos.line, start as u32),
                        active: Position::new(pos.line, end as u32),
                    };
                }
            }
            true
        }
        Action::ShrinkSelection => {
            if let Some(idx) = state.active_tab() {
                if let Some(tab) = state.tabs_mut().get_mut(idx) {
                    let pos = tab.cursors.primary().pos();
                    tab.cursors.primary_mut().collapse_to_end();
                    tab.cursors.primary_mut().move_to(pos);
                }
            }
            true
        }

        // ── AddNextOccurrence and SelectAllOccurrences → forward to app ───────
        Action::AddNextOccurrence | Action::SelectAllOccurrences => false,

        // ── GitStageAll / UnstageAll — forward immediately to git panel ───────
        Action::GitStageAll => {
            state.git_panel.pending_stage_all = true;
            false // also forward to app for git_service dispatch
        }
        Action::GitUnstageAll => {
            state.git_panel.pending_unstage_all = true;
            false
        }

        // ── Peek view ──────────────────────────────────────────────────────────
        Action::ClosePeek => {
            state.peek.close();
            true
        }
        Action::PeekDefinition
        | Action::PeekDeclaration
        | Action::PeekImplementation
        | Action::PeekTypeDefinition
        | Action::PeekReferences => {
            // Forward to app for LSP request.
            false
        }

        // ── Extension custom commands ──────────────────────────────────────────
        Action::Custom(ref cmd) => {
            // Check if this is an extension panel toggle command.
            let panel_id = state
                .extension_panels
                .iter()
                .find(|(_, p)| p.registration.toggle_command.as_deref() == Some(cmd.as_str()))
                .map(|(id, _)| id.clone());
            if let Some(pid) = panel_id {
                if let Some(p) = state.extension_panels.get_mut(&pid) {
                    p.open = !p.open;
                }
                // Also tell the extension (for markdown-preview toggle state sync).
                state.extensions_panel.pending_execute_command = Some((cmd.clone(), vec![]));
                true
            } else {
                false
            }
        }

        _ => false,
    }
}

// ── Cursor movement implementation ────────────────────────────────────────────

// ── Cursor movement implementation ────────────────────────────────────────────

enum MoveKind {
    Up(usize),
    Down(usize),
    Left,
    Right,
    WordLeft,
    WordRight,
    LineStart,
    LineEnd,
    FileStart,
    FileEnd,
}

fn scroll_to_primary(state: &mut UiState) {
    if let Some(idx) = state.active_tab() {
        if let Some(tab) = state.tabs().get(idx) {
            state.pending_scroll_line = Some(tab.cursors.primary().pos().line as usize);
        }
    }
}

fn move_cursors(state: &mut UiState, kind: MoveKind, extend: bool) {
    let Some(active_idx) = state.active_tab() else {
        return;
    };
    let Some(tab) = state.tabs_mut().get_mut(active_idx) else {
        return;
    };

    // Clone lines (cheap Arc bump) to avoid borrow conflict between tab.lines and tab.cursors.
    let lines = tab.lines.clone();
    let n_lines = lines.len().max(1);

    tab.cursors.map_cursors(|cursor| {
        // When moving without extend: if selection exists, jump to its start/end.
        if !extend && cursor.has_selection() {
            match kind {
                MoveKind::Left => {
                    cursor.collapse_to_start();
                    return;
                }
                MoveKind::Right => {
                    cursor.collapse_to_end();
                    return;
                }
                _ => {}
            }
        }

        let pos = cursor.pos();
        let saved_preferred = cursor.preferred_col;
        let new_pos = compute_new_position(pos, &kind, &lines, n_lines, cursor.preferred_col);

        if extend {
            cursor.extend_to(new_pos);
        } else {
            cursor.move_to(new_pos);
        }

        // For vertical movements, restore the preferred column so that
        // repeated Up/Down through short lines lands back on the original column.
        match kind {
            MoveKind::Up(_) | MoveKind::Down(_) => {
                cursor.preferred_col = saved_preferred;
            }
            _ => {}
        }
    });
}

fn compute_new_position(
    pos: Position,
    kind: &MoveKind,
    lines: &[String],
    n_lines: usize,
    preferred_col: u32,
) -> Position {
    let line = pos.line as usize;
    let col = pos.character as usize;

    match kind {
        MoveKind::Left => {
            if col > 0 {
                Position::new(pos.line, col as u32 - 1)
            } else if line > 0 {
                let prev_line_len = line_char_count(lines, line - 1);
                Position::new(pos.line - 1, prev_line_len as u32)
            } else {
                pos
            }
        }
        MoveKind::Right => {
            let line_len = line_char_count(lines, line);
            if col < line_len {
                Position::new(pos.line, col as u32 + 1)
            } else if line + 1 < n_lines {
                Position::new(pos.line + 1, 0)
            } else {
                pos
            }
        }
        MoveKind::Up(n) => {
            let target_line = line.saturating_sub(*n);
            let target_len = line_char_count(lines, target_line);
            let target_col = (preferred_col as usize).min(target_len);
            Position::new(target_line as u32, target_col as u32)
        }
        MoveKind::Down(n) => {
            let target_line = (line + n).min(n_lines.saturating_sub(1));
            let target_len = line_char_count(lines, target_line);
            let target_col = (preferred_col as usize).min(target_len);
            Position::new(target_line as u32, target_col as u32)
        }
        MoveKind::WordLeft => {
            let line_str = lines.get(line).map_or("", String::as_str);
            let new_col = word_left(line_str, col);
            if new_col == 0 && col == 0 && line > 0 {
                let prev_len = line_char_count(lines, line - 1);
                Position::new(pos.line - 1, prev_len as u32)
            } else {
                Position::new(pos.line, new_col as u32)
            }
        }
        MoveKind::WordRight => {
            let line_str = lines.get(line).map_or("", String::as_str);
            let line_len = line_char_count(lines, line);
            let new_col = word_right(line_str, col);
            if new_col == line_len && col == line_len && line + 1 < n_lines {
                Position::new(pos.line + 1, 0)
            } else {
                Position::new(pos.line, new_col as u32)
            }
        }
        MoveKind::LineStart => Position::new(pos.line, 0),
        MoveKind::LineEnd => {
            let len = line_char_count(lines, line);
            Position::new(pos.line, len as u32)
        }
        MoveKind::FileStart => Position::ZERO,
        MoveKind::FileEnd => {
            let last_line = n_lines.saturating_sub(1);
            let last_col = line_char_count(lines, last_line);
            Position::new(last_line as u32, last_col as u32)
        }
    }
}

// ── Word-boundary helpers ─────────────────────────────────────────────────────

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Move left to the previous word boundary.
fn word_left(line: &str, col: usize) -> usize {
    let chars: Vec<char> = line.chars().collect();
    let mut i = col.min(chars.len());
    // Skip leading whitespace before the previous word.
    while i > 0 && chars[i - 1].is_whitespace() {
        i -= 1;
    }
    if i == 0 {
        return 0;
    }
    let word_mode = is_word_char(chars[i - 1]);
    while i > 0 && is_word_char(chars[i - 1]) == word_mode {
        i -= 1;
    }
    i
}

/// Move right to the next word boundary.
fn word_right(line: &str, col: usize) -> usize {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = col.min(len);
    if i == len {
        return len;
    }
    let word_mode = is_word_char(chars[i]);
    while i < len && is_word_char(chars[i]) == word_mode {
        i += 1;
    }
    // Skip trailing whitespace.
    while i < len && chars[i].is_whitespace() {
        i += 1;
    }
    i
}

// ── Dynamic extension panel renderer ─────────────────────────────────────────

/// Render one extension panel including its header strip and content blocks.
///
/// Returns `Some(NavigateTarget)` when the user clicks a row item.
pub(crate) fn render_extension_panel(
    ui: &mut egui::Ui,
    state: &mut UiState,
    title: &str,
    panel_id: &str,
    content: &[crabide_extensions::ContentBlock],
) -> Option<crabide_extensions::NavigateTarget> {
    let bg = cfg_to_egui(state.theme.ui_or(
        "sideBar.background",
        crabide_config::Color::rgb(0x25, 0x25, 0x26),
    ));
    let fg = cfg_to_egui(state.theme.ui_or(
        "editor.foreground",
        crabide_config::Color::rgb(0xcc, 0xcc, 0xcc),
    ));
    let muted = cfg_to_egui(state.theme.ui_or(
        "tab.inactiveForeground",
        crabide_config::Color::rgb(0x85, 0x85, 0x85),
    ));
    let hov_bg = cfg_to_egui(state.theme.ui_or(
        "list.hoverBackground",
        crabide_config::Color::rgb(0x2a, 0x2d, 0x2e),
    ));
    let header_bg = cfg_to_egui(state.theme.ui_or(
        "editorGroupHeader.tabsBackground",
        crabide_config::Color::rgb(0x25, 0x25, 0x26),
    ));

    ui.painter()
        .rect_filled(ui.available_rect_before_wrap(), 0.0, bg);

    // Header bar with title and close button.
    let hdr_height = 28.0;
    let avail_w = ui.available_width();
    let (hdr_rect, _) =
        ui.allocate_exact_size(egui::vec2(avail_w, hdr_height), egui::Sense::hover());
    ui.painter().rect_filled(hdr_rect, 0.0, header_bg);
    ui.painter().text(
        egui::pos2(hdr_rect.left() + 10.0, hdr_rect.center().y),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(11.0),
        muted,
    );
    // Close [×] button.
    let close_rect = egui::Rect::from_min_size(
        egui::pos2(hdr_rect.right() - 24.0, hdr_rect.min.y + 4.0),
        egui::vec2(20.0, 20.0),
    );
    let close_resp = ui.allocate_rect(close_rect, egui::Sense::click());
    ui.painter().text(
        close_rect.center(),
        egui::Align2::CENTER_CENTER,
        "×",
        egui::FontId::proportional(14.0),
        if close_resp.hovered() { fg } else { muted },
    );
    if close_resp.clicked() {
        if let Some(p) = state.extension_panels.get_mut(panel_id) {
            p.open = false;
        }
    }

    let mut nav: Option<crabide_extensions::NavigateTarget> = None;

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(4.0);

            if content.is_empty() {
                ui.add_space(8.0);
                ui.add(egui::Label::new(
                    egui::RichText::new("No content yet.")
                        .color(muted)
                        .italics()
                        .size(12.0),
                ));
                return;
            }

            for block in content {
                match block {
                    crabide_extensions::ContentBlock::Heading(text) => {
                        egui::Frame::NONE
                            .inner_margin(egui::Margin {
                                left: 10,
                                right: 0,
                                top: 6,
                                bottom: 2,
                            })
                            .show(ui, |ui| {
                                ui.add(egui::Label::new(
                                    egui::RichText::new(text).color(fg).size(12.0).strong(),
                                ));
                            });
                    }
                    crabide_extensions::ContentBlock::Paragraph(text) => {
                        egui::Frame::NONE
                            .inner_margin(egui::Margin::symmetric(10, 4))
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(text).color(fg).size(12.0),
                                    )
                                    .wrap(),
                                );
                            });
                    }
                    crabide_extensions::ContentBlock::Preformatted(text) => {
                        egui::Frame::NONE
                            .inner_margin(egui::Margin::symmetric(10, 4))
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(text)
                                            .color(fg)
                                            .font(egui::FontId::monospace(12.0)),
                                    )
                                    .wrap(),
                                );
                            });
                    }
                    crabide_extensions::ContentBlock::Separator => {
                        ui.add_space(2.0);
                        ui.separator();
                        ui.add_space(2.0);
                    }
                    crabide_extensions::ContentBlock::Rows(rows) => {
                        for row in rows {
                            let (row_rect, row_resp) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 22.0),
                                egui::Sense::click(),
                            );
                            if ui.is_rect_visible(row_rect) {
                                let row_bg = if row_resp.hovered() {
                                    hov_bg
                                } else {
                                    egui::Color32::TRANSPARENT
                                };
                                ui.painter().rect_filled(row_rect, 0.0, row_bg);
                                ui.painter().text(
                                    egui::pos2(row_rect.left() + 10.0, row_rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    &row.text,
                                    egui::FontId::proportional(12.0),
                                    fg,
                                );
                            }
                            let clicked = row_resp.clicked();
                            if let Some(tip) = &row.tooltip {
                                row_resp.on_hover_text(tip.as_str());
                            }
                            if clicked {
                                if let Some(target) = &row.on_click {
                                    nav = Some(target.clone());
                                }
                            }
                        }
                    }
                }
            }
        });

    nav
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn line_char_count(lines: &[String], line_idx: usize) -> usize {
    lines.get(line_idx).map_or(0, |l| l.chars().count())
}

// ── Performance profiler overlay ────────────────────────────────────────────────

/// Render the frame time / LSP latency / heap usage overlay window.
fn show_profiler_overlay(ui: &mut egui::Ui, state: &mut UiState) {
    let ctx = ui.ctx();
    let bg = cfg_to_egui(state.theme.ui_or(
        "editor.background",
        crabide_config::Color::rgb(0x1e, 0x1e, 0x1e),
    ));
    let fg = cfg_to_egui(state.theme.ui_or(
        "editor.foreground",
        crabide_config::Color::rgb(0xcc, 0xcc, 0xcc),
    ));

    let screen = ctx.content_rect();
    let win_w = 320.0_f32.min(screen.width() - 20.0);
    let right = screen.right() - 10.0;
    let top = screen.top() + 50.0;

    egui::Window::new("Performance")
        .id(egui::Id::new("profiler_window"))
        .resizable(false)
        .movable(true)
        .title_bar(true)
        .collapsible(true)
        .fixed_pos(egui::pos2(right - win_w, top))
        .fixed_size(egui::vec2(win_w, 0.0))
        .frame(
            egui::Frame::default()
                .fill(bg)
                .corner_radius(egui::CornerRadius::same(6)),
        )
        .show(ctx, |ui| {
            ui.set_width(win_w - 12.0);
            ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);
            ui.style_mut().override_font_id = Some(egui::FontId::monospace(12.0));

            // Frame timing section
            ui.label(egui::RichText::new("Frame Timing").color(fg).strong());
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("FPS:  {:.1}", state.profiler.fps)).color(fg));
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Avg:  {:.2} ms", state.profiler.avg_ms)).color(fg),
                );
                ui.label(
                    egui::RichText::new(format!("P95:  {:.2} ms", state.profiler.p95_ms)).color(fg),
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Min:  {:.2} ms", state.profiler.min_ms)).color(fg),
                );
                ui.label(
                    egui::RichText::new(format!("Max:  {:.2} ms", state.profiler.max_ms)).color(fg),
                );
            });

            ui.add_space(8.0);

            // LSP latency section
            ui.label(egui::RichText::new("LSP Latency").color(fg).strong());
            ui.separator();
            let cnt = state.lsp_latency.count;
            if cnt > 0 {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "Avg: {:.2} ms  Max: {:.2} ms  Samples: {cnt}",
                            state.lsp_latency.avg_ms, state.lsp_latency.max_ms
                        ))
                        .color(fg),
                    );
                });
            } else {
                ui.label(egui::RichText::new("No LSP requests yet").color(fg));
            }

            ui.add_space(8.0);

            // Heap usage section
            ui.label(egui::RichText::new("Heap").color(fg).strong());
            ui.separator();
            let heap_mb = state.heap_used_bytes as f64 / (1024.0 * 1024.0);
            ui.label(egui::RichText::new(format!("Used: {heap_mb:.1} MB")).color(fg));

            ui.add_space(8.0);

            // Reset button
            if ui.button("Reset LSP stats").clicked() {
                state.lsp_latency = LspLatencyTracker::default();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabide_config::{Key, KeyChord, Modifiers};

    // ── egui_key_to_chord tests ─────────────────────────────────────────

    #[test]
    fn egui_key_to_chord_simple() {
        let chord = egui_key_to_chord(egui::Key::A, egui::Modifiers::default());
        assert_eq!(chord, KeyChord::new(Modifiers::empty(), Key::Char('a')));
    }

    #[test]
    fn egui_key_to_chord_ctrl_a() {
        let chord = egui_key_to_chord(
            egui::Key::A,
            egui::Modifiers {
                ctrl: true,
                ..Default::default()
            },
        );
        assert_eq!(chord, KeyChord::new(Modifiers::CTRL, Key::Char('a')));
    }

    #[test]
    fn egui_key_to_chord_ctrl_shift_a() {
        let chord = egui_key_to_chord(
            egui::Key::A,
            egui::Modifiers {
                ctrl: true,
                shift: true,
                ..Default::default()
            },
        );
        assert_eq!(
            chord,
            KeyChord::new(Modifiers::CTRL | Modifiers::SHIFT, Key::Char('a'))
        );
    }

    #[test]
    fn egui_key_to_chord_arrow_keys() {
        let chord = egui_key_to_chord(egui::Key::ArrowUp, egui::Modifiers::default());
        assert_eq!(chord, KeyChord::new(Modifiers::empty(), Key::Up));

        let chord = egui_key_to_chord(egui::Key::ArrowDown, egui::Modifiers::default());
        assert_eq!(chord, KeyChord::new(Modifiers::empty(), Key::Down));

        let chord = egui_key_to_chord(egui::Key::ArrowLeft, egui::Modifiers::default());
        assert_eq!(chord, KeyChord::new(Modifiers::empty(), Key::Left));

        let chord = egui_key_to_chord(egui::Key::ArrowRight, egui::Modifiers::default());
        assert_eq!(chord, KeyChord::new(Modifiers::empty(), Key::Right));
    }

    #[test]
    fn egui_key_to_chord_function_keys() {
        for i in 1..=12 {
            let egui_key = match i {
                1 => egui::Key::F1,
                2 => egui::Key::F2,
                3 => egui::Key::F3,
                4 => egui::Key::F4,
                5 => egui::Key::F5,
                6 => egui::Key::F6,
                7 => egui::Key::F7,
                8 => egui::Key::F8,
                9 => egui::Key::F9,
                10 => egui::Key::F10,
                11 => egui::Key::F11,
                12 => egui::Key::F12,
                _ => unreachable!("F-key out of range 1..=12: {i}"),
            };
            let chord = egui_key_to_chord(egui_key, egui::Modifiers::default());
            assert_eq!(chord, KeyChord::new(Modifiers::empty(), Key::F(i)));
        }
    }

    #[test]
    fn egui_key_to_chord_unknown_key() {
        let chord = egui_key_to_chord(egui::Key::Minus, egui::Modifiers::default());
        assert_eq!(chord, KeyChord::new(Modifiers::empty(), Key::Char('-')));
    }

    // ── is_word_char tests ──────────────────────────────────────────────

    #[test]
    fn word_char_alphanumeric() {
        assert!(is_word_char('a'));
        assert!(is_word_char('Z'));
        assert!(is_word_char('5'));
        assert!(is_word_char('_'));
    }

    #[test]
    fn word_char_non_word() {
        assert!(!is_word_char(' '));
        assert!(!is_word_char('.'));
        assert!(!is_word_char('-'));
        assert!(!is_word_char('!'));
    }

    // ── word_left tests ─────────────────────────────────────────────────

    #[test]
    fn word_left_middle_of_word() {
        // col=6 points to 'w' in "hello world" -> skips back to start (0)
        let result = word_left("hello world", 6);
        assert_eq!(result, 0);
    }

    #[test]
    fn word_left_from_whitespace() {
        // "hello   world", col=8 points inside whitespace -> goes to prev word start
        let result = word_left("hello   world", 8);
        assert_eq!(result, 0); // goes to start of "hello"
    }

    #[test]
    fn word_left_at_start() {
        let result = word_left("hello", 0);
        assert_eq!(result, 0);
    }

    #[test]
    fn word_left_after_word() {
        let result = word_left("hello world", 11);
        assert_eq!(result, 6);
    }

    #[test]
    fn word_left_empty_line() {
        let result = word_left("", 0);
        assert_eq!(result, 0);
    }

    // ── word_right tests ────────────────────────────────────────────────

    #[test]
    fn word_right_from_word_start() {
        // "hello world", col=0 -> moves to end of "hello" + whitespace = 6
        let result = word_right("hello world", 0);
        assert_eq!(result, 6);
    }

    #[test]
    fn word_right_from_whitespace() {
        // "hello   world", col=8 -> moves to end of string (13)
        let result = word_right("hello   world", 8);
        assert_eq!(result, 13);
    }

    #[test]
    fn word_right_at_end() {
        let result = word_right("hello", 5);
        assert_eq!(result, 5);
    }

    #[test]
    fn word_right_empty_line() {
        let result = word_right("", 0);
        assert_eq!(result, 0);
    }

    #[test]
    fn word_right_past_end() {
        let result = word_right("hi", 10);
        assert_eq!(result, 2);
    }

    // ── compute_new_position tests ──────────────────────────────────────

    fn lines() -> Vec<String> {
        vec![
            "hello world".into(),
            "rust".into(),
            "a".into(),
            String::new(),
            "last".into(),
        ]
    }

    #[test]
    fn compute_left_within_line() {
        let pos = Position::new(0, 5);
        let new = compute_new_position(pos, &MoveKind::Left, &lines(), 5, 0);
        assert_eq!(new, Position::new(0, 4));
    }

    #[test]
    fn compute_left_wrap_to_prev_line() {
        let pos = Position::new(1, 0);
        let new = compute_new_position(pos, &MoveKind::Left, &lines(), 5, 0);
        assert_eq!(new, Position::new(0, 11));
    }

    #[test]
    fn compute_left_at_file_start() {
        let pos = Position::new(0, 0);
        let new = compute_new_position(pos, &MoveKind::Left, &lines(), 5, 0);
        assert_eq!(new, Position::ZERO);
    }

    #[test]
    fn compute_right_within_line() {
        let pos = Position::new(0, 0);
        let new = compute_new_position(pos, &MoveKind::Right, &lines(), 5, 0);
        assert_eq!(new, Position::new(0, 1));
    }

    #[test]
    fn compute_right_wrap_to_next_line() {
        let pos = Position::new(0, 11);
        let new = compute_new_position(pos, &MoveKind::Right, &lines(), 5, 0);
        assert_eq!(new, Position::new(1, 0));
    }

    #[test]
    fn compute_right_at_file_end() {
        let pos = Position::new(4, 4);
        let new = compute_new_position(pos, &MoveKind::Right, &lines(), 5, 0);
        assert_eq!(new, Position::new(4, 4));
    }

    #[test]
    fn compute_up() {
        let pos = Position::new(2, 3);
        let new = compute_new_position(pos, &MoveKind::Up(1), &lines(), 5, 3);
        assert_eq!(new, Position::new(1, 3));
    }

    #[test]
    fn compute_up_clamps_to_line_length() {
        let pos = Position::new(1, 10);
        let new = compute_new_position(pos, &MoveKind::Up(1), &lines(), 5, 10);
        assert_eq!(new, Position::new(0, 10)); // line 0 has length 11, so col 10 is OK
    }

    #[test]
    fn compute_up_at_file_start() {
        let pos = Position::new(0, 0);
        let new = compute_new_position(pos, &MoveKind::Up(1), &lines(), 5, 0);
        assert_eq!(new, Position::ZERO);
    }

    #[test]
    fn compute_down() {
        let pos = Position::new(0, 0);
        let new = compute_new_position(pos, &MoveKind::Down(1), &lines(), 5, 0);
        assert_eq!(new, Position::new(1, 0));
    }

    #[test]
    fn compute_down_at_file_end() {
        let pos = Position::new(4, 0);
        let new = compute_new_position(pos, &MoveKind::Down(1), &lines(), 5, 0);
        assert_eq!(new, Position::new(4, 0));
    }

    #[test]
    fn compute_line_start() {
        let pos = Position::new(2, 3);
        let new = compute_new_position(pos, &MoveKind::LineStart, &lines(), 5, 0);
        assert_eq!(new, Position::new(2, 0));
    }

    #[test]
    fn compute_line_end() {
        let pos = Position::new(0, 0);
        let new = compute_new_position(pos, &MoveKind::LineEnd, &lines(), 5, 0);
        assert_eq!(new, Position::new(0, 11));
    }

    #[test]
    fn compute_file_start() {
        let pos = Position::new(3, 1);
        let new = compute_new_position(pos, &MoveKind::FileStart, &lines(), 5, 0);
        assert_eq!(new, Position::ZERO);
    }

    #[test]
    fn compute_file_end() {
        let pos = Position::ZERO;
        let new = compute_new_position(pos, &MoveKind::FileEnd, &lines(), 5, 0);
        assert_eq!(new, Position::new(4, 4));
    }

    #[test]
    fn compute_word_left_wraps_line() {
        let pos = Position::new(1, 0);
        let new = compute_new_position(pos, &MoveKind::WordLeft, &lines(), 5, 0);
        assert_eq!(new, Position::new(0, 11));
    }

    #[test]
    fn compute_word_right_wraps_line() {
        let pos = Position::new(4, 4);
        let new = compute_new_position(pos, &MoveKind::WordRight, &lines(), 5, 0);
        assert_eq!(new, Position::new(4, 4));
    }

    // ── line_char_count tests ───────────────────────────────────────────

    #[test]
    fn line_char_count_returns_length() {
        assert_eq!(line_char_count(&lines(), 0), 11);
        assert_eq!(line_char_count(&lines(), 1), 4);
        assert_eq!(line_char_count(&lines(), 2), 1);
        assert_eq!(line_char_count(&lines(), 3), 0);
        assert_eq!(line_char_count(&lines(), 4), 4);
    }

    #[test]
    fn line_char_count_out_of_bounds() {
        assert_eq!(line_char_count(&lines(), 10), 0);
    }

    // ── handle_ui_action tests ──────────────────────────────────────────

    fn make_ui_state() -> UiState {
        let theme = crabide_config::ColorTheme {
            id: "test".into(),
            name: "Test".into(),
            theme_type: crabide_config::ThemeType::Dark,
            ui_colors: indexmap::IndexMap::new(),
            token_colors: Vec::new(),
        };
        let keybindings = crabide_config::KeybindingEngine::with_defaults();
        UiState::new(theme, keybindings)
    }

    #[test]
    fn handle_command_palette_toggle() {
        let mut state = make_ui_state();
        assert!(!state.command_palette.visible);
        assert!(handle_ui_action(Action::CommandPalette, &mut state));
        assert!(state.command_palette.visible);
        assert!(handle_ui_action(Action::CommandPalette, &mut state));
        assert!(!state.command_palette.visible);
    }

    #[test]
    fn handle_toggle_sidebar() {
        let mut state = make_ui_state();
        assert!(state.sidebar_visible);
        assert!(handle_ui_action(Action::ToggleSidebar, &mut state));
        assert!(!state.sidebar_visible);
    }

    #[test]
    fn handle_zoom_in_out_reset() {
        let mut state = make_ui_state();
        state.font_size = 14.0;
        assert!(handle_ui_action(Action::ZoomIn, &mut state));
        assert_eq!(state.font_size, 15.0);
        assert!(handle_ui_action(Action::ZoomOut, &mut state));
        assert_eq!(state.font_size, 14.0);
        assert!(handle_ui_action(Action::ZoomReset, &mut state));
        assert_eq!(state.font_size, 14.0);
    }

    #[test]
    fn handle_toggle_word_wrap() {
        let mut state = make_ui_state();
        assert!(!state.word_wrap);
        assert!(handle_ui_action(Action::ToggleWordWrap, &mut state));
        assert!(state.word_wrap);
    }

    #[test]
    fn handle_toggle_terminal() {
        let mut state = make_ui_state();
        assert!(!state.terminal.visible);
        assert!(handle_ui_action(Action::ToggleTerminal, &mut state));
        assert!(state.terminal.visible);
        assert!(state.terminal.has_focus);
    }

    #[test]
    fn handle_next_prev_tab_empty() {
        let mut state = make_ui_state();
        // No tabs — no-op.
        assert!(handle_ui_action(Action::NextTab, &mut state));
        assert!(handle_ui_action(Action::PreviousTab, &mut state));
    }

    #[test]
    fn handle_fuzzy_find_file_opens() {
        let mut state = make_ui_state();
        let handled = handle_ui_action(Action::FuzzyFindFile, &mut state);
        assert!(!handled); // forwarded to app
        assert!(state.fuzzy_finder.visible);
    }

    #[test]
    fn handle_goto_line_open() {
        let mut state = make_ui_state();
        assert!(handle_ui_action(Action::GotoLine, &mut state));
        assert!(state.goto_line.visible);
    }

    #[test]
    fn handle_find_opens_panel() {
        let mut state = make_ui_state();
        assert!(handle_ui_action(Action::Find, &mut state));
        assert!(state.find_replace.visible);
        assert!(!state.find_replace.replace_visible);
    }

    #[test]
    fn handle_find_replace_opens_panel() {
        let mut state = make_ui_state();
        assert!(handle_ui_action(Action::FindReplace, &mut state));
        assert!(state.find_replace.visible);
        assert!(state.find_replace.replace_visible);
    }

    #[test]
    fn handle_toggle_panel_behaviors() {
        let mut state = make_ui_state();
        // TogglePanel is still unhandled (returns false) — it's a panel placeholder.
        assert!(!handle_ui_action(Action::TogglePanel, &mut state));
        // ToggleOutputPanel is now handled internally.
        assert!(!state.output_panel.visible);
        assert!(handle_ui_action(Action::ToggleOutputPanel, &mut state));
        assert!(state.output_panel.visible);
        // Toggle again to close
        assert!(handle_ui_action(Action::ToggleOutputPanel, &mut state));
        assert!(!state.output_panel.visible);
    }

    #[test]
    fn handle_unhandled_action_returns_false() {
        let mut state = make_ui_state();
        assert!(!handle_ui_action(Action::SaveFile, &mut state));
        assert!(!handle_ui_action(Action::OpenFile, &mut state));
        assert!(!handle_ui_action(Action::Quit, &mut state));
    }

    // ── PaneKind tests ──────────────────────────────────────────────────

    #[test]
    fn pane_kind_derives() {
        assert_ne!(PaneKind::EditorGroup(0), PaneKind::FileExplorer);
    }
}
