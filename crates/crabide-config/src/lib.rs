#![warn(clippy::pedantic)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::doc_markdown,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::struct_excessive_bools,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::match_same_arms,
    clippy::assigning_clones,
    clippy::needless_pass_by_value,
    clippy::manual_let_else,
    clippy::items_after_statements,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::fn_params_excessive_bools,
    clippy::used_underscore_binding,
    clippy::semicolon_if_nothing_returned,
    clippy::return_self_not_must_use,
    clippy::default_trait_access,
    clippy::cast_possible_wrap,
    clippy::redundant_closure,
    clippy::if_not_else,
    clippy::format_push_string,
    clippy::format_collect,
    clippy::trivially_copy_pass_by_ref,
    clippy::unnecessary_wraps,
    clippy::unnecessary_debug_formatting,
    clippy::wildcard_imports,
    clippy::match_wildcard_for_single_variants,
    clippy::explicit_iter_loop,
    clippy::needless_continue,
    clippy::float_cmp,
    clippy::unused_self,
    clippy::collapsible_else_if,
    clippy::redundant_else,
    clippy::unnecessary_map_or,
    clippy::many_single_char_names,
    clippy::redundant_closure_for_method_calls,
    clippy::uninlined_format_args,
    clippy::cast_lossless,
    clippy::map_unwrap_or
)]
//! `crabide-config` — settings, keybindings, and theme engine.
//!
//! # Usage
//! ```no_run
//! # use crabide_config::ConfigManager;
//! let (config, rx) = ConfigManager::new(None);
//! let settings = config.settings();
//! let theme    = config.active_theme();
//! ```

pub mod keybindings;
pub mod settings;
pub mod theme;

pub use keybindings::{
    Action, ActionRegistry, Key, KeyBinding, KeyChord, KeybindingEngine, Modifiers, WhenCondition,
    WhenContext, all_actions, all_actions_with, parse_chord,
};
pub use settings::{
    AutoSave, CursorBlinking, CursorStyle, EditorSettings, ExtensionsSettings, GitSettings,
    LineNumberStyle, LspSettings, PanelLocation, PartialEditorSettings, RenderWhitespace, Settings,
    SettingsLoader, SidebarLocation, TerminalSettings, UiSettings,
};
pub use theme::{
    Color, ColorTheme, FontStyle, ThemeType, TokenColorRule, TokenStyle, builtin_themes,
    parse_vscode_theme, parse_vscode_theme_str,
};

use indexmap::IndexMap;
use notify::RecursiveMode;
use notify_debouncer_full::new_debouncer;
use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

// ── Config change events ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigEvent {
    SettingsChanged,
    KeybindingsChanged,
    ThemeChanged { theme_id: String },
}

// ── ConfigManager ─────────────────────────────────────────────────────────────

pub struct ConfigManager {
    workspace_root: Option<PathBuf>,
    inner: Arc<RwLock<ConfigInner>>,
    event_tx: crossbeam_channel::Sender<ConfigEvent>,
    _debouncer: Option<
        notify_debouncer_full::Debouncer<
            notify::RecommendedWatcher,
            notify_debouncer_full::FileIdMap,
        >,
    >,
}

struct ConfigInner {
    settings: Arc<Settings>,
    keybinding_engine: KeybindingEngine,
    themes: IndexMap<String, ColorTheme>,
    active_theme_id: String,
    action_registry: ActionRegistry,
}

impl ConfigManager {
    pub fn new(
        workspace_root: Option<PathBuf>,
    ) -> (Self, crossbeam_channel::Receiver<ConfigEvent>) {
        let (tx, rx) = crossbeam_channel::bounded(64);

        let settings = SettingsLoader::load(workspace_root.as_deref());
        let active_theme_id = settings.ui.color_theme.clone();

        let mut keybinding_engine = KeybindingEngine::with_defaults();
        if let Some(user_dir) = SettingsLoader::user_config_dir() {
            let kb_path = user_dir.join("keybindings.toml");
            if let Err(e) = keybinding_engine.load_file(&kb_path) {
                log::warn!("Failed to load keybindings: {e}");
            }
        }

        let themes = {
            let mut all = builtin_themes();
            if let Some(user_dir) = SettingsLoader::user_config_dir() {
                Self::load_user_themes(&user_dir.join("themes"), &mut all);
            }
            all
        };

        let inner = Arc::new(RwLock::new(ConfigInner {
            settings: Arc::new(settings),
            keybinding_engine,
            themes,
            active_theme_id,
            action_registry: ActionRegistry::new(),
        }));

        let debouncer = Self::start_watcher(workspace_root.as_deref(), inner.clone(), tx.clone());

        let manager = Self {
            workspace_root,
            inner,
            event_tx: tx,
            _debouncer: debouncer,
        };
        (manager, rx)
    }

    pub fn settings(&self) -> Arc<Settings> {
        self.inner.read().settings.clone()
    }

