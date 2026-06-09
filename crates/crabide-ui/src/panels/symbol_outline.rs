//! Go-to-symbol overlay (Ctrl+Shift+O).
//!
//! Displays a floating window with a text input and a fuzzy-matched list of
//! symbol outline entries for the active document, similar to VS Code's
//! "Go to Symbol in File..." (Ctrl+Shift+O).
//!
//! # Behaviour
//! * Opening: the app populates `state.symbol_outline.entries` from the
//!   syntax engine's outline extractor.
//! * Scoring: fuzzy matching (same as command palette) is run every frame
//!   when the query changes.
//! * Confirming: pressing Enter or clicking a row sets
//!   `state.pending_scroll_line` and closes the overlay.
//! * Closing: Escape or click-outside dismisses the overlay.

use crabide_config::{Action, Color};
use nucleo::pattern::{CaseMatching, Normalization, Pattern};
use nucleo::{Config, Matcher, Utf32String};

use crate::state::{SymbolOutlineEntry, UiState, cfg_to_egui};

/// Maximum number of results shown.
const MAX_RESULTS: usize = 20;

/// Render the symbol outline overlay window.
///
/// Returns `Some(Action::GotoSymbol)` when the user confirms a selection.
/// The caller should also close the overlay after handling the action.
pub fn show(ctx: &egui::Context, state: &mut UiState) -> Option<Action> {
    if !state.symbol_outline.visible {
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

    let n = state.symbol_outline.entries.len();
    if arrow_up && n > 0 {
        state.symbol_outline.selected_idx = state.symbol_outline.selected_idx.saturating_sub(1);
    }
    if arrow_down && n > 0 {
        state.symbol_outline.selected_idx =
            (state.symbol_outline.selected_idx + 1).min(n.saturating_sub(1));
    }

    let mut confirmed: Option<Action> = None;
    let mut close = escape;

    if enter && n > 0 {
        let idx = state.symbol_outline.selected_idx.min(n - 1);
        if let Some(entry) = state.symbol_outline.entries.get(idx) {
            state.pending_scroll_line = Some(entry.line as usize);
        }
        confirmed = Some(Action::GotoSymbol);
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
    let kind_fg = egui::Color32::from_rgb(0x88, 0x88, 0x88);

    // Snapshot mutable state for use inside the window closure.
    let mut query = state.symbol_outline.query.clone();
    let mut selected_idx = state.symbol_outline.selected_idx;

    // ── Fuzzy filter entries ──────────────────────────────────────────────────
    let filtered: Vec<SymbolOutlineEntry> = if query.is_empty() {
        state.symbol_outline.entries.clone()
    } else {
        let pattern = Pattern::parse(&query, CaseMatching::Ignore, Normalization::Smart);
        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut scored: Vec<(u32, SymbolOutlineEntry)> = state
            .symbol_outline
            .entries
            .iter()
            .filter_map(|entry| {
                let hay = Utf32String::from(entry.name.as_str());
                let score = pattern.score(hay.slice(..), &mut matcher)?;
                Some((score, entry.clone()))
            })
            .collect();
        scored.sort_by_key(|b| std::cmp::Reverse(b.0));
        scored
            .into_iter()
            .take(MAX_RESULTS)
            .map(|(_, e)| e)
            .collect()
    };

    let display_entries = &filtered[..filtered.len().min(MAX_RESULTS)];

    // ── Window ────────────────────────────────────────────────────────────────
    let screen = ctx.content_rect();
    let win_width = 420.0_f32.min(screen.width() - 40.0);
    let win_left = screen.center().x - win_width / 2.0;
    let win_top = screen.top() + 60.0;

    egui::Window::new("##symbol_outline")
        .id(egui::Id::new("symbol_outline_window"))
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

            // ── Query input ──────────────────────────────────────────────
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
                            egui::RichText::new("Type a symbol name...")
                                .color(egui::Color32::from_rgb(0x88, 0x88, 0x88)),
                        );
                    let resp = ui.add(te);
                    resp.request_focus();
                });

            ui.add(egui::Separator::default().horizontal().spacing(0.0));

            // ── Results ──────────────────────────────────────────────────
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
                                egui::RichText::new(&entry.name).color(row_fg).size(13.0),
                            ));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if !entry.kind.is_empty() {
                                        ui.add(egui::Label::new(
                                            egui::RichText::new(&entry.kind)
                                                .color(kind_fg)
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
                    state.pending_scroll_line = Some(entry.line as usize);
                    confirmed = Some(Action::GotoSymbol);
                    close = true;
                }
                if row_resp.hovered() {
                    selected_idx = idx;
                }
            }
        });

    // ── Write back mutable state ──────────────────────────────────────────────
    state.symbol_outline.query = query;
    state.symbol_outline.selected_idx = selected_idx;

    if close {
        state.symbol_outline.visible = false;
        state.symbol_outline.query = String::new();
        state.symbol_outline.selected_idx = 0;
        state.symbol_outline.entries = Vec::new();
        ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
    }

    confirmed
}
