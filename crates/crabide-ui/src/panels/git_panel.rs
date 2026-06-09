//! Source Control (git) panel.
//!
//! Rendered as a resizable bottom strip when `UiState.git_panel.visible` is
//! `true`.  All user interactions set pending flags on `GitPanelState`; the
//! app crate drains those flags each frame and forwards them to `GitService`.

use crabide_core::event::{FileStatus, StatusKind};

use crate::state::{cfg_to_egui, UiState};

/// Render the git / source-control panel.
pub fn show(ui: &mut egui::Ui, state: &mut UiState) {
    let bg = cfg_to_egui(state.theme.ui_or(
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
    let fg = cfg_to_egui(state.theme.ui_or(
        "sideBar.foreground",
        crabide_config::Color::rgb(0xcc, 0xcc, 0xcc),
    ));
    let accent = cfg_to_egui(state.theme.ui_or(
        "button.background",
        crabide_config::Color::rgb(0x00, 0x7a, 0xcc),
    ));
    let input_bg = cfg_to_egui(state.theme.ui_or(
        "input.background",
        crabide_config::Color::rgb(0x3c, 0x3c, 0x3c),
    ));
    let input_fg = cfg_to_egui(state.theme.ui_or(
        "input.foreground",
        crabide_config::Color::rgb(0xcc, 0xcc, 0xcc),
    ));
    let dim_fg = cfg_to_egui(state.theme.ui_or(
        "input.placeholderForeground",
        crabide_config::Color::rgb(0x88, 0x88, 0x88),
    ));
    let btn_bg = cfg_to_egui(state.theme.ui_or(
        "button.secondaryBackground",
        crabide_config::Color::rgb(0x3c, 0x3c, 0x3c),
    ));

    ui.painter()
        .rect_filled(ui.available_rect_before_wrap(), 0.0, bg);

    // ── Header ────────────────────────────────────────────────────────────────
    egui::Frame::NONE
        .fill(header_bg)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("SOURCE CONTROL")
                        .small()
                        .strong()
                        .color(header_fg),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(branch) = &state.git_branch {
                        ui.label(
                            egui::RichText::new(format!("⎇ {branch}"))
                                .small()
                                .color(egui::Color32::from_rgb(0x73, 0xc9, 0x91)),
                        );
                    }
                });
            });
        });

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 1.0);

            // ── Commit message ────────────────────────────────────────────────
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                egui::Frame::default()
                    .fill(input_bg)
                    .inner_margin(egui::Margin::symmetric(6, 4))
                    .corner_radius(egui::CornerRadius::same(2))
                    .show(ui, |ui| {
                        let avail = ui.available_width() - 8.0;
                        ui.set_width(avail);
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut state.git_panel.commit_message)
                                .font(egui::TextStyle::Monospace)
                                .text_color(input_fg)
                                .frame(egui::Frame::NONE)
                                .desired_width(f32::INFINITY)
                                .hint_text(
                                    egui::RichText::new("Message (Ctrl+Enter to commit)")
                                        .color(dim_fg),
                                ),
                        );
                        if resp.has_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.ctrl)
                        {
                            state.git_panel.pending_commit = true;
                        }
                    });
                ui.add_space(8.0);
            });

            // ── Action buttons ────────────────────────────────────────────────
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                let commit_enabled = !state.git_panel.commit_message.is_empty();
                let commit_fill = if commit_enabled { accent } else { btn_bg };
                let commit_btn = egui::Button::new(
                    egui::RichText::new("Commit")
                        .size(12.0)
                        .color(egui::Color32::WHITE),
                )
                .fill(commit_fill);
                if ui.add(commit_btn).clicked() && commit_enabled {
                    state.git_panel.pending_commit = true;
                }

                ui.add_space(4.0);
                let stage_all_btn =
                    egui::Button::new(egui::RichText::new("➕ Stage All").size(11.0).color(fg))
                        .fill(btn_bg);
                if ui
                    .add(stage_all_btn)
                    .on_hover_text("Stage all changes")
                    .clicked()
                {
                    state.git_panel.pending_stage_all = true;
                }

                ui.add_space(4.0);
                let unstage_btn =
                    egui::Button::new(egui::RichText::new("➖ Unstage All").size(11.0).color(fg))
                        .fill(btn_bg);
                if ui
                    .add(unstage_btn)
                    .on_hover_text("Unstage all changes")
                    .clicked()
                {
                    state.git_panel.pending_unstage_all = true;
                }
            });

            ui.add_space(6.0);

            // ── Staged Changes ────────────────────────────────────────────────
            let staged_count = state.git_panel.staged_files.len();
            section_header(ui, header_bg, header_fg, "Staged Changes", staged_count);

            let staged: Vec<FileStatus> = state.git_panel.staged_files.clone();
            for fs in &staged {
                let action = file_row(ui, fs, true, fg);
                match action {
                    FileAction::Unstage => {
                        state.git_panel.pending_unstage_file = Some(fs.path.clone());
                    }
                    FileAction::Discard => {
                        state.git_panel.pending_discard_file = Some(fs.path.clone());
                    }
                    FileAction::None => {}
                    FileAction::Stage => {}
                }
            }

            // ── Changes (unstaged) ────────────────────────────────────────────
            ui.add_space(2.0);
            let unstaged_count = state.git_panel.unstaged_files.len();
            section_header(ui, header_bg, header_fg, "Changes", unstaged_count);

            let unstaged: Vec<FileStatus> = state.git_panel.unstaged_files.clone();
            for fs in &unstaged {
                let action = file_row(ui, fs, false, fg);
                match action {
                    FileAction::Stage => {
                        state.git_panel.pending_stage_file = Some(fs.path.clone());
                    }
                    FileAction::Discard => {
                        state.git_panel.pending_discard_file = Some(fs.path.clone());
                    }
                    FileAction::None => {}
                    FileAction::Unstage => {}
                }
            }

            if staged.is_empty() && unstaged.is_empty() {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new("✅ No changes — working tree clean")
                            .size(12.0)
                            .color(dim_fg),
                    );
                });
            }

            // ── Submodules ──────────────────────────────────────────────────
            let submodule_count = state.git_panel.submodules.len();
            if submodule_count > 0 {
                ui.add_space(2.0);
                section_header(ui, header_bg, header_fg, "Submodules", submodule_count);

                let sms: Vec<crabide_core::event::SubmoduleInfo> =
                    state.git_panel.submodules.clone();
                for sm in &sms {
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);

                        // Status icon
                        let icon = if sm.cloned {
                            if sm.has_changes {
                                "✱"
                            } else {
                                "✓"
                            }
                        } else if sm.initialized {
                            "◌"
                        } else {
                            "○"
                        };
                        let icon_color = if sm.has_changes {
                            egui::Color32::from_rgb(0xe4, 0x43, 0x43)
                        } else if sm.cloned {
                            egui::Color32::from_rgb(0x73, 0xc9, 0x91)
                        } else {
                            dim_fg
                        };
                        ui.label(egui::RichText::new(icon).size(11.0).color(icon_color));
                        ui.add_space(4.0);

                        // Submodule path
                        ui.label(egui::RichText::new(&sm.path).size(12.0).color(fg));

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(8.0);
                            // Short commit hash
                            let short = if sm.commit.len() >= 7 {
                                &sm.commit[..7]
                            } else {
                                &sm.commit
                            };
                            ui.label(egui::RichText::new(short).size(10.0).color(dim_fg));
                        });
                    });
                }
                ui.add_space(4.0);
            }

            ui.add_space(8.0);
        });
}

