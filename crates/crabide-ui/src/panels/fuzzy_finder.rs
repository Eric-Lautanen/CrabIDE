//! Ctrl+P fuzzy file finder overlay.
//!
//! Displays a floating window with a text input and a list of matching file
//! paths, similar to VS Code's "Quick Open" (Ctrl+P).
//!
//! # Behaviour
//! * Opening: set `state.fuzzy_finder.visible = true` (the app also populates
//!   `state.fuzzy_finder.finder` with workspace file paths).
//! * Scoring: nucleo fuzzy matching is run every frame when the query changes.
//! * Confirming: pressing Enter or clicking a row sets
//!   `state.pending_open_path` and returns `Action::OpenFile`.
//! * Closing: Escape or click-outside dismisses the overlay.

use std::path::PathBuf;

use crabide_config::{Action, Color};
use crabide_search::FUZZY_MAX_RESULTS;

use crate::state::{cfg_to_egui, UiState};

// ── Public entry point ────────────────────────────────────────────────────────

/// Render the fuzzy-finder window.
///
/// Returns `Some(Action::OpenFile)` when the user confirms a selection (the
/// confirmed path is stored in `state.pending_open_path`).  The caller should
/// also close the fuzzy finder after handling the returned action.
pub fn show(ctx: &egui::Context, state: &mut UiState) -> Option<Action> {
    if !state.fuzzy_finder.visible {
        return None;
    }

    // ── Keyboard navigation ───────────────────────────────────────────────────
    let (arrow_up, arrow_down, enter, escape) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::Enter),
            i.key_pressed(egui::Key::Escape),
        )
    });

    let n = state.fuzzy_finder.results.len();
    if arrow_up && n > 0 {
        state.fuzzy_finder.selected_idx = state.fuzzy_finder.selected_idx.saturating_sub(1);
    }
    if arrow_down && n > 0 {
        state.fuzzy_finder.selected_idx =
            (state.fuzzy_finder.selected_idx + 1).min(n.saturating_sub(1));
    }

    let mut confirmed: Option<PathBuf> = None;
    let mut close = escape;

    if enter && n > 0 {
        let idx = state.fuzzy_finder.selected_idx.min(n - 1);
        confirmed = Some(state.fuzzy_finder.results[idx].clone());
        close = true;
    }

    // ── Extract colours before the window closure ─────────────────────────────
    let input_bg = cfg_to_egui(
        state
            .theme
            .ui_or("input.background", Color::rgb(0x3c, 0x3c, 0x3c)),
    );
    let input_fg = cfg_to_egui(
        state
            .theme
            .ui_or("input.foreground", Color::rgb(0xcc, 0xcc, 0xcc)),
    );
    let sel_bg = cfg_to_egui(state.theme.ui_or(
        "list.activeSelectionBackground",
        Color::rgb(0x09, 0x47, 0x71),
    ));
    let sel_fg = cfg_to_egui(state.theme.ui_or(
        "list.activeSelectionForeground",
        Color::rgb(0xff, 0xff, 0xff),
    ));
    let item_fg = cfg_to_egui(
        state
            .theme
            .ui_or("sideBar.foreground", Color::rgb(0xcc, 0xcc, 0xcc)),
    );
    let drop_bg = cfg_to_egui(
        state
            .theme
            .ui_or("dropdown.background", Color::rgb(0x3c, 0x3c, 0x3c)),
    );
    let hint_fg = egui::Color32::from_rgb(0x88, 0x88, 0x88);

    let mut query = state.fuzzy_finder.query.clone();
    let mut selected_idx = state.fuzzy_finder.selected_idx;
    let display_labels: Vec<String> = state
        .fuzzy_finder
        .result_labels
        .iter()
        .take(FUZZY_MAX_RESULTS)
        .cloned()
        .collect();
    let file_count = state.fuzzy_finder.finder.index_len();

    // ── Window ────────────────────────────────────────────────────────────────
    let screen = ctx.content_rect();
    let win_width = 560.0_f32.min(screen.width() - 40.0);
    let win_left = screen.center().x - win_width / 2.0;
    let win_top = screen.top() + 60.0;

    egui::Window::new("##fuzzy_finder")
        .id(egui::Id::new("fuzzy_finder_window"))
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
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;

            // ── Query input ───────────────────────────────────────────────────
            egui::Frame::default()
                .fill(input_bg)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.set_width(win_width - 20.0);
                    let hint = if file_count == 0 {
                        "Open a folder first (Ctrl+K Ctrl+O)…".to_owned()
                    } else {
                        format!("Search {file_count} files…")
                    };
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut query)
                            .font(egui::TextStyle::Monospace)
                            .text_color(input_fg)
                            .frame(egui::Frame::NONE)
                            .desired_width(f32::INFINITY)
                            .hint_text(egui::RichText::new(hint).color(hint_fg)),
                    );
                    resp.request_focus();
                });

            ui.add(egui::Separator::default().horizontal().spacing(0.0));

            // ── Results ───────────────────────────────────────────────────────
            if display_labels.is_empty() && !query.is_empty() {
                egui::Frame::default()
                    .fill(drop_bg)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_width(win_width - 24.0);
                        ui.label(egui::RichText::new("No results").color(hint_fg).size(12.0));
                    });
            } else {
                for (idx, label) in display_labels.iter().enumerate() {
                    let is_sel = idx == selected_idx;
                    let row_bg = if is_sel { sel_bg } else { drop_bg };
                    let row_fg = if is_sel { sel_fg } else { item_fg };

                    // Split filename from directory for two-tone display.
                    let (file_part, dir_part) = split_path_display(label);

                    let row_resp = egui::Frame::default()
                        .fill(row_bg)
                        .inner_margin(egui::Margin::symmetric(12, 5))
                        .show(ui, |ui| {
                            ui.set_width(win_width - 24.0);
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(file_part).color(row_fg).size(13.0));
                                if !dir_part.is_empty() {
                                    ui.label(
                                        egui::RichText::new(format!("  {dir_part}"))
                                            .color(hint_fg)
                                            .size(11.0),
                                    );
                                }
                            });
                        });

                    let row_resp = row_resp.response.interact(egui::Sense::click());
                    if row_resp.clicked() {
                        if idx < state.fuzzy_finder.results.len() {
                            confirmed = Some(state.fuzzy_finder.results[idx].clone());
                        }
                        close = true;
                    }
                    if row_resp.hovered() {
                        selected_idx = idx;
                    }
                }
            }
        });

    // ── Write back mutable state ──────────────────────────────────────────────
    let query_changed = query != state.fuzzy_finder.query;
    state.fuzzy_finder.query = query;
    state.fuzzy_finder.selected_idx = selected_idx;

    if query_changed {
        recompute_results(state);
    }

    let action = if let Some(path) = confirmed {
        state.pending_open_path = Some(path);
        Some(Action::OpenFile)
    } else {
        None
    };

    if close {
        state.fuzzy_finder.close();
        ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
    }

    action
}

// ── Result computation ────────────────────────────────────────────────────────

/// Re-run the fuzzy search against the current query and update results.
pub fn recompute_results(state: &mut UiState) {
    let query = state.fuzzy_finder.query.clone();
    // Use the persistent finder — no index clone needed.
    let matches = state.fuzzy_finder.finder.search(&query, FUZZY_MAX_RESULTS);
    state.fuzzy_finder.results = matches.iter().map(|m| m.path.clone()).collect();
    state.fuzzy_finder.result_labels = matches.into_iter().map(|m| m.display).collect();
    state.fuzzy_finder.selected_idx = 0;
}

// ── Display helpers ───────────────────────────────────────────────────────────

/// Split a display path into `(filename, parent_directory)`.
fn split_path_display(display: &str) -> (&str, &str) {
    // Use the last path separator to split.
    if let Some(sep_pos) = display.rfind(['/', '\\']) {
        (&display[sep_pos + 1..], &display[..sep_pos])
    } else {
        (display, "")
    }
}
