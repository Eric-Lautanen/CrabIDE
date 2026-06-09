//! VS Code–compatible theme parser and built-in themes.
//!
//! Parses `*.json` color theme files into a `ColorTheme` struct.
//! Built-in themes: `"crabide-dark"` and `"crabide-light"`.

use bitflags::bitflags;
use crabide_core::error::{Result, crabideError};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;
use std::str::FromStr;

// ── Color ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const WHITE: Self = Self::rgb(255, 255, 255);
    pub fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    pub fn blend(self, other: Self) -> Self {
        if other.a == 255 {
            return other;
        }
        if other.a == 0 {
            return self;
        }
        let a0 = u32::from(other.a);
        let a1 = 255 - a0;
        Self {
            r: ((u32::from(self.r) * a1 + u32::from(other.r) * a0) / 255) as u8,
            g: ((u32::from(self.g) * a1 + u32::from(other.g) * a0) / 255) as u8,
            b: ((u32::from(self.b) * a1 + u32::from(other.b) * a0) / 255) as u8,
            a: 255,
        }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.a == 255 {
            write!(f, "#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        } else {
            write!(
                f,
                "#{:02X}{:02X}{:02X}{:02X}",
                self.r, self.g, self.b, self.a
            )
        }
    }
}

impl FromStr for Color {
    type Err = crabideError;

    fn from_str(s: &str) -> Result<Self> {
        let s = s.trim().strip_prefix('#').unwrap_or(s);
        fn hex2(s: &str) -> std::result::Result<u8, std::num::ParseIntError> {
            u8::from_str_radix(s, 16)
        }
        let err = || crabideError::ConfigParse {
            file: "theme".into(),
            message: format!("invalid hex color: #{s}"),
        };
        match s.len() {
            3 => Ok(Self::rgb(
                hex2(&s[0..1].repeat(2)).map_err(|_| err())?,
                hex2(&s[1..2].repeat(2)).map_err(|_| err())?,
                hex2(&s[2..3].repeat(2)).map_err(|_| err())?,
            )),
            4 => Ok(Self::rgba(
                hex2(&s[0..1].repeat(2)).map_err(|_| err())?,
                hex2(&s[1..2].repeat(2)).map_err(|_| err())?,
                hex2(&s[2..3].repeat(2)).map_err(|_| err())?,
                hex2(&s[3..4].repeat(2)).map_err(|_| err())?,
            )),
            6 => Ok(Self::rgb(
                hex2(&s[0..2]).map_err(|_| err())?,
                hex2(&s[2..4]).map_err(|_| err())?,
                hex2(&s[4..6]).map_err(|_| err())?,
            )),
            8 => Ok(Self::rgba(
                hex2(&s[0..2]).map_err(|_| err())?,
                hex2(&s[2..4]).map_err(|_| err())?,
                hex2(&s[4..6]).map_err(|_| err())?,
                hex2(&s[6..8]).map_err(|_| err())?,
            )),
            _ => Err(err()),
        }
    }
}

// ── Font style flags ──────────────────────────────────────────────────────────

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct FontStyle: u8 {
        const BOLD          = 0b0001;
        const ITALIC        = 0b0010;
        const UNDERLINE     = 0b0100;
        const STRIKETHROUGH = 0b1000;
    }
}

impl FontStyle {
    pub fn parse(s: &str) -> Self {
        let mut style = FontStyle::empty();
        for part in s.split_whitespace() {
            match part {
                "bold" => style |= FontStyle::BOLD,
                "italic" => style |= FontStyle::ITALIC,
                "underline" => style |= FontStyle::UNDERLINE,
                "strikethrough" => style |= FontStyle::STRIKETHROUGH,
                _ => {}
            }
        }
        style
    }
}

// ── Token color rule ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TokenStyle {
    pub foreground: Option<Color>,
    pub font_style: FontStyle,
}

#[derive(Debug, Clone)]
pub struct TokenColorRule {
    pub scopes: Vec<String>,
    pub style: TokenStyle,
}

// ── ColorTheme ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeType {
    Dark,
    Light,
    HighContrastDark,
    HighContrastLight,
}