// ── Internal action type ──────────────────────────────────────────────────────

enum FileAction {
    None,
    Stage,
    Unstage,
    Discard,
}

// ── Section header ────────────────────────────────────────────────────────────

fn section_header(
    ui: &mut egui::Ui,
    bg: egui::Color32,
    fg: egui::Color32,
    label: &str,
    count: usize,
) {
    egui::Frame::NONE
        .fill(bg)
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new(format!("{label} ({count})"))
                    .small()
                    .strong()
                    .color(fg),
            );
        });
}

// ── File row ──────────────────────────────────────────────────────────────────

fn file_row(
    ui: &mut egui::Ui,
    status: &FileStatus,
    is_staged: bool,
    fg: egui::Color32,
) -> FileAction {
    let file_name = status
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?");

    let (status_char, status_color) = if is_staged {
        (
            kind_char(status.index_status),
            kind_color(status.index_status),
        )
    } else {
        (
            kind_char(status.worktree_status),
            kind_color(status.worktree_status),
        )
    };

    let mut action = FileAction::None;

    ui.horizontal(|ui| {
        ui.add_space(12.0);

        // Status badge
        ui.label(
            egui::RichText::new(status_char)
                .monospace()
                .size(11.0)
                .color(status_color),
        );
        ui.add_space(4.0);

        // File name
        ui.label(egui::RichText::new(file_name).size(12.0).color(fg));

        // Buttons flush-right
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);

            // Discard button
            let disc = egui::Button::new(
                egui::RichText::new("🗑")
                    .size(13.0)
                    .color(egui::Color32::from_rgb(0x99, 0x44, 0x44)),
            )
            .frame(false);
            if ui.add(disc).on_hover_text("Discard changes").clicked() {
                action = FileAction::Discard;
            }
            ui.add_space(4.0);

            // Stage / Unstage toggle
            if is_staged {
                let btn = egui::Button::new(
                    egui::RichText::new("➖")
                        .size(13.0)
                        .color(egui::Color32::from_rgb(0x99, 0x99, 0x99)),
                )
                .frame(false);
                if ui.add(btn).on_hover_text("Unstage file").clicked() {
                    action = FileAction::Unstage;
                }
            } else {
                let btn = egui::Button::new(
                    egui::RichText::new("➕")
                        .size(13.0)
                        .color(egui::Color32::from_rgb(0x58, 0x7c, 0x0c)),
                )
                .frame(false);
                if ui.add(btn).on_hover_text("Stage file").clicked() {
                    action = FileAction::Stage;
                }
            }
        });
    });

    action
}

