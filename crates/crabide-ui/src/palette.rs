//! Command palette (Ctrl+Shift+P): fuzzy-search over all `Action`s.
//!
//! Uses nucleo's `Matcher` for scoring and displays up to 10 results.
//! The palette is a floating `egui::Window` anchored to the top-centre of
//! the screen and dismisses on Escape or when focus is lost.

use nucleo::pattern::{CaseMatching, Normalization, Pattern};
use nucleo::{Config, Matcher, Utf32String};

use crabide_config::{all_actions_with, Action, ActionRegistry};

use crate::state::{cfg_to_egui, CommandPaletteState, PaletteEntry, UiState};

/// Maximum number of results shown in the palette list.
const MAX_RESULTS: usize = 10;

// ── Public entry point ────────────────────────────────────────────────────────

/// Render the command palette window.
///
/// Returns `Some(Action)` when the user confirms a selection.
/// The caller handles the action and the palette hides itself.
pub fn show(ctx: &egui::Context, state: &mut UiState, registry: &ActionRegistry) -> Option<Action> {
    if !state.command_palette.visible {
        return None;
    }

    // ── Pre-populate entries on first open ────────────────────────────────────
    if state.command_palette.entries.is_empty() {
        rebuild_entries(&mut state.command_palette, &state.keybindings, registry);
    }

    // ── Keyboard navigation (processed outside the window to run every frame) ─
    let (arrow_up, arrow_down, enter, escape) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::Enter),
            i.key_pressed(egui::Key::Escape),
        )
    });

    let n_entries = state.command_palette.entries.len();
    if arrow_up && n_entries > 0 {
        state.command_palette.selected_idx = state.command_palette.selected_idx.saturating_sub(1);
    }
    if arrow_down && n_entries > 0 {
        state.command_palette.selected_idx =
            (state.command_palette.selected_idx + 1).min(n_entries.saturating_sub(1));
    }

    let mut confirmed_action: Option<Action> = None;
    let mut close = escape;

    if enter && n_entries > 0 {
        let idx = state.command_palette.selected_idx.min(n_entries - 1);
        confirmed_action = Some(state.command_palette.entries[idx].action.clone());
        close = true;
    }

    // ── Extract colors and entry data before the window closure ───────────────
    // This avoids needing to borrow `state` inside the Window closure while
    // also holding a mutable borrow through UiState.
    let input_bg = cfg_to_egui(state.theme.ui_or(
        "input.background",
        crabide_config::Color::rgb(0x3c, 0x3c, 0x3c),
    ));
    let input_fg = cfg_to_egui(state.theme.ui_or(
        "input.foreground",
        crabide_config::Color::rgb(0xcc, 0xcc, 0xcc),
    ));
    let sel_bg = cfg_to_egui(state.theme.ui_or(
        "list.activeSelectionBackground",
        crabide_config::Color::rgb(0x09, 0x47, 0x71),
    ));
    let sel_fg = cfg_to_egui(state.theme.ui_or(
        "list.activeSelectionForeground",
        crabide_config::Color::rgb(0xff, 0xff, 0xff),
    ));
    let item_fg = cfg_to_egui(state.theme.ui_or(
        "sideBar.foreground",
        crabide_config::Color::rgb(0xcc, 0xcc, 0xcc),
    ));
    let drop_bg = cfg_to_egui(state.theme.ui_or(
        "dropdown.background",
        crabide_config::Color::rgb(0x3c, 0x3c, 0x3c),
    ));
    let shortcut_fg = egui::Color32::from_rgb(0x88, 0x88, 0x88);

    // Snapshot mutable state that needs to be touched inside the closure.
    let mut query = state.command_palette.query.clone();
    let mut selected_idx = state.command_palette.selected_idx;
    let display_entries: Vec<PaletteEntry> = state
        .command_palette
        .entries
        .iter()
        .take(MAX_RESULTS)
        .cloned()
        .collect();

    // ── Window ────────────────────────────────────────────────────────────────
    let screen = ctx.content_rect();
    let win_width = 520.0_f32.min(screen.width() - 40.0);
    let win_left = screen.center().x - win_width / 2.0;
    let win_top = screen.top() + 60.0;

    let id = egui::Id::new("command_palette_window");

    egui::Window::new("##command_palette")
        .id(id)
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

            // ── Query input ───────────────────────────────────────────────
            egui::Frame::default()
                .fill(input_bg)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.set_width(win_width - 20.0);
                    let te = egui::TextEdit::singleline(&mut query)
                        .font(egui::TextStyle::Monospace)
                        .text_color(input_fg)
                        .frame(egui::Frame::NONE)
                        .desired_width(f32::INFINITY)
                        .hint_text(
                            egui::RichText::new("Type a command...")
                                .color(egui::Color32::from_rgb(0x88, 0x88, 0x88)),
                        );
                    let resp = ui.add(te);
                    // Keep the text box focused.
                    resp.request_focus();
                });

            ui.add(egui::Separator::default().horizontal().spacing(0.0));

            // ── Results ───────────────────────────────────────────────────
            for (idx, entry) in display_entries.iter().enumerate() {
                let is_sel = idx == selected_idx;
                let row_bg = if is_sel { sel_bg } else { drop_bg };
                let row_fg = if is_sel { sel_fg } else { item_fg };

                let row_resp = egui::Frame::default()
                    .fill(row_bg)
                    .inner_margin(egui::Margin::symmetric(12, 5))
                    .show(ui, |ui| {
                        ui.set_width(win_width - 24.0);
                        ui.horizontal(|ui| {
                            ui.add(egui::Label::new(
                                egui::RichText::new(entry.label.as_str())
                                    .color(row_fg)
                                    .size(13.0),
                            ));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if !entry.shortcut.is_empty() {
                                        ui.add(egui::Label::new(
                                            egui::RichText::new(&entry.shortcut)
                                                .color(shortcut_fg)
                                                .size(11.0)
                                                .monospace(),
                                        ));
                                    }
                                },
                            );
                        });
                    });

                let row_resp = row_resp.response.interact(egui::Sense::click());
                if row_resp.clicked() {
                    confirmed_action = Some(display_entries[idx].action.clone());
                    close = true;
                }
                if row_resp.hovered() {
                    selected_idx = idx;
                }
            }
        });

    // ── Write back mutable state ──────────────────────────────────────────────
    let query_changed = query != state.command_palette.query;
    state.command_palette.query = query;
    state.command_palette.selected_idx = selected_idx;

    if query_changed {
        rebuild_entries(&mut state.command_palette, &state.keybindings, registry);
    }

    if close {
        state.command_palette.visible = false;
        state.command_palette.query = String::new();
        state.command_palette.selected_idx = 0;
        state.command_palette.entries = Vec::new();
        ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
    }

    confirmed_action
}

