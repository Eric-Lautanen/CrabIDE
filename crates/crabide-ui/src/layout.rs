//! Panel layout using `egui_tiles`.
//!
//! The editor shell is divided into named panes — sidebar and editor area.
//! The sidebar renders a compact tab strip at the top ([Files] [Extensions]) and
//! routes to the appropriate sub-panel.  The dockable tree is stored in `UiState`
//! and mutated via drag-and-drop or programmatic splits.

use egui_tiles::{SimplificationOptions, TileId, UiResponse};

use crate::panels;
use crate::state::{SidebarTab, UiState, cfg_to_egui};

// ── PaneKind ─────────────────────────────────────────────────────────────────

/// The identity of a renderable pane in the tile layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PaneKind {
    /// An editor group: tab bar + gutter + syntax-highlighted content.
    /// The `usize` is the index into `UiState.editor_groups`.
    EditorGroup(usize),
    /// The left sidebar: tab strip + file-system tree or extensions panel.
    FileExplorer,
}

// ── Default layout ────────────────────────────────────────────────────────────

/// Build the initial tile layout: Sidebar (20%) | Editor (80%).
pub fn default_layout() -> egui_tiles::Tree<PaneKind> {
    let mut tiles = egui_tiles::Tiles::default();

    let explorer_id = tiles.insert_pane(PaneKind::FileExplorer);
    let editor_id = tiles.insert_pane(PaneKind::EditorGroup(0));

    let mut linear = egui_tiles::Linear::new(
        egui_tiles::LinearDir::Horizontal,
        vec![explorer_id, editor_id],
    );
    linear.shares.set_share(explorer_id, 0.2);
    linear.shares.set_share(editor_id, 0.8);

    let root = tiles.insert_container(egui_tiles::Container::Linear(linear));

    egui_tiles::Tree::new("crabide_layout", root, tiles)
}

// ── Sidebar tab strip ─────────────────────────────────────────────────────────

/// Render the compact [Files] [Extensions] tab strip at the top of the sidebar.
///
/// Returns `true` if a repaint is needed (tab changed this frame).
fn show_sidebar_tabs(ui: &mut egui::Ui, state: &mut UiState) -> bool {
    let sidebar_bg = cfg_to_egui(state.theme.ui_or(
        "sideBar.background",
        crabide_config::Color::rgb(0x25, 0x25, 0x26),
    ));
    let tab_active_bg = cfg_to_egui(state.theme.ui_or(
        "tab.activeBackground",
        crabide_config::Color::rgb(0x1e, 0x1e, 0x1e),
    ));
    let tab_inactive_bg = cfg_to_egui(state.theme.ui_or(
        "tab.inactiveBackground",
        crabide_config::Color::rgb(0x2d, 0x2d, 0x2d),
    ));
    let tab_active_fg = cfg_to_egui(state.theme.ui_or(
        "tab.activeForeground",
        crabide_config::Color::rgb(0xff, 0xff, 0xff),
    ));
    let tab_inactive_fg = cfg_to_egui(state.theme.ui_or(
        "tab.inactiveForeground",
        crabide_config::Color::rgb(0x99, 0x99, 0x99),
    ));
    let accent = cfg_to_egui(state.theme.ui_or(
        "activityBarBadge.background",
        crabide_config::Color::rgb(0x00, 0x7a, 0xcc),
    ));

    let mut changed = false;

    let strip_height = 30.0;
    let (strip_rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), strip_height),
        egui::Sense::hover(),
    );

    ui.painter().rect_filled(strip_rect, 0.0, sidebar_bg);

    // Collect all tabs: built-ins first, then extension panes.
    let builtin_tabs: &[(SidebarTab, &str)] = &[
        (SidebarTab::Explorer, "Files"),
        (SidebarTab::Extensions, "Extensions"),
    ];
    let ext_pane_ids: Vec<(String, String)> = state
        .sidebar_panes
        .values()
        .map(|p| (p.registration.id.clone(), p.registration.icon.clone()))
        .collect();

    let total = builtin_tabs.len() + ext_pane_ids.len();
    let tab_w = if total > 0 {
        strip_rect.width() / total as f32
    } else {
        strip_rect.width()
    };

    // Built-in tabs.
    for (i, (variant, label)) in builtin_tabs.iter().enumerate() {
        let tab_rect = egui::Rect::from_min_size(
            strip_rect.min + egui::vec2(i as f32 * tab_w, 0.0),
            egui::vec2(tab_w, strip_height),
        );
        let is_active = state.sidebar_tab == *variant;
        let tab_resp = ui.allocate_rect(tab_rect, egui::Sense::click());
        let bg = if is_active {
            tab_active_bg
        } else {
            tab_inactive_bg
        };
        ui.painter().rect_filled(tab_rect, 0.0, bg);
        if is_active {
            let bar = egui::Rect::from_min_size(
                egui::pos2(tab_rect.min.x, tab_rect.max.y - 2.0),
                egui::vec2(tab_rect.width(), 2.0),
            );
            ui.painter().rect_filled(bar, 0.0, accent);
        }
        let fg = if is_active {
            tab_active_fg
        } else {
            tab_inactive_fg
        };
        ui.painter().text(
            tab_rect.center(),
            egui::Align2::CENTER_CENTER,
            *label,
            egui::FontId::proportional(11.5),
            fg,
        );
        if tab_resp.clicked() && !is_active {
            state.sidebar_tab = variant.clone();
            changed = true;
        }
    }

    // Extension pane tabs.
    for (rel_i, (pane_id, icon)) in ext_pane_ids.iter().enumerate() {
        let i = builtin_tabs.len() + rel_i;
        let tab_rect = egui::Rect::from_min_size(
            strip_rect.min + egui::vec2(i as f32 * tab_w, 0.0),
            egui::vec2(tab_w, strip_height),
        );
        let this_tab = SidebarTab::ExtensionPane(pane_id.clone());
        let is_active = state.sidebar_tab == this_tab;
        let tab_resp = ui.allocate_rect(tab_rect, egui::Sense::click());
        let bg = if is_active {
            tab_active_bg
        } else {
            tab_inactive_bg
        };
        ui.painter().rect_filled(tab_rect, 0.0, bg);
        if is_active {
            let bar = egui::Rect::from_min_size(
                egui::pos2(tab_rect.min.x, tab_rect.max.y - 2.0),
                egui::vec2(tab_rect.width(), 2.0),
            );
            ui.painter().rect_filled(bar, 0.0, accent);
        }
        let fg = if is_active {
            tab_active_fg
        } else {
            tab_inactive_fg
        };
        // Show icon in the tab.
        ui.painter().text(
            tab_rect.center(),
            egui::Align2::CENTER_CENTER,
            icon.as_str(),
            egui::FontId::proportional(14.0),
            fg,
        );
        if tab_resp.clicked() && !is_active {
            state.sidebar_tab = this_tab;
            changed = true;
        }
    }

    changed
}

