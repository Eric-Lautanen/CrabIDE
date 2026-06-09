//! Find / replace floating window.
//!
//! Rendered as a floating, movable egui::Window in the top-right corner
//! of the content area when `UiState::find_replace.visible` is true.
//! Works across all tabs — toggling tabs while find is open keeps searching.
//!
//! Two-row layout (VS Code-inspired):
//!
//!  ╭────────────────────────────────────────────────╮
//!  │  [ Find input .............. ]  [.*] `Aa` `W`  │
//!  │  [⬆ Prev] [⬇ Next]  3 of 10            [ ✕ ] │
//!  ├────────────────────────────────────────────────┤  (optional)
//!  │  [ Replace input ........... ]  \[One\] \[All\]    │
//!  ╰────────────────────────────────────────────────╯
//!
//! Enter / Shift+Enter navigate matches.  The Enter key is *consumed* via
//! `consume_key` so it never leaks to the editor buffer.

use std::sync::Arc;

use crabide_config::{Action, Color};
use crabide_core::types::{Position, Range};
use regex::Regex;

use crate::state::{UiState, cfg_to_egui};

// ── Public entry point ────────────────────────────────────────────────────────

/// Render the find/replace floating window and push any backend actions.
pub fn show(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<Action>) {
    if !state.find_replace.visible {
        return;
    }

    // ── Theme / dark-light detection ──────────────────────────────────────────
    let bg_col = state
        .theme
        .ui_or("editor.background", Color::rgb(0x1e, 0x1e, 0x1e));
    let is_dark = (u32::from(bg_col.r) + u32::from(bg_col.g) + u32::from(bg_col.b)) / 3 < 128;

    let widget_bg = cfg_to_egui(state.theme.ui_or(
        "editorWidget.background",
        if is_dark {
            Color::rgb(0x25, 0x25, 0x26)
        } else {
            Color::rgb(0xf3, 0xf3, 0xf3)
        },
    ));
    let border_col = cfg_to_egui(state.theme.ui_or(
        "editorWidget.border",
        if is_dark {
            Color::rgb(0x45, 0x45, 0x45)
        } else {
            Color::rgb(0xc8, 0xc8, 0xc8)
        },
    ));
    let input_bg = cfg_to_egui(state.theme.ui_or(
        "input.background",
        if is_dark {
            Color::rgb(0x3c, 0x3c, 0x3c)
        } else {
            Color::rgb(0xff, 0xff, 0xff)
        },
    ));
    let input_fg = cfg_to_egui(state.theme.ui_or(
        "input.foreground",
        if is_dark {
            Color::rgb(0xcc, 0xcc, 0xcc)
        } else {
            Color::rgb(0x33, 0x33, 0x33)
        },
    ));
    let input_bdr = cfg_to_egui(state.theme.ui_or(
        "input.border",
        if is_dark {
            Color::rgb(0x55, 0x55, 0x55)
        } else {
            Color::rgb(0xbb, 0xbb, 0xbb)
        },
    ));

    // Active flag button: blue tint.  Inactive: subdued grey.
    let flag_active_fg = egui::Color32::from_rgb(0x00, 0x7a, 0xcc);
    let flag_active_bg = egui::Color32::from_rgba_unmultiplied(0x00, 0x7a, 0xcc, 0x33);
    let flag_inactive = if is_dark {
        egui::Color32::from_rgb(0x99, 0x99, 0x99)
    } else {
        egui::Color32::from_rgb(0x55, 0x55, 0x55)
    };
    let nav_fg = if is_dark {
        egui::Color32::from_rgb(0xcc, 0xcc, 0xcc)
    } else {
        egui::Color32::from_rgb(0x33, 0x33, 0x33)
    };
    let count_col = if is_dark {
        egui::Color32::from_rgb(0x88, 0x88, 0x88)
    } else {
        egui::Color32::from_rgb(0x77, 0x77, 0x77)
    };
    let muted = if is_dark {
        egui::Color32::from_rgb(0x88, 0x88, 0x88)
    } else {
        egui::Color32::from_rgb(0x66, 0x66, 0x66)
    };

    // ── Window placement ──────────────────────────────────────────────────────
    // Default position: top-right of the content area.
    let screen = ctx.content_rect();
    let win_w = 360.0_f32;
    let right_x = (screen.right() - win_w - 24.0).max(screen.left() + 8.0);
    let top_y = screen.top() + 8.0;

    let mut dispatch_next = false;
    let mut dispatch_prev = false;

    egui::Window::new("##find_replace_window")
        .id(egui::Id::new("find_replace_window"))
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .min_width(win_w)
        .default_pos(egui::pos2(right_x, top_y))
        .frame(
            egui::Frame::default()
                .fill(widget_bg)
                .stroke(egui::Stroke::new(1.0, border_col))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(10, 8)),
        )
        .show(ctx, |ui| {
            ui.set_min_width(win_w - 24.0); // subtract window inner margin
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);

            // ══ ROW 1: input field + flag toggles ════════════════════════════
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(3.0, 0.0);

                // Query text field — takes up the bulk of the row.
                let query_id = egui::Id::new("find_query_textedit");

                // Sample focus state BEFORE rendering.  A singleline TextEdit
                // surrenders focus the moment it processes Enter, so by the time
                // we inspect `query_resp.has_focus()` it is already false on the
                // Enter-key frame.  We must capture it here instead.
                let query_had_focus = ctx.memory(|m| m.focused() == Some(query_id));

                let query_resp = ui.add(
                    egui::TextEdit::singleline(&mut state.find_replace.query)
                        .id(query_id)
                        .desired_width(200.0)
                        .font(egui::TextStyle::Monospace)
                        .text_color(input_fg)
                        .background_color(input_bg)
                        .hint_text("Find")
                        .frame(egui::Frame::NONE),
                );
                // Custom border around the field for consistent styling.
                ui.painter().rect_stroke(
                    query_resp.rect.expand(2.0),
                    egui::CornerRadius::same(3),
                    egui::Stroke::new(1.0, input_bdr),
                    egui::StrokeKind::Outside,
                );

                if state.find_replace.just_opened || state.find_replace.needs_refocus {
                    query_resp.request_focus();
                    state.find_replace.just_opened = false;
                    state.find_replace.needs_refocus = false;
                }
                if query_resp.changed() {
                    recompute_matches(state);
                }

                // Consume Enter/Shift+Enter when the query field had focus.
                // Using `query_had_focus` (pre-render) because singleline TextEdit
                // surrenders focus on Enter before we get to check has_focus().
                if query_had_focus || query_resp.has_focus() {
                    let next =
                        ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                    let prev =
                        ui.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter));
                    if next {
                        dispatch_next = true;
                    }
                    if prev {
                        dispatch_prev = true;
                    }
                }

                ui.add_space(4.0);

                // Flag toggles — ASCII labels only.
                let re_ch = flag_toggle(
                    ui,
                    &mut state.find_replace.use_regex,
                    ".*",
                    "Regular expression",
                    flag_active_fg,
                    flag_active_bg,
                    flag_inactive,
                );
                let cs_ch = flag_toggle(
                    ui,
                    &mut state.find_replace.case_sensitive,
                    "Aa",
                    "Match case",
                    flag_active_fg,
                    flag_active_bg,
                    flag_inactive,
                );
                let ww_ch = flag_toggle(
                    ui,
                    &mut state.find_replace.whole_word,
                    "W",
                    "Match whole word",
                    flag_active_fg,
                    flag_active_bg,
                    flag_inactive,
                );
                if re_ch || cs_ch || ww_ch {
                    recompute_matches(state);
                }
            });

            // ══ ROW 2: navigation + match count + close ═══════════════════════
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(3.0, 0.0);

                // Prev / Next buttons.
                if ui
                    .add(small_btn("⬆ Prev", nav_fg))
                    .on_hover_text("Previous match (Shift+Enter)")
                    .clicked()
                {
                    dispatch_prev = true;
                    state.find_replace.needs_refocus = true;
                }
                if ui
                    .add(small_btn("⬇ Next", nav_fg))
                    .on_hover_text("Next match (Enter)")
                    .clicked()
                {
                    dispatch_next = true;
                    state.find_replace.needs_refocus = true;
                }

                ui.add_space(6.0);

                // Match count label.
                let count = state.find_replace.match_ranges.len();
                let current = if count == 0 {
                    0
                } else {
                    state.find_replace.current_match_idx + 1
                };
                let count_text = if state.find_replace.query.is_empty() {
                    String::new()
                } else if count == 0 {
                    "No results".to_owned()
                } else {
                    format!("{current} of {count}")
                };
                ui.label(egui::RichText::new(count_text).size(11.0).color(count_col));

                // Right-side: replace toggle + close.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(3.0, 0.0);

                    // Close button.
                    if ui
                        .add(small_btn("✕", muted))
                        .on_hover_text("Close (Escape)")
                        .clicked()
                    {
                        state.find_replace.visible = false;
                        state.find_replace.match_ranges = Arc::new(Vec::new());
                        ctx.memory_mut(|m| {
                            if let Some(id) = m.focused() {
                                m.surrender_focus(id);
                            }
                        });
                    }

                    // Replace row toggle: "▾" when open, "▸" when closed.
                    let lbl = if state.find_replace.replace_visible {
                        "▾"
                    } else {
                        "▸"
                    };
                    if ui
                        .add(small_btn(lbl, muted))
                        .on_hover_text("Toggle replace")
                        .clicked()
                    {
                        state.find_replace.replace_visible = !state.find_replace.replace_visible;
                    }
                });
            });

            // ══ ROW 3 (optional): replace field + action buttons ══════════════
            if state.find_replace.replace_visible {
                ui.add(egui::Separator::default().horizontal().spacing(4.0));

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);

                    let repl_id = egui::Id::new("find_replace_textedit");
                    let repl_had_focus = ctx.memory(|m| m.focused() == Some(repl_id));

                    let repl_resp = ui.add(
                        egui::TextEdit::singleline(&mut state.find_replace.replacement)
                            .id(repl_id)
                            .desired_width(200.0)
                            .font(egui::TextStyle::Monospace)
                            .text_color(input_fg)
                            .background_color(input_bg)
                            .hint_text("Replace with")
                            .frame(egui::Frame::NONE),
                    );
                    ui.painter().rect_stroke(
                        repl_resp.rect.expand(2.0),
                        egui::CornerRadius::same(3),
                        egui::Stroke::new(1.0, input_bdr),
                        egui::StrokeKind::Outside,
                    );

                    // Enter in the replace field → replace current match.
                    if (repl_had_focus || repl_resp.has_focus())
                        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
                    {
                        actions.push(Action::FindReplace);
                    }

                    ui.add_space(4.0);

                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Replace").size(12.0).color(input_fg),
                            )
                            .fill(input_bg),
                        )
                        .on_hover_text("Replace current match")
                        .clicked()
                    {
                        actions.push(Action::FindReplace);
                    }
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("All").size(12.0).color(input_fg),
                            )
                            .fill(input_bg),
                        )
                        .on_hover_text("Replace all matches")
                        .clicked()
                    {
                        actions.push(Action::ReplaceInFiles);
                    }
                });
            }
        });

    // Apply navigation after the window is fully rendered (avoids double-borrow).
    if dispatch_prev && state.find_replace.has_matches() {
        state.find_replace.prev_match();
        state.find_replace.needs_refocus = true;
        actions.push(Action::FindPrevious);
    }
    if dispatch_next && state.find_replace.has_matches() {
        state.find_replace.next_match();
        state.find_replace.needs_refocus = true;
        actions.push(Action::FindNext);
    }
}

