//! Workspace-grep results panel (Ctrl+Shift+F).
//!
//! Shows the search query bar at the top and a scrollable list of grep
//! matches below.  Clicking a result sets `state.pending_open_path` and
//! `state.pending_scroll_line` then returns `Action::OpenFile` so the app
//! can open / activate the relevant tab.
//!
//! # Layout
//! ```text
//! ┌── Find in Files ─────────────────────────────────────────────────────┐
//! │ 🔍 [query___________] [.*] [Aa]  [Search]  [42 results]             │
//! ├── results ────────────────────────────────────────────────────────────│
//! │  src/app.rs                                                          │
//! │    42 │ fn handle_action(…)                                          │
//! │    87 │ match action {                                               │
//! │  src/lib.rs                                                          │
//! │    12 │ use crate::Action;                                           │
//! └───────────────────────────────────────────────────────────────────────┘
//! ```

use std::path::PathBuf;

use crabide_config::{Action, Color};

use crate::state::{UiState, cfg_to_egui};

// ── Public entry point ────────────────────────────────────────────────────────

/// Render the workspace search panel.
///
/// Returns `Some(Action::OpenFile)` when the user clicks a result (the target
/// path + line are stored in `state.pending_open_path` /
/// `state.pending_scroll_line`).
/// Returns `Some(Action::FindInFiles)` when the user presses Enter or clicks
/// the Search button (the app should then run the grep and populate results).
pub fn show(ui: &mut egui::Ui, state: &mut UiState, actions: &mut Vec<Action>) {
    if !state.workspace_search.visible {
        return;
    }

    let bar_bg = cfg_to_egui(
        state
            .theme
            .ui_or("editorWidget.background", Color::rgb(0x25, 0x25, 0x26)),
    );
    let border_color = cfg_to_egui(
        state
            .theme
            .ui_or("editorWidget.border", Color::rgb(0x45, 0x45, 0x45)),
    );
    let result_bg = cfg_to_egui(
        state
            .theme
            .ui_or("sideBar.background", Color::rgb(0x25, 0x25, 0x26)),
    );
    let result_fg = cfg_to_egui(
        state
            .theme
            .ui_or("sideBar.foreground", Color::rgb(0xcc, 0xcc, 0xcc)),
    );
    let file_header_fg = cfg_to_egui(
        state
            .theme
            .ui_or("sideBarTitle.foreground", Color::rgb(0xe0, 0xe0, 0xe0)),
    );
    let line_num_fg = egui::Color32::from_rgb(0x75, 0x75, 0x75);
    let sel_bg = cfg_to_egui(state.theme.ui_or(
        "list.activeSelectionBackground",
        Color::rgb(0x09, 0x47, 0x71),
    ));
    let sel_fg = cfg_to_egui(state.theme.ui_or(
        "list.activeSelectionForeground",
        Color::rgb(0xff, 0xff, 0xff),
    ));

    egui::Frame::NONE
        .fill(bar_bg)
        .stroke(egui::Stroke::new(1.0, border_color))
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 2.0);

            // ── Search bar ────────────────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Find in Files")
                        .size(12.0)
                        .color(egui::Color32::GRAY),
                );

                let query_resp = ui.add(
                    egui::TextEdit::singleline(&mut state.workspace_search.query)
                        .desired_width(200.0)
                        .font(egui::TextStyle::Monospace)
                        .hint_text("Search workspace…"),
                );
                if state.workspace_search.just_opened {
                    query_resp.request_focus();
                    state.workspace_search.just_opened = false;
                }

                // Track query changes for debounced auto-search.
                if query_resp.changed() {
                    state.workspace_search.last_change = Some(std::time::Instant::now());
                }

                // Auto-trigger search after debounce (300 ms of inactivity).
                const DEBOUNCE_MS: u64 = 300;
                if let Some(t) = state.workspace_search.last_change {
                    if t.elapsed().as_millis() >= DEBOUNCE_MS as u128 {
                        if !state.workspace_search.query.is_empty()
                            && !state.workspace_search.is_searching
                        {
                            actions.push(Action::FindInFiles);
                        }
                        state.workspace_search.last_change = None;
                    }
                }

                // Flags
                flag_toggle(ui, &mut state.workspace_search.use_regex, ".*", "Regex");
                flag_toggle(
                    ui,
                    &mut state.workspace_search.case_sensitive,
                    "Aa",
                    "Case sensitive",
                );

                // Search button / Enter
                let search_clicked = ui.small_button("Search").clicked();
                let enter_pressed =
                    query_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                if search_clicked || enter_pressed {
                    actions.push(Action::FindInFiles);
                }

                // Result count / spinner
                if state.workspace_search.is_searching {
                    ui.label(
                        egui::RichText::new("Searching…")
                            .size(11.0)
                            .color(egui::Color32::GRAY),
                    );
                } else {
                    let count = state.workspace_search.results.len();
                    let label = if count == 0 {
                        "No results".to_owned()
                    } else {
                        format!("{count} result{}", if count == 1 { "" } else { "s" })
                    };
                    ui.label(
                        egui::RichText::new(label)
                            .size(11.0)
                            .color(egui::Color32::GRAY),
                    );
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("×").on_hover_text("Close").clicked() {
                        state.workspace_search.visible = false;
                    }
                });
            });
        });

    if state.workspace_search.results.is_empty() {
        return;
    }

    // ── Results list ──────────────────────────────────────────────────────────
    // Group results by file for display.
    egui::ScrollArea::vertical()
        .id_salt("workspace_search_results")
        .max_height(300.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            // Collect file groups: (path, indices into results).
            let results = &state.workspace_search.results;
            let mut current_path: Option<PathBuf> = None;

            for (i, m) in results.iter().enumerate() {
                // File header when path changes.
                if current_path.as_ref() != Some(&m.path) {
                    current_path = Some(m.path.clone());
                    let file_name = m
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| m.path.to_string_lossy().into_owned());
                    egui::Frame::NONE
                        .fill(bar_bg)
                        .inner_margin(egui::Margin::symmetric(8, 3))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                egui::RichText::new(file_name)
                                    .size(12.0)
                                    .strong()
                                    .color(file_header_fg),
                            );
                        });
                }

                let is_sel = i == state.workspace_search.selected_idx;
                let row_bg = if is_sel { sel_bg } else { result_bg };
                let row_fg = if is_sel { sel_fg } else { result_fg };

                let row_resp = egui::Frame::NONE
                    .fill(row_bg)
                    .inner_margin(egui::Margin {
                        left: 24,
                        right: 8,
                        top: 1,
                        bottom: 1,
                    })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                            // Line number
                            ui.label(
                                egui::RichText::new(format!("{:>4}", m.line_number + 1))
                                    .size(11.0)
                                    .monospace()
                                    .color(line_num_fg),
                            );
                            ui.label(
                                egui::RichText::new("│")
                                    .size(11.0)
                                    .monospace()
                                    .color(line_num_fg),
                            );
                            // Line preview (trimmed)
                            let preview = m.line_text.trim();
                            let preview = if preview.len() > 100 {
                                &preview[..100]
                            } else {
                                preview
                            };
                            ui.label(
                                egui::RichText::new(preview)
                                    .size(12.0)
                                    .monospace()
                                    .color(row_fg),
                            );
                        });
                    });

                let row_resp = row_resp.response.interact(egui::Sense::click());
                if row_resp.clicked() {
                    state.workspace_search.selected_idx = i;
                    state.pending_open_path = Some(m.path.clone());
                    state.pending_scroll_line = Some(m.line_number);
                    actions.push(Action::OpenFile);
                }
                if row_resp.hovered() {
                    state.workspace_search.selected_idx = i;
                }
            }
        });
}

// ── Widget helpers ────────────────────────────────────────────────────────────

fn flag_toggle(ui: &mut egui::Ui, state: &mut bool, label: &str, tooltip: &str) {
    let color = if *state {
        egui::Color32::from_rgb(0x00, 0x7a, 0xcc)
    } else {
        egui::Color32::GRAY
    };
    let btn = ui
        .add(
            egui::Button::new(egui::RichText::new(label).size(11.0).color(color))
                .small()
                .frame(true),
        )
        .on_hover_text(tooltip);
    if btn.clicked() {
        *state = !*state;
    }
}