// ── Status kind helpers ───────────────────────────────────────────────────────

fn kind_char(kind: StatusKind) -> &'static str {
    match kind {
        StatusKind::Added => "A",
        StatusKind::Modified => "M",
        StatusKind::Deleted => "D",
        StatusKind::Renamed => "R",
        StatusKind::Copied => "C",
        StatusKind::Untracked => "U",
        StatusKind::Conflicted => "!",
        StatusKind::Unmodified => " ",
        StatusKind::Ignored => "I",
        _ => "?",
    }
}

fn kind_color(kind: StatusKind) -> egui::Color32 {
    match kind {
        StatusKind::Added => egui::Color32::from_rgb(0x58, 0x7c, 0x0c),
        StatusKind::Modified => egui::Color32::from_rgb(0x0c, 0x7d, 0x9d),
        StatusKind::Deleted => egui::Color32::from_rgb(0x94, 0x15, 0x1b),
        StatusKind::Renamed => egui::Color32::from_rgb(0x0c, 0x7d, 0x9d),
        StatusKind::Copied => egui::Color32::from_rgb(0x58, 0x7c, 0x0c),
        StatusKind::Untracked => egui::Color32::from_rgb(0x73, 0xc9, 0x91),
        StatusKind::Conflicted => egui::Color32::from_rgb(0xe4, 0x43, 0x43),
        _ => egui::Color32::from_rgb(0x88, 0x88, 0x88),
    }
}
