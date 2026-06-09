//! Core editor panel: syntax-highlighted document rendering.
//!
//! # Layout
//! ```text
//! ┌── tab bar (35 px) ───────────────────────────────────────────────┐
//! │  file.rs ●  main.rs  ×                                          │
//! ├── find/replace bar (optional) ──────────────────────────────────┤
//! │  🔍 [query___________] [.*] [Aa] [W]  ↑↓  [3 of 12]  [×]      │
//! ├── scroll area ───────────────────────────────────────────────────┤
//! │ ┌ gutter ┐ ┌── editor content ─────────────────────────────────┐ │
//! │ │  1     │ │ fn main() {                                       │ │
//! │ │  2  ●  │ │     let x = 1;                                    │ │
//! │ │  3  │  │ │ }                                                 │ │
//! │ └────────┘ └───────────────────────────────────────────────────┘ │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Large-file virtualisation
//! Without word-wrap: `ScrollArea::show_rows` renders only the visible line
//! range.  With word-wrap: `ScrollArea::vertical` renders all lines (each may
//! wrap to several screen rows) so we cannot rely on uniform row heights.
//!
//! # Cursor rendering
//! Carets and selections are drawn with the egui `Painter` directly on top of
//! the text galleys, so they never interfere with text layout metrics.

use egui::text::{LayoutJob, TextFormat};

use crabide_config::Action;
use crabide_core::event::{Diagnostic, DiagnosticSeverity, FoldingRange};
use crabide_core::types::Position;
use crabide_syntax::highlight::scope_to_vscode;

use crate::panels::{gutter, tab_bar};
use crate::state::{cfg_to_egui, UiState};

