//! Problems panel — bottom strip showing all LSP diagnostics.
//!
//! Groups diagnostics by open tab.  Each row is clickable and navigates to the
//! offending file and line.

use crabide_core::event::DiagnosticSeverity;
use crabide_core::types::Position;

use crate::state::{UiState, cfg_to_egui};

/// Minimum panel height in logical pixels.
pub const MIN_HEIGHT: f32 = 80.0;

// ── Colors (local aliases) ────────────────────────────────────────────────────

fn c(r: u8, g: u8, b: u8) -> crabide_config::Color {
    crabide_config::Color::rgb(r, g, b)
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Render the Problems panel inside the provided `ui`.
///
/// When the user clicks a diagnostic the function directly mutates `state` to
/// activate the correct tab and schedule a scroll (the same pattern used by
/// the workspace-search and git panels).
pub fn show(ui: &mut egui::Ui, state: &mut UiState) {
    let bg = cfg_to_egui(state.theme.ui_or("sideBar.background", c(0x25, 0x25, 0x26)));
    let fg = cfg_to_egui(state.theme.ui_or("editor.foreground", c(0xcc, 0xcc, 0xcc)));
    let muted = cfg_to_egui(
        state
            .theme
            .ui_or("tab.inactiveForeground", c(0x85, 0x85, 0x85)),
    );
    let hov_bg = cfg_to_egui(
        state
            .theme
            .ui_or("list.hoverBackground", c(0x2a, 0x2d, 0x2e)),
    );

    let error_color = egui::Color32::from_rgb(0xf4, 0x43, 0x36);
    let warn_color = egui::Color32::from_rgb(0xff, 0xb3, 0x00);
    let info_color = egui::Color32::from_rgb(0x29, 0xb6, 0xf6);
    let hint_color = egui::Color32::from_rgba_unmultiplied(0x80, 0x80, 0x80, 0xb0);

    ui.painter()
        .rect_filled(ui.available_rect_before_wrap(), 0.0, bg);

    // ── Count totals ──────────────────────────────────────────────────────────
    let total_errors: usize = state
        .tabs()
        .iter()
        .flat_map(|t| &t.diagnostics)
        .filter(|d| d.severity == DiagnosticSeverity::Error)
        .count();
    let total_warnings: usize = state
        .tabs()
        .iter()
        .flat_map(|t| &t.diagnostics)
        .filter(|d| d.severity == DiagnosticSeverity::Warning)
        .count();

    // ── Header ────────────────────────────────────────────────────────────────
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(
                    egui::RichText::new("PROBLEMS")
                        .color(muted)
                        .size(11.0)
                        .strong(),
                ));
                if total_errors > 0 {
                    ui.add(egui::Label::new(
                        egui::RichText::new(format!("  ✗ {total_errors}"))
                            .color(error_color)
                            .size(11.0),
                    ));
                }
                if total_warnings > 0 {
                    ui.add(egui::Label::new(
                        egui::RichText::new(format!("  ⚠ {total_warnings}"))
                            .color(warn_color)
                            .size(11.0),
                    ));
                }
                if total_errors == 0 && total_warnings == 0 {
                    ui.add(egui::Label::new(
                        egui::RichText::new("  No problems").color(muted).size(11.0),
                    ));
                }
            });
        });

    // ── Collect click targets (to avoid borrow-checker conflicts in loops) ────
    let mut navigate: Option<Navigate> = None;

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;

            // ── Per-tab diagnostics ───────────────────────────────────────────
            let has_diagnostics = state.tabs().iter().any(|t| !t.diagnostics.is_empty());
            if has_diagnostics {
                for (tab_idx, tab) in state.tabs().iter().enumerate() {
                    if tab.diagnostics.is_empty() {
                        continue;
                    }

                    // Tab section header.
                    let (file_errors, file_warnings) =
                        tab.diagnostics.iter().fold((0usize, 0usize), |(e, w), d| {
                            match d.severity {
                                DiagnosticSeverity::Error => (e + 1, w),
                                DiagnosticSeverity::Warning => (e, w + 1),
                                _ => (e, w),
                            }
                        });
                    let badge = format!(
                        "  ({})",
                        [
                            (file_errors > 0).then(|| format!("✗{file_errors}")),
                            (file_warnings > 0).then(|| format!("⚠{file_warnings}")),
                        ]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join("  ")
                    );

                    egui::Frame::NONE
                        .inner_margin(egui::Margin {
                            left: 10,
                            right: 0,
                            top: 4,
                            bottom: 2,
                        })
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.add(egui::Label::new(
                                    egui::RichText::new(&tab.title)
                                        .color(fg)
                                        .size(12.0)
                                        .strong(),
                                ));
                                ui.add(egui::Label::new(
                                    egui::RichText::new(&badge).color(muted).size(11.0),
                                ));
                            });
                        });

                    // Diagnostic rows.
                    for diag in &tab.diagnostics {
                        let (sev_icon, sev_color) = match diag.severity {
                            DiagnosticSeverity::Error => ("✗", error_color),
                            DiagnosticSeverity::Warning => ("⚠", warn_color),
                            DiagnosticSeverity::Information => ("ℹ", info_color),
                            DiagnosticSeverity::Hint => ("·", hint_color),
                            _ => ("·", hint_color),
                        };
                        let line = diag.range.start.line + 1; // 1-based
                        let col = diag.range.start.character + 1;
                        let source = diag.source.as_deref().unwrap_or("");
                        let msg_text = if source.is_empty() {
                            format!("{sev_icon}  {}  [Ln {line}, Col {col}]", diag.message)
                        } else {
                            format!(
                                "{sev_icon}  {}  [Ln {line}, Col {col}] ({source})",
                                diag.message
                            )
                        };

                        let row_id = egui::Id::new(("prob_diag", tab_idx, line, col));
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

                            // Severity icon coloured pill.
                            let icon_pos = egui::pos2(row_rect.left() + 24.0, row_rect.center().y);
                            ui.painter().text(
                                icon_pos,
                                egui::Align2::LEFT_CENTER,
                                sev_icon,
                                egui::FontId::proportional(12.0),
                                sev_color,
                            );
                            // Message text.
                            ui.painter().text(
                                egui::pos2(row_rect.left() + 40.0, row_rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                msg_text,
                                egui::FontId::proportional(12.0),
                                fg,
                            );
                        }
                        let _ = row_id; // suppress unused warning

                        if row_resp.clicked() {
                            navigate = Some(Navigate::Diagnostic {
                                tab_idx,
                                line: diag.range.start.line,
                                col: diag.range.start.character,
                            });
                        }
                        // Tooltip with full message.
                        row_resp.on_hover_text(&diag.message);
                    }
                }
            }
        });

    // ── Apply navigation (after borrow of tabs ends) ───────────────────────
    if let Some(Navigate::Diagnostic { tab_idx, line, col }) = navigate {
        state.active_group_mut().active_tab = Some(tab_idx);
        state.pending_scroll_line = Some(line as usize);
        if tab_idx < state.tabs().len() {
            state.tabs_mut()[tab_idx]
                .cursors
                .set_single(Position::new(line, col));
        }
    }
}

// ── Navigation target ─────────────────────────────────────────────────────────

enum Navigate {
    Diagnostic { tab_idx: usize, line: u32, col: u32 },
}