// ── Fuzzy filtering ───────────────────────────────────────────────────────────

fn rebuild_entries(
    cp: &mut CommandPaletteState,
    keybindings: &crabide_config::KeybindingEngine,
    registry: &ActionRegistry,
) {
    let all = all_actions_with(registry);
    cp.selected_idx = 0;

    if cp.query.is_empty() {
        cp.entries = all
            .iter()
            .map(|(action, label)| PaletteEntry {
                action: action.clone(),
                label: label.clone(),
                shortcut: format_shortcut(action, keybindings),
            })
            .collect();
        return;
    }

    let pattern = Pattern::parse(&cp.query, CaseMatching::Ignore, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT);

    let mut scored: Vec<(u32, Action, String)> = all
        .iter()
        .filter_map(|(action, label)| {
            let hay = Utf32String::from(label.as_str());
            let score = pattern.score(hay.slice(..), &mut matcher)?;
            Some((score, action.clone(), label.clone()))
        })
        .collect();

    scored.sort_by_key(|b| std::cmp::Reverse(b.0));

    cp.entries = scored
        .into_iter()
        .take(MAX_RESULTS)
        .map(|(_, action, label)| PaletteEntry {
            shortcut: format_shortcut(&action, keybindings),
            action,
            label,
        })
        .collect();
}

// ── Keybinding formatter ──────────────────────────────────────────────────────

fn format_shortcut(action: &Action, kb: &crabide_config::KeybindingEngine) -> String {
    let chords = kb.chords_for_action(action);
    if chords.is_empty() {
        return String::new();
    }

    chords[0]
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}