/// Returns `true` if `line_idx` is hidden by a collapsed fold range.
/// A fold range hides lines strictly between `start_line` and `end_line`
/// (the first line of the range is always visible as the fold marker).
fn is_line_folded(
    line_idx: usize,
    folding_ranges: &[FoldingRange],
    collapsed_folds: &[usize],
) -> bool {
    collapsed_folds.iter().any(|&i| {
        if let Some(fr) = folding_ranges.get(i) {
            let start = fr.start_line as usize;
            let end = fr.end_line as usize;
            line_idx > start && line_idx < end
        } else {
            false
        }
    })
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Render the full editor pane (tab bar + find bar + scrollable content).
/// Appends any backend actions to `actions`.
pub fn show(ui: &mut egui::Ui, state: &mut UiState, actions: &mut Vec<Action>) {
    // ── Tab bar ───────────────────────────────────────────────────────────────
    let tab_action = tab_bar::show(ui, state);
    match tab_action {
        tab_bar::TabBarAction::Activate(idx) => {
            state.active_tab = Some(idx);
        }
        tab_bar::TabBarAction::Close(idx) => {
            if let Some(id) = state.close_tab(idx) {
                state.pending_close_buffer = Some(id);
                actions.push(Action::CloseTab);
            }
        }
        tab_bar::TabBarAction::None => {}
    }

    // ── No open document — show welcome screen ────────────────────────────────
    let Some(active_idx) = state.active_tab else {
        show_welcome(ui, state);
        return;
    };
    if active_idx >= state.tabs.len() {
        show_welcome(ui, state);
        return;
    }

    // ── Markdown preview toolbar (visible when active tab is a .md file) ────────
    // This is the primary UX entry point for the markdown-preview extension.
    {
        let uri = state.tabs[active_idx].uri.to_string();
        let is_md = uri.ends_with(".md") || uri.ends_with(".markdown");
        if is_md {
            let toolbar_bg = cfg_to_egui(state.theme.ui_or(
                "editorGroupHeader.tabsBackground",
                crabide_config::Color::rgb(0x25, 0x25, 0x26),
            ));
            let btn_idle_fg = cfg_to_egui(state.theme.ui_or(
                "tab.inactiveForeground",
                crabide_config::Color::rgb(0x99, 0x99, 0x99),
            ));
            let btn_active_fg = cfg_to_egui(state.theme.ui_or(
                "activityBarBadge.background",
                crabide_config::Color::rgb(0x00, 0x7a, 0xcc),
            ));
            let hov_bg = cfg_to_egui(state.theme.ui_or(
                "list.hoverBackground",
                crabide_config::Color::rgba(0x2a, 0x2d, 0x2e, 0xff),
            ));

            let (bar_rect, _) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::hover());
            ui.painter().rect_filled(bar_rect, 0.0, toolbar_bg);

            // "👁 Open/Close Preview" button on the right edge of the toolbar.
            let preview_open = state
                .extension_panels
                .get("markdown-preview.panel")
                .map(|p| p.open)
                .unwrap_or(false);
            let btn_label = if preview_open {
                "👁 Close Preview"
            } else {
                "👁 Open Preview"
            };
            let btn_fg = if preview_open {
                btn_active_fg
            } else {
                btn_idle_fg
            };
            let btn_w = 126.0_f32;
            let btn_rect = egui::Rect::from_min_size(
                egui::pos2(bar_rect.right() - btn_w - 8.0, bar_rect.min.y + 2.0),
                egui::vec2(btn_w, 20.0),
            );
            let btn_resp = ui.allocate_rect(btn_rect, egui::Sense::click());
            if btn_resp.hovered() {
                ui.painter().rect_filled(btn_rect, 3.0, hov_bg);
            }
            ui.painter().text(
                btn_rect.center(),
                egui::Align2::CENTER_CENTER,
                btn_label,
                egui::FontId::proportional(11.5),
                btn_fg,
            );
            if btn_resp.clicked() {
                // Toggle the dynamic extension panel and notify the extension.
                if let Some(p) = state.extension_panels.get_mut("markdown-preview.panel") {
                    p.open = !p.open;
                }
                state.extensions_panel.pending_execute_command =
                    Some(("markdown-preview.toggle".to_owned(), vec![]));
            }
        }
    }

    // ── Breadcrumb bar ──────────────────────────────────────────────────────
    crate::panels::breadcrumbs::show(ui, state);

    // ── Compute display metrics ───────────────────────────────────────────────
    let font_id = egui::FontId::monospace(state.font_size);
    let line_height = ui.fonts_mut(|f| f.row_height(&font_id));
    let char_width = ui.fonts_mut(|f| f.glyph_width(&font_id, ' '));

    let caret_color = cfg_to_egui(state.theme.ui_or(
        "editorCursor.foreground",
        crabide_config::Color::rgb(0xae, 0xaf, 0xad),
    ));
    let sel_color = cfg_to_egui(state.theme.ui_or(
        "editor.selectionBackground",
        crabide_config::Color::rgba(0x26, 0x4f, 0x78, 0xb0),
    ));
    let line_hl_color = cfg_to_egui(state.theme.ui_or(
        "editor.lineHighlightBackground",
        crabide_config::Color::rgba(0x2d, 0x2d, 0x2d, 0x40),
    ));
    // Find match colors — "other" matches are a muted orange; the active/current
    // match is a vivid yellow-orange so it stands out clearly against the rest.
    let find_match_color = cfg_to_egui(state.theme.ui_or(
        "editor.findMatchHighlightBackground",
        crabide_config::Color::rgba(0xea, 0x5c, 0x00, 0x55),
    ));
    let find_current_color = cfg_to_egui(state.theme.ui_or(
        "editor.findMatchBackground",
        crabide_config::Color::rgba(0xff, 0xcc, 0x00, 0xcc),
    ));
    // Bracket match colors
    let bracket_bg = cfg_to_egui(state.theme.ui_or(
        "editorBracketMatch.background",
        crabide_config::Color::rgba(0x0d, 0x3a, 0x58, 0x80),
    ));
    let bracket_border = cfg_to_egui(state.theme.ui_or(
        "editorBracketMatch.border",
        crabide_config::Color::rgb(0x88, 0x88, 0x88),
    ));
    // Snippet tabstop highlight color
    let tabstop_color = cfg_to_egui(state.theme.ui_or(
        "editor.wordHighlightBackground",
        crabide_config::Color::rgba(0x57, 0x57, 0x57, 0xa0),
    ));

    let caret_visible = state.caret_visible;
    let word_wrap = state.word_wrap;

    let tab = &state.tabs[active_idx];
    let n_lines = tab.lines.len().max(1);
    let scroll_id = tab.scroll_id;

    // Snapshot the data we need from the tab to avoid borrow checker friction
    // inside the scroll closure.
    let bracket_match = tab.bracket_match;
    let active_tabstop = tab.active_tabstop();
    let inlay_hints: Vec<crabide_core::event::InlayHint> = tab.inlay_hints.clone();
    let find_matches: Vec<crabide_core::types::Range> = state.find_replace.match_ranges.clone();
    let current_match = state.find_replace.current_match_idx;

    // Snapshot selection ranges so we can embed them in TextFormat::background
    // inside build_line_job (guarantees highlight renders BEHIND glyphs).
    let cursor_sel_ranges: Vec<crabide_core::types::Range> = state.tabs[active_idx]
        .cursors
        .all()
        .iter()
        .filter(|c| c.has_selection())
        .map(|c| c.range())
        .collect();

    // Collect pointer events here and apply after the scroll area closes.
    let mut pointer_event: Option<PointerEvent> = None;
    // Snapshot per-tab flags before the scroll closures borrow `state`.
    let drag_active = state.tabs[active_idx].drag_anchor.is_some();
    let last_click_time = state.tabs[active_idx].last_click_time;
    let last_click_pos = state.tabs[active_idx].last_click_pos;
    let prev_click_count = state.tabs[active_idx].click_count;

    // Determine once whether an overlay (egui::Window, Area, dialog) is
    // sitting on top of the editor at the current pointer position.
    // We use egui's layer system: `layer_id_at` returns the topmost
    // *interactive* layer at a screen position.  The editor content lives in
    // the CentralPanel layer; any Window/Area overlay has a different LayerId.
    // When the pointer is over such an overlay we must NOT let new press
    // events fall through to the editor below.
    let editor_layer_id = ui.layer_id();
    let pointer_blocked = ui
        .input(|i| i.pointer.hover_pos())
        .map(|pp| {
            ui.ctx()
                .layer_id_at(pp)
                .map(|top| top != editor_layer_id)
                .unwrap_or(false)
        })
        .unwrap_or(false);

    // Consume any pending scroll-to-line request (set by goto-line, find-next, etc.)
    let pending_scroll_line = state.pending_scroll_line.take();

    // ── Rendering (two paths: word-wrap vs virtual rows) ──────────────────────

    // Breakpoint gutter clicks are collected here and applied to state.dap_panel
    // AFTER the scroll closures so the closures don't need mutable state access.
    let mut bp_gutter_click: Option<(std::path::PathBuf, Vec<u32>)> = None;
    // Fold toggles are also collected and applied after the scroll area.
    let mut unsorted_fold_toggles: Vec<usize> = Vec::new();

    if word_wrap {
        // ── Word-wrap mode: one label per line, all lines rendered ─────────────
        let mut scroll_area = egui::ScrollArea::vertical()
            .id_salt(scroll_id)
            .auto_shrink([false, false])
            .scroll_source(egui::scroll_area::ScrollSource {
                scroll_bar: true,
                drag: false,
                mouse_wheel: true,
            });
        if let Some(target) = pending_scroll_line {
            let viewport_h = ui.available_height();
            let offset_y =
                (target as f32 * line_height - viewport_h / 2.0 + line_height / 2.0).max(0.0);
            scroll_area = scroll_area.scroll_offset(egui::vec2(0.0, offset_y));
        }
        scroll_area.show(ui, |ui| {
            let tab = &state.tabs[active_idx];
            let theme = &state.theme;
            let n_vis = tab.lines.len();
            let clip = ui.clip_rect();

            // Build background highlight ranges once — same for every line.
            // These are passed into build_line_job as TextFormat::background
            // so the highlight is guaranteed to render behind the glyphs.
            let mut bg_ranges: Vec<(crabide_core::types::Range, egui::Color32)> =
                cursor_sel_ranges.iter().map(|r| (*r, sel_color)).collect();
            for (i, m) in find_matches.iter().enumerate() {
                let color = if i == current_match {
                    find_current_color
                } else {
                    find_match_color
                };
                bg_ranges.push((*m, color));
            }

            for line_idx in 0..tab.lines.len() {
                let line_text = tab.lines.get(line_idx).map(String::as_str).unwrap_or("");
                let is_active = tab.cursors.primary().pos().line as usize == line_idx;
                let row_rect = ui.available_rect_before_wrap();
                let row_top = row_rect.top();

                // Skip lines hidden by collapsed folds.
                if is_line_folded(line_idx, &tab.folding_ranges, &tab.collapsed_folds) {
                    ui.add_space(line_height);
                    continue;
                }

                // Skip lines that are fully above or below the visible clip rect.
                // For lines above we add the minimum height so scroll geometry is
                // preserved; below, we stop rendering entirely.
                if row_top + line_height < clip.top() {
                    ui.add_space(line_height);
                    continue;
                }
                if row_top > clip.bottom() {
                    // Remaining lines are below the viewport — add their aggregate
                    // height so the scroll bar reflects the full document size.
                    let remaining = (n_vis - line_idx) as f32 * line_height;
                    ui.add_space(remaining);
                    break;
                }

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;

                    if is_active {
                        let hl_rect = egui::Rect::from_min_size(
                            egui::pos2(row_rect.left(), row_top),
                            egui::vec2(row_rect.width(), line_height),
                        );
                        ui.painter().rect_filled(hl_rect, 0.0, line_hl_color);
                    }

                    // Collect per-tab data before the gutter call so there is no
                    // overlapping borrow of `state` when we later touch `state.dap_panel`.
                    let tab_bp_path = tab.uri.as_url().to_file_path().ok();
                    let tab_bps_snap = tab.breakpoints.clone();
                    match gutter::show_line(ui, tab, state, line_idx) {
                        gutter::GutterAction::ToggleBreakpoint => {
                            let bp_line = line_idx as u32;
                            actions.push(Action::ToggleBreakpoint);
                            if let Some(path) = tab_bp_path {
                                let mut lines = tab_bps_snap;
                                if let Some(pos) = lines.iter().position(|&l| l == bp_line) {
                                    lines.remove(pos);
                                } else {
                                    lines.push(bp_line);
                                    lines.sort_unstable();
                                }
                                bp_gutter_click = Some((path, lines));
                            }
                        }
                        gutter::GutterAction::ToggleFold(idx) => {
                            // Toggle fold is handled after the scroll area to avoid
                            // borrowing conflicts.
                            unsorted_fold_toggles.push(idx);
                        }
                        gutter::GutterAction::None => {}
                    }
                    let text_left_x = ui.cursor().left();

                    // Selection and find-match highlights are embedded in
                    // TextFormat::background inside build_line_job so they
                    // render BEHIND glyphs (no separate painter rect needed).
                    let lrc = LineRenderCtx {
                        line_idx,
                        line_text,
                        text_left_x,
                        row_top,
                        line_height,
                        char_width,
                    };
                    if let Some(ref ts) = active_tabstop {
                        paint_tabstop_on_line(ui, ts.range, lrc, tabstop_color);
                    }
                    if let Some((open_r, close_r)) = bracket_match {
                        paint_bracket_on_line(ui, open_r, lrc, bracket_bg, bracket_border);
                        paint_bracket_on_line(ui, close_r, lrc, bracket_bg, bracket_border);
                    }

                    // ── Text with embedded selection/find backgrounds ──────────────────────
                    let job = build_line_job(
                        line_text,
                        line_idx,
                        &tab.highlight_spans,
                        theme,
                        font_id.clone(),
                        &bg_ranges,
                    );
                    ui.add(
                        egui::Label::new(job)
                            .wrap_mode(egui::TextWrapMode::Wrap)
                            .selectable(false),
                    );

                    // ── Inlay hints (rendered inline after text) ────────────────────────
                    paint_inlay_hints_on_line(ui, &inlay_hints, lrc, &state.theme);

                    // ── Carets (on top of text) ────────────────────────────────────────────
                    if caret_visible {
                        for cursor in tab.cursors.all() {
                            let col = cursor.pos();
                            if col.line as usize != line_idx {
                                continue;
                            }
                            let cx = text_left_x + col.character as f32 * char_width;
                            let caret_rect = egui::Rect::from_min_size(
                                egui::pos2(cx, row_top),
                                egui::vec2(2.0, line_height),
                            );
                            ui.painter().rect_filled(caret_rect, 0.0, caret_color);
                        }
                    }
                });

                // Pointer event detection for this row
                let row_bottom = row_top + line_height;
                detect_row_pointer(
                    ui,
                    &RowPointerCtx {
                        row_top,
                        row_bottom,
                        row_left: row_rect.left(),
                        content_right: row_rect.right(),
                        line_idx,
                        char_width,
                        lines: &tab.lines,
                        drag_active,
                        is_first: line_idx == 0,
                        is_last: line_idx + 1 == n_vis,
                        last_click_time,
                        last_click_pos,
                        prev_click_count,
                        pointer_blocked,
                    },
                    &mut pointer_event,
                );
            }

            // Auto-scroll while drag-selecting near viewport edges.
            if drag_active {
                let (down, dragging, pp) = ui.input(|i| {
                    (
                        i.pointer.primary_down(),
                        i.pointer.is_decidedly_dragging(),
                        i.pointer.interact_pos(),
                    )
                });
                if down && dragging {
                    if let Some(pp) = pp {
                        let clip = ui.clip_rect();
                        let zone = line_height * 2.0;
                        if pp.y < clip.top() + zone {
                            ui.scroll_with_delta(egui::Vec2::new(0.0, line_height));
                        } else if pp.y > clip.bottom() - zone {
                            ui.scroll_with_delta(egui::Vec2::new(0.0, -line_height));
                        }
                    }
                }
            }
        });
    } else {
        // ── Virtual-row mode: show_rows for O(visible) rendering ──────────────
        let mut scroll_area = egui::ScrollArea::both()
            .id_salt(scroll_id)
            .auto_shrink([false, false])
            .scroll_source(egui::scroll_area::ScrollSource {
                scroll_bar: true,
                drag: false,
                mouse_wheel: true,
            });
        if let Some(target) = pending_scroll_line {
            let viewport_h = ui.available_height();
            let offset_y =
                (target as f32 * line_height - viewport_h / 2.0 + line_height / 2.0).max(0.0);
            scroll_area = scroll_area.scroll_offset(egui::vec2(0.0, offset_y));
        }
        scroll_area.show_rows(ui, line_height, n_lines, |ui, visible_range| {
            let tab = &state.tabs[active_idx];
            let theme = &state.theme;
            let first_vis = visible_range.start;
            let last_vis = visible_range.end.saturating_sub(1);

            // Build background highlight ranges once — same for every line.
            let mut bg_ranges: Vec<(crabide_core::types::Range, egui::Color32)> =
                cursor_sel_ranges.iter().map(|r| (*r, sel_color)).collect();
            for (i, m) in find_matches.iter().enumerate() {
                let color = if i == current_match {
                    find_current_color
                } else {
                    find_match_color
                };
                bg_ranges.push((*m, color));
            }

            for line_idx in visible_range {
                let line_text = tab.lines.get(line_idx).map(String::as_str).unwrap_or("");
                let is_active = tab.cursors.primary().pos().line as usize == line_idx;
                let row_rect = ui.available_rect_before_wrap();
                let row_top = row_rect.top();

                // Skip lines hidden by collapsed folds.
                if is_line_folded(line_idx, &tab.folding_ranges, &tab.collapsed_folds) {
                    ui.add_space(line_height);
                    continue;
                }

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;

                    if is_active {
                        let hl_rect = egui::Rect::from_min_size(
                            egui::pos2(row_rect.left(), row_top),
                            egui::vec2(row_rect.width(), line_height),
                        );
                        ui.painter().rect_filled(hl_rect, 0.0, line_hl_color);
                    }

                    // Collect per-tab data before the gutter call so there is no
                    // overlapping borrow of `state` when we later touch `state.dap_panel`.
                    let tab_bp_path = tab.uri.as_url().to_file_path().ok();
                    let tab_bps_snap = tab.breakpoints.clone();
                    match gutter::show_line(ui, tab, state, line_idx) {
                        gutter::GutterAction::ToggleBreakpoint => {
                            let bp_line = line_idx as u32;
                            actions.push(Action::ToggleBreakpoint);
                            if let Some(path) = tab_bp_path {
                                let mut lines = tab_bps_snap;
                                if let Some(pos) = lines.iter().position(|&l| l == bp_line) {
                                    lines.remove(pos);
                                } else {
                                    lines.push(bp_line);
                                    lines.sort_unstable();
                                }
                                bp_gutter_click = Some((path, lines));
                            }
                        }
                        gutter::GutterAction::ToggleFold(idx) => {
                            unsorted_fold_toggles.push(idx);
                        }
                        gutter::GutterAction::None => {}
                    }
                    let text_left_x = ui.cursor().left();

                    // Selection and find-match highlights are embedded in
                    // TextFormat::background inside build_line_job so they
                    // render BEHIND glyphs (no separate painter rect needed).
                    let lrc = LineRenderCtx {
                        line_idx,
                        line_text,
                        text_left_x,
                        row_top,
                        line_height,
                        char_width,
                    };
                    if let Some(ref ts) = active_tabstop {
                        paint_tabstop_on_line(ui, ts.range, lrc, tabstop_color);
                    }
                    if let Some((open_r, close_r)) = bracket_match {
                        paint_bracket_on_line(ui, open_r, lrc, bracket_bg, bracket_border);
                        paint_bracket_on_line(ui, close_r, lrc, bracket_bg, bracket_border);
                    }

                    // ── Text with embedded selection/find backgrounds ──────────────────────
                    let job = build_line_job(
                        line_text,
                        line_idx,
                        &tab.highlight_spans,
                        theme,
                        font_id.clone(),
                        &bg_ranges,
                    );
                    ui.add(
                        egui::Label::new(job)
                            .wrap_mode(egui::TextWrapMode::Extend)
                            .selectable(false),
                    );

                    // ── Diagnostic squiggly underlines (below text, above carets) ─────────
                    paint_diagnostic_underlines(ui, &tab.diagnostics, lrc);

                    // ── Inlay hints (rendered inline after text) ────────────────────────
                    paint_inlay_hints_on_line(ui, &inlay_hints, lrc, &state.theme);
                    paint_diagnostic_underlines(ui, &tab.diagnostics, lrc);

                    // ── Carets (on top of text) ────────────────────────────────────────────
                    if caret_visible {
                        for cursor in tab.cursors.all() {
                            let col = cursor.pos();
                            if col.line as usize != line_idx {
                                continue;
                            }
                            let cx = text_left_x + col.character as f32 * char_width;
                            let caret_rect = egui::Rect::from_min_size(
                                egui::pos2(cx, row_top),
                                egui::vec2(2.0, line_height),
                            );
                            ui.painter().rect_filled(caret_rect, 0.0, caret_color);
                        }
                    }
                });

                // Pointer event detection for this row
                let row_bottom = row_top + line_height;
                detect_row_pointer(
                    ui,
                    &RowPointerCtx {
                        row_top,
                        row_bottom,
                        row_left: row_rect.left(),
                        content_right: row_rect.right(),
                        line_idx,
                        char_width,
                        lines: &tab.lines,
                        drag_active,
                        is_first: line_idx == first_vis,
                        is_last: line_idx == last_vis,
                        last_click_time,
                        last_click_pos,
                        prev_click_count,
                        pointer_blocked,
                    },
                    &mut pointer_event,
                );
            }

            // Auto-scroll while drag-selecting near viewport edges.
            if drag_active {
                let (down, dragging, pp) = ui.input(|i| {
                    (
                        i.pointer.primary_down(),
                        i.pointer.is_decidedly_dragging(),
                        i.pointer.interact_pos(),
                    )
                });
                if down && dragging {
                    if let Some(pp) = pp {
                        let clip = ui.clip_rect();
                        let zone = line_height * 2.0;
                        if pp.y < clip.top() + zone {
                            ui.scroll_with_delta(egui::Vec2::new(0.0, line_height));
                        } else if pp.y > clip.bottom() - zone {
                            ui.scroll_with_delta(egui::Vec2::new(0.0, -line_height));
                        }
                    }
                }
            }
        });
    }

    // ── Apply breakpoint gutter click → dap_panel ────────────────────────────
    if let Some((path, lines)) = bp_gutter_click {
        state
            .dap_panel
            .pending_set_breakpoints
            .retain(|(p, _)| p != &path);
        state.dap_panel.pending_set_breakpoints.push((path, lines));
    }

    // ── Apply fold toggles ──────────────────────────────────────────────────
    if let Some(tab) = state.tabs.get_mut(active_idx) {
        for idx in unsorted_fold_toggles {
            if idx < tab.folding_ranges.len() {
                if let Some(pos) = tab.collapsed_folds.iter().position(|&i| i == idx) {
                    tab.collapsed_folds.remove(pos);
                } else {
                    tab.collapsed_folds.push(idx);
                    tab.collapsed_folds.sort_unstable();
                }
            }
        }
    }

    // ── Apply pointer events → cursor placement / drag selection ──────────────
    match pointer_event {
        Some(PointerEvent::Press {
            pos,
            add_cursor,
            click_count,
        }) => {
            // Clicking in the editor clears any stale widget focus (e.g. from a
            // previously-open find bar) so plain-key bindings work immediately.
            ui.ctx().memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
            let now = ui.input(|i| i.time);
            let tab = &mut state.tabs[active_idx];

            // Update click-count tracking.
            tab.last_click_time = now;
            tab.last_click_pos = Some(pos);
            tab.click_count = click_count;

            match click_count {
                2 => {
                    // Double-click: select the word under the cursor.
                    tab.drag_anchor = None;
                    select_word_at(tab, pos);
                }
                n if n >= 3 => {
                    // Triple-click: select the entire line.
                    tab.drag_anchor = None;
                    select_line_at(tab, pos);
                }
                _ => {
                    // Single click: place cursor and begin potential drag.
                    tab.drag_anchor = Some(pos);
                    if add_cursor {
                        tab.cursors.add(pos);
                    } else {
                        tab.cursors.set_single(pos);
                    }
                }
            }
            state.caret_visible = true;
            state.last_blink_toggle = now;

            // Click in the editor dismisses LSP popups.
            state.hover_text = None;
            state.completion_visible = false;
            state.code_actions_visible = false;
            state.signature_help = None;
        }
        Some(PointerEvent::Drag { pos }) => {
            let tab = &mut state.tabs[active_idx];
            if let Some(anchor) = tab.drag_anchor {
                // Extend the primary cursor's selection from anchor to pos.
                let c = tab.cursors.primary_mut();
                c.selection.anchor = anchor;
                c.selection.active = pos;
                c.preferred_col = pos.character;
            }
            state.caret_visible = true;
            state.last_blink_toggle = ui.input(|i| i.time);
        }
        None => {
            // When mouse is released (anywhere, including scrollbar), clear the drag anchor.
            let released = ui.input(|i| i.pointer.primary_released());
            if released {
                state.tabs[active_idx].drag_anchor = None;
            }
        }
    }

    // ── Minimap (right side of editor content) ──────────────────────────────
    if state.minimap_visible {
        egui::Panel::right("minimap_panel")
            .resizable(true)
            .default_size(80.0)
            .min_size(40.0)
            .max_size(200.0)
            .frame(egui::Frame::NONE)
            .show_inside(ui, |ui| {
                crate::panels::minimap::show(ui, state);
            });
    }

    // ── LSP popup overlays (rendered after scroll area, positioned near cursor) ─
    if let Some(tab) = state.tabs.get(active_idx) {
        let cursor_pos = tab.cursors.primary().pos();

        // Compute approximate screen position for the popups.
        // We use the scroll area's known geometry: content starts at `text_left_x`
        // (after gutter) and each line is `line_height` pixels tall.
        // For the initial implementation we position near the cursor line.
        let screen_rect = ui.ctx().content_rect();
        let popup_x = screen_rect.left() + 80.0; // approximate gutter width + some padding
        let popup_y = screen_rect.top() + 100.0 + cursor_pos.line as f32 * line_height;

        show_hover_popup(ui, state, popup_x, popup_y);
        show_completion_popup(ui, state, popup_x, popup_y + line_height + 4.0);
        show_code_actions_popup(ui, state, popup_x, popup_y + line_height + 4.0);
        show_signature_help_popup(ui, state, popup_x, popup_y);
    }
}

