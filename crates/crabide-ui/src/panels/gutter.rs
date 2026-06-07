//! Editor gutter: line numbers, diagnostic severity icons, git diff markers,
//! breakpoint indicators, and code folding controls.
//!
//! `show_line` is called once per visible line from inside the `ScrollArea`
//! in `editor.rs`, so its coordinate system is always correct for the
//! current scroll position.

use crabide_core::event::{Diagnostic, DiagnosticSeverity, DiffHunk, FoldingRange, HunkKind};

use crate::state::{cfg_to_egui, EditorTab, UiState};

/// Width of the gutter column in logical pixels.
/// Extended to accommodate the breakpoint circle and fold controls.
pub const GUTTER_WIDTH: f32 = 60.0;

/// Result of processing a gutter row.
pub enum GutterAction {
    /// No interaction.
    None,
    /// User clicked in the breakpoint zone for this line.
    ToggleBreakpoint,
    /// User clicked on a fold marker for the folding range at this index.
    ToggleFold(usize),
}

/// Render one gutter row for `line_idx` (0-based).
///
/// The caller must have already started a horizontal layout group for the row;
/// this function allocates exactly `GUTTER_WIDTH` pixels wide.
///
/// Returns a `GutterAction` indicating what interaction occurred on this row.
pub fn show_line(
    ui: &mut egui::Ui,
    tab: &EditorTab,
    state: &UiState,
    line_idx: usize,
) -> GutterAction {
    let line_no = line_idx + 1; // 1-based display
    let is_active = tab.cursors.primary().pos().line as usize == line_idx;

    // ── Colors ────────────────────────────────────────────────────────────────
    let line_no_color = if is_active {
        cfg_to_egui(
            state
                .theme
                .ui_or("editorLineNumber.activeForeground", c(0xc6, 0xc6, 0xc6)),
        )
    } else {
        cfg_to_egui(
            state
                .theme
                .ui_or("editorLineNumber.foreground", c(0x85, 0x85, 0x85)),
        )
    };
    let gutter_bg = cfg_to_egui(
        state
            .theme
            .ui_or("editorGutter.background", c(0x1e, 0x1e, 0x1e)),
    );

    // ── Diagnostic severity for this line ─────────────────────────────────────
    let diag_marker = worst_diagnostic_on_line(&tab.diagnostics, line_idx);

    // ── Git hunk marker for this line ─────────────────────────────────────────
    let git_marker = git_marker_on_line(&tab.git_hunks, line_idx);

    // ── Breakpoint state for this line ────────────────────────────────────────
    let line_u32 = line_idx as u32;
    let has_bp = tab.breakpoints.contains(&line_u32);
    let bp_verified = has_bp && breakpoint_verified(state, tab, line_u32);

    // ── Folding state for this line ───────────────────────────────────────────
    let (fold_marker, fold_idx, is_collapsed) =
        fold_info_on_line(&tab.folding_ranges, &tab.collapsed_folds, line_idx);

    // ── Layout ────────────────────────────────────────────────────────────────
    // Claim GUTTER_WIDTH px; fill with gutter background.
    let (gutter_rect, gutter_resp) = ui.allocate_exact_size(
        egui::vec2(GUTTER_WIDTH, ui.available_height()),
        egui::Sense::click(),
    );
    ui.painter().rect_filled(gutter_rect, 0.0, gutter_bg);

    // ── Breakpoint circle (left edge, 10px wide zone) ─────────────────────────
    let bp_zone =
        egui::Rect::from_min_size(gutter_rect.min, egui::vec2(16.0, gutter_rect.height()));
    let bp_zone_resp = ui.interact(bp_zone, gutter_resp.id.with("bp"), egui::Sense::click());

    // Render the breakpoint indicator.
    let bp_center = egui::pos2(gutter_rect.left() + 8.0, gutter_rect.center().y);
    let bp_radius = 4.5_f32;

    if has_bp {
        if bp_verified {
            ui.painter().circle_filled(
                bp_center,
                bp_radius,
                egui::Color32::from_rgb(0xe5, 0x1b, 0x1b),
            );
        } else {
            ui.painter().circle_stroke(
                bp_center,
                bp_radius,
                egui::Stroke::new(1.5, egui::Color32::from_rgb(0x94, 0x44, 0x44)),
            );
        }
    } else if gutter_resp.hovered() {
        ui.painter().circle_stroke(
            bp_center,
            bp_radius,
            egui::Stroke::new(
                1.0,
                egui::Color32::from_rgba_unmultiplied(0xe5, 0x1b, 0x1b, 0x55),
            ),
        );
    }

    // ── Fold marker ───────────────────────────────────────────────────────────
    // If this line starts a folding range, render ▶ (collapsed) or ▼ (expanded).
    // The marker sits just to the right of the breakpoint zone.
    if let Some((marker, idx, collapsed)) =
        fold_marker.map(|m| (m, fold_idx.unwrap(), is_collapsed.unwrap()))
    {
        let fold_center = egui::pos2(gutter_rect.left() + 24.0, gutter_rect.center().y);
        let fold_zone = egui::Rect::from_min_size(
            egui::pos2(gutter_rect.left() + 18.0, gutter_rect.top()),
            egui::vec2(16.0, gutter_rect.height()),
        );
        let fold_resp = ui.interact(fold_zone, gutter_resp.id.with("fold"), egui::Sense::click());
        ui.painter().text(
            fold_center,
            egui::Align2::CENTER_CENTER,
            marker,
            egui::FontId::proportional(state.font_size - 2.0),
            egui::Color32::from_rgb(0xcc, 0xcc, 0xcc),
        );
        if fold_resp.clicked() {
            return GutterAction::ToggleFold(idx);
        }

        // Also add a faint line below the fold marker to hint at the folded region.
        if collapsed {
            // Draw a thin horizontal line from fold marker to the right edge.
            let line_y = gutter_rect.center().y;
            ui.painter().line_segment(
                [
                    egui::pos2(gutter_rect.left() + 24.0, line_y),
                    egui::pos2(gutter_rect.right() - 4.0, line_y),
                ],
                egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgba_unmultiplied(0x88, 0x88, 0x88, 0x60),
                ),
            );
        }
    }

    // ── Line number right-aligned in gutter ───────────────────────────────────
    let galley = ui.fonts_mut(|f| {
        f.layout_no_wrap(
            format!("{line_no}"),
            egui::FontId::monospace(state.font_size - 1.0),
            line_no_color,
        )
    });

    // Reserve 14 px on the right for the git/diagnostic strip + 8 for padding,
    // and 16 px on the left for the breakpoint zone + fold marker zone.
    let number_right_x = gutter_rect.right() - 14.0;
    let text_left = (number_right_x - galley.rect.width()).max(gutter_rect.left() + 32.0);
    let text_top = gutter_rect.center().y - galley.rect.height() / 2.0;
    ui.painter().galley(
        egui::pos2(text_left, text_top),
        galley,
        egui::Color32::WHITE,
    );

    // ── 4 px wide decoration strip on the right edge of the gutter ────────────
    let strip_x = gutter_rect.right() - 6.0;
    let strip_width = 4.0;
    let strip_rect = egui::Rect::from_min_size(
        egui::pos2(strip_x, gutter_rect.top()),
        egui::vec2(strip_width, gutter_rect.height()),
    );

    // Git marker takes the background; diagnostic icon takes the foreground.
    if let Some(git_color) = git_marker {
        ui.painter().rect_filled(strip_rect, 0.0, git_color);
    }

    if let Some((icon, icon_color)) = diag_icon(diag_marker) {
        let icon_galley = ui.fonts_mut(|f| {
            f.layout_no_wrap(
                icon.to_owned(),
                egui::FontId::proportional(state.font_size - 3.0),
                icon_color,
            )
        });
        let icon_pos = egui::pos2(
            strip_x - icon_galley.rect.width() - 1.0,
            gutter_rect.center().y - icon_galley.rect.height() / 2.0,
        );
        ui.painter()
            .galley(icon_pos, icon_galley, egui::Color32::WHITE);
    }

    // ── Current execution line marker (yellow arrow) ───────────────────────────
    if is_paused_at_line(state, tab, line_u32) {
        let arrow_x = text_left - 10.0;
        ui.painter().text(
            egui::pos2(arrow_x, gutter_rect.center().y),
            egui::Align2::CENTER_CENTER,
            "→",
            egui::FontId::proportional(state.font_size),
            egui::Color32::from_rgb(0xff, 0xd7, 0x00),
        );
    }

    if bp_zone_resp.clicked() {
        GutterAction::ToggleBreakpoint
    } else {
        GutterAction::None
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Return the worst (most severe) diagnostic severity that touches `line_idx`,
/// or `None` if no diagnostic is present on that line.
fn worst_diagnostic_on_line(
    diagnostics: &[Diagnostic],
    line_idx: usize,
) -> Option<DiagnosticSeverity> {
    let line = line_idx as u32;
    diagnostics
        .iter()
        .filter(|d| d.range.start.line <= line && d.range.end.line >= line)
        .map(|d| d.severity)
        .min() // DiagnosticSeverity is ordered: Error(1) < Warning(2) < …
}

/// Return the icon character and color for a diagnostic severity, or `None`.
fn diag_icon(sev: Option<DiagnosticSeverity>) -> Option<(&'static str, egui::Color32)> {
    match sev? {
        DiagnosticSeverity::Error => Some(("E", egui::Color32::from_rgb(0xf4, 0x47, 0x47))),
        DiagnosticSeverity::Warning => Some(("W", egui::Color32::from_rgb(0xcc, 0xa7, 0x00))),
        DiagnosticSeverity::Information => Some(("I", egui::Color32::from_rgb(0x75, 0xbe, 0xff))),
        DiagnosticSeverity::Hint => Some((".", egui::Color32::from_rgb(0xaa, 0xaa, 0xaa))),
    }
}

/// Return the gutter color for a git hunk that covers `line_idx`, or `None`.
fn git_marker_on_line(hunks: &[DiffHunk], line_idx: usize) -> Option<egui::Color32> {
    let line = line_idx as u32 + 1; // hunks use 1-based line numbers
    hunks
        .iter()
        .find(|h| h.new_start <= line && line < h.new_start + h.new_lines.max(1))
        .map(|h| match h.kind {
            HunkKind::Added => egui::Color32::from_rgb(0x58, 0x7c, 0x0c),
            HunkKind::Modified => egui::Color32::from_rgb(0x0c, 0x7d, 0x9d),
            HunkKind::Removed => egui::Color32::from_rgb(0x94, 0x15, 0x1b),
        })
}

/// Return fold marker info for a line: the marker character (if any), the index
/// into `folding_ranges`, and whether it's collapsed.
fn fold_info_on_line(
    folding_ranges: &[FoldingRange],
    collapsed_folds: &[usize],
    line_idx: usize,
) -> (Option<&'static str>, Option<usize>, Option<bool>) {
    for (i, fr) in folding_ranges.iter().enumerate() {
        if fr.start_line as usize == line_idx {
            let collapsed = collapsed_folds.contains(&i);
            let marker = if collapsed { "▶" } else { "▼" };
            return (Some(marker), Some(i), Some(collapsed));
        }
    }
    (None, None, None)
}

/// Returns `true` if the DAP session has a verified breakpoint on this line.
fn breakpoint_verified(state: &UiState, tab: &EditorTab, line: u32) -> bool {
    // Check DAP panel breakpoint states.
    if !state.dap_panel.session_active {
        // Outside a session: treat all user-set breakpoints as "set" (show as red).
        return true;
    }
    let path = tab.uri.as_url().to_file_path().ok();
    state.dap_panel.breakpoint_states.iter().any(|bp| {
        bp.verified
            && bp.line.map(|l| l == line + 1).unwrap_or(false) // DAP uses 1-based lines
            && bp.source_path.as_ref().map(|p| Some(p) == path.as_ref()).unwrap_or(true)
    })
}

/// Returns `true` if the debugger is currently paused at this line in this file.
fn is_paused_at_line(state: &UiState, tab: &EditorTab, line: u32) -> bool {
    let dap = &state.dap_panel;
    if !dap.session_active || !dap.paused {
        return false;
    }
    dap.call_stack
        .first()
        .map(|f| {
            f.line == line + 1  // DAP 1-based
            && f.source_path.as_ref().map(|p| {
                tab.uri.as_url().to_file_path().ok().as_ref() == Some(p)
            }).unwrap_or(false)
        })
        .unwrap_or(false)
}

fn c(r: u8, g: u8, b: u8) -> crabide_config::Color {
    crabide_config::Color::rgb(r, g, b)
}
