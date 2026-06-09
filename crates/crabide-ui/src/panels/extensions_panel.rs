//! Extensions Manager panel — installed extensions, marketplace search, and
//! extension output views (todo items, markdown preview, status-bar slots).
//!
//! Layout (inside the sidebar below the [Files][Extensions] tab strip):
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │ EXTENSIONS              [Load from file] │
//! ├─────────────────────────────────────────┤
//! │  [Installed ●]  [Search]  [Recommended] │
//! ├─────────────────────────────────────────┤
//! │  (content for active tab)               │
//! │  …                                      │
//! │  ── TODO Items (n) ───────────────────  │
//! │  ── Markdown Preview ─────────────────  │
//! └─────────────────────────────────────────┘
//! ```

use egui::{Align, Color32, FontId, Frame, Layout, Margin, RichText, ScrollArea, Sense, Vec2};

use crabide_extensions::{InstalledExtension, RegistryExtension};

use crate::state::{ExtensionsPanelTab, UiState, cfg_to_egui};

// ── Theme colour helpers ───────────────────────────────────────────────────────

struct Colors {
    sidebar_bg: Color32,
    fg: Color32,
    muted: Color32,
    accent: Color32,
    header_bg: Color32,
    card_bg: Color32,
    sel_bg: Color32,
}

impl Colors {
    fn from_state(state: &UiState) -> Self {
        Self {
            sidebar_bg: cfg_to_egui(state.theme.ui_or(
                "sideBar.background",
                crabide_config::Color::rgb(0x25, 0x25, 0x26),
            )),
            fg: cfg_to_egui(state.theme.ui_or(
                "sideBar.foreground",
                crabide_config::Color::rgb(0xcc, 0xcc, 0xcc),
            )),
            muted: cfg_to_egui(state.theme.ui_or(
                "descriptionForeground",
                crabide_config::Color::rgb(0x88, 0x88, 0x88),
            )),
            accent: cfg_to_egui(state.theme.ui_or(
                "activityBarBadge.background",
                crabide_config::Color::rgb(0x00, 0x7a, 0xcc),
            )),
            header_bg: cfg_to_egui(state.theme.ui_or(
                "sideBarSectionHeader.background",
                crabide_config::Color::rgb(0x2d, 0x2d, 0x2d),
            )),
            card_bg: cfg_to_egui(state.theme.ui_or(
                "editor.background",
                crabide_config::Color::rgb(0x1e, 0x1e, 0x1e),
            )),
            sel_bg: cfg_to_egui(state.theme.ui_or(
                "list.activeSelectionBackground",
                crabide_config::Color::rgb(0x09, 0x47, 0x71),
            )),
        }
    }
}

fn warn_col() -> Color32 {
    Color32::from_rgb(0xff, 0xcc, 0x00)
}
fn err_col() -> Color32 {
    Color32::from_rgb(0xf4, 0x43, 0x36)
}
fn ok_col() -> Color32 {
    Color32::from_rgb(0x4e, 0xc9, 0xb0)
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Render the Extensions manager panel inside the sidebar.
pub fn show(ui: &mut egui::Ui, state: &mut UiState) {
    let c = Colors::from_state(state);

    // ── Panel header ──────────────────────────────────────────────────────────
    Frame::NONE
        .fill(c.header_bg)
        .inner_margin(Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("EXTENSIONS").small().strong().color(c.fg));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let btn =
                        egui::Button::new(RichText::new("⊕ Load file").small().color(c.accent))
                            .frame(false);
                    if ui
                        .add(btn)
                        .on_hover_text("Install a .wasm extension from a local file")
                        .clicked()
                    {
                        state.extensions_panel.pending_install_local = true;
                    }
                });
            });
        });

    // ── Sub-tab strip ─────────────────────────────────────────────────────────
    show_subtab_strip(ui, state, c.accent, c.fg, c.muted);

    // ── Content ───────────────────────────────────────────────────────────────
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 4.0);
            match state.extensions_panel.active_tab {
                ExtensionsPanelTab::Installed => show_installed_tab(ui, state, &c),
                ExtensionsPanelTab::Search => show_search_tab(ui, state, &c),
                ExtensionsPanelTab::Recommended => show_recommended_tab(ui, state, &c),
            }
        });
}