// ── Multi-click selection helpers ─────────────────────────────────────────────

fn select_word_at(tab: &mut crate::state::EditorTab, pos: Position) {
    let Some(line) = tab.lines.get(pos.line as usize) else {
        return;
    };
    let chars: Vec<char> = line.chars().collect();
    let col = (pos.character as usize).min(chars.len());

    let is_word = |c: char| c.is_alphanumeric() || c == '_';

    let (start, end) = if col < chars.len() && is_word(chars[col]) {
        let mut s = col;
        let mut e = col;
        while s > 0 && is_word(chars[s - 1]) {
            s -= 1;
        }
        while e < chars.len() && is_word(chars[e]) {
            e += 1;
        }
        (s, e)
    } else if col < chars.len() {
        (col, col + 1)
    } else {
        return;
    };

    use crabide_core::types::Selection;
    tab.cursors.set_single(Position::new(pos.line, end as u32));
    tab.cursors.primary_mut().selection = Selection {
        anchor: Position::new(pos.line, start as u32),
        active: Position::new(pos.line, end as u32),
    };
}

fn select_line_at(tab: &mut crate::state::EditorTab, pos: Position) {
    let end_col = tab
        .lines
        .get(pos.line as usize)
        .map(|l| l.chars().count() as u32)
        .unwrap_or(0);
    use crabide_core::types::Selection;
    tab.cursors.set_single(Position::new(pos.line, end_col));
    tab.cursors.primary_mut().selection = Selection {
        anchor: Position::new(pos.line, 0),
        active: Position::new(pos.line, end_col),
    };
}