// ── UiBehavior ────────────────────────────────────────────────────────────────

/// Drives rendering of each pane inside the egui_tiles layout.
///
/// Holds a mutable borrow of `UiState` so every panel can read/write
/// shared state (active tab, scroll offset, palette visibility, etc.)
/// and a `Vec<crabide_config::Action>` that accumulates backend actions
/// for the app to handle after the frame.
pub struct UiBehavior<'a> {
    pub state: &'a mut UiState,
    pub actions: &'a mut Vec<crabide_config::Action>,
}

impl<'a> egui_tiles::Behavior<PaneKind> for UiBehavior<'a> {
    fn tab_title_for_pane(&mut self, pane: &PaneKind) -> egui::WidgetText {
        match pane {
            PaneKind::EditorGroup(idx) => {
                let group = &self.state.editor_groups[*idx];
                let label = group
                    .active_tab_ref()
                    .map(|t| t.title.as_str())
                    .unwrap_or("Editor");
                label.into()
            }
            PaneKind::FileExplorer => "Sidebar".into(),
        }
    }

    fn pane_ui(&mut self, ui: &mut egui::Ui, _tile_id: TileId, pane: &mut PaneKind) -> UiResponse {
        match pane {
            PaneKind::EditorGroup(idx) => {
                // Render the editor for this specific group.
                panels::editor::show_for_group(*idx, ui, self.state, self.actions);
            }
            PaneKind::FileExplorer => {
                // Fill sidebar background before rendering anything.
                let bg = cfg_to_egui(self.state.theme.ui_or(
                    "sideBar.background",
                    crabide_config::Color::rgb(0x25, 0x25, 0x26),
                ));
                ui.painter()
                    .rect_filled(ui.available_rect_before_wrap(), 0.0, bg);

                // Tab strip at the top — [Files] [Extensions].
                show_sidebar_tabs(ui, self.state);

                // Route content area to the active sidebar sub-panel.
                match self.state.sidebar_tab.clone() {
                    SidebarTab::Explorer => {
                        if let Some(path) = panels::file_explorer::show(ui, self.state) {
                            self.actions.push(crabide_config::Action::OpenFile);
                            self.state.pending_open_path = Some(path);
                        }
                    }
                    SidebarTab::Extensions => {
                        panels::extensions_panel::show(ui, self.state);
                    }
                    SidebarTab::ExtensionPane(ref pane_id) => {
                        let pane_id = pane_id.clone();
                        if let Some(pane) = self.state.sidebar_panes.get(&pane_id) {
                            let content = pane.content.clone();
                            let title = pane.registration.title.clone();
                            crate::render_extension_panel(
                                ui, self.state, &title, &pane_id, &content,
                            );
                        }
                    }
                }
            }
        }
        UiResponse::None
    }

    fn simplification_options(&self) -> SimplificationOptions {
        SimplificationOptions {
            prune_empty_tabs: true,
            prune_empty_containers: true,
            prune_single_child_tabs: true,
            prune_single_child_containers: false,
            all_panes_must_have_tabs: false,
            join_nested_linear_containers: true,
        }
    }
}