// ── Sub-tab strip ─────────────────────────────────────────────────────────────

fn show_subtab_strip(
    ui: &mut egui::Ui,
    state: &mut UiState,
    acc: Color32,
    fgc: Color32,
    mtd: Color32,
) {
    ui.horizontal(|ui| {
        ui.set_height(28.0);
        ui.spacing_mut().item_spacing = Vec2::ZERO;

        let tabs: &[(ExtensionsPanelTab, &str)] = &[
            (ExtensionsPanelTab::Installed, "Installed"),
            (ExtensionsPanelTab::Search, "Search"),
            (ExtensionsPanelTab::Recommended, "Recommended"),
        ];

        for (variant, label) in tabs {
            let is_active = state.extensions_panel.active_tab == *variant;
            let text = RichText::new(*label)
                .size(11.0)
                .color(if is_active { fgc } else { mtd });

            let resp = ui.add(
                egui::Button::new(text)
                    .frame(false)
                    .min_size(Vec2::new(80.0, 28.0)),
            );

            if resp.clicked() {
                state.extensions_panel.active_tab = *variant;
                if *variant == ExtensionsPanelTab::Search {
                    state.extensions_panel.just_opened_search = true;
                }
            }

            if is_active {
                let r = resp.rect;
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(r.min.x, r.max.y - 2.0),
                        Vec2::new(r.width(), 2.0),
                    ),
                    0.0,
                    acc,
                );
            }
        }
    });
    ui.separator();
}

// ── Installed tab ─────────────────────────────────────────────────────────────

fn show_installed_tab(ui: &mut egui::Ui, state: &mut UiState, c: &Colors) {
    let exts = std::mem::take(&mut state.extensions_panel.installed);

    if exts.is_empty() {
        ui.add_space(16.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("No extensions installed.")
                    .color(c.muted)
                    .size(12.0),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new("Use Search to discover extensions.")
                    .color(c.muted)
                    .small(),
            );
        });
    } else {
        for ext in &exts {
            show_extension_card(ui, ext, state, c);
            ui.add_space(2.0);
        }
    }

    state.extensions_panel.installed = exts;

    // ── Extension status-bar items ────────────────────────────────────────────
    let status_items: Vec<(String, String, Option<String>)> = state
        .extensions_panel
        .status_bar_items
        .iter()
        .filter(|(_, item)| !item.text.is_empty())
        .map(|(id, item)| (id.clone(), item.text.clone(), item.tooltip.clone()))
        .collect();

    if !status_items.is_empty() {
        ui.add_space(8.0);
        show_section_header(ui, c.sidebar_bg, c.fg, "Extension Status");
        for (id, text, tip) in &status_items {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(RichText::new(text).size(11.0).color(ok_col()));
            })
            .response
            .on_hover_text(tip.as_deref().unwrap_or(id.as_str()));
        }
    }
}

// ── Extension card ────────────────────────────────────────────────────────────