// ── Pointer event types ───────────────────────────────────────────────────────

/// A pointer interaction that should update cursor state.
enum PointerEvent {
    /// Mouse pressed.  `click_count` is 1 for single, 2 for double, 3+ for triple.
    Press {
        pos: Position,
        add_cursor: bool,
        click_count: u32,
    },
    /// Mouse held and dragged beyond the click threshold.  Extends selection.
    Drag { pos: Position },
}

// ── Pointer detection ─────────────────────────────────────────────────────────

/// All per-row geometry and click-state needed by `detect_row_pointer`.
#[derive(Copy, Clone)]
struct RowPointerCtx<'a> {
    row_top: f32,
    row_bottom: f32,
    row_left: f32,
    content_right: f32,
    line_idx: usize,
    char_width: f32,
    lines: &'a [String],
    drag_active: bool,
    is_first: bool,
    is_last: bool,
    last_click_time: f64,
    last_click_pos: Option<Position>,
    prev_click_count: u32,
    /// True when an overlay (Window/Area) owns the pointer — block new presses.
    pointer_blocked: bool,
}

/// Check for pointer press or drag within this row and update `pointer_event`.
///
/// When `drag_active` is true and this is the topmost (`is_first`) or bottommost
/// (`is_last`) visible row, the drag is clamped to that row so selection continues
/// to update even when the pointer has moved above or below the visible content.
fn detect_row_pointer(
    ui: &egui::Ui,
    ctx: &RowPointerCtx<'_>,
    pointer_event: &mut Option<PointerEvent>,
) {
    let RowPointerCtx {
        row_top,
        row_bottom,
        row_left,
        content_right,
        line_idx,
        char_width,
        lines,
        drag_active,
        is_first,
        is_last,
        last_click_time,
        last_click_pos,
        prev_click_count,
        pointer_blocked,
    } = *ctx;
    let (pressed, down, dragging, alt, hover_pos, now) = ui.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.is_decidedly_dragging(),
            i.modifiers.alt,
            i.pointer.hover_pos(),
            i.time,
        )
    });

    // Block new press events when an overlay (Window/Area) owns the pointer.
    if pressed && pointer_blocked {
        return;
    }

    // Block ALL pointer events while another egui widget has claimed the drag
    // via ui.interact() — the canonical example is the scrollbar thumb.
    // The editor's own text-selection logic never calls ui.interact(), so
    // is_using_pointer() is false during editor drags and this guard is a no-op
    // for text selection.  It fires on frame 2+ of a scrollbar drag (the
    // scrollbar claimed the drag in the previous frame via interact(Sense::drag()))
    // and suppresses both the Drag and Press paths below.
    if ui.ctx().egui_is_using_pointer() {
        return;
    }

    let Some(pp) = hover_pos else { return };

    // Cap content_right at the clip rect's right edge.
    // `available_rect_before_wrap().right()` can extend into the scrollbar
    // region because it comes from max_rect, not the clipped content rect.
    // egui's ScrollArea always sets clip_rect to the content area *excluding*
    // the scrollbar, so this is the authoritative right boundary.
    let content_right = content_right.min(ui.clip_rect().right());

    // Restrict to the scroll-area content rect (excludes scrollbar on the right).
    let row_hit = pp.y >= row_top && pp.y < row_bottom && pp.x >= row_left && pp.x < content_right;
    let drag_top =
        drag_active && is_first && pp.y < row_top && pp.x >= row_left && pp.x < content_right;
    let drag_bot =
        drag_active && is_last && pp.y >= row_bottom && pp.x >= row_left && pp.x < content_right;

    if !row_hit && !drag_top && !drag_bot {
        return;
    }

    let text_left_x = row_left + gutter::GUTTER_WIDTH;
    let line_char_count = lines.get(line_idx).map(|l| l.chars().count()).unwrap_or(0);

    let col = if drag_top {
        0
    } else if drag_bot {
        line_char_count
    } else {
        let rel_x = (pp.x - text_left_x).max(0.0);
        ((rel_x / char_width).round() as usize).min(line_char_count)
    };

    let pos = Position::new(line_idx as u32, col as u32);

    if pressed && row_hit {
        // Compute consecutive-click count (double/triple click detection).
        const MULTI_CLICK_SECS: f64 = 0.4;
        let same_pos = last_click_pos == Some(pos);
        let in_time = (now - last_click_time) < MULTI_CLICK_SECS;
        let click_count = if same_pos && in_time {
            prev_click_count + 1
        } else {
            1
        };
        *pointer_event = Some(PointerEvent::Press {
            pos,
            add_cursor: alt,
            click_count,
        });
    } else if down && dragging && drag_active && (row_hit || drag_top || drag_bot) {
        *pointer_event = Some(PointerEvent::Drag { pos });
    }
}

// ── Highlight painters ────────────────────────────────────────────────────────

/// Per-line geometry shared by all paint helpers.
#[derive(Copy, Clone)]
struct LineRenderCtx<'a> {
    line_idx: usize,
    line_text: &'a str,
    text_left_x: f32,
    row_top: f32,
    line_height: f32,
    char_width: f32,
}