#[derive(Debug, Clone)]
pub struct ColorTheme {
    pub id: String,
    pub name: String,
    pub theme_type: ThemeType,
    pub ui_colors: IndexMap<String, Color>,
    pub token_colors: Vec<TokenColorRule>,
}

impl ColorTheme {
    pub fn ui(&self, key: &str) -> Option<Color> {
        self.ui_colors.get(key).copied()
    }
    pub fn ui_or(&self, key: &str, fallback: Color) -> Color {
        self.ui_colors.get(key).copied().unwrap_or(fallback)
    }
    pub fn token_style(&self, scope: &str) -> TokenStyle {
        let mut style = TokenStyle {
            foreground: None,
            font_style: FontStyle::empty(),
        };
        for rule in &self.token_colors {
            if rule.scopes.iter().any(|s| scope_matches(scope, s)) {
                if style.foreground.is_none() {
                    style.foreground = rule.style.foreground;
                }
                style.font_style |= rule.style.font_style;
            }
        }
        style
    }
}

fn scope_matches(scope: &str, selector: &str) -> bool {
    scope == selector || scope.starts_with(&format!("{selector}."))
}

// ── JSON theme parser ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct VsThemeJson {
    name: Option<String>,
    #[serde(rename = "type")]
    theme_type: Option<String>,
    colors: Option<IndexMap<String, String>>,
    #[serde(rename = "tokenColors", default)]
    token_colors: Vec<VsTokenColor>,
}

#[derive(Debug, Deserialize)]
struct VsTokenColor {
    name: Option<String>,
    #[serde(deserialize_with = "deser_scope")]
    scope: Vec<String>,
    settings: VsTokenSettings,
}

fn deser_scope<'de, D>(d: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Deserialize;
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ScopeField {
        One(String),
        Many(Vec<String>),
    }
    match Option::<ScopeField>::deserialize(d)? {
        None => Ok(Vec::new()),
        Some(ScopeField::One(s)) => Ok(s
            .split(',')
            .map(|p| p.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect()),
        Some(ScopeField::Many(v)) => Ok(v),
    }
}

#[derive(Debug, Deserialize)]
struct VsTokenSettings {
    foreground: Option<String>,
    #[serde(rename = "fontStyle")]
    font_style: Option<String>,
    background: Option<String>,
}

pub fn parse_vscode_theme(id: &str, path: &Path) -> Result<ColorTheme> {
    let content = std::fs::read_to_string(path)?;
    parse_vscode_theme_str(id, path.to_string_lossy().as_ref(), &content)
}

pub fn parse_vscode_theme_str(id: &str, source: &str, json: &str) -> Result<ColorTheme> {
    let stripped = strip_json_comments(json);
    let raw: VsThemeJson =
        serde_json::from_str(&stripped).map_err(|e| crabideError::ConfigParse {
            file: source.to_owned(),
            message: e.to_string(),
        })?;

    let theme_type = match raw.theme_type.as_deref().unwrap_or("dark") {
        "light" => ThemeType::Light,
        "hc-dark" | "hc-black" => ThemeType::HighContrastDark,
        "hc-light" | "hc-white" => ThemeType::HighContrastLight,
        _ => ThemeType::Dark,
    };

    let mut ui_colors = IndexMap::new();
    for (k, v) in raw.colors.unwrap_or_default() {
        match v.parse::<Color>() {
            Ok(c) => {
                ui_colors.insert(k, c);
            }
            Err(e) => log::debug!("Theme {source}: skipping color {k}: {e}"),
        }
    }

    let mut token_colors = Vec::new();
    for tc in raw.token_colors {
        if tc.scope.is_empty() {
            continue;
        }
        let _ = tc.name; // unused but present in JSON
        let _ = tc.settings.background; // background in token rules ignored
        let foreground = tc
            .settings
            .foreground
            .as_deref()
            .and_then(|s| s.parse::<Color>().ok());
        let font_style = tc
            .settings
            .font_style
            .as_deref()
            .map(FontStyle::parse)
            .unwrap_or_default();
        token_colors.push(TokenColorRule {
            scopes: tc.scope,
            style: TokenStyle {
                foreground,
                font_style,
            },
        });
    }

    Ok(ColorTheme {
        id: id.to_owned(),
        name: raw.name.unwrap_or_else(|| id.to_owned()),
        theme_type,
        ui_colors,
        token_colors,
    })
}

