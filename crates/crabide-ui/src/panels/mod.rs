//! Renderable panels for the crabide editor shell.
//!
//! Each sub-module owns the rendering logic for one UI surface.  All panels
//! take a shared/mutable reference to `UiState` — they never own data.

pub mod breadcrumbs;
pub mod context_menu;
pub mod debug_panel;
pub mod debug_toolbar;
pub mod editor;
pub mod extensions_panel;
pub mod file_explorer;
pub mod find_replace;
pub mod fuzzy_finder;
pub mod git_panel;
pub mod gutter;
pub mod minimap;
pub mod output_panel;
pub mod problems_panel;
pub mod status_bar;
pub mod symbol_outline;
pub mod tab_bar;
pub mod terminal_panel;
pub mod workspace_search;