/// Paint a snippet tabstop highlight on `line_idx` if the range overlaps.
fn paint_tabstop_on_line(
    ui: &mut egui::Ui,
    range: crabide_core::types::Range,
    ctx: LineRenderCtx<'_>,
    color: egui::Color32,
) {
    let LineRenderCtx {
        line_idx,
        line_text,
        text_left_x,
        row_top,
        line_height,
        char_width,
    } = ctx;
    let line = line_idx as u32;
    if range.end.line < line || range.start.line > line {
        return;
    }

    let line_char_count = line_text.chars().count() as u32;
    let col_start = if range.start.line < line {
        0
    } else {
        range.start.character.min(line_char_count)
    };
    let col_end = if range.end.line > line {
        line_char_count
    } else {
        range.end.character.min(line_char_count)
    };

    if col_start > col_end {
        return;
    }
    let effective_end = if col_end == col_start {
        col_start + 1
    } else {
        col_end
    };

    // Draw a filled rect for the placeholder body.
    let x0 = text_left_x + col_start as f32 * char_width;
    let x1 = text_left_x + effective_end as f32 * char_width;
    ui.painter().rect_filled(
        egui::Rect::from_min_max(
            egui::pos2(x0, row_top),
            egui::pos2(x1, row_top + line_height),
        ),
        2.0,
        color,
    );
    // Draw a 1px underline at the bottom of the highlight to make it distinct.
    let underline_color =
        egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 255);
    ui.painter().line_segment(
        [
            egui::pos2(x0, row_top + line_height - 1.0),
            egui::pos2(x1, row_top + line_height - 1.0),
        ],
        egui::Stroke::new(1.5, underline_color),
    );
}

/// Paint squiggly diagnostic underlines on `line_idx` for all diagnostics
/// whose range touches this line.  Error = red, Warning = yellow,
/// Information = blue, Hint = grey.  Each severity uses a zigzag pattern
/// (3 px wide teeth, 1.5 px amplitude) so it's visually distinct from the
/// straight snippet-tabstop underline.
fn paint_diagnostic_underlines(
    ui: &mut egui::Ui,
    diagnostics: &[Diagnostic],
    ctx: LineRenderCtx<'_>,
) {
    let LineRenderCtx {
        line_idx,
        line_text,
        text_left_x,
        row_top,
        line_height,
        char_width,
    } = ctx;
    let line = line_idx as u32;
    let line_len = line_text.chars().count() as u32;

    for diag in diagnostics {
        if diag.range.end.line < line || diag.range.start.line > line {
            continue;
        }

        let col_start = if diag.range.start.line < line {
            0
        } else {
            diag.range.start.character.min(line_len)
        };
        let col_end = if diag.range.end.line > line {
            line_len
        } else {
            diag.range.end.character.min(line_len)
        };
        // Guarantee at least one character width so zero-length ranges are visible.
        let col_end = col_end.max(col_start + 1).min(line_len.max(col_start + 1));

        let x0 = text_left_x + col_start as f32 * char_width;
        let x1 = text_left_x + col_end as f32 * char_width;
        let y = row_top + line_height - 2.5;

        let color = match diag.severity {
            DiagnosticSeverity::Error => egui::Color32::from_rgb(0xf4, 0x43, 0x36),
            DiagnosticSeverity::Warning => egui::Color32::from_rgb(0xff, 0xb3, 0x00),
            DiagnosticSeverity::Information => egui::Color32::from_rgb(0x29, 0xb6, 0xf6),
            DiagnosticSeverity::Hint => {
                egui::Color32::from_rgba_unmultiplied(0x80, 0x80, 0x80, 0xa0)
            }
        };

        // Zigzag: alternate between y and y+amp every `step` pixels.
        let step = 3.0_f32;
        let amp = 1.5_f32;
        let mut x = x0;
        let mut top = true;
        while x < x1 {
            let x_end = (x + step).min(x1);
            let y_start = if top { y } else { y + amp };
            let y_end = if top { y + amp } else { y };
            ui.painter().line_segment(
                [egui::pos2(x, y_start), egui::pos2(x_end, y_end)],
                egui::Stroke::new(1.0, color),
            );
            x = x_end;
            top = !top;
        }
    }
}

/// Paint bracket match border on `line_idx` for a given bracket `range`.
fn paint_bracket_on_line(
    ui: &mut egui::Ui,
    range: crabide_core::types::Range,
    ctx: LineRenderCtx<'_>,
    bg_color: egui::Color32,
    border_color: egui::Color32,
) {
    let LineRenderCtx {
        line_idx,
        line_text,
        text_left_x,
        row_top,
        line_height,
        char_width,
    } = ctx;
    let line = line_idx as u32;
    if range.start.line != line || range.end.line != line {
        return;
    }

    let line_char_count = line_text.chars().count() as u32;
    let col_start = range.start.character.min(line_char_count);
    let col_end = range.end.character.min(line_char_count);
    if col_start >= col_end {
        return;
    }

    let x0 = text_left_x + col_start as f32 * char_width;
    let x1 = text_left_x + col_end as f32 * char_width;
    let rect = egui::Rect::from_min_max(
        egui::pos2(x0, row_top),
        egui::pos2(x1, row_top + line_height),
    );

    ui.painter().rect_filled(rect, 0.0, bg_color);
    ui.painter().rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(1.0, border_color),
        egui::StrokeKind::Middle,
    );
}

/// Paint inlay hints on a line. Inlay hints are rendered as small, muted text
/// at the hint's position. Type hints appear after the expression with a
/// leading `:`; parameter hints appear before the argument with a trailing `:`.
fn paint_inlay_hints_on_line(
    ui: &mut egui::Ui,
    hints: &[crabide_core::event::InlayHint],
    ctx: LineRenderCtx<'_>,
    theme: &crabide_config::ColorTheme,
) {
    let LineRenderCtx {
        line_idx,
        text_left_x,
        row_top,
        line_height,
        char_width,
        ..
    } = ctx;
    let line = line_idx as u32;

    let type_fg = cfg_to_egui(theme.ui_or(
        "editorInlayHint.foreground",
        crabide_config::Color::rgb(0x96, 0x96, 0x96),
    ));
    let param_fg = cfg_to_egui(theme.ui_or(
        "editorInlayHint.foreground",
        crabide_config::Color::rgb(0x96, 0x96, 0x96),
    ));
    let bg = cfg_to_egui(theme.ui_or(
        "editorInlayHint.background",
        crabide_config::Color::rgba(0x40, 0x40, 0x40, 0x80),
    ));

    let font_id = egui::FontId::monospace(11.0);

    for hint in hints {
        if hint.position.line != line {
            continue;
        }

        let label = match hint.kind {
            Some(crabide_core::event::InlayHintKind::Type) => {
                format!(": {}", hint.label)
            }
            Some(crabide_core::event::InlayHintKind::Parameter) => {
                format!("{}:", hint.label)
            }
            None => hint.label.clone(),
        };

        let fg = match hint.kind {
            Some(crabide_core::event::InlayHintKind::Type) => type_fg,
            Some(crabide_core::event::InlayHintKind::Parameter) => param_fg,
            None => type_fg,
        };

        let col = hint.position.character;
        let x = text_left_x + col as f32 * char_width;
        let galley = ui.fonts_mut(|f| f.layout(label.clone(), font_id.clone(), fg, f32::INFINITY));
        let galley_w = galley.size().x;
        let galley_h = galley.size().y;

        // Background pill behind the hint text.
        let pad = 2.0;
        let pill_rect = egui::Rect::from_min_size(
            egui::pos2(x - pad, row_top + (line_height - galley_h) / 2.0 - 1.0),
            egui::vec2(galley_w + pad * 2.0, galley_h + 2.0),
        );
        ui.painter().rect_filled(pill_rect, 3.0, bg);

        // Hint text.
        ui.painter().galley(
            egui::pos2(x, row_top + (line_height - galley_h) / 2.0),
            galley,
            fg,
        );
    }
}

// ── Syntax-highlighted line builder ──────────────────────────────────────────

