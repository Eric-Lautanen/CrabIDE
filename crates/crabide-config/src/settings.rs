//! TOML settings loader — merges user + workspace settings.
//!
//! Load order (later overrides earlier):
//! 1. Built-in defaults (hardcoded)
//! 2. User settings   `~/.crabide/settings.toml`
//! 3. Workspace settings `{root}/.crabide/settings.toml`

use crabide_core::error::{crabideError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ── Settings structs ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[derive(Default)]
pub struct Settings {
    pub editor: EditorSettings,
    pub ui: UiSettings,
    pub terminal: TerminalSettings,
    pub git: GitSettings,
    pub lsp: LspSettings,
    // Per-language editor settings overrides, keyed by language ID.
    // E.g., `[language.rust] tab_size = 4`
    #[serde(rename = "language", default)]
    pub language_overrides: HashMap<String, PartialEditorSettings>,
}

impl Settings {
    /// Return the effective editor settings for a given language, merging the
    /// base `editor` settings with any per-language overrides.
    pub fn editor_for_language(&self, language: &str) -> EditorSettings {
        let mut base = self.editor.clone();
        if let Some(overrides) = self.language_overrides.get(language) {
            overrides.apply_to(&mut base);
        }
        base
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EditorSettings {
    pub font_family: String,
    pub font_size: f32,
    pub line_height: f32,
    pub tab_size: u32,
    pub insert_spaces: bool,
    pub word_wrap: bool,
    pub line_numbers: LineNumberStyle,
    pub auto_save: AutoSave,
    pub format_on_save: bool,
    pub trim_trailing_whitespace: bool,
    pub render_whitespace: RenderWhitespace,
    pub minimap_enabled: bool,
    pub bracket_pair_colorization: bool,
    pub inlay_hints_enabled: bool,
    pub auto_closing_brackets: bool,
    pub scroll_beyond_last_line: bool,
    pub cursor_blinking: CursorBlinking,
    pub cursor_style: CursorStyle,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            font_family: "Cascadia Code, Consolas, monospace".into(),
            font_size: 14.0,
            line_height: 1.5,
            tab_size: 4,
            insert_spaces: true,
            word_wrap: false,
            line_numbers: LineNumberStyle::On,
            auto_save: AutoSave::Off,
            format_on_save: false,
            trim_trailing_whitespace: true,
            render_whitespace: RenderWhitespace::None,
            minimap_enabled: false,
            bracket_pair_colorization: true,
            inlay_hints_enabled: true,
            auto_closing_brackets: true,
            scroll_beyond_last_line: true,
            cursor_blinking: CursorBlinking::Blink,
            cursor_style: CursorStyle::Line,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum LineNumberStyle {
    #[default]
    On,
    Off,
    Relative,
    Interval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AutoSave {
    #[default]
    Off,
    AfterDelay,
    OnFocusChange,
    OnWindowChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum RenderWhitespace {
    #[default]
    None,
    Boundary,
    Selection,
    Trailing,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum CursorBlinking {
    #[default]
    Blink,
    Smooth,
    Phase,
    Expand,
    Solid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum CursorStyle {
    #[default]
    Line,
    Block,
    Underline,
    LineThin,
    BlockOutline,
    UnderlineThin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiSettings {
    pub color_theme: String,
    pub icon_theme: String,
    pub sidebar_location: SidebarLocation,
    pub activity_bar_visible: bool,
    pub status_bar_visible: bool,
    pub panel_default_location: PanelLocation,
    pub zoom_level: f32,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            color_theme: "crabide-dark".into(),
            icon_theme: "default".into(),
            sidebar_location: SidebarLocation::Left,
            activity_bar_visible: true,
            status_bar_visible: true,
            panel_default_location: PanelLocation::Bottom,
            zoom_level: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SidebarLocation {
    #[default]
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum PanelLocation {
    #[default]
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TerminalSettings {
    pub shell: String,
    pub font_family: String,
    pub font_size: f32,
    pub scrollback_lines: usize,
    pub cursor_blinking: bool,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            shell: default_shell(),
            font_family: "Cascadia Mono, Consolas, monospace".into(),
            font_size: 13.0,
            scrollback_lines: 10_000,
            cursor_blinking: true,
        }
    }
}

fn default_shell() -> String {
    if cfg!(windows) {
        "powershell.exe".into()
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GitSettings {
    pub enabled: bool,
    pub auto_fetch: bool,
    pub auto_fetch_interval_secs: u64,
    pub confirm_sync: bool,
}

impl Default for GitSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_fetch: true,
            auto_fetch_interval_secs: 180,
            confirm_sync: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LspSettings {
    pub enabled: bool,
    pub inlay_hints: bool,
    pub format_on_save: bool,
    pub completion_trigger_characters: Vec<String>,
}

impl Default for LspSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            inlay_hints: true,
            format_on_save: false,
            completion_trigger_characters: vec![".".into(), ":".into(), "(".into()],
        }
    }
}

// ── Partial overlay (all fields optional for merging) ─────────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PartialSettings {
    editor: PartialEditorSettings,
    ui: PartialUiSettings,
    terminal: PartialTerminalSettings,
    git: PartialGitSettings,
    lsp: PartialLspSettings,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PartialEditorSettings {
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub line_height: Option<f32>,
    pub tab_size: Option<u32>,
    pub insert_spaces: Option<bool>,
    pub word_wrap: Option<bool>,
    pub line_numbers: Option<LineNumberStyle>,
    pub auto_save: Option<AutoSave>,
    pub format_on_save: Option<bool>,
    pub trim_trailing_whitespace: Option<bool>,
    pub render_whitespace: Option<RenderWhitespace>,
    pub minimap_enabled: Option<bool>,
    pub bracket_pair_colorization: Option<bool>,
    pub inlay_hints_enabled: Option<bool>,
    pub auto_closing_brackets: Option<bool>,
    pub scroll_beyond_last_line: Option<bool>,
    pub cursor_blinking: Option<CursorBlinking>,
    pub cursor_style: Option<CursorStyle>,
}

impl PartialEditorSettings {
    /// Apply these partial overrides onto a base `EditorSettings`.
    pub fn apply_to(&self, base: &mut EditorSettings) {
        if let Some(v) = &self.font_family {
            base.font_family = v.clone();
        }
        if let Some(v) = self.font_size {
            base.font_size = v;
        }
        if let Some(v) = self.line_height {
            base.line_height = v;
        }
        if let Some(v) = self.tab_size {
            base.tab_size = v;
        }
        if let Some(v) = self.insert_spaces {
            base.insert_spaces = v;
        }
        if let Some(v) = self.word_wrap {
            base.word_wrap = v;
        }
        if let Some(v) = self.line_numbers {
            base.line_numbers = v;
        }
        if let Some(v) = self.auto_save {
            base.auto_save = v;
        }
        if let Some(v) = self.format_on_save {
            base.format_on_save = v;
        }
        if let Some(v) = self.trim_trailing_whitespace {
            base.trim_trailing_whitespace = v;
        }
        if let Some(v) = self.render_whitespace {
            base.render_whitespace = v;
        }
        if let Some(v) = self.minimap_enabled {
            base.minimap_enabled = v;
        }
        if let Some(v) = self.bracket_pair_colorization {
            base.bracket_pair_colorization = v;
        }
        if let Some(v) = self.inlay_hints_enabled {
            base.inlay_hints_enabled = v;
        }
        if let Some(v) = self.auto_closing_brackets {
            base.auto_closing_brackets = v;
        }
        if let Some(v) = self.scroll_beyond_last_line {
            base.scroll_beyond_last_line = v;
        }
        if let Some(v) = self.cursor_blinking {
            base.cursor_blinking = v;
        }
        if let Some(v) = self.cursor_style {
            base.cursor_style = v;
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PartialUiSettings {
    color_theme: Option<String>,
    icon_theme: Option<String>,
    sidebar_location: Option<SidebarLocation>,
    activity_bar_visible: Option<bool>,
    status_bar_visible: Option<bool>,
    panel_default_location: Option<PanelLocation>,
    zoom_level: Option<f32>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PartialTerminalSettings {
    shell: Option<String>,
    font_family: Option<String>,
    font_size: Option<f32>,
    scrollback_lines: Option<usize>,
    cursor_blinking: Option<bool>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PartialGitSettings {
    enabled: Option<bool>,
    auto_fetch: Option<bool>,
    auto_fetch_interval_secs: Option<u64>,
    confirm_sync: Option<bool>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PartialLspSettings {
    enabled: Option<bool>,
    inlay_hints: Option<bool>,
    format_on_save: Option<bool>,
    completion_trigger_characters: Option<Vec<String>>,
}

impl PartialSettings {
    fn apply_onto(&self, base: &mut Settings) {
        let e = &self.editor;
        if let Some(v) = e.font_family.clone() {
            base.editor.font_family = v
        }
        if let Some(v) = e.font_size {
            base.editor.font_size = v
        }
        if let Some(v) = e.line_height {
            base.editor.line_height = v
        }
        if let Some(v) = e.tab_size {
            base.editor.tab_size = v
        }
        if let Some(v) = e.insert_spaces {
            base.editor.insert_spaces = v
        }
        if let Some(v) = e.word_wrap {
            base.editor.word_wrap = v
        }
        if let Some(v) = e.line_numbers {
            base.editor.line_numbers = v
        }
        if let Some(v) = e.auto_save {
            base.editor.auto_save = v
        }
        if let Some(v) = e.format_on_save {
            base.editor.format_on_save = v
        }
        if let Some(v) = e.trim_trailing_whitespace {
            base.editor.trim_trailing_whitespace = v
        }
        if let Some(v) = e.render_whitespace {
            base.editor.render_whitespace = v
        }
        if let Some(v) = e.minimap_enabled {
            base.editor.minimap_enabled = v
        }
        if let Some(v) = e.bracket_pair_colorization {
            base.editor.bracket_pair_colorization = v
        }
        if let Some(v) = e.inlay_hints_enabled {
            base.editor.inlay_hints_enabled = v
        }
        if let Some(v) = e.auto_closing_brackets {
            base.editor.auto_closing_brackets = v
        }
        if let Some(v) = e.scroll_beyond_last_line {
            base.editor.scroll_beyond_last_line = v
        }
        if let Some(v) = e.cursor_blinking {
            base.editor.cursor_blinking = v
        }
        if let Some(v) = e.cursor_style {
            base.editor.cursor_style = v
        }

        let u = &self.ui;
        if let Some(v) = u.color_theme.clone() {
            base.ui.color_theme = v
        }
        if let Some(v) = u.icon_theme.clone() {
            base.ui.icon_theme = v
        }
        if let Some(v) = u.sidebar_location {
            base.ui.sidebar_location = v
        }
        if let Some(v) = u.activity_bar_visible {
            base.ui.activity_bar_visible = v
        }
        if let Some(v) = u.status_bar_visible {
            base.ui.status_bar_visible = v
        }
        if let Some(v) = u.panel_default_location {
            base.ui.panel_default_location = v
        }
        if let Some(v) = u.zoom_level {
            base.ui.zoom_level = v
        }

        let t = &self.terminal;
        if let Some(v) = t.shell.clone() {
            base.terminal.shell = v
        }
        if let Some(v) = t.font_family.clone() {
            base.terminal.font_family = v
        }
        if let Some(v) = t.font_size {
            base.terminal.font_size = v
        }
        if let Some(v) = t.scrollback_lines {
            base.terminal.scrollback_lines = v
        }
        if let Some(v) = t.cursor_blinking {
            base.terminal.cursor_blinking = v
        }

        let g = &self.git;
        if let Some(v) = g.enabled {
            base.git.enabled = v
        }
        if let Some(v) = g.auto_fetch {
            base.git.auto_fetch = v
        }
        if let Some(v) = g.auto_fetch_interval_secs {
            base.git.auto_fetch_interval_secs = v
        }
        if let Some(v) = g.confirm_sync {
            base.git.confirm_sync = v
        }

        let l = &self.lsp;
        if let Some(v) = l.enabled {
            base.lsp.enabled = v
        }
        if let Some(v) = l.inlay_hints {
            base.lsp.inlay_hints = v
        }
        if let Some(v) = l.format_on_save {
            base.lsp.format_on_save = v
        }
        if let Some(v) = l.completion_trigger_characters.clone() {
            base.lsp.completion_trigger_characters = v
        }
    }
}

// ── SettingsLoader ────────────────────────────────────────────────────────────

pub struct SettingsLoader;

impl SettingsLoader {
    pub fn user_config_dir() -> Option<PathBuf> {
        home_dir().map(|h| h.join(".crabide"))
    }

    pub fn workspace_config_dir(workspace_root: &Path) -> PathBuf {
        workspace_root.join(".crabide")
    }

    /// Load merged settings; missing files are silently skipped.
    pub fn load(workspace_root: Option<&Path>) -> Settings {
        let mut settings = Settings::default();

        if let Some(user_dir) = Self::user_config_dir() {
            let path = user_dir.join("settings.toml");
            if let Err(e) = Self::load_and_apply(&path, &mut settings) {
                log::warn!("Failed to load user settings from {}: {e}", path.display());
            }
        }

        if let Some(root) = workspace_root {
            let path = Self::workspace_config_dir(root).join("settings.toml");
            if let Err(e) = Self::load_and_apply(&path, &mut settings) {
                log::warn!(
                    "Failed to load workspace settings from {}: {e}",
                    path.display()
                );
            }
        }

        settings
    }

    fn load_and_apply(path: &Path, settings: &mut Settings) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }
        let content = std::fs::read_to_string(path).map_err(|e| crabideError::ConfigParse {
            file: path.display().to_string(),
            message: e.to_string(),
        })?;
        let partial: PartialSettings =
            toml::from_str(&content).map_err(|e| crabideError::ConfigParse {
                file: path.display().to_string(),
                message: e.to_string(),
            })?;
        partial.apply_onto(settings);
        Ok(())
    }

    pub fn save(settings: &Settings, path: &Path) -> Result<()> {
        let content =
            toml::to_string_pretty(settings).map_err(|e| crabideError::Other(e.to_string()))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}