// ── Match computation ─────────────────────────────────────────────────────────

/// Recompute match ranges from the current query and active tab lines.
pub fn recompute_matches(state: &mut UiState) {
    let query = state.find_replace.query.clone();
    let use_regex = state.find_replace.use_regex;
    let case_sensitive = state.find_replace.case_sensitive;
    let whole_word = state.find_replace.whole_word;

    state.find_replace.last_computed_query = query.clone();
    state.find_replace.current_match_idx = 0;

    if query.is_empty() {
        return;
    }

    let pattern = build_pattern(&query, use_regex, case_sensitive, whole_word);
    let re = match Regex::new(&pattern) {
        Ok(r) => r,
        Err(_) => return,
    };

    let Some(active_idx) = state.active_tab() else {
        return;
    };
    let mut new_ranges: Vec<Range> = Vec::new();
    {
        let Some(tab) = state.tabs().get(active_idx) else {
            return;
        };
        for (line_idx, line) in tab.lines.iter().enumerate() {
            for m in re.find_iter(line) {
                let start_col = char_offset(line, m.start());
                let end_col = char_offset(line, m.end());
                new_ranges.push(Range::new(
                    Position::new(line_idx as u32, start_col as u32),
                    Position::new(line_idx as u32, end_col as u32),
                ));
            }
        }
    }
    state.find_replace.match_ranges = Arc::new(new_ranges);
}