/// Build an egui `LayoutJob` for `line_text` coloured with the highlight spans
/// that overlap `line_idx`.
///
/// `bg_ranges` carries (document-range, color) pairs for selections and find
/// matches.  Each pair is translated to `TextFormat::background` so the colour
/// is painted by egui's text layout engine — guaranteed behind the glyphs and
/// never clipped by the wgpu render-batch ordering.
fn build_line_job(
    line_text: &str,
    line_idx: usize,
    highlight_spans: &[crabide_syntax::HighlightSpan],
    theme: &crabide_config::ColorTheme,
    font_id: egui::FontId,
    bg_ranges: &[(crabide_core::types::Range, egui::Color32)],
) -> LayoutJob {
    let mut job = LayoutJob::default();

    let default_fg = cfg_to_egui(theme.ui_or(
        "editor.foreground",
        crabide_config::Color::rgb(0xd4, 0xd4, 0xd4),
    ));

    let chars: Vec<char> = line_text.chars().collect();
    let line_char_count = chars.len();

    if line_char_count == 0 {
        // Empty line: check whether any bg_range covers it (e.g. whole-line selection).
        let line_u32 = line_idx as u32;
        let bg = bg_ranges
            .iter()
            .rev()
            .find_map(|(r, c)| {
                if r.start.line <= line_u32 && line_u32 <= r.end.line {
                    Some(*c)
                } else {
                    None
                }
            })
            .unwrap_or(egui::Color32::TRANSPARENT);
        job.append(
            " ",
            0.0,
            TextFormat {
                font_id,
                color: default_fg,
                background: bg,
                ..Default::default()
            },
        );
        return job;
    }

    let line_u32 = line_idx as u32;

    struct ColorSeg {
        start: usize,
        end: usize,
        color: egui::Color32,
    }

    // Foreground colour segments from syntax highlight spans.
    let mut fg_segs: Vec<ColorSeg> = highlight_spans
        .iter()
        .filter_map(|sp| {
            if sp.range.end.line < line_u32 || sp.range.start.line > line_u32 {
                return None;
            }
            let col_start = if sp.range.start.line < line_u32 {
                0
            } else {
                sp.range.start.character as usize
            };
            let col_end = if sp.range.end.line > line_u32 {
                line_char_count
            } else {
                sp.range.end.character as usize
            };
            let col_end = col_end.min(line_char_count);
            if col_start >= col_end {
                return None;
            }
            let vscope = scope_to_vscode(sp.scope.as_str());
            let ts = theme.token_style(vscope);
            let color = ts.foreground.map(cfg_to_egui).unwrap_or(default_fg);
            Some(ColorSeg {
                start: col_start,
                end: col_end,
                color,
            })
        })
        .collect();
    fg_segs.sort_by_key(|s| s.start);

    // Background colour segments from selection / find-match ranges.
    let mut bg_segs: Vec<ColorSeg> = bg_ranges
        .iter()
        .filter_map(|(r, color)| {
            if r.end.line < line_u32 || r.start.line > line_u32 {
                return None;
            }
            let col_start = if r.start.line < line_u32 {
                0
            } else {
                r.start.character as usize
            };
            let col_end = if r.end.line > line_u32 {
                line_char_count
            } else {
                r.end.character as usize
            };
            let col_end = col_end.min(line_char_count);
            if col_start >= col_end {
                return None;
            }
            Some(ColorSeg {
                start: col_start,
                end: col_end,
                color: *color,
            })
        })
        .collect();
    bg_segs.sort_by_key(|s| s.start);

    // Collect every boundary point from both segment sets, then iterate the
    // resulting intervals so each interval has a uniform (fg, bg) colour pair.
    let mut boundaries: Vec<usize> = vec![0, line_char_count];
    for s in &fg_segs {
        boundaries.push(s.start);
        boundaries.push(s.end);
    }
    for s in &bg_segs {
        boundaries.push(s.start);
        boundaries.push(s.end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let fg_at = |col: usize| -> egui::Color32 {
        for seg in fg_segs.iter().rev() {
            if seg.start <= col && col < seg.end {
                return seg.color;
            }
        }
        default_fg
    };
    let bg_at = |col: usize| -> egui::Color32 {
        // Later entries (higher priority, e.g. current match) win.
        for seg in bg_segs.iter().rev() {
            if seg.start <= col && col < seg.end {
                return seg.color;
            }
        }
        egui::Color32::TRANSPARENT
    };

    for w in boundaries.windows(2) {
        let start = w[0];
        let end = w[1].min(line_char_count);
        if start >= line_char_count || start >= end {
            continue;
        }
        let text: String = chars[start..end].iter().collect();
        job.append(
            &text,
            0.0,
            TextFormat {
                font_id: font_id.clone(),
                color: fg_at(start),
                background: bg_at(start),
                ..Default::default()
            },
        );
    }

    if job.sections.is_empty() {
        job.append(
            " ",
            0.0,
            TextFormat {
                font_id,
                color: default_fg,
                ..Default::default()
            },
        );
    }

    job
}

// ── Welcome screen ────────────────────────────────────────────────────────────

fn show_welcome(ui: &mut egui::Ui, state: &UiState) {
    let t = &state.theme;
    let accent = cfg_to_egui(t.ui_or(
        "statusBar.background",
        crabide_config::Color::rgb(0x00, 0x7a, 0xcc),
    ));
    let text = cfg_to_egui(t.ui_or(
        "editor.foreground",
        crabide_config::Color::rgb(0xd4, 0xd4, 0xd4),
    ));
    let dim = cfg_to_egui(t.ui_or(
        "editorLineNumber.foreground",
        crabide_config::Color::rgb(0x85, 0x85, 0x85),
    ));
    let card_bg = cfg_to_egui(t.ui_or(
        "sideBarSectionHeader.background",
        crabide_config::Color::rgb(0x2d, 0x2d, 0x2d),
    ));
    let border = cfg_to_egui(t.ui_or(
        "sideBar.border",
        crabide_config::Color::rgb(0x33, 0x33, 0x33),
    ));
    let key_bg = cfg_to_egui(t.ui_or(
        "list.hoverBackground",
        crabide_config::Color::rgb(0x2a, 0x2d, 0x2e),
    ));

    let total_h = ui.available_height();
    let top_pad = (total_h * 0.12).clamp(32.0, 80.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                // Constrain the content area so it doesn't stretch edge-to-edge on wide windows.
                let avail = ui.available_width();
                let content_w = avail.min(680.0);
                ui.set_max_width(content_w);

                ui.add_space(top_pad);

                // ── Brand block ───────────────────────────────────────────────
                ui.add(egui::Label::new(
                    egui::RichText::new("crabide")
                        .size(46.0)
                        .color(accent)
                        .strong(),
                ));
                ui.add_space(8.0);
                ui.add(egui::Label::new(
                    egui::RichText::new("Resource-efficient code editor  |  Rust + egui")
                        .size(13.0)
                        .color(dim),
                ));

                ui.add_space(32.0);

                // ── Cards — two equal columns ─────────────────────────────────
                let col_w = ((content_w - 16.0) / 2.0).clamp(180.0, 300.0);
                let card_theme = WelcomeCardTheme {
                    card_bg,
                    border,
                    text,
                    dim,
                    key_bg,
                    accent,
                };
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    welcome_card(
                        ui,
                        col_w,
                        card_theme,
                        "Start",
                        &[
                            ("New File", "Ctrl+N"),
                            ("Open File...", "Ctrl+O"),
                            ("Open Recent...", "Ctrl+P"),
                            ("Command Palette", "Ctrl+Shift+P"),
                        ],
                    );
                    welcome_card(
                        ui,
                        col_w,
                        card_theme,
                        "Tools",
                        &[
                            ("Find in Files", "Ctrl+Shift+F"),
                            ("Toggle Terminal", "Ctrl+`"),
                            ("Source Control", "Ctrl+Shift+G"),
                            ("Toggle Theme", "Ctrl+K T"),
                        ],
                    );
                });

                ui.add_space(28.0);

                // ── Tiny footer ───────────────────────────────────────────────
                ui.add(egui::Label::new(
                    egui::RichText::new("Tip: use Ctrl+Shift+P to discover all commands")
                        .size(11.0)
                        .color(dim),
                ));

                ui.add_space(24.0);
            });
        });
}

/// Theme colors shared by all welcome-screen cards.
#[derive(Copy, Clone)]
struct WelcomeCardTheme {
    card_bg: egui::Color32,
    border: egui::Color32,
    text: egui::Color32,
    dim: egui::Color32,
    key_bg: egui::Color32,
    accent: egui::Color32,
}

/// Render one welcome-screen card.
fn welcome_card(
    ui: &mut egui::Ui,
    width: f32,
    theme: WelcomeCardTheme,
    title: &str,
    rows: &[(&str, &str)],
) {
    let WelcomeCardTheme {
        card_bg,
        border,
        text,
        dim,
        key_bg,
        accent,
    } = theme;
    egui::Frame::NONE
        .fill(card_bg)
        .stroke(egui::Stroke::new(1.0, border))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin {
            left: 14,
            right: 14,
            top: 12,
            bottom: 14,
        })
        .show(ui, |ui| {
            ui.set_width(width - 2.0);

            // Card title
            ui.add(egui::Label::new(
                egui::RichText::new(title.to_uppercase())
                    .size(10.5)
                    .color(accent)
                    .strong(),
            ));
            ui.add_space(4.0);
            ui.add(egui::Separator::default().horizontal().spacing(0.0));
            ui.add_space(4.0);

            let label_font = egui::FontId::proportional(13.0);
            let key_font = egui::FontId::monospace(10.5);
            let row_h = 26.0_f32;

            for &(label, key) in rows {
                let inner_w = ui.available_width();

                // Measure the key pill so we can clip the label to the left portion.
                let pill_w = if key.is_empty() {
                    0.0_f32
                } else {
                    let g = ui
                        .painter()
                        .layout_no_wrap(key.to_owned(), key_font.clone(), dim);
                    g.rect.width() + 10.0
                };

                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(inner_w, row_h), egui::Sense::hover());
                if !ui.is_rect_visible(rect) {
                    continue;
                }

                // Label clipped to avoid overlapping the key pill.
                let label_clip_x = if pill_w > 0.0 {
                    rect.right() - pill_w - 8.0
                } else {
                    rect.right()
                };
                let label_clip = egui::Rect::from_min_max(
                    egui::pos2(rect.left(), rect.top()),
                    egui::pos2(label_clip_x, rect.bottom()),
                );
                ui.painter().with_clip_rect(label_clip).text(
                    egui::pos2(rect.left(), rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    label,
                    label_font.clone(),
                    text,
                );

                // Key pill — right-aligned.
                if pill_w > 0.0 {
                    let g = ui
                        .painter()
                        .layout_no_wrap(key.to_owned(), key_font.clone(), dim);
                    let kw = g.rect.width() + 10.0;
                    let kh = g.rect.height() + 4.0;
                    let kr = egui::Rect::from_min_size(
                        egui::pos2(rect.right() - kw, rect.center().y - kh / 2.0),
                        egui::vec2(kw, kh),
                    );
                    ui.painter()
                        .rect_filled(kr, egui::CornerRadius::same(3), key_bg);
                    ui.painter().text(
                        kr.center(),
                        egui::Align2::CENTER_CENTER,
                        key,
                        key_font.clone(),
                        dim,
                    );
                }
            }
        });
}