fn show_extension_card(
    ui: &mut egui::Ui,
    ext: &InstalledExtension,
    state: &mut UiState,
    c: &Colors,
) {
    let is_selected = state.extensions_panel.selected_id.as_deref() == Some(&ext.manifest.id);
    let fill = if is_selected { c.sel_bg } else { c.card_bg };

    let resp = Frame::NONE
        .fill(fill)
        .corner_radius(4)
        .inner_margin(Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            // Header: category dot + name + version + toggle.
            ui.horizontal(|ui| {
                let cat_col = ext
                    .manifest
                    .categories
                    .first()
                    .map(|cat| {
                        let (r, g, b) = cat.color();
                        Color32::from_rgb(r, g, b)
                    })
                    .unwrap_or(Color32::from_gray(0x80));

                let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                ui.painter().circle_filled(dot_rect.center(), 5.0, cat_col);

                ui.label(
                    RichText::new(&ext.manifest.name)
                        .strong()
                        .size(12.5)
                        .color(c.fg),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (tog_label, tog_col) = if ext.enabled {
                        ("● On", ok_col())
                    } else {
                        ("○ Off", c.muted)
                    };
                    let tog_tip = if ext.enabled {
                        "Click to disable"
                    } else {
                        "Click to enable"
                    };
                    if ui
                        .add(
                            egui::Button::new(RichText::new(tog_label).size(10.5).color(tog_col))
                                .frame(false),
                        )
                        .on_hover_text(tog_tip)
                        .clicked()
                    {
                        state.extensions_panel.pending_toggle = Some(ext.manifest.id.clone());
                    }
                    ui.label(
                        RichText::new(format!("v{}", ext.manifest.version))
                            .size(10.0)
                            .color(c.muted),
                    );
                });
            });

            // Author.
            ui.label(
                RichText::new(&ext.manifest.author)
                    .size(10.5)
                    .color(c.muted),
            );

            // Description.
            let desc = if ext.manifest.description.len() > 80 {
                format!("{}…", &ext.manifest.description[..80])
            } else {
                ext.manifest.description.clone()
            };
            ui.label(RichText::new(desc).size(11.0).color(c.fg));

            // Category tags.
            ui.horizontal(|ui| {
                for cat in &ext.manifest.categories {
                    let (r, g, b) = cat.color();
                    ui.label(
                        RichText::new(cat.label())
                            .size(9.5)
                            .color(Color32::from_rgb(r, g, b)),
                    );
                }
            });

            // Action buttons.
            ui.horizontal(|ui| {
                if ext.manifest.is_builtin {
                    ui.label(RichText::new("Built-in").size(9.5).color(c.muted));
                } else if ui
                    .add(
                        egui::Button::new(RichText::new("Uninstall").size(10.5).color(err_col()))
                            .frame(false),
                    )
                    .clicked()
                {
                    state.extensions_panel.pending_uninstall = Some(ext.manifest.id.clone());
                }
            });
        })
        .response;

    if resp.clicked() {
        let id = ext.manifest.id.clone();
        if is_selected {
            state.extensions_panel.selected_id = None;
        } else {
            state.extensions_panel.selected_id = Some(id);
        }
    }
}

// ── Search tab ────────────────────────────────────────────────────────────────

fn show_search_tab(ui: &mut egui::Ui, state: &mut UiState, c: &Colors) {
    ui.add_space(6.0);

    // Search input row.
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        ui.label(RichText::new("🔍").size(12.0).color(c.muted));
        let resp = ui.add(
            egui::TextEdit::singleline(&mut state.extensions_panel.search_query)
                .hint_text("Search extensions…")
                .desired_width(ui.available_width() - 12.0)
                .font(FontId::proportional(12.0)),
        );
        if state.extensions_panel.just_opened_search {
            resp.request_focus();
            state.extensions_panel.just_opened_search = false;
        }
        if resp.changed() {
            let q = state.extensions_panel.search_query.clone();
            state.extensions_panel.pending_search = Some(q);
        }
    });

    ui.add_space(4.0);
    ui.separator();

    if state.extensions_panel.is_searching {
        ui.add_space(16.0);
        ui.vertical_centered(|ui| {
            ui.spinner();
            ui.label(RichText::new("Searching…").color(c.muted).size(11.0));
        });
        return;
    }

    let results = std::mem::take(&mut state.extensions_panel.search_results);

    if results.is_empty() {
        ui.add_space(16.0);
        ui.vertical_centered(|ui| {
            if state.extensions_panel.search_query.is_empty() {
                ui.label(
                    RichText::new("Type to search extensions.")
                        .color(c.muted)
                        .size(11.5),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new("e.g. \"rust\", \"git\", \"theme\"")
                        .color(c.muted)
                        .small(),
                );
            } else {
                ui.label(RichText::new("No results found.").color(c.muted).size(12.0));
            }
        });
    } else {
        for ext in &results {
            show_registry_card(ui, ext, state, c);
            ui.add_space(2.0);
        }
    }

    state.extensions_panel.search_results = results;
}