/// Build a regex pattern string from query + flags.
fn build_pattern(query: &str, use_regex: bool, case_sensitive: bool, whole_word: bool) -> String {
    let escaped = if use_regex {
        query.to_owned()
    } else {
        regex::escape(query)
    };
    let bounded = if whole_word {
        format!(r"\b{escaped}\b")
    } else {
        escaped
    };
    if case_sensitive {
        bounded
    } else {
        format!("(?i){bounded}")
    }
}

/// Convert a byte offset in a UTF-8 string to a char offset.
fn char_offset(s: &str, byte_offset: usize) -> usize {
    s[..byte_offset].chars().count()
}

// ── Replacement helpers (called from app.rs) ──────────────────────────────────

/// Compute the replacement string for the current match.
pub fn apply_replacement(matched: &str, replacement: &str) -> String {
    replacement.replace("$&", matched).replace("$0", matched)
}

// ── Widget helpers ────────────────────────────────────────────────────────────

/// A toggle button that lights up (blue tint + border) when active.
/// Returns `true` when the state changed.
fn flag_toggle(
    ui: &mut egui::Ui,
    active: &mut bool,
    label: &str,
    tooltip: &str,
    active_fg: egui::Color32,
    active_bg: egui::Color32,
    inactive_fg: egui::Color32,
) -> bool {
    let (fg, fill, stroke_col) = if *active {
        (active_fg, active_bg, active_fg)
    } else {
        (
            inactive_fg,
            egui::Color32::TRANSPARENT,
            egui::Color32::TRANSPARENT,
        )
    };
    let resp = ui
        .add(
            egui::Button::new(egui::RichText::new(label).size(11.0).color(fg))
                .small()
                .fill(fill)
                .stroke(egui::Stroke::new(1.0, stroke_col))
                .corner_radius(egui::CornerRadius::same(3)),
        )
        .on_hover_text(tooltip);

    if resp.clicked() {
        *active = !*active;
        true
    } else {
        false
    }
}

/// Helper: build a frameless small button with coloured text.
fn small_btn(label: &str, fg: egui::Color32) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(label.to_owned()).size(11.0).color(fg))
        .small()
        .frame(false)
}