// ── Click → cursor placement (public helper) ──────────────────────────────────

/// Convert a pointer position (in content-area coordinates) to a `Position`.
pub fn pointer_to_position(
    pointer: egui::Pos2,
    content_top_left: egui::Pos2,
    line_height: f32,
    char_width: f32,
    max_line: usize,
    lines: &[String],
) -> Position {
    let rel_y = (pointer.y - content_top_left.y).max(0.0);
    let rel_x = (pointer.x - content_top_left.x).max(0.0);
    let line = ((rel_y / line_height) as usize).min(max_line);
    let line_len = lines.get(line).map(|l| l.chars().count()).unwrap_or(0);
    let col = ((rel_x / char_width).round() as usize).min(line_len);
    Position::new(line as u32, col as u32)
}

// ── LSP popup overlay renderers ───────────────────────────────────────────────

/// Theme-consistent popup background/fg/style settings.
struct PopupColors {
    bg: egui::Color32,
    fg: egui::Color32,
    dim: egui::Color32,
    selected_bg: egui::Color32,
    border: egui::Color32,
    keyword: egui::Color32,
}

fn popup_colors(state: &UiState) -> PopupColors {
    PopupColors {
        bg: cfg_to_egui(state.theme.ui_or(
            "dropdown.background",
            crabide_config::Color::rgb(0x25, 0x25, 0x26),
        )),
        fg: cfg_to_egui(state.theme.ui_or(
            "editor.foreground",
            crabide_config::Color::rgb(0xd4, 0xd4, 0xd4),
        )),
        dim: cfg_to_egui(state.theme.ui_or(
            "editorLineNumber.foreground",
            crabide_config::Color::rgb(0x85, 0x85, 0x85),
        )),
        selected_bg: cfg_to_egui(state.theme.ui_or(
            "list.activeSelectionBackground",
            crabide_config::Color::rgb(0x09, 0x47, 0x71),
        )),
        border: cfg_to_egui(
            state
                .theme
                .ui_or("panel.border", crabide_config::Color::rgb(0x45, 0x45, 0x45)),
        ),
        keyword: cfg_to_egui(state.theme.ui_or(
            "textLink.foreground",
            crabide_config::Color::rgb(0x37, 0x95, 0xff),
        )),
    }
}

/// Render the hover popup (shows `hover_text` as a floating overlay).
fn show_hover_popup(ui: &mut egui::Ui, state: &mut UiState, x: f32, y: f32) {
    let Some(ref hover_text) = state.hover_text else {
        return;
    };
    if hover_text.is_empty() {
        state.hover_text = None;
        return;
    }

    let hover_text = hover_text.clone();
    let pc = popup_colors(state);
    let max_w = 400.0_f32.min(ui.ctx().content_rect().width() * 0.6);

    egui::Area::new(egui::Id::new("lsp_hover_popup"))
        .fixed_pos(egui::pos2(
            x.min(ui.ctx().content_rect().right() - max_w - 20.0),
            y,
        ))
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            egui::Frame::default()
                .fill(pc.bg)
                .stroke(egui::Stroke::new(1.0, pc.border))
                .corner_radius(egui::CornerRadius::same(4))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_max_width(max_w);
                    // Handle Escape to close.
                    let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
                    if escape {
                        state.hover_text = None;
                        return;
                    }

                    // Parse markdown-like hover content (simple line-based rendering).
                    let mut first = true;
                    for line in hover_text.lines() {
                        if !first {
                            ui.add_space(2.0);
                        }
                        first = false;
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }
                        // Code blocks (```) are just skipped in this simple renderer.
                        if line.starts_with("```") {
                            continue;
                        }
                        // Simple markdown formatting: **bold** and `code`
                        let has_code = line.contains('`');
                        if has_code {
                            // Split on backticks and style code segments.
                            let mut in_code = false;
                            for segment in line.split('`') {
                                if in_code {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(segment)
                                                .code()
                                                .color(pc.keyword)
                                                .size(state.font_size - 1.0),
                                        )
                                        .wrap(),
                                    );
                                } else {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(segment)
                                                .color(pc.fg)
                                                .size(state.font_size - 1.0),
                                        )
                                        .wrap(),
                                    );
                                }
                                in_code = !in_code;
                            }
                        } else {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(line)
                                        .color(pc.fg)
                                        .size(state.font_size - 1.0),
                                )
                                .wrap(),
                            );
                        }
                    }
                });
        });
}

/// Render the completion item popup (dropdown list).
fn show_completion_popup(ui: &mut egui::Ui, state: &mut UiState, x: f32, y: f32) {
    if !state.completion_visible || state.completion_items.is_empty() {
        return;
    }

    let pc = popup_colors(state);
    // Clone items to avoid borrow conflict with state mutations inside the closure.
    let items: Vec<_> = state.completion_items.clone();
    let item_count = items.len();

    // Navigation: Up/Down/Enter/Escape.
    let (pressed_up, pressed_down, pressed_enter, pressed_escape) = ui.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::Enter),
            i.key_pressed(egui::Key::Escape),
        )
    });

    if pressed_escape {
        state.completion_visible = false;
        state.completion_items.clear();
        state.completion_selected_idx = 0;
        return;
    }

    if pressed_up && state.completion_selected_idx > 0 {
        state.completion_selected_idx -= 1;
    }
    if pressed_down && state.completion_selected_idx + 1 < item_count {
        state.completion_selected_idx += 1;
    }

    if pressed_enter && !items.is_empty() {
        let idx = state.completion_selected_idx.min(item_count - 1);
        let item = &items[idx];
        let insert = item
            .insert_text
            .as_deref()
            .unwrap_or(&item.label)
            .to_owned();
        // Close popup and insert the selected completion text.
        state.completion_visible = false;
        state.completion_items.clear();
        state.completion_selected_idx = 0;
        // We can't push to actions here (not in scope), so instead we queue
        // the text via an InsertText action stored in a dedicated pending field.
        state.pending_completion_insert = Some(insert);
        return;
    }

    let max_visible = 8.min(item_count);
    let item_h = 22.0_f32;
    let popup_w = 320.0_f32.min(ui.ctx().content_rect().width() * 0.4);

    egui::Area::new(egui::Id::new("lsp_completion_popup"))
        .fixed_pos(egui::pos2(
            x.min(ui.ctx().content_rect().right() - popup_w - 20.0),
            y,
        ))
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            egui::Frame::default()
                .fill(pc.bg)
                .stroke(egui::Stroke::new(1.0, pc.border))
                .corner_radius(egui::CornerRadius::same(4))
                .show(ui, |ui| {
                    ui.set_min_width(popup_w);
                    ui.set_max_width(popup_w);
                    let avail = ui.available_height();
                    let needed = item_h * max_visible as f32 + 8.0;
                    ui.set_max_height(needed.min(avail));

                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for (i, item) in items.iter().enumerate() {
                                let is_selected = i == state.completion_selected_idx;
                                let label_bg = if is_selected {
                                    pc.selected_bg
                                } else {
                                    egui::Color32::TRANSPARENT
                                };
                                let label_fg = if is_selected {
                                    egui::Color32::WHITE
                                } else {
                                    pc.fg
                                };

                                let (rect, resp) = ui.allocate_exact_size(
                                    egui::vec2(popup_w, item_h),
                                    egui::Sense::click(),
                                );
                                if !ui.is_rect_visible(rect) {
                                    continue;
                                }

                                // Fill background
                                ui.painter().rect_filled(rect, 0.0, label_bg);

                                // Kind icon / label
                                let kind_str = completion_kind_icon(item.kind);
                                let left_x = rect.left() + 4.0;
                                ui.painter().text(
                                    egui::pos2(left_x, rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    format!("{kind_str} {}", item.label),
                                    egui::FontId::proportional(state.font_size - 1.0),
                                    label_fg,
                                );

                                // Detail text (right-aligned)
                                if let Some(ref detail) = item.detail {
                                    let _ = ui.painter().layout_no_wrap(
                                        detail.clone(),
                                        egui::FontId::proportional(state.font_size - 2.0),
                                        pc.dim,
                                    );
                                    ui.painter().text(
                                        egui::pos2(rect.right() - 4.0, rect.center().y),
                                        egui::Align2::RIGHT_CENTER,
                                        detail.clone(),
                                        egui::FontId::proportional(state.font_size - 2.0),
                                        pc.dim,
                                    );
                                }

                                if resp.clicked() {
                                    let insert = item
                                        .insert_text
                                        .as_deref()
                                        .unwrap_or(&item.label)
                                        .to_owned();
                                    state.completion_visible = false;
                                    state.completion_items.clear();
                                    state.completion_selected_idx = 0;
                                    state.pending_completion_insert = Some(insert);
                                }
                            }
                        });
                });
        });
}