// ── Recommended tab ───────────────────────────────────────────────────────────

fn show_recommended_tab(ui: &mut egui::Ui, state: &mut UiState, c: &Colors) {
    ui.add_space(6.0);
    show_section_header(ui, c.sidebar_bg, c.fg, "Recommended for you");
    ui.add_space(4.0);

    let recommended = std::mem::take(&mut state.extensions_panel.recommended);

    if recommended.is_empty() {
        ui.add_space(16.0);
        ui.vertical_centered(|ui| {
            ui.spinner();
            ui.label(
                RichText::new("Loading recommendations…")
                    .color(c.muted)
                    .size(11.0),
            );
        });
    } else {
        for ext in &recommended {
            show_registry_card(ui, ext, state, c);
            ui.add_space(2.0);
        }
    }

    state.extensions_panel.recommended = recommended;
}

// ── Registry extension card ───────────────────────────────────────────────────

fn show_registry_card(ui: &mut egui::Ui, ext: &RegistryExtension, state: &mut UiState, c: &Colors) {
    let already_installed = state
        .extensions_panel
        .installed
        .iter()
        .any(|e| e.manifest.id == ext.id);

    Frame::NONE
        .fill(c.card_bg)
        .corner_radius(4)
        .inner_margin(Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            // Header: name + version + install button.
            ui.horizontal(|ui| {
                ui.label(RichText::new(&ext.name).strong().size(12.5).color(c.fg));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if already_installed {
                        ui.label(RichText::new("✓ Installed").size(10.5).color(ok_col()));
                    } else if ui
                        .add(
                            egui::Button::new(
                                RichText::new("⊕ Install").size(10.5).color(c.accent),
                            )
                            .frame(false),
                        )
                        .clicked()
                    {
                        state.extensions_panel.pending_install_registry = Some(ext.id.clone());
                    }
                    ui.label(
                        RichText::new(format!("v{}", ext.version))
                            .size(10.0)
                            .color(c.muted),
                    );
                });
            });

            // Author + category.
            ui.horizontal(|ui| {
                ui.label(RichText::new(&ext.author).size(10.5).color(c.muted));
                ui.label(RichText::new("·").size(10.5).color(c.muted));
                ui.label(RichText::new(&ext.category).size(10.5).color(c.muted));
            });

            // Description.
            let desc = if ext.description.len() > 80 {
                format!("{}…", &ext.description[..80])
            } else {
                ext.description.clone()
            };
            ui.label(RichText::new(desc).size(11.0).color(c.fg));

            // Downloads + star rating.
            ui.horizontal(|ui| {
                let dl = if ext.downloads >= 1_000_000 {
                    format!("{:.1}M ↓", ext.downloads as f64 / 1_000_000.0)
                } else if ext.downloads >= 1_000 {
                    format!("{:.0}K ↓", ext.downloads as f64 / 1_000.0)
                } else {
                    format!("{} ↓", ext.downloads)
                };
                ui.label(RichText::new(dl).size(9.5).color(c.muted));

                let full = ext.rating.floor() as usize;
                let empty = 5usize.saturating_sub(full);
                let stars = format!("{}{}", "★".repeat(full), "☆".repeat(empty));
                ui.label(RichText::new(stars).size(9.5).color(warn_col()));
                ui.label(
                    RichText::new(format!("{:.1}", ext.rating))
                        .size(9.5)
                        .color(c.muted),
                );
            });
        });
}

// ── Section header ────────────────────────────────────────────────────────────

fn show_section_header(ui: &mut egui::Ui, bg: Color32, fg: Color32, title: &str) {
    Frame::NONE
        .fill(bg)
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).size(10.5).strong().color(fg));
        });
}