    pub fn with_keybinding_engine<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut KeybindingEngine) -> R,
    {
        f(&mut self.inner.write().keybinding_engine)
    }

    /// Access the action registry for extensions to register custom actions.
    pub fn action_registry(&self) -> ActionRegistry {
        self.inner.read().action_registry.clone()
    }

    /// Mutate the action registry.
    pub fn with_action_registry<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut ActionRegistry) -> R,
    {
        f(&mut self.inner.write().action_registry)
    }

    pub fn active_theme(&self) -> ColorTheme {
        let guard = self.inner.read();
        guard
            .themes
            .get(&guard.active_theme_id)
            .or_else(|| guard.themes.get("crabide-dark"))
            .cloned()
            .unwrap_or_else(|| {
                builtin_themes()
                    .swap_remove("crabide-dark")
                    .expect("crabide-dark is always in builtin_themes")
            })
    }

    pub fn themes(&self) -> IndexMap<String, ColorTheme> {
        self.inner.read().themes.clone()
    }

    pub fn set_active_theme(&self, theme_id: &str) {
        self.inner.write().active_theme_id = theme_id.to_owned();
        let _ = self.event_tx.try_send(ConfigEvent::ThemeChanged {
            theme_id: theme_id.to_owned(),
        });
    }

    pub fn reload_settings(&self) {
        let new_settings = SettingsLoader::load(self.workspace_root.as_deref());
        self.inner.write().settings = Arc::new(new_settings);
        let _ = self.event_tx.try_send(ConfigEvent::SettingsChanged);
    }

    pub fn reload_keybindings(&self) {
        let mut engine = KeybindingEngine::with_defaults();
        if let Some(user_dir) = SettingsLoader::user_config_dir() {
            if let Err(e) = engine.load_file(&user_dir.join("keybindings.toml")) {
                log::warn!("Failed to reload keybindings: {e}");
            }
        }
        self.inner.write().keybinding_engine = engine;
        let _ = self.event_tx.try_send(ConfigEvent::KeybindingsChanged);
    }

    fn load_user_themes(theme_dir: &Path, map: &mut IndexMap<String, ColorTheme>) {
        let Ok(entries) = std::fs::read_dir(theme_dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_owned();
                match parse_vscode_theme(&id, &path) {
                    Ok(theme) => {
                        map.insert(id, theme);
                    }
                    Err(e) => log::warn!("Failed to load theme {}: {e}", path.display()),
                }
            }
        }
    }

    fn start_watcher(
        workspace_root: Option<&Path>,
        inner: Arc<RwLock<ConfigInner>>,
        tx: crossbeam_channel::Sender<ConfigEvent>,
    ) -> Option<
        notify_debouncer_full::Debouncer<
            notify::RecommendedWatcher,
            notify_debouncer_full::FileIdMap,
        >,
    > {
        let user_dir = SettingsLoader::user_config_dir()?;
        let workspace_dir = workspace_root.map(SettingsLoader::workspace_config_dir);
        let workspace_root_owned = workspace_root.map(std::path::Path::to_path_buf);

        let result = new_debouncer(
            Duration::from_millis(300),
            None,
            move |result: notify_debouncer_full::DebounceEventResult| {
                let events = match result {
                    Ok(evs) => evs,
                    Err(errs) => {
                        for e in errs {
                            log::debug!("Config watcher error: {e}");
                        }
                        return;
                    }
                };

                let mut reload_settings = false;
                let mut reload_keybindings = false;
                let mut reload_themes: Vec<PathBuf> = Vec::new();

                for ev in events {
                    for path in &ev.event.paths {
                        match path.file_name().and_then(|n| n.to_str()).unwrap_or("") {
                            "settings.toml" => reload_settings = true,
                            "keybindings.toml" => reload_keybindings = true,
                            n if n.ends_with(".json") => reload_themes.push(path.clone()),
                            _ => {}
                        }
                    }
                }

                if reload_settings {
                    inner.write().settings =
                        Arc::new(SettingsLoader::load(workspace_root_owned.as_deref()));
                    let _ = tx.try_send(ConfigEvent::SettingsChanged);
                }
                if reload_keybindings {
                    let mut engine = KeybindingEngine::with_defaults();
                    if let Some(kbp) =
                        SettingsLoader::user_config_dir().map(|d| d.join("keybindings.toml"))
                    {
                        if let Err(e) = engine.load_file(&kbp) {
                            log::warn!("Failed to reload keybindings: {e}");
                        }
                    }
                    inner.write().keybinding_engine = engine;
                    let _ = tx.try_send(ConfigEvent::KeybindingsChanged);
                }
                for theme_path in reload_themes {
                    let id = theme_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_owned();
                    match parse_vscode_theme(&id, &theme_path) {
                        Ok(theme) => {
                            inner.write().themes.insert(id.clone(), theme);
                            let _ = tx.try_send(ConfigEvent::ThemeChanged { theme_id: id });
                        }
                        Err(e) => log::warn!("Failed to reload theme: {e}"),
                    }
                }
            },
        );

        let mut debouncer = match result {
            Ok(d) => d,
            Err(e) => {
                log::warn!("Failed to start config watcher: {e}");
                return None;
            }
        };

        if user_dir.exists() {
            if let Err(e) = debouncer.watch(&user_dir, RecursiveMode::Recursive) {
                log::debug!("Cannot watch user config dir: {e}");
            }
        }
        if let Some(ws_dir) = workspace_dir {
            if ws_dir.exists() {
                if let Err(e) = debouncer.watch(&ws_dir, RecursiveMode::Recursive) {
                    log::debug!("Cannot watch workspace config dir: {e}");
                }
            }
        }

        Some(debouncer)
    }
}
