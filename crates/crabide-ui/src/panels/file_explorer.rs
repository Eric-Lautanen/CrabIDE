//! File-explorer sidebar: collapsible directory tree with git decorations.
//!
//! The tree is pre-populated by the app (via `UiState.file_explorer`).
//! Rendering is pure egui; no async I/O is performed here.
//!
//! Returns `Some(PathBuf)` when the user clicks a file, so the app can open
//! it in the workspace and add a new editor tab.

use std::path::PathBuf;

use crate::state::{cfg_to_egui, FileNode, GitDecoration, UiState};

/// Render the file-explorer sidebar.
///
/// Returns the path of a file the user wants to open, or `None`.
pub fn show(ui: &mut egui::Ui, state: &mut UiState) -> Option<PathBuf> {
    let sidebar_bg = cfg_to_egui(state.theme.ui_or(
        "sideBar.background",
        crabide_config::Color::rgb(0x25, 0x25, 0x26),
    ));
    let header_bg = cfg_to_egui(state.theme.ui_or(
        "sideBarSectionHeader.background",
        crabide_config::Color::rgb(0x2d, 0x2d, 0x2d),
    ));
    let header_fg = cfg_to_egui(state.theme.ui_or(
        "sideBarSectionHeader.foreground",
        crabide_config::Color::rgb(0xbb, 0xbb, 0xbb),
    ));
    let fg_color = cfg_to_egui(state.theme.ui_or(
        "sideBar.foreground",
        crabide_config::Color::rgb(0xcc, 0xcc, 0xcc),
    ));
    let hover_bg = cfg_to_egui(state.theme.ui_or(
        "list.hoverBackground",
        crabide_config::Color::rgba(0x2a, 0x2d, 0x2e, 0xff),
    ));
    let sel_bg = cfg_to_egui(state.theme.ui_or(
        "list.activeSelectionBackground",
        crabide_config::Color::rgb(0x09, 0x47, 0x71),
    ));
    let sel_fg = cfg_to_egui(state.theme.ui_or(
        "list.activeSelectionForeground",
        crabide_config::Color::rgb(0xff, 0xff, 0xff),
    ));

    // Background is already filled by the sidebar layout pane renderer.
    // The "EXPLORER" section header is replaced by the sidebar tab strip
    // drawn in layout.rs — we only need the section title for the roots.
    let _ = (sidebar_bg, header_bg, header_fg);

    let mut open_request: Option<PathBuf> = None;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            let n = state.file_explorer.roots.len();
            for i in 0..n {
                let result = show_node(
                    ui,
                    &mut state.file_explorer.roots[i],
                    0,
                    fg_color,
                    hover_bg,
                    sel_bg,
                    sel_fg,
                );
                if open_request.is_none() {
                    open_request = result;
                }
            }
        });

    open_request
}

// ── Recursive node renderer ───────────────────────────────────────────────────

/// Render one `FileNode` and, if it is an expanded directory, all of its
/// descendants recursively.  Returns the path to open if the user clicked a
/// file, or a directory that needs its children loaded.
fn show_node(
    ui: &mut egui::Ui,
    node: &mut FileNode,
    depth: usize,
    fg_color: egui::Color32,
    hover_bg: egui::Color32,
    sel_bg: egui::Color32,
    sel_fg: egui::Color32,
) -> Option<PathBuf> {
    let indent_px = depth as f32 * 14.0 + 6.0;
    let w = ui.available_width();

    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 22.0), egui::Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);

    if ui.is_rect_visible(rect) {
        let bg = if resp.hovered() {
            hover_bg
        } else {
            egui::Color32::TRANSPARENT
        };
        if resp.hovered() {
            ui.painter().rect_filled(rect, 0.0, bg);
        }

        let text_color = if resp.hovered() {
            sel_fg
        } else {
            git_color(node.git_status, fg_color)
        };

        // Expand/collapse arrow for directories.
        if node.is_dir {
            let arrow = if node.expanded { "▾" } else { "▸" };
            ui.painter().text(
                egui::pos2(rect.left() + indent_px, rect.center().y),
                egui::Align2::LEFT_CENTER,
                arrow,
                egui::FontId::proportional(13.0),
                text_color.gamma_multiply(0.7),
            );
        }

        // Node name + git badge.
        let label = format!("{}{}", node.name, git_badge(node.git_status));
        let name_x = indent_px + 14.0;
        ui.painter().text(
            egui::pos2(rect.left() + name_x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            &label,
            egui::FontId::proportional(13.0),
            text_color,
        );

        // Thin left-edge accent for hovered row.
        if resp.hovered() && node.is_dir {
            let accent = sel_bg;
            ui.painter().rect_filled(
                egui::Rect::from_min_size(rect.min, egui::vec2(2.0, rect.height())),
                0.0,
                accent,
            );
        }
    }

    let mut result = None;
    if resp.clicked() {
        if node.is_dir {
            node.expanded = !node.expanded;
            // Always return path so the app can load children if needed.
            result = Some(node.path.clone());
        } else {
            result = Some(node.path.clone());
        }
    }

    // Recurse into children for expanded directories.
    if node.is_dir && node.expanded {
        for child in &mut node.children {
            if let Some(p) = show_node(ui, child, depth + 1, fg_color, hover_bg, sel_bg, sel_fg) {
                result.get_or_insert(p);
            }
        }
    }

    result
}

// ── Git decoration helpers ────────────────────────────────────────────────────

fn git_badge(status: Option<GitDecoration>) -> &'static str {
    match status {
        Some(GitDecoration::Modified) => " ●", // filled dot  — modified
        Some(GitDecoration::Added) => " ✚",    // heavy plus  — added / new
        Some(GitDecoration::Deleted) => " ✖",  // heavy cross — deleted
        Some(GitDecoration::Untracked) => " ◌", // dotted ring — untracked
        Some(GitDecoration::Conflicted) => " ⚡", // lightning   — conflict
        None => "",
    }
}

fn git_color(status: Option<GitDecoration>, default: egui::Color32) -> egui::Color32 {
    match status {
        Some(GitDecoration::Modified) => egui::Color32::from_rgb(0x0c, 0x7d, 0x9d),
        Some(GitDecoration::Added) => egui::Color32::from_rgb(0x58, 0x7c, 0x0c),
        Some(GitDecoration::Deleted) => egui::Color32::from_rgb(0x94, 0x15, 0x1b),
        Some(GitDecoration::Untracked) => egui::Color32::from_rgb(0x73, 0xc9, 0x91),
        Some(GitDecoration::Conflicted) => egui::Color32::from_rgb(0xe4, 0x43, 0x43),
        None => default,
    }
}
