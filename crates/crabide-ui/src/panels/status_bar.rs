//! Status bar: language, line:col, encoding, EOL style, git toggle, LSP
//! indicator, and extension status bar contributions.
//!
//! Rendered as a `TopBottomPanel::bottom`.

use crabide_config::Action;
use crabide_extensions::StatusBarAlignment;

use crate::state::{LspStatus, StatusBarItem, UiState, cfg_to_egui};

/// Height of the status bar in logical pixels.
pub const STATUS_BAR_HEIGHT: f32 = 22.0;

/// Render the bottom status bar.
///
/// Returns a `Vec` of backend actions triggered by user interaction in the bar
/// this frame (e.g. `Action::ToggleGit`).
pub fn show(ui: &mut egui::Ui, state: &UiState) -> Vec<Action> {
    let bg_color = cfg_to_egui(state.theme.ui_or(
        "statusBar.background",
        crabide_config::Color::rgb(0x00, 0x7a, 0xcc),
    ));
    let fg_color = cfg_to_egui(state.theme.ui_or(
        "statusBar.foreground",
        crabide_config::Color::rgb(0xff, 0xff, 0xff),
    ));

    let mut triggered: Vec<Action> = Vec::new();

    egui::Panel::bottom("status_bar")
        .exact_size(STATUS_BAR_HEIGHT)
        .frame(
            egui::Frame::NONE
                .fill(bg_color)
                .inner_margin(egui::Margin::symmetric(8, 2)),
        )
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;

                // ── Left items ────────────────────────────────────────────────

                // Git status indicator — clickable to toggle git service.
                let (git_text, git_color) = if state.git_enabled {
                    let branch = state
                        .git_branch
                        .as_deref()
                        .map(|b| format!("⎇ {}", b))
                        .unwrap_or_else(|| "⎇".to_owned());
                    (branch, fg_color)
                } else {
                    ("⎇".to_owned(), fg_color.gamma_multiply(0.45))
                };

                let git_btn =
                    egui::Button::new(egui::RichText::new(&git_text).color(git_color).size(12.0))
                        .frame(false);
                if ui
                    .add(git_btn)
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(if state.git_enabled {
                        "Git enabled -- click to disable"
                    } else {
                        "Git disabled -- click to enable"
                    })
                    .clicked()
                {
                    triggered.push(Action::ToggleGit);
                }

                // Debugger toggle button — mirrors the git toggle style.
                let (dbg_text, dbg_color) = if state.dap_panel.enabled {
                    let dbg_label = if state.dap_panel.session_active {
                        if state.dap_panel.paused {
                            "⏸ 🐛".to_owned()
                        } else {
                            "▶ 🐛".to_owned()
                        }
                    } else {
                        "🐛".to_owned()
                    };
                    (dbg_label, egui::Color32::from_rgb(0x4e, 0xc9, 0xb0))
                } else {
                    ("🐛".to_owned(), fg_color.gamma_multiply(0.45))
                };
                let dbg_btn =
                    egui::Button::new(egui::RichText::new(&dbg_text).color(dbg_color).size(12.0))
                        .frame(false);
                if ui
                    .add(dbg_btn)
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(if state.dap_panel.enabled {
                        "Debugger enabled — click to disable"
                    } else {
                        "Debugger disabled — click to enable"
                    })
                    .clicked()
                {
                    triggered.push(Action::ToggleDebug);
                }

                // App name / version (always shown).
                status_item(ui, "⭐ crabide 0.1.0", fg_color);

                // Status message (timed).
                if let Some((msg, _)) = &state.status_message {
                    ui.separator();
                    status_item(ui, msg, fg_color.gamma_multiply(0.85));
                }

                // Left-aligned extension status bar contributions (e.g. git blame).
                for (_ext_id, item) in state
                    .extensions_panel
                    .status_bar_items
                    .iter()
                    .filter(|(_, i)| i.alignment == StatusBarAlignment::Left)
                {
                    ui.separator();
                    render_status_item(ui, item, fg_color, &mut triggered);
                }

                // ── Right items (push to right) ───────────────────────────────
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;

                    // Right-aligned extension contributions (e.g. theme switcher) — rendered
                    // first so they sit at the far right, before LSP/document info.
                    for (_ext_id, item) in state
                        .extensions_panel
                        .status_bar_items
                        .iter()
                        .filter(|(_, i)| i.alignment == StatusBarAlignment::Right)
                    {
                        render_status_item(ui, item, fg_color, &mut triggered);
                        ui.separator();
                    }

                    // LSP indicators.
                    for (lang, lsp_status) in &state.lsp_indicators {
                        let (icon, color) = match lsp_status {
                            LspStatus::Starting => ("⏳", fg_color.gamma_multiply(0.7)),
                            LspStatus::Ready => ("✅", fg_color),
                            LspStatus::Error => ("⚠", egui::Color32::from_rgb(0xff, 0x80, 0x80)),
                        };
                        status_item(ui, &format!("{icon} {lang}"), color);
                    }

                    // Active document info.
                    if let Some(tab) = state.active_tab_ref() {
                        // EOL style.
                        status_item(ui, "LF", fg_color);

                        // Encoding.
                        status_item(ui, "UTF-8", fg_color);

                        // Language.
                        status_item(ui, tab.language.as_str(), fg_color);

                        // Line : Col (1-based).
                        let primary = tab.cursors.primary();
                        let line = primary.pos().line + 1;
                        let col = primary.pos().character + 1;
                        status_item(ui, &format!("Ln {line}, Col {col}"), fg_color);
                    }
                });
            });
        });

    triggered
}

fn status_item(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.add(egui::Label::new(
        egui::RichText::new(text).color(color).size(12.0),
    ));
}

/// Render a single extension status bar item, emitting any triggered action.
fn render_status_item(
    ui: &mut egui::Ui,
    item: &StatusBarItem,
    fg_color: egui::Color32,
    triggered: &mut Vec<Action>,
) {
    let text_widget = egui::RichText::new(&item.text)
        .color(fg_color.gamma_multiply(0.85))
        .size(12.0);
    if let Some(cmd) = &item.command {
        let btn = egui::Button::new(text_widget).frame(false);
        let mut resp = ui.add(btn).on_hover_cursor(egui::CursorIcon::PointingHand);
        if let Some(tip) = &item.tooltip {
            resp = resp.on_hover_text(tip.as_str());
        }
        if resp.clicked() {
            triggered.push(Action::Custom(cmd.clone()));
        }
    } else {
        let resp = ui.add(egui::Label::new(text_widget));
        if let Some(tip) = &item.tooltip {
            resp.on_hover_text(tip.as_str());
        }
    }
}