/// Render the code actions popup (dropdown list).
fn show_code_actions_popup(ui: &mut egui::Ui, state: &mut UiState, x: f32, y: f32) {
    if !state.code_actions_visible || state.code_actions.is_empty() {
        return;
    }

    let pc = popup_colors(state);
    let items = state.code_actions.clone();
    let item_count = items.len();

    let (pressed_up, pressed_down, pressed_enter, pressed_escape) = ui.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::Enter),
            i.key_pressed(egui::Key::Escape),
        )
    });

    if pressed_escape {
        state.code_actions_visible = false;
        state.code_actions.clear();
        state.code_actions_selected_idx = 0;
        return;
    }

    if pressed_up && state.code_actions_selected_idx > 0 {
        state.code_actions_selected_idx -= 1;
    }
    if pressed_down && state.code_actions_selected_idx + 1 < item_count {
        state.code_actions_selected_idx += 1;
    }

    if pressed_enter && !items.is_empty() {
        let idx = state.code_actions_selected_idx.min(item_count - 1);
        // Queue the selected code action index for the app to process.
        state.pending_code_action_idx = Some(idx);
        state.code_actions_visible = false;
        state.code_actions_selected_idx = 0;
        return;
    }

    let max_visible = 8.min(item_count);
    let item_h = 22.0_f32;
    let popup_w = 360.0_f32.min(ui.ctx().content_rect().width() * 0.5);

    egui::Area::new(egui::Id::new("lsp_code_actions_popup"))
        .fixed_pos(egui::pos2(
            x.min(ui.ctx().content_rect().right() - popup_w - 20.0),
            y,
        ))
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            egui::Frame::default()
                .fill(pc.bg)
                .stroke(egui::Stroke::new(1.0, pc.border))
                .corner_radius(egui::CornerRadius::same(4))
                .show(ui, |ui| {
                    ui.set_min_width(popup_w);
                    ui.set_max_width(popup_w);
                    let avail = ui.available_height();
                    let needed = item_h * max_visible as f32 + 8.0;
                    ui.set_max_height(needed.min(avail));

                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for (i, item) in items.iter().enumerate() {
                                let is_selected = i == state.code_actions_selected_idx;
                                let label_bg = if is_selected {
                                    pc.selected_bg
                                } else {
                                    egui::Color32::TRANSPARENT
                                };
                                let label_fg = if is_selected {
                                    egui::Color32::WHITE
                                } else {
                                    pc.fg
                                };

                                let (rect, resp) = ui.allocate_exact_size(
                                    egui::vec2(popup_w, item_h),
                                    egui::Sense::click(),
                                );
                                if !ui.is_rect_visible(rect) {
                                    continue;
                                }

                                ui.painter().rect_filled(rect, 0.0, label_bg);

                                // Kind badge (optional)
                                let kind_prefix = item
                                    .kind
                                    .as_deref()
                                    .map(|k| format!("[{k}] "))
                                    .unwrap_or_default();
                                ui.painter().text(
                                    egui::pos2(rect.left() + 4.0, rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    format!("{kind_prefix}{}", item.title),
                                    egui::FontId::proportional(state.font_size - 1.0),
                                    label_fg,
                                );

                                if resp.clicked() {
                                    state.pending_code_action_idx = Some(i);
                                    state.code_actions_visible = false;
                                    state.code_actions.clear();
                                    state.code_actions_selected_idx = 0;
                                }
                            }
                        });
                });
        });
}

/// Render the signature help popup (shows function signature overlay).
fn show_signature_help_popup(ui: &mut egui::Ui, state: &mut UiState, x: f32, y: f32) {
    let Some(ref sig) = state.signature_help else {
        return;
    };
    if sig.signatures.is_empty() {
        state.signature_help = None;
        return;
    }

    let pc = popup_colors(state);
    let active_sig_idx = sig.active_signature.unwrap_or(0) as usize;
    let active_sig = sig
        .signatures
        .get(active_sig_idx.min(sig.signatures.len() - 1));
    let Some(sig_info) = active_sig else {
        return;
    };

    // Clone data needed inside the closure to avoid borrow conflicts.
    let sig_info = sig_info.clone();
    let active_param = sig.active_parameter.map(|p| p as usize);

    let max_w = 450.0_f32.min(ui.ctx().content_rect().width() * 0.6);

    egui::Area::new(egui::Id::new("lsp_signature_help_popup"))
        .fixed_pos(egui::pos2(
            x.min(ui.ctx().content_rect().right() - max_w - 20.0),
            y - 60.0, // show above the cursor line
        ))
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            egui::Frame::default()
                .fill(pc.bg)
                .stroke(egui::Stroke::new(1.0, pc.border))
                .corner_radius(egui::CornerRadius::same(4))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_max_width(max_w);

                    // Escape closes.
                    let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
                    if escape {
                        state.signature_help = None;
                        return;
                    }

                    // Show signature label with active parameter highlighted.
                    let full_label = &sig_info.label;

                    // Highlight the active parameter if we can split on paren/commas.
                    // Simple approach: split by parameter boundaries or show raw label.
                    if let Some(param_idx) = active_param {
                        if let Some(param) = sig_info.parameters.get(param_idx) {
                            let param_label = match &param.label {
                                crabide_core::event::ParameterLabel::Simple(s) => s.clone(),
                                crabide_core::event::ParameterLabel::Offsets(s, e) => {
                                    let s = *s as usize;
                                    let e = (*e as usize).min(full_label.len());
                                    if s < e {
                                        full_label[s..e].to_owned()
                                    } else {
                                        String::new()
                                    }
                                }
                            };
                            // Show the signature with the active parameter emphasized.
                            if !param_label.is_empty() {
                                if let Some(pos) = full_label.find(&param_label) {
                                    let before = &full_label[..pos];
                                    let after = &full_label[pos + param_label.len()..];
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(before)
                                                    .color(pc.fg)
                                                    .size(state.font_size - 1.0),
                                            )
                                            .wrap(),
                                        );
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(&param_label)
                                                    .color(pc.keyword)
                                                    .strong()
                                                    .size(state.font_size - 1.0),
                                            )
                                            .wrap(),
                                        );
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(after)
                                                    .color(pc.fg)
                                                    .size(state.font_size - 1.0),
                                            )
                                            .wrap(),
                                        );
                                    });
                                } else {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(full_label)
                                                .color(pc.fg)
                                                .size(state.font_size - 1.0),
                                        )
                                        .wrap(),
                                    );
                                }
                            } else {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(full_label)
                                            .color(pc.fg)
                                            .size(state.font_size - 1.0),
                                    )
                                    .wrap(),
                                );
                            }
                        } else {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(full_label)
                                        .color(pc.fg)
                                        .size(state.font_size - 1.0),
                                )
                                .wrap(),
                            );
                        }
                    } else {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(full_label)
                                    .color(pc.fg)
                                    .size(state.font_size - 1.0),
                            )
                            .wrap(),
                        );
                    }

                    // Show documentation below the signature.
                    if let Some(ref docs) = sig_info.documentation {
                        for line in docs.lines() {
                            let line = line.trim();
                            if line.is_empty() {
                                continue;
                            }
                            ui.add_space(4.0);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(line)
                                        .color(pc.dim)
                                        .size(state.font_size - 2.0),
                                )
                                .wrap(),
                            );
                        }
                    }
                });
        });
}

/// Return a short icon/emoji string for a completion kind.
fn completion_kind_icon(kind: Option<crabide_core::event::CompletionKind>) -> &'static str {
    use crabide_core::event::CompletionKind;
    match kind {
        None => "",
        Some(CompletionKind::Text) => "",
        Some(CompletionKind::Method) => "▣",
        Some(CompletionKind::Function) => "ƒ",
        Some(CompletionKind::Constructor) => "◈",
        Some(CompletionKind::Field) => "◎",
        Some(CompletionKind::Variable) => "●",
        Some(CompletionKind::Class) => "◆",
        Some(CompletionKind::Interface) => "◇",
        Some(CompletionKind::Module) => "◻",
        Some(CompletionKind::Property) => "■",
        Some(CompletionKind::Unit) => "□",
        Some(CompletionKind::Value) => "•",
        Some(CompletionKind::Enum) => "◉",
        Some(CompletionKind::Keyword) => "🔑",
        Some(CompletionKind::Snippet) => "✂",
        Some(CompletionKind::Color) => "🎨",
        Some(CompletionKind::File) => "📄",
        Some(CompletionKind::Reference) => "→",
        Some(CompletionKind::Folder) => "📁",
        Some(CompletionKind::EnumMember) => "◉",
        Some(CompletionKind::Constant) => "π",
        Some(CompletionKind::Struct) => "◈",
        Some(CompletionKind::Event) => "⚡",
        Some(CompletionKind::Operator) => "⊕",
        Some(CompletionKind::TypeParameter) => "τ",
    }
}