fn strip_json_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    let mut in_string = false;
    let mut escape_next = false;

    while let Some(ch) = chars.next() {
        if escape_next {
            out.push(ch);
            escape_next = false;
            continue;
        }
        if ch == '\\' && in_string {
            out.push(ch);
            escape_next = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            out.push(ch);
            continue;
        }
        if in_string {
            out.push(ch);
            continue;
        }
        if ch == '/' {
            match chars.peek() {
                Some('/') => {
                    chars.next();
                    for c in chars.by_ref() {
                        if c == '\n' {
                            out.push('\n');
                            break;
                        }
                    }
                }
                Some('*') => {
                    chars.next();
                    let mut prev = ' ';
                    for c in chars.by_ref() {
                        if prev == '*' && c == '/' {
                            break;
                        }
                        if c == '\n' {
                            out.push('\n');
                        }
                        prev = c;
                    }
                }
                _ => out.push(ch),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

// ── Built-in themes ───────────────────────────────────────────────────────────

pub fn builtin_themes() -> IndexMap<String, ColorTheme> {
    let mut map = IndexMap::new();
    map.insert("crabide-dark".into(), dark_theme());
    map.insert("crabide-light".into(), light_theme());
    map
}

// Helper: parse a hex literal to Color, panic on invalid input (compile-time constants only).
fn c(hex: &str) -> Color {
    hex.parse::<Color>()
        .expect("invalid built-in color literal")
}

// Helper: add a token rule.
fn tok(scopes: &[&str], fg: &str, style: FontStyle) -> TokenColorRule {
    TokenColorRule {
        scopes: scopes.iter().map(|s| (*s).to_owned()).collect(),
        style: TokenStyle {
            foreground: Some(c(fg)),
            font_style: style,
        },
    }
}

fn dark_theme() -> ColorTheme {
    let mut ui = IndexMap::new();
    let mut u = |k: &str, v: &str| {
        ui.insert(k.to_owned(), c(v));
    };

    u("editor.background", "#1e1e1e");
    u("editor.foreground", "#d4d4d4");
    u("editor.lineHighlightBackground", "#2d2d2d40");
    u("editor.selectionBackground", "#264f78a0");
    u("editor.inactiveSelectionBackground", "#3a3d41a0");
    u("editor.findMatchBackground", "#515c6ab0");
    u("editor.findMatchHighlightBackground", "#ea5c0055");
    u("editorCursor.foreground", "#aeafad");
    u("editorWhitespace.foreground", "#3b3b3b");
    u("editorIndentGuide.background1", "#404040");
    u("editorIndentGuide.activeBackground1", "#707070");
    u("editorBracketMatch.background", "#0064001a");
    u("editorBracketMatch.border", "#888888");
    u("editorLineNumber.foreground", "#858585");
    u("editorLineNumber.activeForeground", "#c6c6c6");
    u("editorGutter.background", "#1e1e1e");
    u("editorGutter.addedBackground", "#587c0c");
    u("editorGutter.modifiedBackground", "#0c7d9d");
    u("editorGutter.deletedBackground", "#94151b");
    u("sideBar.background", "#252526");
    u("sideBar.foreground", "#cccccc");
    u("sideBar.border", "#333333");
    u("sideBarSectionHeader.background", "#2d2d2d");
    u("sideBarSectionHeader.foreground", "#bbbbbb");
    u("activityBar.background", "#333333");
    u("activityBar.foreground", "#d7d7d7");
    u("activityBar.inactiveForeground", "#888888");
    u("activityBarBadge.background", "#007acc");
    u("activityBarBadge.foreground", "#ffffff");
    u("statusBar.background", "#007acc");
    u("statusBar.foreground", "#ffffff");
    u("statusBar.noFolderBackground", "#68217a");
    u("statusBarItem.hoverBackground", "#ffffff1a");
    u("statusBarItem.errorBackground", "#c72e0f");
    u("tab.activeBackground", "#1e1e1e");
    u("tab.activeForeground", "#ffffff");
    u("tab.inactiveBackground", "#2d2d2d");
    u("tab.inactiveForeground", "#ffffff80");
    u("tab.border", "#252526");
    u("tab.activeBorderTop", "#007acc");
    u("editorGroupHeader.tabsBackground", "#252526");
    u("list.activeSelectionBackground", "#094771");
    u("list.activeSelectionForeground", "#ffffff");
    u("list.inactiveSelectionBackground", "#37373d");
    u("list.hoverBackground", "#2a2d2e");
    u("list.focusBackground", "#062f4a");
    u("input.background", "#3c3c3c");
    u("input.foreground", "#cccccc");
    u("input.border", "#3c3c3c");
    u("input.placeholderForeground", "#a6a6a6");
    u("dropdown.background", "#3c3c3c");
    u("dropdown.foreground", "#f0f0f0");
    u("button.background", "#0e639c");
    u("button.foreground", "#ffffff");
    u("button.hoverBackground", "#1177bb");
    u("button.secondaryBackground", "#3c3c3c");
    u("scrollbarSlider.background", "#79797966");
    u("scrollbarSlider.hoverBackground", "#646464b3");
    u("scrollbarSlider.activeBackground", "#bfbfbf66");
    u("panel.background", "#1e1e1e");
    u("panel.border", "#808080");
    u("panelTitle.activeForeground", "#e7e7e7");
    u("panelTitle.inactiveForeground", "#e7e7e799");
    u("panelTitle.activeBorder", "#e7e7e7");
    u("terminal.background", "#1e1e1e");
    u("terminal.foreground", "#cccccc");
    u("terminal.ansiBlack", "#000000");
    u("terminal.ansiRed", "#cd3131");
    u("terminal.ansiGreen", "#0dbc79");
    u("terminal.ansiYellow", "#e5e510");
    u("terminal.ansiBlue", "#2472c8");
    u("terminal.ansiMagenta", "#bc3fbc");
    u("terminal.ansiCyan", "#11a8cd");
    u("terminal.ansiWhite", "#e5e5e5");
    u("terminal.ansiBrightBlack", "#666666");
    u("terminal.ansiBrightRed", "#f14c4c");
    u("terminal.ansiBrightGreen", "#23d18b");
    u("terminal.ansiBrightYellow", "#f5f543");
    u("terminal.ansiBrightBlue", "#3b8eea");
    u("terminal.ansiBrightMagenta", "#d670d6");
    u("terminal.ansiBrightCyan", "#29b8db");
    u("terminal.ansiBrightWhite", "#e5e5e5");
    u("editorError.foreground", "#f44747");
    u("editorWarning.foreground", "#cca700");
    u("editorInfo.foreground", "#75beff");
    u("editorHint.foreground", "#eeeeeeb3");

    let none = FontStyle::empty();
    let tokens = vec![
        tok(
            &["comment", "comment.block", "comment.line"],
            "#6a9955",
            FontStyle::ITALIC,
        ),
        tok(
            &["string", "string.quoted", "string.template"],
            "#ce9178",
            none,
        ),
        tok(&["constant.numeric", "constant.language"], "#b5cea8", none),
        tok(&["constant.character.escape"], "#d7ba7d", none),
        tok(
            &[
                "keyword",
                "keyword.control",
                "storage.type",
                "storage.modifier",
            ],
            "#569cd6",
            none,
        ),
        tok(&["keyword.operator"], "#d4d4d4", none),
        tok(
            &["entity.name.function", "support.function"],
            "#dcdcaa",
            none,
        ),
        tok(
            &[
                "entity.name.type",
                "entity.name.class",
                "support.type",
                "support.class",
            ],
            "#4ec9b0",
            none,
        ),
        tok(&["variable", "variable.other"], "#9cdcfe", none),
        tok(&["variable.language"], "#569cd6", none),
        tok(
            &["entity.name.namespace", "entity.name.module"],
            "#4ec9b0",
            none,
        ),
        tok(&["entity.name.tag"], "#569cd6", none),
        tok(&["entity.other.attribute-name"], "#9cdcfe", none),
        tok(
            &["support.constant", "variable.other.constant"],
            "#4fc1ff",
            none,
        ),
        tok(&["punctuation"], "#d4d4d4", none),
        tok(&["meta.preprocessor"], "#569cd6", none),
        tok(&["invalid.deprecated"], "#d4d4d4", FontStyle::STRIKETHROUGH),
        tok(&["markup.heading"], "#569cd6", FontStyle::BOLD),
        tok(&["markup.bold"], "#569cd6", FontStyle::BOLD),
        tok(&["markup.italic"], "#569cd6", FontStyle::ITALIC),
        tok(
            &["markup.inline.raw", "markup.fenced_code"],
            "#ce9178",
            none,
        ),
        tok(&["markup.inserted"], "#b5cea8", none),
        tok(&["markup.deleted"], "#f44747", none),
        tok(&["markup.changed"], "#cca700", none),
    ];

    ColorTheme {
        id: "crabide-dark".into(),
        name: "crabide Dark".into(),
        theme_type: ThemeType::Dark,
        ui_colors: ui,
        token_colors: tokens,
    }
}

fn light_theme() -> ColorTheme {
    let mut ui = IndexMap::new();
    let mut u = |k: &str, v: &str| {
        ui.insert(k.to_owned(), c(v));
    };

    u("editor.background", "#ffffff");
    u("editor.foreground", "#1e1e1e");
    u("editor.lineHighlightBackground", "#f5f5f540");
    u("editor.selectionBackground", "#add6ffa0");
    u("editor.inactiveSelectionBackground", "#e5ebf1a0");
    u("editor.findMatchBackground", "#a8c7fab0");
    u("editor.findMatchHighlightBackground", "#f8c00040");
    u("editorCursor.foreground", "#0066cc");
    u("editorWhitespace.foreground", "#d0d0d0");
    u("editorIndentGuide.background1", "#d3d3d3");
    u("editorIndentGuide.activeBackground1", "#939393");
    u("editorBracketMatch.background", "#c0e0ff80");
    u("editorBracketMatch.border", "#0080c0");
    u("editorLineNumber.foreground", "#999999");
    u("editorLineNumber.activeForeground", "#333333");
    u("editorGutter.background", "#ffffff");
    u("editorGutter.addedBackground", "#2d7600");
    u("editorGutter.modifiedBackground", "#0c7d9d");
    u("editorGutter.deletedBackground", "#c41515");
    u("editorError.foreground", "#e51400");
    u("editorWarning.foreground", "#bf8803");
    u("editorInfo.foreground", "#1a85ff");
    u("editorHint.foreground", "#6c6c6c");
    u("sideBar.background", "#f3f3f3");
    u("sideBar.foreground", "#333333");
    u("sideBar.border", "#d4d4d4");
    u("sideBarSectionHeader.background", "#e8e8e8");
    u("sideBarSectionHeader.foreground", "#333333");
    u("activityBar.background", "#2c2c2c");
    u("activityBar.foreground", "#ffffff");
    u("activityBar.inactiveForeground", "#ffffff66");
    u("activityBarBadge.background", "#007acc");
    u("activityBarBadge.foreground", "#ffffff");
    u("statusBar.background", "#007acc");
    u("statusBar.foreground", "#ffffff");
    u("statusBar.noFolderBackground", "#68217a");
    u("statusBarItem.hoverBackground", "#ffffff1a");
    u("tab.activeBackground", "#ffffff");
    u("tab.activeForeground", "#1e1e1e");
    u("tab.inactiveBackground", "#ececec");
    u("tab.inactiveForeground", "#666666");
    u("tab.border", "#d4d4d4");
    u("tab.activeBorderTop", "#007acc");
    u("editorGroupHeader.tabsBackground", "#f3f3f3");
    u("list.activeSelectionBackground", "#0060c0");
    u("list.activeSelectionForeground", "#ffffff");
    u("list.inactiveSelectionBackground", "#cce8ff");
    u("list.hoverBackground", "#e8e8e8");
    u("list.focusBackground", "#cce8ff");
    u("input.background", "#ffffff");
    u("input.foreground", "#1e1e1e");
    u("input.border", "#bebebe");
    u("input.placeholderForeground", "#aaaaaa");
    u("dropdown.background", "#f8f8f8");
    u("dropdown.foreground", "#1e1e1e");
    u("dropdown.border", "#cecece");
    u("button.background", "#007acc");
    u("button.foreground", "#ffffff");
    u("button.hoverBackground", "#0069b5");
    u("button.secondaryBackground", "#e8e8e8");
    u("scrollbarSlider.background", "#64646440");
    u("scrollbarSlider.hoverBackground", "#64646480");
    u("scrollbarSlider.activeBackground", "#646464b3");
    u("panel.background", "#f3f3f3");
    u("panel.border", "#c0c0c0");
    u("panelTitle.activeForeground", "#1e1e1e");
    u("panelTitle.inactiveForeground", "#777777");
    u("panelTitle.activeBorder", "#007acc");
    u("terminal.background", "#ffffff");
    u("terminal.foreground", "#1e1e1e");
    u("terminal.ansiBlack", "#000000");
    u("terminal.ansiRed", "#de3e35");
    u("terminal.ansiGreen", "#3f953a");
    u("terminal.ansiYellow", "#d2b67b");
    u("terminal.ansiBlue", "#2f5af3");
    u("terminal.ansiMagenta", "#950095");
    u("terminal.ansiCyan", "#3f953a");
    u("terminal.ansiWhite", "#bbbbbb");
    u("terminal.ansiBrightBlack", "#808080");
    u("terminal.ansiBrightRed", "#de3e35");
    u("terminal.ansiBrightGreen", "#3f953a");
    u("terminal.ansiBrightYellow", "#d2b67b");
    u("terminal.ansiBrightBlue", "#2f5af3");
    u("terminal.ansiBrightMagenta", "#a00095");
    u("terminal.ansiBrightCyan", "#3f953a");
    u("terminal.ansiBrightWhite", "#bbbbbb");
    u("editorError.foreground", "#e51400");
    u("editorWarning.foreground", "#bf8803");
    u("editorInfo.foreground", "#1a85ff");
    u("editorHint.foreground", "#6c6c6c");

    let none = FontStyle::empty();
    let tokens = vec![
        tok(
            &["comment", "comment.block", "comment.line"],
            "#008000",
            FontStyle::ITALIC,
        ),
        tok(
            &["string", "string.quoted", "string.template"],
            "#a31515",
            none,
        ),
        tok(&["constant.numeric", "constant.language"], "#098658", none),
        tok(&["constant.character.escape"], "#ee0000", none),
        tok(
            &[
                "keyword",
                "keyword.control",
                "storage.type",
                "storage.modifier",
            ],
            "#0000ff",
            none,
        ),
        tok(
            &["entity.name.function", "support.function"],
            "#795e26",
            none,
        ),
        tok(
            &[
                "entity.name.type",
                "entity.name.class",
                "support.type",
                "support.class",
            ],
            "#267f99",
            none,
        ),
        tok(&["variable", "variable.other"], "#001080", none),
        tok(&["variable.language"], "#0000ff", none),
        tok(&["entity.name.tag"], "#800000", none),
        tok(&["entity.other.attribute-name"], "#ff0000", none),
        tok(
            &["support.constant", "variable.other.constant"],
            "#0070c1",
            none,
        ),
        tok(&["markup.heading"], "#0000ff", FontStyle::BOLD),
        tok(&["markup.bold"], "#0000ff", FontStyle::BOLD),
        tok(&["markup.italic"], "#0000ff", FontStyle::ITALIC),
        tok(&["markup.inserted"], "#098658", none),
        tok(&["markup.deleted"], "#a31515", none),
    ];

    ColorTheme {
        id: "crabide-light".into(),
        name: "crabide Light".into(),
        theme_type: ThemeType::Light,
        ui_colors: ui,
        token_colors: tokens,
    }
}
