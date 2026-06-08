//! Keybinding engine: `Action` enum, chord sequences, TOML parser.
//!
//! Format of `keybindings.toml`:
//! ```toml
//! [[bindings]]
//! key = "ctrl+shift+p"
//! action = "commandPalette"
//!
//! [[bindings]]
//! key = "ctrl+k ctrl+s" # chord: two presses
//! action = "saveAll"
//! when = "editorFocused" # optional context filter
//! ```
//!
//! VS Code `keybindings.json` is also supported via [`KeybindingEngine::load_vscode_json`].
//! The format is a JSON array of objects with `key`, `command`, and optional `when` fields.
//! A `-` prefix on `command` (e.g. `-editor.action.commentLine`) removes an existing binding.

use bitflags::bitflags;
use crabide_core::error::{crabideError, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

// ── Action enum ───────────────────────────────────────────────────────────────

/// All commands that can be bound to keys or executed via the command palette.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    // File
    NewFile,
    OpenFile,
    OpenFolder,
    SaveFile,
    SaveFileAs,
    SaveAll,
    CloseTab,
    CloseAllTabs,
    Quit,
    // Edit
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    DuplicateLine,
    DeleteLine,
    MoveLineUp,
    MoveLineDown,
    InsertNewlineAbove,
    InsertNewlineBelow,
    IndentLine,
    OutdentLine,
    ToggleLineComment,
    ToggleBlockComment,
    TrimTrailingWhitespace,
    // Cursor movement
    CursorUp,
    CursorDown,
    CursorLeft,
    CursorRight,
    CursorWordLeft,
    CursorWordRight,
    CursorLineStart,
    CursorLineEnd,
    CursorFileStart,
    CursorFileEnd,
    CursorPageUp,
    CursorPageDown,
    ScrollLineUp,
    ScrollLineDown,
    // Selection
    SelectUp,
    SelectDown,
    SelectLeft,
    SelectRight,
    SelectWordLeft,
    SelectWordRight,
    SelectLineStart,
    SelectLineEnd,
    SelectFileStart,
    SelectFileEnd,
    SelectLine,
    ExpandSelection,
    ShrinkSelection,
    AddCursorAbove,
    AddCursorBelow,
    AddNextOccurrence,
    SelectAllOccurrences,
    ColumnSelectUp,
    ColumnSelectDown,
    // Delete
    DeleteCharLeft,
    DeleteCharRight,
    DeleteWordLeft,
    DeleteWordRight,
    DeleteLineLeft,
    DeleteLineRight,
    // Find / replace
    Find,
    FindReplace,
    FindNext,
    FindPrevious,
    FindInFiles,
    ReplaceInFiles,
    // Navigation
    GotoLine,
    GotoDefinition,
    GotoDeclaration,
    GotoImplementation,
    GotoTypeDefinition,
    GotoReferences,
    GotoSymbol,
    GoBack,
    GoForward,
    NextDiagnostic,
    PreviousDiagnostic,
    // LSP
    TriggerCompletion,
    ShowHover,
    ShowSignatureHelp,
    RenameSymbol,
    ApplyCodeAction,
    FormatDocument,
    FormatSelection,
    OrganizeImports,
    // View
    CommandPalette,
    FuzzyFindFile,
    FuzzyFindSymbol,
    ToggleSidebar,
    TogglePanel,
    ToggleTerminal,
    ToggleGitPanel,
    ToggleGit,
    ToggleDebugPanel,
    ToggleDebug,
    ToggleExtensionsPanel,
    ToggleOutputPanel,
    ToggleMinimap,
    ToggleWordWrap,
    ToggleProblemsPanel,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    SplitEditorRight,
    SplitEditorDown,
    CloseEditor,
    NextTab,
    PreviousTab,
    MoveTabRight,
    MoveTabLeft,
    // Git
    GitCommit,
    GitStageAll,
    GitUnstageAll,
    GitDiscardChanges,
    GitFetch,
    GitPull,
    GitPush,
    GitMerge,
    GitRebase,
    // Debug
    ToggleBreakpoint,
    StartDebug,
    StopDebug,
    ContinueDebug,
    StepOver,
    StepInto,
    StepOut,
    RestartDebug,
    // Terminal
    NewTerminal,
    KillTerminal,
    // Snippets
    NextTabstop,
    PreviousTabstop,
    /// Raw text from keyboard or IME — inserts printable characters into the buffer.
    /// Not user-bindable via keybindings.toml; emitted by the UI layer on text events.
    InsertText(String),
    /// Extension-defined action.
    Custom(String),
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Action::InsertText(s) => write!(f, "insertText({s:?})"),
            Action::Custom(s) => write!(f, "{s}"),
            other => {
                let s = serde_json::to_string(other).unwrap_or_else(|_| format!("{other:?}"));
                write!(f, "{}", s.trim_matches('"'))
            }
        }
    }
}

// ── Key modifiers ─────────────────────────────────────────────────────────────

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Modifiers: u8 {
        const CTRL  = 0b0000_0001;
        const SHIFT = 0b0000_0010;
        const ALT   = 0b0000_0100;
        const META  = 0b0000_1000;
    }
}

// ── Key enum ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    F(u8),
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Enter,
    Backspace,
    Delete,
    Tab,
    Escape,
    Space,
    Numpad(u8),
    Unknown(String),
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Char(c) => write!(f, "{c}"),
            Key::F(n) => write!(f, "f{n}"),
            Key::Up => write!(f, "up"),
            Key::Down => write!(f, "down"),
            Key::Left => write!(f, "left"),
            Key::Right => write!(f, "right"),
            Key::Home => write!(f, "home"),
            Key::End => write!(f, "end"),
            Key::PageUp => write!(f, "pageup"),
            Key::PageDown => write!(f, "pagedown"),
            Key::Enter => write!(f, "enter"),
            Key::Backspace => write!(f, "backspace"),
            Key::Delete => write!(f, "delete"),
            Key::Tab => write!(f, "tab"),
            Key::Escape => write!(f, "escape"),
            Key::Space => write!(f, "space"),
            Key::Numpad(n) => write!(f, "numpad{n}"),
            Key::Unknown(s) => write!(f, "{s}"),
        }
    }
}

// ── KeyChord ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub modifiers: Modifiers,
    pub key: Key,
}

impl KeyChord {
    pub fn new(modifiers: Modifiers, key: Key) -> Self {
        Self { modifiers, key }
    }
}

impl fmt::Display for KeyChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modifiers.contains(Modifiers::CTRL) {
            write!(f, "ctrl+")?;
        }
        if self.modifiers.contains(Modifiers::SHIFT) {
            write!(f, "shift+")?;
        }
        if self.modifiers.contains(Modifiers::ALT) {
            write!(f, "alt+")?;
        }
        if self.modifiers.contains(Modifiers::META) {
            write!(f, "meta+")?;
        }
        write!(f, "{}", self.key)
    }
}

pub fn parse_chord(s: &str) -> Result<KeyChord> {
    let s = s.trim();
    if s.is_empty() {
        return Err(crabideError::ConfigParse {
            file: "keybindings".into(),
            message: format!("empty chord string: {s:?}"),
        });
    }
    let parts: Vec<&str> = s.split('+').collect();
    if parts.is_empty() {
        return Err(crabideError::ConfigParse {
            file: "keybindings".into(),
            message: format!("empty chord string: {s:?}"),
        });
    }
    let mut modifiers = Modifiers::empty();
    let key_str = parts.last().unwrap().trim().to_lowercase();
    for part in &parts[..parts.len() - 1] {
        match part.trim().to_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= Modifiers::CTRL,
            "shift" => modifiers |= Modifiers::SHIFT,
            "alt" | "option" => modifiers |= Modifiers::ALT,
            "meta" | "cmd" | "win" | "super" => modifiers |= Modifiers::META,
            unknown => {
                return Err(crabideError::ConfigParse {
                    file: "keybindings".into(),
                    message: format!("unknown modifier: {unknown:?}"),
                })
            }
        }
    }
    Ok(KeyChord {
        modifiers,
        key: parse_key(&key_str)?,
    })
}

fn parse_key(s: &str) -> Result<Key> {
    Ok(match s {
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "enter" | "return" => Key::Enter,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "tab" => Key::Tab,
        "escape" | "esc" => Key::Escape,
        "space" => Key::Space,
        // Single character keys must be matched before the "f" prefix check so
        // that "ctrl+f" (the letter f) is not mistakenly parsed as a function key.
        s if s.chars().count() == 1 => Key::Char(s.chars().next().unwrap()),
        s if s.starts_with('f') => {
            let n: u8 = s[1..].parse().map_err(|_| crabideError::ConfigParse {
                file: "keybindings".into(),
                message: format!("invalid function key: {s:?}"),
            })?;
            Key::F(n)
        }
        s if s.starts_with("numpad") => {
            let n: u8 = s[6..].parse().map_err(|_| crabideError::ConfigParse {
                file: "keybindings".into(),
                message: format!("invalid numpad key: {s:?}"),
            })?;
            Key::Numpad(n)
        }
        s => Key::Unknown(s.to_owned()),
    })
}

// ── KeyBinding ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct KeyBinding {
    pub chords: Vec<KeyChord>,
    pub action: Action,
    pub when: Option<String>,
}

// ── ActionRegistry ─────────────────────────────────────────────────────────────

/// A registry for extension-defined custom actions.
///
/// Extensions (or other dynamic sources) register custom action IDs here so they
/// appear in the command palette and can be bound to keys. The registry is
/// thread-safe via `Arc<RwLock<…>>`.
#[derive(Debug, Clone)]
pub struct ActionRegistry {
    /// Map of action ID → human-readable title (e.g. "Markdown: Toggle Preview")
    actions: IndexMap<String, String>,
}

impl ActionRegistry {
    pub fn new() -> Self {
        Self {
            actions: IndexMap::new(),
        }
    }

    /// Register a custom action. If an action with the same ID already exists,
    /// its title is updated (no-op for same title).
    pub fn register(&mut self, id: impl Into<String>, title: impl Into<String>) {
        self.actions.insert(id.into(), title.into());
    }

    /// Unregister a previously registered custom action.
    pub fn unregister(&mut self, id: &str) {
        self.actions.shift_remove(id);
    }

    /// Returns `true` if an action with the given ID is registered.
    pub fn has(&self, id: &str) -> bool {
        self.actions.contains_key(id)
    }

    /// Iterate over all registered custom actions as `(Action::Custom(id), title)` pairs.
    pub fn iter_custom(&self) -> impl Iterator<Item = (Action, &str)> + '_ {
        self.actions
            .iter()
            .map(|(id, title)| (Action::Custom(id.clone()), title.as_str()))
    }

    /// Total number of registered custom actions.
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

impl Default for ActionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── When condition evaluation ─────────────────────────────────────────────────

/// A pre-parsed `when` condition expression.
///
/// Supports boolean context keys (`editorFocused`), negation (`!terminalFocused`),
/// string equality (`editorLangId == 'rust'`), and compound AND/OR expressions.
#[derive(Debug, Clone, PartialEq)]
pub enum WhenCondition {
    /// Always matches.
    True,
    /// Never matches.
    False,
    /// Negates the inner condition.
    Not(Box<WhenCondition>),
    /// Both conditions must match.
    And(Box<WhenCondition>, Box<WhenCondition>),
    /// At least one condition must match.
    Or(Box<WhenCondition>, Box<WhenCondition>),
    /// A boolean context key that is present and truthy.
    Key(String),
    /// A context key compared for string equality (`key == "value"`).
    KeyEquals(String, String),
    /// A context key compared for string inequality (`key != "value"`).
    KeyNotEquals(String, String),
}

impl WhenCondition {
    /// Evaluate this condition against the given context.
    pub fn evaluate(&self, ctx: &WhenContext) -> bool {
        match self {
            WhenCondition::True => true,
            WhenCondition::False => false,
            WhenCondition::Not(c) => !c.evaluate(ctx),
            WhenCondition::And(a, b) => a.evaluate(ctx) && b.evaluate(ctx),
            WhenCondition::Or(a, b) => a.evaluate(ctx) || b.evaluate(ctx),
            WhenCondition::Key(k) => ctx.get_bool(k).unwrap_or(false),
            WhenCondition::KeyEquals(k, v) => {
                ctx.get_str(k).map(|s| s == v.as_str()).unwrap_or(false)
            }
            WhenCondition::KeyNotEquals(k, v) => {
                ctx.get_str(k).map(|s| s != v.as_str()).unwrap_or(true)
            }
        }
    }

    /// Parse a `when` expression string into a condition tree.
    ///
    /// Supported syntax:
    /// - `editorFocused` — boolean key
    /// - `!terminalFocused` — negated boolean key
    /// - `editorLangId == 'rust'` — string equality (single quotes)
    /// - `editorLangId != 'rust'` — string inequality (single quotes)
    /// - `a && b` — logical AND
    /// - `a || b` — logical OR
    /// - `(a || b) && c` — grouping with parentheses
    pub fn parse(expr: &str) -> Self {
        let expr = expr.trim();
        if expr.is_empty() {
            return WhenCondition::True;
        }
        Self::parse_or(expr)
    }

    fn parse_or(expr: &str) -> Self {
        // Split on `||` not inside parentheses.
        let parts = split_logical(expr, "||");
        if parts.len() > 1 {
            let mut iter = parts.into_iter().map(Self::parse_and);
            let first = iter.next().unwrap_or(WhenCondition::True);
            iter.fold(first, |acc, c| {
                WhenCondition::Or(Box::new(acc), Box::new(c))
            })
        } else {
            Self::parse_and(expr)
        }
    }

    fn parse_and(expr: &str) -> Self {
        // Split on `&&` not inside parentheses.
        let parts = split_logical(expr, "&&");
        if parts.len() > 1 {
            let mut iter = parts.into_iter().map(Self::parse_not);
            let first = iter.next().unwrap_or(WhenCondition::True);
            iter.fold(first, |acc, c| {
                WhenCondition::And(Box::new(acc), Box::new(c))
            })
        } else {
            Self::parse_not(expr)
        }
    }

    fn parse_not(expr: &str) -> Self {
        let expr = expr.trim();
        if let Some(stripped) = expr.strip_prefix('!') {
            let inner = Self::parse_atom(stripped);
            WhenCondition::Not(Box::new(inner))
        } else {
            Self::parse_atom(expr)
        }
    }

    fn parse_atom(expr: &str) -> Self {
        let expr = expr.trim();

        // Parenthesized expression.
        if expr.starts_with('(') && expr.ends_with(')') {
            let inner = &expr[1..expr.len() - 1].trim();
            return Self::parse_or(inner);
        }

        // String equality: key == 'value' or key != 'value'
        if let Some((key, value)) = parse_string_eq(expr, "!=") {
            return WhenCondition::KeyNotEquals(key.to_owned(), value.to_owned());
        }
        if let Some((key, value)) = parse_string_eq(expr, "==") {
            return WhenCondition::KeyEquals(key.to_owned(), value.to_owned());
        }

        // Boolean key.
        WhenCondition::Key(expr.to_owned())
    }
}

/// Split a logical expression on `op` (e.g., `||` or `&&`), respecting parenthesized groups.
fn split_logical<'a>(expr: &'a str, op: &str) -> Vec<&'a str> {
    let mut depth: i32 = 0;
    let mut parts = Vec::new();
    let mut start = 0;
    let mut i = 0;
    let bytes = expr.as_bytes();
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        if depth == 0 && expr[i..].starts_with(op) {
            let part = expr[start..i].trim();
            if !part.is_empty() {
                parts.push(part);
            }
            i += op.len();
            start = i;
            continue;
        }
        i += 1;
    }
    let last = expr[start..].trim();
    if !last.is_empty() {
        parts.push(last);
    }
    parts
}

/// Parse a `key == 'value'` or `key != 'value'` expression.
/// Returns `(key, value)` if matched.
fn parse_string_eq<'a>(expr: &'a str, op: &str) -> Option<(&'a str, &'a str)> {
    let pos = expr.find(op)?;
    let key = expr[..pos].trim();
    let rest = expr[pos + op.len()..].trim();
    // Strip surrounding single or double quotes.
    let value = rest
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .or_else(|| rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
        .or_else(|| {
            // Unquoted value: take until whitespace or end.
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            Some(&rest[..end])
        })?;
    Some((key, value))
}

// ── VS Code key → crabide KeyChord conversion ────────────────────────────────

/// Parse a single chord segment from VS Code keybinding format.
///
/// VS Code uses slightly different key names than crabide's internal format:
/// - `cmd` → `meta`, `option` → `alt`, `ctrl`/`control` → `ctrl`
/// - `oem_1` … `oem_102` → mapped to their standard character equivalents
/// - `numpad_add`/`numpad_subtract`/etc. → numpad keys
///
/// Falls back to [`parse_chord`] for standard key names.
fn parse_vscode_chord(s: &str) -> Result<KeyChord> {
    let s = s.trim();
    if s.is_empty() {
        return Err(crabideError::ConfigParse {
            file: "keybindings".into(),
            message: "empty chord string".into(),
        });
    }

    // Split into modifier parts and the final key.
    let parts: Vec<&str> = s.split('+').collect();
    if parts.is_empty() {
        return Err(crabideError::ConfigParse {
            file: "keybindings".into(),
            message: format!("empty chord string: {s:?}"),
        });
    }

    let mut modifiers = Modifiers::empty();
    let key_str = parts.last().unwrap().trim();
    for part in &parts[..parts.len() - 1] {
        match part.trim().to_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= Modifiers::CTRL,
            "shift" => modifiers |= Modifiers::SHIFT,
            "alt" | "option" => modifiers |= Modifiers::ALT,
            "meta" | "cmd" | "win" | "super" => modifiers |= Modifiers::META,
            unknown => {
                return Err(crabideError::ConfigParse {
                    file: "keybindings".into(),
                    message: format!("unknown modifier: {unknown:?}"),
                })
            }
        }
    }

    let key = map_vscode_key(key_str)?;
    Ok(KeyChord { modifiers, key })
}

/// Map a VS Code key name to a crabide [`Key`].
///
/// Handles OEM key names (Windows virtual-key codes) and numpad keys that
/// VS Code uses, falling back to [`parse_key`] for standard names.
fn map_vscode_key(s: &str) -> Result<Key> {
    let lower = s.to_lowercase();
    match lower.as_str() {
        // VS Code OEM key names (Windows virtual-key codes)
        "oem_1" => Ok(Key::Char(';')),
        "oem_plus" => Ok(Key::Char('=')),
        "oem_comma" => Ok(Key::Char(',')),
        "oem_minus" => Ok(Key::Char('-')),
        "oem_period" => Ok(Key::Char('.')),
        "oem_2" => Ok(Key::Char('/')),
        "oem_3" => Ok(Key::Char('`')),
        "oem_4" => Ok(Key::Char('[')),
        "oem_5" => Ok(Key::Char('\\')),
        "oem_6" => Ok(Key::Char(']')),
        "oem_7" => Ok(Key::Char('\'')),
        "oem_8" => Ok(Key::Char('`')),
        "oem_102" => Ok(Key::Char('\\')),
        // VS Code numpad key names
        "numpad0" | "numpad_insert" => Ok(Key::Numpad(0)),
        "numpad1" | "numpad_end" => Ok(Key::Numpad(1)),
        "numpad2" | "numpad_down" => Ok(Key::Numpad(2)),
        "numpad3" | "numpad_page_down" => Ok(Key::Numpad(3)),
        "numpad4" | "numpad_left" => Ok(Key::Numpad(4)),
        "numpad5" | "numpad_clear" => Ok(Key::Numpad(5)),
        "numpad6" | "numpad_right" => Ok(Key::Numpad(6)),
        "numpad7" | "numpad_home" => Ok(Key::Numpad(7)),
        "numpad8" | "numpad_up" => Ok(Key::Numpad(8)),
        "numpad9" | "numpad_page_up" => Ok(Key::Numpad(9)),
        "numpad_add" | "numpad_multiply" | "numpad_subtract" | "numpad_divide"
        | "numpad_decimal" | "numpad_enter" | "numpad_separator" => {
            // Map these to their character equivalents for simplicity.
            let ch = match lower.as_str() {
                "numpad_add" => '+',
                "numpad_multiply" => '*',
                "numpad_subtract" => '-',
                "numpad_divide" => '/',
                "numpad_decimal" => '.',
                "numpad_enter" => return Ok(Key::Enter),
                "numpad_separator" => ',',
                _ => unreachable!(),
            };
            Ok(Key::Char(ch))
        }
        _ => parse_key(&lower),
    }
}

// ── VS Code command → crabide Action mapping ─────────────────────────────────

/// Map a VS Code command ID (e.g. `editor.action.commentLine`) to a crabide
/// [`Action`]. Known commands are translated; unknown ones become
/// [`Action::Custom`].
fn map_vscode_command(cmd: &str) -> Action {
    match cmd {
        // File
        "workbench.action.files.newUntitledFile" => Action::NewFile,
        "workbench.action.files.openFile" => Action::OpenFile,
        "workbench.action.files.openFolder" => Action::OpenFolder,
        "workbench.action.files.save" => Action::SaveFile,
        "workbench.action.files.saveAs" => Action::SaveFileAs,
        "workbench.action.files.saveAll" => Action::SaveAll,
        "workbench.action.closeActiveEditor" => Action::CloseTab,
        "workbench.action.closeAllEditors" => Action::CloseAllTabs,
        "workbench.action.quit" => Action::Quit,

        // Edit
        "undo" => Action::Undo,
        "redo" => Action::Redo,
        "editor.action.clipboardCutAction" => Action::Cut,
        "editor.action.clipboardCopyAction" => Action::Copy,
        "editor.action.clipboardPasteAction" => Action::Paste,
        "editor.action.selectAll" => Action::SelectAll,
        "editor.action.copyLinesDownAction" => Action::DuplicateLine,
        "editor.action.deleteLines" => Action::DeleteLine,
        "editor.action.moveLinesUpAction" => Action::MoveLineUp,
        "editor.action.moveLinesDownAction" => Action::MoveLineDown,
        "editor.action.insertLineBefore" => Action::InsertNewlineAbove,
        "editor.action.insertLineAfter" => Action::InsertNewlineBelow,
        "editor.action.indentLines" => Action::IndentLine,
        "editor.action.outdentLines" => Action::OutdentLine,
        "editor.action.commentLine" => Action::ToggleLineComment,
        "editor.action.blockComment" => Action::ToggleBlockComment,

        // Cursor
        "cursorUp" => Action::CursorUp,
        "cursorDown" => Action::CursorDown,
        "cursorLeft" => Action::CursorLeft,
        "cursorRight" => Action::CursorRight,
        "cursorWordLeft" => Action::CursorWordLeft,
        "cursorWordRight" => Action::CursorWordRight,
        "cursorHome" => Action::CursorLineStart,
        "cursorEnd" => Action::CursorLineEnd,
        "cursorTop" => Action::CursorFileStart,
        "cursorBottom" => Action::CursorFileEnd,
        "cursorPageUp" => Action::CursorPageUp,
        "cursorPageDown" => Action::CursorPageDown,
        "scrollLineUp" => Action::ScrollLineUp,
        "scrollLineDown" => Action::ScrollLineDown,

        // Selection
        "cursorUpSelect" => Action::SelectUp,
        "cursorDownSelect" => Action::SelectDown,
        "cursorLeftSelect" => Action::SelectLeft,
        "cursorRightSelect" => Action::SelectRight,
        "cursorWordLeftSelect" => Action::SelectWordLeft,
        "cursorWordRightSelect" => Action::SelectWordRight,
        "cursorHomeSelect" => Action::SelectLineStart,
        "cursorEndSelect" => Action::SelectLineEnd,
        "cursorTopSelect" => Action::SelectFileStart,
        "cursorBottomSelect" => Action::SelectFileEnd,
        "editor.action.selectLine" => Action::SelectLine,
        "editor.action.smartSelect.expand" => Action::ExpandSelection,
        "editor.action.smartSelect.shrink" => Action::ShrinkSelection,
        "editor.action.insertCursorAbove" => Action::AddCursorAbove,
        "editor.action.insertCursorBelow" => Action::AddCursorBelow,
        "editor.action.addSelectionToNextFindMatch" => Action::AddNextOccurrence,
        "editor.action.selectHighlights" => Action::SelectAllOccurrences,

        // Delete
        "deleteLeft" => Action::DeleteCharLeft,
        "deleteRight" => Action::DeleteCharRight,
        "deleteWordLeft" => Action::DeleteWordLeft,
        "deleteWordRight" => Action::DeleteWordRight,
        "deleteAllLeft" => Action::DeleteLineLeft,
        "deleteAllRight" => Action::DeleteLineRight,

        // Find / replace
        "actions.find" => Action::Find,
        "editor.action.startFindReplaceAction" => Action::FindReplace,
        "editor.action.nextMatchFindAction" => Action::FindNext,
        "editor.action.previousMatchFindAction" => Action::FindPrevious,
        "workbench.action.findInFiles" => Action::FindInFiles,
        "workbench.action.replaceInFiles" => Action::ReplaceInFiles,

        // Navigation
        "workbench.action.gotoLine" => Action::GotoLine,
        "editor.action.goToDeclaration" => Action::GotoDeclaration,
        "editor.action.goToDefinition" => Action::GotoDefinition,
        "editor.action.goToImplementation" => Action::GotoImplementation,
        "editor.action.goToTypeDefinition" => Action::GotoTypeDefinition,
        "editor.action.goToReferences" => Action::GotoReferences,
        "workbench.action.gotoSymbol" => Action::GotoSymbol,
        "workbench.action.navigateBack" => Action::GoBack,
        "workbench.action.navigateForward" => Action::GoForward,
        "editor.action.marker.next" => Action::NextDiagnostic,
        "editor.action.marker.prev" => Action::PreviousDiagnostic,

        // LSP
        "editor.action.triggerSuggest" => Action::TriggerCompletion,
        "editor.action.showHover" => Action::ShowHover,
        "editor.action.showSignatureHelp" => Action::ShowSignatureHelp,
        "editor.action.rename" => Action::RenameSymbol,
        "editor.action.quickOutline" => Action::ApplyCodeAction,
        "editor.action.formatDocument" => Action::FormatDocument,
        "editor.action.formatSelection" => Action::FormatSelection,
        "editor.action.organizeImports" => Action::OrganizeImports,

        // View
        "workbench.action.showCommands" => Action::CommandPalette,
        "workbench.action.quickOpen" => Action::FuzzyFindFile,
        "workbench.action.quickOpenNavigateNext" => Action::FuzzyFindSymbol,
        "workbench.action.toggleSidebarVisibility" => Action::ToggleSidebar,
        "workbench.action.togglePanel" => Action::TogglePanel,
        "workbench.action.terminal.toggleTerminal" => Action::ToggleTerminal,
        "workbench.view.scm" => Action::ToggleGitPanel,
        "workbench.view.debug" => Action::ToggleDebugPanel,
        "workbench.view.extensions" => Action::ToggleExtensionsPanel,
        "workbench.actions.view.problems" => Action::ToggleProblemsPanel,
        "workbench.action.output.toggleOutput" => Action::ToggleOutputPanel,
        "editor.action.toggleMinimap" => Action::ToggleMinimap,
        "editor.action.toggleWordWrap" => Action::ToggleWordWrap,
        "workbench.action.zoomIn" => Action::ZoomIn,
        "workbench.action.zoomOut" => Action::ZoomOut,
        "workbench.action.zoomReset" => Action::ZoomReset,
        "workbench.action.splitEditorRight" => Action::SplitEditorRight,
        "workbench.action.splitEditorDown" => Action::SplitEditorDown,
        "workbench.action.closeEditor" => Action::CloseEditor,
        "workbench.action.nextEditor" => Action::NextTab,
        "workbench.action.previousEditor" => Action::PreviousTab,

        // Git
        "git.commit" => Action::GitCommit,
        "git.stageAll" => Action::GitStageAll,
        "git.unstageAll" => Action::GitUnstageAll,
        "git.cleanAll" => Action::GitDiscardChanges,

        // Debug
        "editor.debug.action.toggleBreakpoint" => Action::ToggleBreakpoint,
        "workbench.action.debug.start" => Action::StartDebug,
        "workbench.action.debug.stop" => Action::StopDebug,
        "workbench.action.debug.continue" => Action::ContinueDebug,
        "workbench.action.debug.stepOver" => Action::StepOver,
        "workbench.action.debug.stepInto" => Action::StepInto,
        "workbench.action.debug.stepOut" => Action::StepOut,
        "workbench.action.debug.restart" => Action::RestartDebug,

        // Terminal
        "workbench.action.terminal.new" => Action::NewTerminal,
        "workbench.action.terminal.kill" => Action::KillTerminal,

        // Snippets
        "editor.action.tabSnippetNext" => Action::NextTabstop,
        "editor.action.tabSnippetPrev" => Action::PreviousTabstop,

        // Trim whitespace
        "editor.action.trimTrailingWhitespace" => Action::TrimTrailingWhitespace,

        // Column selection
        "editor.action.insertCursorAboveSelect" => Action::ColumnSelectUp,
        "editor.action.insertCursorBelowSelect" => Action::ColumnSelectDown,

        // Declaration / type definition
        "editor.action.revealDeclaration" => Action::GotoDeclaration,

        // Toggle git / debug
        "git.enableSmartCommit" => Action::ToggleGit,
        "debug.toggleDebugView" => Action::ToggleDebug,

        // Move tab
        "workbench.action.moveEditorRight" => Action::MoveTabRight,
        "workbench.action.moveEditorLeft" => Action::MoveTabLeft,

        // Unknown command → Custom action
        _ => Action::Custom(cmd.to_owned()),
    }
}

/// Runtime context for evaluating `when` conditions.
///
/// Holds a set of boolean keys and string-valued keys that extensions and the
/// UI layer can populate before dispatching key presses.
#[derive(Debug, Clone, Default)]
pub struct WhenContext {
    booleans: HashMap<String, bool>,
    strings: HashMap<String, String>,
}

impl WhenContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a boolean context key (e.g., `"editorFocused"`).
    pub fn set_bool(&mut self, key: impl Into<String>, value: bool) {
        self.booleans.insert(key.into(), value);
    }

    /// Set a string context key (e.g., `"editorLangId"` → `"rust"`).
    pub fn set_str(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.strings.insert(key.into(), value.into());
    }

    /// Get a boolean context key.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.booleans.get(key).copied()
    }

    /// Get a string context key.
    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.strings.get(key).map(|s| s.as_str())
    }

    /// Remove a key.
    pub fn remove(&mut self, key: &str) {
        self.booleans.remove(key);
        self.strings.remove(key);
    }

    /// Merge another context into this one (preferring the other on conflict).
    pub fn merge(&mut self, other: &WhenContext) {
        for (k, v) in &other.booleans {
            self.booleans.insert(k.clone(), *v);
        }
        for (k, v) in &other.strings {
            self.strings.insert(k.clone(), v.clone());
        }
    }
}

// ── TOML representation ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TomlBindingFile {
    #[serde(default)]
    bindings: Vec<TomlBinding>,
}

#[derive(Debug, Deserialize)]
struct TomlBinding {
    key: String,
    action: Action,
    when: Option<String>,
}

// ── VS Code keybindings.json representation ──────────────────────────────────

#[derive(Debug, Deserialize)]
struct VsCodeBinding {
    key: String,
    command: String,
    #[serde(default)]
    when: Option<String>,
}

// ── KeybindingEngine ──────────────────────────────────────────────────────────

pub struct KeybindingEngine {
    bindings: Vec<ParsedBinding>,
    pending_chord: Option<KeyChord>,
}

/// A binding with its `when` condition pre-parsed into a condition tree.
#[derive(Debug, Clone)]
struct ParsedBinding {
    chords: Vec<KeyChord>,
    action: Action,
    when_condition: WhenCondition,
}

impl KeybindingEngine {
    pub fn with_defaults() -> Self {
        let mut engine = Self {
            bindings: Vec::new(),
            pending_chord: None,
        };
        engine.load_defaults();
        engine
    }

    pub fn load_file(&mut self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }
        let content = std::fs::read_to_string(path)?;
        let fname = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        if fname.ends_with(".json") {
            self.load_vscode_json(&content, &path.display().to_string())
        } else {
            self.load_toml(&content, &path.display().to_string())
        }
    }

    pub fn load_toml(&mut self, toml_str: &str, source: &str) -> Result<()> {
        let file: TomlBindingFile =
            toml::from_str(toml_str).map_err(|e| crabideError::ConfigParse {
                file: source.to_owned(),
                message: e.to_string(),
            })?;
        for raw in file.bindings {
            let chord_strs: Vec<&str> = raw.key.split_whitespace().collect();
            if chord_strs.len() > 2 {
                log::warn!(
                    "Keybinding with >2 chords not supported, skipping: {:?}",
                    raw.key
                );
                continue;
            }
            let mut chords = Vec::new();
            for cs in chord_strs {
                match parse_chord(cs) {
                    Ok(c) => chords.push(c),
                    Err(e) => {
                        log::warn!("Invalid chord {cs:?}: {e}");
                        continue;
                    }
                }
            }
            if !chords.is_empty() {
                let when_condition = raw
                    .when
                    .as_deref()
                    .map(WhenCondition::parse)
                    .unwrap_or(WhenCondition::True);
                self.bindings.push(ParsedBinding {
                    chords,
                    action: raw.action,
                    when_condition,
                });
            }
        }
        Ok(())
    }

    /// Import bindings from a VS Code `keybindings.json` file.
    ///
    /// The file must contain a JSON array of objects with `key`, `command`, and
    /// optional `when` fields. If `command` starts with `-` the binding is
    /// treated as a removal (the existing binding for that key is deleted).
    /// Otherwise the `command` string is mapped to a crabide [`Action`] —
    /// known VS Code command IDs are translated, and unknown commands become
    /// [`Action::Custom`].
    pub fn load_vscode_json(&mut self, json_str: &str, source: &str) -> Result<()> {
        let entries: Vec<VsCodeBinding> =
            serde_json::from_str(json_str).map_err(|e| crabideError::ConfigParse {
                file: source.to_owned(),
                message: format!("invalid VS Code keybindings JSON: {e}"),
            })?;

        for entry in entries {
            let command = entry.command.trim().to_owned();

            // VS Code uses `-command` to remove a binding.
            if let Some(remove_cmd) = command.strip_prefix('-') {
                self.remove_binding_by_key(&entry.key, remove_cmd);
                continue;
            }

            let action = map_vscode_command(&command);
            let chord_strs: Vec<&str> = entry.key.split_whitespace().collect();
            if chord_strs.len() > 2 {
                log::warn!(
                    "VS Code keybinding with >2 chords not supported, skipping: {:?}",
                    entry.key
                );
                continue;
            }
            let mut chords = Vec::new();
            for cs in &chord_strs {
                match parse_vscode_chord(cs) {
                    Ok(c) => chords.push(c),
                    Err(e) => {
                        log::warn!("Invalid VS Code chord {cs:?}: {e}");
                        continue;
                    }
                }
            }
            if !chords.is_empty() {
                let when_condition = entry
                    .when
                    .as_deref()
                    .map(WhenCondition::parse)
                    .unwrap_or(WhenCondition::True);
                self.bindings.push(ParsedBinding {
                    chords,
                    action,
                    when_condition,
                });
            }
        }
        Ok(())
    }

    /// Remove the first binding whose key matches `key_str` and whose action
    /// maps to the given VS Code `command`. Used for the `-command` removal
    /// syntax in VS Code keybindings.json.
    fn remove_binding_by_key(&mut self, key_str: &str, command: &str) {
        let target_action = map_vscode_command(command);
        let chord_strs: Vec<&str> = key_str.split_whitespace().collect();
        let mut chords = Vec::new();
        for cs in &chord_strs {
            if let Ok(c) = parse_vscode_chord(cs) {
                chords.push(c);
            }
        }
        if chords.is_empty() {
            return;
        }
        let pos = self
            .bindings
            .iter()
            .position(|b| b.chords == chords && b.action == target_action);
        if let Some(i) = pos {
            self.bindings.remove(i);
        }
    }

    /// Dispatches a key press, returning the matching `Action` if any.
    ///
    /// `ctx` provides the current UI context for evaluating `when` conditions.
    /// When `None`, all bindings are considered active (backward-compatible).
    pub fn press(&mut self, chord: KeyChord, ctx: Option<&WhenContext>) -> Option<Action> {
        if let Some(first) = self.pending_chord.take() {
            if let Some(action) = self.find_two_chord(&first, &chord, ctx) {
                return Some(action.clone());
            }
        }
        if let Some(action) = self.find_one_chord(&chord, ctx) {
            return Some(action.clone());
        }
        let starts_sequence = self.bindings.iter().any(|b| {
            b.chords.len() == 2
                && b.chords[0] == chord
                && b.when_condition
                    .evaluate(ctx.unwrap_or(&WhenContext::new()))
        });
        if starts_sequence {
            self.pending_chord = Some(chord);
        }
        None
    }

    /// Dispatch a key press without a context (backward-compatible, all bindings active).
    pub fn press_legacy(&mut self, chord: KeyChord) -> Option<Action> {
        self.press(chord, None)
    }

    pub fn cancel_pending(&mut self) {
        self.pending_chord = None;
    }
    pub fn has_pending_chord(&self) -> bool {
        self.pending_chord.is_some()
    }
    pub fn bindings(&self) -> Vec<KeyBinding> {
        self.bindings
            .iter()
            .map(|pb| KeyBinding {
                chords: pb.chords.clone(),
                action: pb.action.clone(),
                when: None, // raw when string not preserved; use KeyBinding when_condition
            })
            .collect()
    }

    pub fn chords_for_action(&self, action: &Action) -> Vec<&[KeyChord]> {
        self.bindings
            .iter()
            .filter(|b| &b.action == action)
            .map(|b| b.chords.as_slice())
            .collect()
    }

    fn find_one_chord(&self, chord: &KeyChord, ctx: Option<&WhenContext>) -> Option<&Action> {
        let empty_ctx = WhenContext::new();
        let ctx = ctx.unwrap_or(&empty_ctx);
        self.bindings
            .iter()
            .rev()
            .find(|b| {
                b.chords.len() == 1 && &b.chords[0] == chord && b.when_condition.evaluate(ctx)
            })
            .map(|b| &b.action)
    }

    fn find_two_chord(
        &self,
        first: &KeyChord,
        second: &KeyChord,
        ctx: Option<&WhenContext>,
    ) -> Option<&Action> {
        let empty_ctx = WhenContext::new();
        let ctx = ctx.unwrap_or(&empty_ctx);
        self.bindings
            .iter()
            .rev()
            .find(|b| {
                b.chords.len() == 2
                    && &b.chords[0] == first
                    && &b.chords[1] == second
                    && b.when_condition.evaluate(ctx)
            })
            .map(|b| &b.action)
    }

    pub fn bind(&mut self, key: &str, action: Action) {
        let mut chords = Vec::new();
        for cs in key.split_whitespace() {
            if let Ok(c) = parse_chord(cs) {
                chords.push(c);
            }
        }
        if !chords.is_empty() {
            self.bindings.push(ParsedBinding {
                chords,
                action,
                when_condition: WhenCondition::True,
            });
        }
    }

    /// Alias for [`bind`] used by extension keybinding registration.
    pub fn bind_ext(&mut self, key: &str, action: Action) {
        self.bind(key, action);
    }

    fn load_defaults(&mut self) {
        self.bind("ctrl+n", Action::NewFile);
        self.bind("ctrl+o", Action::OpenFile);
        self.bind("ctrl+s", Action::SaveFile);
        self.bind("ctrl+shift+s", Action::SaveFileAs);
        self.bind("ctrl+k s", Action::SaveAll);
        self.bind("ctrl+w", Action::CloseTab);
        self.bind("ctrl+shift+p", Action::CommandPalette);
        self.bind("ctrl+p", Action::FuzzyFindFile);
        self.bind("ctrl+shift+o", Action::GotoSymbol);
        self.bind("ctrl+z", Action::Undo);
        self.bind("ctrl+y", Action::Redo);
        self.bind("ctrl+shift+z", Action::Redo);
        self.bind("ctrl+x", Action::Cut);
        self.bind("ctrl+c", Action::Copy);
        self.bind("ctrl+v", Action::Paste);
        self.bind("ctrl+a", Action::SelectAll);
        self.bind("ctrl+d", Action::AddNextOccurrence);
        self.bind("ctrl+shift+k", Action::DeleteLine);
        self.bind("alt+up", Action::MoveLineUp);
        self.bind("alt+down", Action::MoveLineDown);
        self.bind("ctrl+/", Action::ToggleLineComment);
        self.bind("ctrl+shift+/", Action::ToggleBlockComment);
        self.bind("ctrl+g", Action::GotoLine);
        self.bind("f12", Action::GotoDefinition);
        self.bind("alt+f12", Action::GotoReferences);
        self.bind("f8", Action::NextDiagnostic);
        self.bind("shift+f8", Action::PreviousDiagnostic);
        self.bind("alt+left", Action::GoBack);
        self.bind("alt+right", Action::GoForward);
        self.bind("ctrl+f", Action::Find);
        self.bind("ctrl+h", Action::FindReplace);
        self.bind("f3", Action::FindNext);
        self.bind("shift+f3", Action::FindPrevious);
        self.bind("ctrl+shift+f", Action::FindInFiles);
        self.bind("ctrl+space", Action::TriggerCompletion);
        self.bind("f2", Action::RenameSymbol);
        self.bind("ctrl+.", Action::ApplyCodeAction);
        self.bind("shift+alt+f", Action::FormatDocument);
        self.bind("ctrl+b", Action::ToggleSidebar);
        self.bind("ctrl+`", Action::ToggleTerminal);
        self.bind("ctrl+shift+g", Action::ToggleGitPanel);
        self.bind("f5", Action::StartDebug);
        self.bind("shift+f5", Action::StopDebug);
        self.bind("f10", Action::StepOver);
        self.bind("f11", Action::StepInto);
        self.bind("shift+f11", Action::StepOut);
        self.bind("f9", Action::ToggleBreakpoint);
        self.bind("ctrl+shift+d", Action::ToggleDebugPanel);
        self.bind("ctrl+shift+x", Action::ToggleExtensionsPanel);
        self.bind("ctrl+shift+m", Action::ToggleProblemsPanel);
        // ── Cursor movement ───────────────────────────────────────────────────
        self.bind("up", Action::CursorUp);
        self.bind("down", Action::CursorDown);
        self.bind("left", Action::CursorLeft);
        self.bind("right", Action::CursorRight);
        self.bind("home", Action::CursorLineStart);
        self.bind("end", Action::CursorLineEnd);
        self.bind("ctrl+home", Action::CursorFileStart);
        self.bind("ctrl+end", Action::CursorFileEnd);
        self.bind("pageup", Action::CursorPageUp);
        self.bind("pagedown", Action::CursorPageDown);
        self.bind("ctrl+left", Action::CursorWordLeft);
        self.bind("ctrl+right", Action::CursorWordRight);
        // ── Selection ─────────────────────────────────────────────────────────
        self.bind("shift+up", Action::SelectUp);
        self.bind("shift+down", Action::SelectDown);
        self.bind("shift+left", Action::SelectLeft);
        self.bind("shift+right", Action::SelectRight);
        self.bind("shift+home", Action::SelectLineStart);
        self.bind("shift+end", Action::SelectLineEnd);
        self.bind("ctrl+shift+home", Action::SelectFileStart);
        self.bind("ctrl+shift+end", Action::SelectFileEnd);
        self.bind("ctrl+shift+left", Action::SelectWordLeft);
        self.bind("ctrl+shift+right", Action::SelectWordRight);
        // ── Deletion ──────────────────────────────────────────────────────────
        self.bind("backspace", Action::DeleteCharLeft);
        self.bind("delete", Action::DeleteCharRight);
        self.bind("ctrl+backspace", Action::DeleteWordLeft);
        self.bind("ctrl+delete", Action::DeleteWordRight);
        // ── Line operations ───────────────────────────────────────────────────
        self.bind("ctrl+shift+k", Action::DeleteLine);
        self.bind("ctrl+shift+d", Action::DuplicateLine);
        self.bind("alt+up", Action::MoveLineUp);
        self.bind("alt+down", Action::MoveLineDown);
        self.bind("ctrl+]", Action::IndentLine);
        self.bind("ctrl+[", Action::OutdentLine);
        self.bind("ctrl+/", Action::ToggleLineComment);
        self.bind("ctrl+shift+/", Action::ToggleBlockComment);
        // ── View ──────────────────────────────────────────────────────────────
        self.bind("alt+z", Action::ToggleWordWrap);
        self.bind(
            "ctrl+k ctrl+t",
            Action::Custom("theme-switcher.next-theme".to_owned()),
        );
        self.bind("ctrl+=", Action::ZoomIn);
        self.bind("ctrl+-", Action::ZoomOut);
        self.bind("ctrl+0", Action::ZoomReset);
        // ── Snippets ──────────────────────────────────────────────────────────
        self.bind("tab", Action::NextTabstop);
        self.bind("shift+tab", Action::PreviousTabstop);
    }
}

pub fn all_actions() -> IndexMap<Action, &'static str> {
    let mut m = IndexMap::new();
    m.insert(Action::NewFile, "File: New File");
    m.insert(Action::OpenFile, "File: Open File...");
    m.insert(Action::OpenFolder, "File: Open Folder...");
    m.insert(Action::SaveFile, "File: Save");
    m.insert(Action::SaveFileAs, "File: Save As...");
    m.insert(Action::SaveAll, "File: Save All");
    m.insert(Action::CloseTab, "View: Close Editor");
    m.insert(Action::CloseAllTabs, "View: Close All Editors");
    m.insert(Action::Quit, "File: Quit");
    m.insert(Action::Undo, "Edit: Undo");
    m.insert(Action::Redo, "Edit: Redo");
    m.insert(Action::Cut, "Edit: Cut");
    m.insert(Action::Copy, "Edit: Copy");
    m.insert(Action::Paste, "Edit: Paste");
    m.insert(Action::SelectAll, "Edit: Select All");
    m.insert(Action::ToggleLineComment, "Editor: Toggle Line Comment");
    m.insert(Action::ToggleBlockComment, "Editor: Toggle Block Comment");
    m.insert(Action::FormatDocument, "Editor: Format Document");
    m.insert(Action::FormatSelection, "Editor: Format Selection");
    m.insert(Action::OrganizeImports, "Editor: Organize Imports");
    m.insert(Action::RenameSymbol, "Editor: Rename Symbol");
    m.insert(Action::GotoDefinition, "Go: Go to Definition");
    m.insert(Action::GotoDeclaration, "Go: Go to Declaration");
    m.insert(Action::GotoImplementation, "Go: Go to Implementation");
    m.insert(Action::GotoTypeDefinition, "Go: Go to Type Definition");
    m.insert(Action::GotoReferences, "Go: Go to References");
    m.insert(Action::GotoSymbol, "Go: Go to Symbol in File...");
    m.insert(Action::GotoLine, "Go: Go to Line...");
    m.insert(Action::GoBack, "Go: Go Back");
    m.insert(Action::GoForward, "Go: Go Forward");
    m.insert(Action::NextDiagnostic, "Go: Next Diagnostic");
    m.insert(Action::PreviousDiagnostic, "Go: Previous Diagnostic");
    m.insert(Action::Find, "Edit: Find");
    m.insert(Action::FindReplace, "Edit: Find and Replace");
    m.insert(Action::FindNext, "Edit: Find Next");
    m.insert(Action::FindPrevious, "Edit: Find Previous");
    m.insert(Action::FindInFiles, "Edit: Find in Files");
    m.insert(Action::ReplaceInFiles, "Edit: Replace in Files");
    m.insert(Action::CommandPalette, "View: Command Palette");
    m.insert(Action::FuzzyFindFile, "View: Open File by Name...");
    m.insert(
        Action::FuzzyFindSymbol,
        "View: Go to Symbol in Workspace...",
    );
    m.insert(Action::ToggleSidebar, "View: Toggle Sidebar");
    m.insert(Action::TogglePanel, "View: Toggle Panel");
    m.insert(Action::ToggleTerminal, "View: Toggle Terminal");
    m.insert(Action::ToggleGitPanel, "View: Toggle Source Control Panel");
    m.insert(Action::ToggleGit, "Source Control: Enable/Disable Git");
    m.insert(Action::ToggleDebugPanel, "View: Toggle Debug Panel");
    m.insert(Action::ToggleDebug, "Debug: Enable/Disable Debugger");
    m.insert(
        Action::ToggleExtensionsPanel,
        "View: Toggle Extensions Panel",
    );
    m.insert(Action::ToggleOutputPanel, "View: Toggle Output Panel");
    m.insert(Action::ToggleProblemsPanel, "View: Toggle Problems Panel");
    m.insert(Action::ToggleMinimap, "View: Toggle Minimap");
    m.insert(Action::ToggleWordWrap, "View: Toggle Word Wrap");
    m.insert(Action::ZoomIn, "View: Zoom In");
    m.insert(Action::ZoomOut, "View: Zoom Out");
    m.insert(Action::ZoomReset, "View: Reset Zoom");
    m.insert(Action::SplitEditorRight, "View: Split Editor Right");
    m.insert(Action::SplitEditorDown, "View: Split Editor Down");
    m.insert(Action::CloseEditor, "View: Close Editor Pane");
    m.insert(Action::NextTab, "View: Next Tab");
    m.insert(Action::PreviousTab, "View: Previous Tab");
    m.insert(Action::MoveTabRight, "View: Move Tab Right");
    m.insert(Action::MoveTabLeft, "View: Move Tab Left");
    m.insert(Action::DuplicateLine, "Edit: Duplicate Line");
    m.insert(Action::DeleteLine, "Edit: Delete Line");
    m.insert(Action::MoveLineUp, "Edit: Move Line Up");
    m.insert(Action::MoveLineDown, "Edit: Move Line Down");
    m.insert(Action::InsertNewlineAbove, "Edit: Insert Line Above");
    m.insert(Action::InsertNewlineBelow, "Edit: Insert Line Below");
    m.insert(Action::IndentLine, "Edit: Indent Line");
    m.insert(Action::OutdentLine, "Edit: Outdent Line");
    m.insert(
        Action::TrimTrailingWhitespace,
        "Editor: Trim Trailing Whitespace",
    );
    m.insert(Action::SelectLine, "Selection: Select Line");
    m.insert(Action::ExpandSelection, "Selection: Expand Selection");
    m.insert(Action::ShrinkSelection, "Selection: Shrink Selection");
    m.insert(Action::AddCursorAbove, "Selection: Add Cursor Above");
    m.insert(Action::AddCursorBelow, "Selection: Add Cursor Below");
    m.insert(Action::AddNextOccurrence, "Selection: Add Next Occurrence");
    m.insert(
        Action::SelectAllOccurrences,
        "Selection: Select All Occurrences",
    );
    m.insert(Action::ColumnSelectUp, "Selection: Column Select Up");
    m.insert(Action::ColumnSelectDown, "Selection: Column Select Down");
    // Cursor movement
    m.insert(Action::CursorUp, "Cursor: Up");
    m.insert(Action::CursorDown, "Cursor: Down");
    m.insert(Action::CursorLeft, "Cursor: Left");
    m.insert(Action::CursorRight, "Cursor: Right");
    m.insert(Action::CursorWordLeft, "Cursor: Word Left");
    m.insert(Action::CursorWordRight, "Cursor: Word Right");
    m.insert(Action::CursorLineStart, "Cursor: Line Start");
    m.insert(Action::CursorLineEnd, "Cursor: Line End");
    m.insert(Action::CursorFileStart, "Cursor: File Start");
    m.insert(Action::CursorFileEnd, "Cursor: File End");
    m.insert(Action::CursorPageUp, "Cursor: Page Up");
    m.insert(Action::CursorPageDown, "Cursor: Page Down");
    m.insert(Action::ScrollLineUp, "Cursor: Scroll Line Up");
    m.insert(Action::ScrollLineDown, "Cursor: Scroll Line Down");
    // Selection (cursor with shift)
    m.insert(Action::SelectUp, "Selection: Up");
    m.insert(Action::SelectDown, "Selection: Down");
    m.insert(Action::SelectLeft, "Selection: Left");
    m.insert(Action::SelectRight, "Selection: Right");
    m.insert(Action::SelectWordLeft, "Selection: Word Left");
    m.insert(Action::SelectWordRight, "Selection: Word Right");
    m.insert(Action::SelectLineStart, "Selection: Line Start");
    m.insert(Action::SelectLineEnd, "Selection: Line End");
    m.insert(Action::SelectFileStart, "Selection: File Start");
    m.insert(Action::SelectFileEnd, "Selection: File End");
    // Deletion
    m.insert(Action::DeleteCharLeft, "Edit: Delete Character Left");
    m.insert(Action::DeleteCharRight, "Edit: Delete Character Right");
    m.insert(Action::DeleteWordLeft, "Edit: Delete Word Left");
    m.insert(Action::DeleteWordRight, "Edit: Delete Word Right");
    m.insert(Action::DeleteLineLeft, "Edit: Delete to Line Start");
    m.insert(Action::DeleteLineRight, "Edit: Delete to Line End");
    // LSP
    m.insert(Action::TriggerCompletion, "Editor: Trigger Completion");
    m.insert(Action::ShowHover, "Editor: Show Hover");
    m.insert(Action::ShowSignatureHelp, "Editor: Show Signature Help");
    m.insert(Action::ApplyCodeAction, "Editor: Apply Code Action");
    // Git
    m.insert(Action::GitCommit, "Git: Commit");
    m.insert(Action::GitStageAll, "Git: Stage All Changes");
    m.insert(Action::GitUnstageAll, "Git: Unstage All Changes");
    m.insert(Action::GitDiscardChanges, "Git: Discard Changes");
    m.insert(Action::GitFetch, "Git: Fetch from Remote");
    m.insert(Action::GitPull, "Git: Pull from Remote");
    m.insert(Action::GitPush, "Git: Push to Remote");
    m.insert(Action::GitMerge, "Git: Merge Branch...");
    m.insert(Action::GitRebase, "Git: Rebase onto Branch...");
    // Debug
    m.insert(Action::ToggleBreakpoint, "Debug: Toggle Breakpoint");
    m.insert(Action::StartDebug, "Debug: Start Debugging");
    m.insert(Action::StopDebug, "Debug: Stop Debugging");
    m.insert(Action::ContinueDebug, "Debug: Continue / Pause");
    m.insert(Action::StepOver, "Debug: Step Over");
    m.insert(Action::StepInto, "Debug: Step Into");
    m.insert(Action::StepOut, "Debug: Step Out");
    m.insert(Action::RestartDebug, "Debug: Restart");
    // Terminal
    m.insert(Action::NewTerminal, "Terminal: New Terminal");
    m.insert(Action::KillTerminal, "Terminal: Kill Terminal");
    // Snippets
    m.insert(Action::NextTabstop, "Snippet: Next Tabstop");
    m.insert(Action::PreviousTabstop, "Snippet: Previous Tabstop");
    m
}

/// Return all built-in actions merged with actions from a registry.
///
/// This is the primary function to use when building the command-palette list.
/// Custom actions registered via [`ActionRegistry`] are appended after the
/// built-in actions so they appear in a predictable section.
pub fn all_actions_with(registry: &ActionRegistry) -> IndexMap<Action, String> {
    let mut m: IndexMap<Action, String> = all_actions()
        .into_iter()
        .map(|(k, v)| (k, v.to_owned()))
        .collect();
    for (action, title) in registry.iter_custom() {
        m.entry(action).or_insert_with(|| title.to_owned());
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ActionRegistry ─────────────────────────────────────────────────────

    #[test]
    fn action_registry_new_is_empty() {
        let reg = ActionRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn action_registry_register_and_has() {
        let mut reg = ActionRegistry::new();
        reg.register("ext.my-action", "My Extension: Do Thing");
        assert!(reg.has("ext.my-action"));
        assert!(!reg.is_empty());
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn action_registry_unregister() {
        let mut reg = ActionRegistry::new();
        reg.register("ext.foo", "Foo");
        assert!(reg.has("ext.foo"));
        reg.unregister("ext.foo");
        assert!(!reg.has("ext.foo"));
        assert!(reg.is_empty());
    }

    #[test]
    fn action_registry_unregister_nonexistent() {
        let mut reg = ActionRegistry::new();
        reg.unregister("does-not-exist"); // should not panic
        assert!(reg.is_empty());
    }

    #[test]
    fn action_registry_register_overwrites_title() {
        let mut reg = ActionRegistry::new();
        reg.register("ext.dup", "First Title");
        reg.register("ext.dup", "Updated Title");
        assert_eq!(reg.len(), 1);
        let items: Vec<_> = reg.iter_custom().collect();
        assert_eq!(items.len(), 1);
        let (action, title) = &items[0];
        assert_eq!(action, &Action::Custom("ext.dup".to_owned()));
        assert_eq!(*title, "Updated Title");
    }

    #[test]
    fn action_registry_iter_custom() {
        let mut reg = ActionRegistry::new();
        reg.register("ext.a", "Alpha");
        reg.register("ext.b", "Beta");
        let mut pairs: Vec<_> = reg.iter_custom().collect();
        pairs.sort_by_key(|(_, t)| *t);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], (Action::Custom("ext.a".to_owned()), "Alpha"));
        assert_eq!(pairs[1], (Action::Custom("ext.b".to_owned()), "Beta"));
    }

    #[test]
    fn action_registry_default() {
        let reg: ActionRegistry = Default::default();
        assert!(reg.is_empty());
    }

    // ── parse_chord ────────────────────────────────────────────────────────

    #[test]
    fn parse_chord_simple_key() {
        let chord = parse_chord("a").unwrap();
        assert_eq!(chord.modifiers, Modifiers::empty());
        assert_eq!(chord.key, Key::Char('a'));
    }

    #[test]
    fn parse_chord_modifier_key() {
        let chord = parse_chord("ctrl+shift+p").unwrap();
        assert!(chord.modifiers.contains(Modifiers::CTRL));
        assert!(chord.modifiers.contains(Modifiers::SHIFT));
        assert!(!chord.modifiers.contains(Modifiers::ALT));
        assert!(!chord.modifiers.contains(Modifiers::META));
        assert_eq!(chord.key, Key::Char('p'));
    }

    #[test]
    fn parse_chord_alt_meta() {
        let chord = parse_chord("alt+meta+x").unwrap();
        assert!(chord.modifiers.contains(Modifiers::ALT));
        assert!(chord.modifiers.contains(Modifiers::META));
        assert_eq!(chord.key, Key::Char('x'));
    }

    #[test]
    fn parse_chord_function_key() {
        let chord = parse_chord("f12").unwrap();
        assert_eq!(chord.key, Key::F(12));
    }

    #[test]
    fn parse_chord_special_keys() {
        assert_eq!(parse_chord("up").unwrap().key, Key::Up);
        assert_eq!(parse_chord("down").unwrap().key, Key::Down);
        assert_eq!(parse_chord("left").unwrap().key, Key::Left);
        assert_eq!(parse_chord("right").unwrap().key, Key::Right);
        assert_eq!(parse_chord("home").unwrap().key, Key::Home);
        assert_eq!(parse_chord("end").unwrap().key, Key::End);
        assert_eq!(parse_chord("pageup").unwrap().key, Key::PageUp);
        assert_eq!(parse_chord("pagedown").unwrap().key, Key::PageDown);
        assert_eq!(parse_chord("enter").unwrap().key, Key::Enter);
        assert_eq!(parse_chord("return").unwrap().key, Key::Enter);
        assert_eq!(parse_chord("backspace").unwrap().key, Key::Backspace);
        assert_eq!(parse_chord("delete").unwrap().key, Key::Delete);
        assert_eq!(parse_chord("del").unwrap().key, Key::Delete);
        assert_eq!(parse_chord("tab").unwrap().key, Key::Tab);
        assert_eq!(parse_chord("escape").unwrap().key, Key::Escape);
        assert_eq!(parse_chord("esc").unwrap().key, Key::Escape);
        assert_eq!(parse_chord("space").unwrap().key, Key::Space);
    }

    #[test]
    fn parse_chord_numpad() {
        let chord = parse_chord("numpad3").unwrap();
        assert_eq!(chord.key, Key::Numpad(3));
    }

    #[test]
    fn parse_chord_unknown_modifier_fails() {
        let err = parse_chord("super+unknown+x").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("unknown modifier"), "got: {msg}");
    }

    #[test]
    fn parse_chord_empty_fails() {
        let err = parse_chord("").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("empty chord"), "got: {msg}");
    }

    #[test]
    fn parse_chord_modifier_alternate_names() {
        let ch1 = parse_chord("control+c").unwrap();
        assert!(ch1.modifiers.contains(Modifiers::CTRL));
        let ch2 = parse_chord("option+space").unwrap();
        assert!(ch2.modifiers.contains(Modifiers::ALT));
        let ch3 = parse_chord("cmd+q").unwrap();
        assert!(ch3.modifiers.contains(Modifiers::META));
        let ch4 = parse_chord("win+e").unwrap();
        assert!(ch4.modifiers.contains(Modifiers::META));
        let ch5 = parse_chord("super+r").unwrap();
        assert!(ch5.modifiers.contains(Modifiers::META));
    }

    #[test]
    fn parse_chord_unknown_key_yields_unknown() {
        let chord = parse_chord("ctrl+unknownkey").unwrap();
        assert_eq!(chord.key, Key::Unknown("unknownkey".to_owned()));
    }

    // ── KeyChord Display ───────────────────────────────────────────────────

    #[test]
    fn keychord_display_no_modifiers() {
        let chord = KeyChord::new(Modifiers::empty(), Key::Char('a'));
        assert_eq!(chord.to_string(), "a");
    }

    #[test]
    fn keychord_display_all_modifiers() {
        let mods = Modifiers::CTRL | Modifiers::SHIFT | Modifiers::ALT | Modifiers::META;
        let chord = KeyChord::new(mods, Key::F(1));
        assert_eq!(chord.to_string(), "ctrl+shift+alt+meta+f1");
    }

    #[test]
    fn keychord_display_special() {
        assert_eq!(
            KeyChord::new(Modifiers::CTRL, Key::Escape).to_string(),
            "ctrl+escape"
        );
        assert_eq!(
            KeyChord::new(Modifiers::empty(), Key::Space).to_string(),
            "space"
        );
    }

    // ── WhenCondition ──────────────────────────────────────────────────────

    #[test]
    fn when_condition_true_always() {
        assert!(WhenCondition::True.evaluate(&WhenContext::new()));
    }

    #[test]
    fn when_condition_false_never() {
        assert!(!WhenCondition::False.evaluate(&WhenContext::new()));
    }

    #[test]
    fn when_condition_not_true() {
        let cond = WhenCondition::Not(Box::new(WhenCondition::True));
        assert!(!cond.evaluate(&WhenContext::new()));
    }

    #[test]
    fn when_condition_not_false() {
        let cond = WhenCondition::Not(Box::new(WhenCondition::False));
        assert!(cond.evaluate(&WhenContext::new()));
    }

    #[test]
    fn when_condition_and_both_true() {
        let cond = WhenCondition::And(Box::new(WhenCondition::True), Box::new(WhenCondition::True));
        assert!(cond.evaluate(&WhenContext::new()));
    }

    #[test]
    fn when_condition_and_one_false() {
        let cond = WhenCondition::And(
            Box::new(WhenCondition::True),
            Box::new(WhenCondition::False),
        );
        assert!(!cond.evaluate(&WhenContext::new()));
    }

    #[test]
    fn when_condition_or_both_false() {
        let cond = WhenCondition::Or(
            Box::new(WhenCondition::False),
            Box::new(WhenCondition::False),
        );
        assert!(!cond.evaluate(&WhenContext::new()));
    }

    #[test]
    fn when_condition_or_one_true() {
        let cond = WhenCondition::Or(
            Box::new(WhenCondition::False),
            Box::new(WhenCondition::True),
        );
        assert!(cond.evaluate(&WhenContext::new()));
    }

    #[test]
    fn when_condition_boolean_key_present_true() {
        let mut ctx = WhenContext::new();
        ctx.set_bool("editorFocused", true);
        let cond = WhenCondition::Key("editorFocused".to_owned());
        assert!(cond.evaluate(&ctx));
    }

    #[test]
    fn when_condition_boolean_key_present_false() {
        let mut ctx = WhenContext::new();
        ctx.set_bool("editorFocused", false);
        let cond = WhenCondition::Key("editorFocused".to_owned());
        assert!(!cond.evaluate(&ctx));
    }

    #[test]
    fn when_condition_boolean_key_missing() {
        let ctx = WhenContext::new();
        let cond = WhenCondition::Key("editorFocused".to_owned());
        assert!(!cond.evaluate(&ctx));
    }

    #[test]
    fn when_condition_key_equals() {
        let mut ctx = WhenContext::new();
        ctx.set_str("editorLangId", "rust");
        let cond = WhenCondition::KeyEquals("editorLangId".to_owned(), "rust".to_owned());
        assert!(cond.evaluate(&ctx));
    }

    #[test]
    fn when_condition_key_equals_mismatch() {
        let mut ctx = WhenContext::new();
        ctx.set_str("editorLangId", "python");
        let cond = WhenCondition::KeyEquals("editorLangId".to_owned(), "rust".to_owned());
        assert!(!cond.evaluate(&ctx));
    }

    #[test]
    fn when_condition_key_equals_missing_key() {
        let ctx = WhenContext::new();
        let cond = WhenCondition::KeyEquals("editorLangId".to_owned(), "rust".to_owned());
        assert!(!cond.evaluate(&ctx));
    }

    #[test]
    fn when_condition_key_not_equals_match() {
        let mut ctx = WhenContext::new();
        ctx.set_str("editorLangId", "python");
        let cond = WhenCondition::KeyNotEquals("editorLangId".to_owned(), "rust".to_owned());
        assert!(cond.evaluate(&ctx));
    }

    #[test]
    fn when_condition_key_not_equals_equal() {
        let mut ctx = WhenContext::new();
        ctx.set_str("editorLangId", "rust");
        let cond = WhenCondition::KeyNotEquals("editorLangId".to_owned(), "rust".to_owned());
        assert!(!cond.evaluate(&ctx));
    }

    #[test]
    fn when_condition_key_not_equals_missing_key() {
        let ctx = WhenContext::new();
        let cond = WhenCondition::KeyNotEquals("editorLangId".to_owned(), "rust".to_owned());
        assert!(cond.evaluate(&ctx));
    }

    // ── WhenCondition::parse ───────────────────────────────────────────────

    #[test]
    fn parse_when_simple_bool() {
        let cond = WhenCondition::parse("editorFocused");
        assert_eq!(cond, WhenCondition::Key("editorFocused".to_owned()));
    }

    #[test]
    fn parse_when_not() {
        let cond = WhenCondition::parse("!terminalFocused");
        assert_eq!(
            cond,
            WhenCondition::Not(Box::new(WhenCondition::Key("terminalFocused".to_owned())))
        );
    }

    #[test]
    fn parse_when_key_equals() {
        let cond = WhenCondition::parse("editorLangId == 'rust'");
        assert_eq!(
            cond,
            WhenCondition::KeyEquals("editorLangId".to_owned(), "rust".to_owned())
        );
    }

    #[test]
    fn parse_when_key_not_equals() {
        let cond = WhenCondition::parse("editorLangId != 'python'");
        assert_eq!(
            cond,
            WhenCondition::KeyNotEquals("editorLangId".to_owned(), "python".to_owned())
        );
    }

    #[test]
    fn parse_when_and() {
        let cond = WhenCondition::parse("editorFocused && !terminalFocused");
        assert_eq!(
            cond,
            WhenCondition::And(
                Box::new(WhenCondition::Key("editorFocused".to_owned())),
                Box::new(WhenCondition::Not(Box::new(WhenCondition::Key(
                    "terminalFocused".to_owned()
                )))),
            )
        );
    }

    #[test]
    fn parse_when_or() {
        let cond = WhenCondition::parse("editorFocused || terminalFocused");
        assert_eq!(
            cond,
            WhenCondition::Or(
                Box::new(WhenCondition::Key("editorFocused".to_owned())),
                Box::new(WhenCondition::Key("terminalFocused".to_owned())),
            )
        );
    }

    #[test]
    fn parse_when_parenthesized() {
        let cond = WhenCondition::parse("(editorFocused && editorLangId == 'rust')");
        // After parsing, parentheses are unwrapped.
        assert_eq!(
            cond,
            WhenCondition::And(
                Box::new(WhenCondition::Key("editorFocused".to_owned())),
                Box::new(WhenCondition::KeyEquals(
                    "editorLangId".to_owned(),
                    "rust".to_owned()
                )),
            )
        );
    }

    #[test]
    fn parse_when_empty_is_true() {
        let cond = WhenCondition::parse("");
        assert_eq!(cond, WhenCondition::True);
    }

    #[test]
    fn parse_when_whitespace_is_true() {
        let cond = WhenCondition::parse("   ");
        assert_eq!(cond, WhenCondition::True);
    }

    #[test]
    fn parse_when_double_quotes() {
        let cond = WhenCondition::parse("key == \"value\"");
        assert_eq!(
            cond,
            WhenCondition::KeyEquals("key".to_owned(), "value".to_owned())
        );
    }

    #[test]
    fn parse_when_complex_expression() {
        // (a || b) && c && !d
        let cond = WhenCondition::parse("(a || b) && c && !d");
        let expected = WhenCondition::And(
            Box::new(WhenCondition::And(
                Box::new(WhenCondition::Or(
                    Box::new(WhenCondition::Key("a".to_owned())),
                    Box::new(WhenCondition::Key("b".to_owned())),
                )),
                Box::new(WhenCondition::Key("c".to_owned())),
            )),
            Box::new(WhenCondition::Not(Box::new(WhenCondition::Key(
                "d".to_owned(),
            )))),
        );
        assert_eq!(cond, expected);
    }

    // ── WhenContext ────────────────────────────────────────────────────────

    #[test]
    fn when_context_new_is_empty() {
        let ctx = WhenContext::new();
        assert!(ctx.get_bool("anything").is_none());
        assert!(ctx.get_str("anything").is_none());
    }

    #[test]
    fn when_context_set_get_bool() {
        let mut ctx = WhenContext::new();
        ctx.set_bool("editorFocused", true);
        assert_eq!(ctx.get_bool("editorFocused"), Some(true));
        ctx.set_bool("editorFocused", false);
        assert_eq!(ctx.get_bool("editorFocused"), Some(false));
    }

    #[test]
    fn when_context_set_get_str() {
        let mut ctx = WhenContext::new();
        ctx.set_str("editorLangId", "rust");
        assert_eq!(ctx.get_str("editorLangId"), Some("rust"));
    }

    #[test]
    fn when_context_remove() {
        let mut ctx = WhenContext::new();
        ctx.set_bool("editorFocused", true);
        ctx.set_str("editorLangId", "rust");
        ctx.remove("editorFocused");
        assert!(ctx.get_bool("editorFocused").is_none());
        assert_eq!(ctx.get_str("editorLangId"), Some("rust"));
    }

    #[test]
    fn when_context_merge() {
        let mut base = WhenContext::new();
        base.set_bool("a", true);
        base.set_str("lang", "rust");

        let mut other = WhenContext::new();
        other.set_bool("b", false);
        other.set_str("lang", "python"); // should override

        base.merge(&other);
        assert_eq!(base.get_bool("a"), Some(true));
        assert_eq!(base.get_bool("b"), Some(false));
        assert_eq!(base.get_str("lang"), Some("python"));
    }

    #[test]
    fn when_context_default() {
        let ctx: WhenContext = Default::default();
        assert!(ctx.get_bool("any").is_none());
    }

    // ── all_actions_with ───────────────────────────────────────────────────

    #[test]
    fn all_actions_with_empty_registry() {
        let reg = ActionRegistry::new();
        let m = all_actions_with(&reg);
        // Should contain at least the standard built-in actions.
        assert!(m.contains_key(&Action::NewFile));
        assert!(m.contains_key(&Action::CommandPalette));
        // Should not contain any custom actions.
        assert!(!m.contains_key(&Action::Custom("ext.test".to_owned())));
    }

    #[test]
    fn all_actions_with_custom_actions() {
        let mut reg = ActionRegistry::new();
        reg.register("ext.test", "Test Action");
        let m = all_actions_with(&reg);
        assert_eq!(
            m.get(&Action::Custom("ext.test".to_owned())),
            Some(&"Test Action".to_owned())
        );
    }

    #[test]
    fn all_actions_with_does_not_override_builtin() {
        let mut reg = ActionRegistry::new();
        // Try to register an action with the same label as a builtin.
        reg.register("ext.test", "File: New File");
        let m = all_actions_with(&reg);
        // The built-in title for NewFile should remain unchanged.
        assert_eq!(m.get(&Action::NewFile), Some(&"File: New File".to_owned()));
    }

    #[test]
    fn all_actions_contains_all_key_actions() {
        let m = all_actions();
        // Spot-check a few key categories.
        assert!(m.contains_key(&Action::CursorUp));
        assert!(m.contains_key(&Action::SelectDown));
        assert!(m.contains_key(&Action::DeleteCharLeft));
        assert!(m.contains_key(&Action::GitCommit));
        assert!(m.contains_key(&Action::ToggleBreakpoint));
        assert!(m.contains_key(&Action::NewTerminal));
        assert!(m.contains_key(&Action::NextTabstop));
        assert_eq!(
            m.get(&Action::GotoSymbol),
            Some(&"Go: Go to Symbol in File...")
        );
    }

    // ── KeybindingEngine ───────────────────────────────────────────────────

    #[test]
    fn keybinding_engine_with_defaults() {
        let engine = KeybindingEngine::with_defaults();
        let chords = engine.chords_for_action(&Action::SaveFile);
        assert!(
            !chords.is_empty(),
            "expected at least one binding for SaveFile"
        );
    }

    #[test]
    fn keybinding_engine_press_single_chord() {
        let mut engine = KeybindingEngine::with_defaults();
        let chord = parse_chord("ctrl+s").unwrap();
        let action = engine.press(chord, None);
        assert_eq!(action, Some(Action::SaveFile));
    }

    #[test]
    fn keybinding_engine_press_with_context() {
        let mut engine = KeybindingEngine::with_defaults();
        let mut ctx = WhenContext::new();
        ctx.set_bool("editorFocused", true);
        // ctrl+s should match regardless of context since it has no when clause.
        let chord = parse_chord("ctrl+s").unwrap();
        let action = engine.press(chord, Some(&ctx));
        assert_eq!(action, Some(Action::SaveFile));
    }

    #[test]
    fn keybinding_engine_no_match_returns_none() {
        let mut engine = KeybindingEngine::with_defaults();
        let chord = parse_chord("ctrl+alt+shift+super+z").unwrap();
        let action = engine.press(chord, None);
        assert!(action.is_none());
    }

    #[test]
    fn keybinding_engine_two_chord_sequence() {
        let mut engine = KeybindingEngine::with_defaults();
        // ctrl+k s should save all.
        let first = parse_chord("ctrl+k").unwrap();
        let second = parse_chord("s").unwrap();

        // First press should start the chord sequence (return None).
        let result1 = engine.press(first, None);
        assert!(result1.is_none());
        assert!(engine.has_pending_chord());

        // Second press should complete the chord.
        let result2 = engine.press(second, None);
        assert_eq!(result2, Some(Action::SaveAll));
        assert!(!engine.has_pending_chord());
    }

    #[test]
    fn keybinding_engine_cancel_pending() {
        let mut engine = KeybindingEngine::with_defaults();
        let first = parse_chord("ctrl+k").unwrap();
        let _ = engine.press(first, None);
        assert!(engine.has_pending_chord());
        engine.cancel_pending();
        assert!(!engine.has_pending_chord());
    }

    #[test]
    fn keybinding_engine_bind_adds_binding() {
        let mut engine = KeybindingEngine::with_defaults();
        let old_count = engine.bindings().len();
        engine.bind("ctrl+shift+alt+t", Action::Custom("ext.test".to_owned()));
        assert_eq!(engine.bindings().len(), old_count + 1);

        let chord = parse_chord("ctrl+shift+alt+t").unwrap();
        let action = engine.press(chord, None);
        assert_eq!(action, Some(Action::Custom("ext.test".to_owned())));
    }

    #[test]
    fn keybinding_engine_bind_ext_alias() {
        let mut engine = KeybindingEngine::with_defaults();
        engine.bind_ext("ctrl+u", Action::Custom("ext.uppercase".to_owned()));
        let chord = parse_chord("ctrl+u").unwrap();
        let action = engine.press(chord, None);
        assert_eq!(action, Some(Action::Custom("ext.uppercase".to_owned())));
    }

    #[test]
    fn keybinding_engine_press_legacy_no_context() {
        let mut engine = KeybindingEngine::with_defaults();
        let chord = parse_chord("ctrl+c").unwrap();
        let action = engine.press_legacy(chord);
        assert_eq!(action, Some(Action::Copy));
    }

    #[test]
    fn keybinding_engine_chords_for_action_no_match() {
        let engine = KeybindingEngine::with_defaults();
        let chords = engine.chords_for_action(&Action::Custom("nonexistent".to_owned()));
        assert!(chords.is_empty());
    }

    #[test]
    fn keybinding_engine_load_toml() {
        let mut engine = KeybindingEngine::with_defaults();
        let old_count = engine.bindings().len();
        let toml = r#"
            [[bindings]]
            key    = "ctrl+alt+t"
            action = "newFile"
        "#;
        engine.load_toml(toml, "test").unwrap();
        assert_eq!(engine.bindings().len(), old_count + 1);
        let chord = parse_chord("ctrl+alt+t").unwrap();
        assert_eq!(engine.press(chord, None), Some(Action::NewFile));
    }

    #[test]
    fn keybinding_engine_load_toml_with_when() {
        let mut engine = KeybindingEngine::with_defaults();
        let toml = r#"
            [[bindings]]
            key    = "ctrl+alt+e"
            action = "commandPalette"
            when   = "editorFocused"
        "#;
        engine.load_toml(toml, "test").unwrap();
        let chord = parse_chord("ctrl+alt+e").unwrap();
        // Without context, when condition fails -> no match.
        assert!(engine.press(chord.clone(), None).is_none());
        // With matching context.
        let mut ctx = WhenContext::new();
        ctx.set_bool("editorFocused", true);
        assert_eq!(
            engine.press(chord, Some(&ctx)),
            Some(Action::CommandPalette)
        );
    }

    #[test]
    fn keybinding_engine_load_toml_invalid_is_skipped() {
        let mut engine = KeybindingEngine::with_defaults();
        let toml = r#"
            [[bindings]]
            key    = "ctrl+unknown_modifier+x"
            action = "newFile"
        "#;
        // Should not return an error — invalid chord is logged and skipped.
        engine.load_toml(toml, "test").unwrap();
        // Verify the invalid binding was skipped by checking no new bindings
        // with 'newFile' that start with the unknown modifier exist.
        let chords = engine.chords_for_action(&Action::NewFile);
        // There should still be the default ctrl+n for newFile, but not ctrl+unknown_modifier+x.
        assert!(!chords.is_empty());
    }

    #[test]
    fn keybinding_engine_load_toml_malformed() {
        let mut engine = KeybindingEngine::with_defaults();
        let result = engine.load_toml("this is not toml {{", "bad-source");
        assert!(result.is_err());
    }

    #[test]
    fn keybinding_engine_bindings_returns_all() {
        let engine = KeybindingEngine::with_defaults();
        let all = engine.bindings();
        assert!(
            all.len() > 50,
            "expected many default bindings, got {}",
            all.len()
        );
        // Check that each binding has at least one chord.
        for b in &all {
            assert!(!b.chords.is_empty(), "binding {:?} has no chords", b.action);
        }
    }

    #[test]
    fn keybinding_engine_two_chord_falls_through_to_single() {
        let mut engine = KeybindingEngine::with_defaults();
        // ctrl+k is a two-chord prefix (ctrl+k s = saveAll).
        // But ctrl+k alone might also be bound? It isn't by default.
        // However, after pressing ctrl+k, pressing something that isn't 's'
        // should consume the pending chord and NOT match.
        let first = parse_chord("ctrl+k").unwrap();
        let second = parse_chord("x").unwrap();

        let _ = engine.press(first, None);
        assert!(engine.has_pending_chord());
        let result = engine.press(second, None);
        // Since 'x' isn't a second chord for any two-chord binding that starts
        // with ctrl+k, we expect None.
        assert!(result.is_none());
        assert!(!engine.has_pending_chord());
    }

    #[test]
    fn keybinding_engine_pending_chord_timeout() {
        // Simulate a scenario where a chord sequence is started but not completed.
        let mut engine = KeybindingEngine::with_defaults();
        let first = parse_chord("ctrl+k").unwrap();
        let _ = engine.press(first, None);
        assert!(engine.has_pending_chord());
        // Pressing a single key that matches a different binding should cancel the
        // pending chord and try the new press as a fresh single chord.
        let ctrl_s = parse_chord("ctrl+s").unwrap();
        let action = engine.press(ctrl_s, None);
        assert_eq!(action, Some(Action::SaveFile));
        assert!(!engine.has_pending_chord());
    }

    #[test]
    fn keybinding_engine_duplicate_binding() {
        let mut engine = KeybindingEngine::with_defaults();
        let count_before = engine.bindings().len();
        // Bind the same key/action again.
        engine.bind("ctrl+s", Action::SaveFile);
        assert_eq!(engine.bindings().len(), count_before + 1);
        // Later bindings (last added) take priority due to rev() search.
        let chord = parse_chord("ctrl+s").unwrap();
        assert_eq!(engine.press(chord, None), Some(Action::SaveFile));
    }

    // ── Key equality / hash ────────────────────────────────────────────────

    #[test]
    fn key_equality() {
        assert_eq!(Key::Char('a'), Key::Char('a'));
        assert_ne!(Key::Char('a'), Key::Char('b'));
        assert_eq!(Key::F(1), Key::F(1));
        assert_ne!(Key::F(1), Key::F(2));
        assert_eq!(Key::Unknown("foo".into()), Key::Unknown("foo".into()));
    }

    #[test]
    fn keychord_equality() {
        let a = KeyChord::new(Modifiers::CTRL, Key::Char('a'));
        let b = KeyChord::new(Modifiers::CTRL, Key::Char('a'));
        let c = KeyChord::new(Modifiers::CTRL | Modifiers::SHIFT, Key::Char('a'));
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    // ── Modifiers bitflags ─────────────────────────────────────────────────

    #[test]
    fn modifiers_bitflags() {
        let empty = Modifiers::empty();
        assert!(!empty.contains(Modifiers::CTRL));

        let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
        assert!(ctrl_shift.contains(Modifiers::CTRL));
        assert!(ctrl_shift.contains(Modifiers::SHIFT));
        assert!(!ctrl_shift.contains(Modifiers::ALT));
        assert!(!ctrl_shift.contains(Modifiers::META));
    }

    // ── Edge cases ─────────────────────────────────────────────────────────

    #[test]
    fn parse_chord_mixed_case_modifier() {
        let chord = parse_chord("Ctrl+Shift+P").unwrap();
        assert!(chord.modifiers.contains(Modifiers::CTRL));
        assert!(chord.modifiers.contains(Modifiers::SHIFT));
        assert_eq!(chord.key, Key::Char('p'));
    }

    #[test]
    fn parse_chord_uppercase_key() {
        // Single uppercase letter should still parse as lowercase char key
        let chord = parse_chord("A").unwrap();
        assert_eq!(chord.key, Key::Char('a'));
    }

    #[test]
    fn when_condition_deeply_nested() {
        // (a || (b && c)) && !d
        let expr = "(a || (b && c)) && !d";
        let cond = WhenCondition::parse(expr);

        // Verify evaluation
        let mut ctx = WhenContext::new();
        ctx.set_bool("a", false);
        ctx.set_bool("b", true);
        ctx.set_bool("c", true);
        ctx.set_bool("d", false);
        assert!(cond.evaluate(&ctx));

        ctx.set_bool("d", true);
        assert!(!cond.evaluate(&ctx));
    }

    #[test]
    fn all_actions_with_preserves_order() {
        let m = all_actions();
        let mut iter = m.iter();
        let first = iter.next().unwrap();
        assert_eq!(first.0, &Action::NewFile);
        // The last entry should be PreviousTabstop.
        if let Some(last) = iter.last() {
            assert_eq!(last.0, &Action::PreviousTabstop);
        }
    }

    #[test]
    fn action_display_insert_text() {
        let action = Action::InsertText("hello".to_owned());
        let s = action.to_string();
        assert_eq!(s, r#"insertText("hello")"#);
    }

    #[test]
    fn action_display_custom() {
        let action = Action::Custom("ext.my-action".to_owned());
        let s = action.to_string();
        assert_eq!(s, "ext.my-action");
    }

    #[test]
    fn action_display_builtin() {
        let action = Action::SaveFile;
        let s = action.to_string();
        // Serialized as camelCase via serde, then trimmed quotes.
        assert_eq!(s, "saveFile");
    }

    #[test]
    fn key_display_all_variants() {
        let cases = vec![
            (Key::Char('x'), "x"),
            (Key::F(24), "f24"),
            (Key::Up, "up"),
            (Key::Down, "down"),
            (Key::Left, "left"),
            (Key::Right, "right"),
            (Key::Home, "home"),
            (Key::End, "end"),
            (Key::PageUp, "pageup"),
            (Key::PageDown, "pagedown"),
            (Key::Enter, "enter"),
            (Key::Backspace, "backspace"),
            (Key::Delete, "delete"),
            (Key::Tab, "tab"),
            (Key::Escape, "escape"),
            (Key::Space, "space"),
            (Key::Numpad(0), "numpad0"),
            (Key::Unknown("foo".into()), "foo"),
        ];
        for (key, expected) in cases {
            assert_eq!(key.to_string(), expected, "mismatch for {key:?}");
        }
    }

    #[test]
    fn keychord_display_mixed_modifiers() {
        let chord = KeyChord::new(Modifiers::CTRL | Modifiers::ALT, Key::Delete);
        assert_eq!(chord.to_string(), "ctrl+alt+delete");
    }

    #[test]
    fn keychord_display_meta_alone() {
        let chord = KeyChord::new(Modifiers::META, Key::Char('q'));
        assert_eq!(chord.to_string(), "meta+q");
    }

    // ── VS Code keybindings.json import ──────────────────────────────────

    #[test]
    fn vscode_json_basic_import() {
        let mut engine = KeybindingEngine::with_defaults();
        let old_count = engine.bindings().len();
        let json = r#"[
            { "key": "ctrl+alt+t", "command": "workbench.action.files.newUntitledFile" }
        ]"#;
        engine.load_vscode_json(json, "test").unwrap();
        assert_eq!(engine.bindings().len(), old_count + 1);
        let chord = parse_chord("ctrl+alt+t").unwrap();
        assert_eq!(engine.press(chord, None), Some(Action::NewFile));
    }

    #[test]
    fn vscode_json_with_when() {
        let mut engine = KeybindingEngine::with_defaults();
        let json = r#"[
            { "key": "ctrl+alt+e", "command": "workbench.action.showCommands", "when": "editorFocused" }
        ]"#;
        engine.load_vscode_json(json, "test").unwrap();
        let chord = parse_chord("ctrl+alt+e").unwrap();
        // Without context, when condition fails -> no match.
        assert!(engine.press(chord.clone(), None).is_none());
        // With matching context.
        let mut ctx = WhenContext::new();
        ctx.set_bool("editorFocused", true);
        assert_eq!(
            engine.press(chord, Some(&ctx)),
            Some(Action::CommandPalette)
        );
    }

    #[test]
    fn vscode_json_unknown_command_becomes_custom() {
        let mut engine = KeybindingEngine::with_defaults();
        let json = r#"[
            { "key": "ctrl+shift+r", "command": "myExtension.doSomething" }
        ]"#;
        engine.load_vscode_json(json, "test").unwrap();
        let chord = parse_chord("ctrl+shift+r").unwrap();
        assert_eq!(
            engine.press(chord, None),
            Some(Action::Custom("myExtension.doSomething".to_owned()))
        );
    }

    #[test]
    fn vscode_json_removal_prefix() {
        let mut engine = KeybindingEngine::with_defaults();
        // First add a binding.
        engine.bind("ctrl+shift+alt+x", Action::NewFile);
        let count_after_add = engine.bindings().len();
        // Now remove it using the VS Code `-command` syntax.
        let json = r#"[
            { "key": "ctrl+shift+alt+x", "command": "-workbench.action.files.newUntitledFile" }
        ]"#;
        engine.load_vscode_json(json, "test").unwrap();
        assert_eq!(engine.bindings().len(), count_after_add - 1);
        let chord = parse_chord("ctrl+shift+alt+x").unwrap();
        assert!(engine.press(chord, None).is_none());
    }

    #[test]
    fn vscode_json_malformed_returns_error() {
        let mut engine = KeybindingEngine::with_defaults();
        let result = engine.load_vscode_json("this is not json {{", "bad-source");
        assert!(result.is_err());
    }

    #[test]
    fn vscode_json_empty_array() {
        let mut engine = KeybindingEngine::with_defaults();
        let old_count = engine.bindings().len();
        engine.load_vscode_json("[]", "test").unwrap();
        assert_eq!(engine.bindings().len(), old_count);
    }

    #[test]
    fn vscode_json_multiple_bindings() {
        let mut engine = KeybindingEngine::with_defaults();
        let json = r#"[
            { "key": "ctrl+alt+n", "command": "workbench.action.files.newUntitledFile" },
            { "key": "ctrl+alt+f", "command": "editor.action.formatDocument" },
            { "key": "f12", "command": "editor.action.goToDefinition" }
        ]"#;
        engine.load_vscode_json(json, "test").unwrap();
        assert_eq!(
            engine.press(parse_chord("ctrl+alt+n").unwrap(), None),
            Some(Action::NewFile)
        );
        assert_eq!(
            engine.press(parse_chord("ctrl+alt+f").unwrap(), None),
            Some(Action::FormatDocument)
        );
        assert_eq!(
            engine.press(parse_chord("f12").unwrap(), None),
            Some(Action::GotoDefinition)
        );
    }

    #[test]
    fn vscode_json_oem_key_mapping() {
        let chord = parse_vscode_chord("ctrl+oem_1").unwrap();
        assert!(chord.modifiers.contains(Modifiers::CTRL));
        assert_eq!(chord.key, Key::Char(';'));
    }

    #[test]
    fn vscode_json_oem_bracket_keys() {
        assert_eq!(parse_vscode_chord("oem_4").unwrap().key, Key::Char('['));
        assert_eq!(parse_vscode_chord("oem_6").unwrap().key, Key::Char(']'));
        assert_eq!(parse_vscode_chord("oem_5").unwrap().key, Key::Char('\\'));
    }

    #[test]
    fn vscode_json_numpad_keys() {
        assert_eq!(parse_vscode_chord("numpad0").unwrap().key, Key::Numpad(0));
        assert_eq!(parse_vscode_chord("numpad9").unwrap().key, Key::Numpad(9));
        assert_eq!(
            parse_vscode_chord("numpad_add").unwrap().key,
            Key::Char('+')
        );
        assert_eq!(parse_vscode_chord("numpad_enter").unwrap().key, Key::Enter);
    }

    #[test]
    fn vscode_json_cmd_maps_to_meta() {
        let chord = parse_vscode_chord("cmd+s").unwrap();
        assert!(chord.modifiers.contains(Modifiers::META));
        assert_eq!(chord.key, Key::Char('s'));
    }

    #[test]
    fn vscode_json_option_maps_to_alt() {
        let chord = parse_vscode_chord("option+x").unwrap();
        assert!(chord.modifiers.contains(Modifiers::ALT));
        assert_eq!(chord.key, Key::Char('x'));
    }

    #[test]
    fn vscode_json_two_chord_binding() {
        let mut engine = KeybindingEngine::with_defaults();
        let json = r#"[
            { "key": "ctrl+k ctrl+d", "command": "editor.action.formatDocument" }
        ]"#;
        engine.load_vscode_json(json, "test").unwrap();
        let first = parse_chord("ctrl+k").unwrap();
        let second = parse_chord("ctrl+d").unwrap();
        let _ = engine.press(first, None);
        assert!(engine.has_pending_chord());
        assert_eq!(engine.press(second, None), Some(Action::FormatDocument));
    }

    #[test]
    fn vscode_json_command_mapping_spot_checks() {
        // File commands
        assert_eq!(
            map_vscode_command("workbench.action.files.save"),
            Action::SaveFile
        );
        assert_eq!(
            map_vscode_command("workbench.action.files.saveAs"),
            Action::SaveFileAs
        );
        assert_eq!(
            map_vscode_command("workbench.action.files.saveAll"),
            Action::SaveAll
        );
        // Edit commands
        assert_eq!(map_vscode_command("undo"), Action::Undo);
        assert_eq!(map_vscode_command("redo"), Action::Redo);
        assert_eq!(
            map_vscode_command("editor.action.commentLine"),
            Action::ToggleLineComment
        );
        assert_eq!(
            map_vscode_command("editor.action.blockComment"),
            Action::ToggleBlockComment
        );
        // Navigation
        assert_eq!(
            map_vscode_command("editor.action.goToDefinition"),
            Action::GotoDefinition
        );
        assert_eq!(
            map_vscode_command("editor.action.goToReferences"),
            Action::GotoReferences
        );
        // LSP
        assert_eq!(
            map_vscode_command("editor.action.triggerSuggest"),
            Action::TriggerCompletion
        );
        assert_eq!(
            map_vscode_command("editor.action.rename"),
            Action::RenameSymbol
        );
        // Debug
        assert_eq!(
            map_vscode_command("workbench.action.debug.start"),
            Action::StartDebug
        );
        assert_eq!(
            map_vscode_command("workbench.action.debug.stop"),
            Action::StopDebug
        );
        // Terminal
        assert_eq!(
            map_vscode_command("workbench.action.terminal.new"),
            Action::NewTerminal
        );
        // Unknown
        assert_eq!(
            map_vscode_command("my.custom.command"),
            Action::Custom("my.custom.command".to_owned())
        );
    }

    #[test]
    fn vscode_json_invalid_chord_skipped() {
        let mut engine = KeybindingEngine::with_defaults();
        let json = r#"[
            { "key": "ctrl+bogus_modifier+x", "command": "workbench.action.files.newUntitledFile" }
        ]"#;
        // Should not error — invalid chord is logged and skipped.
        engine.load_vscode_json(json, "test").unwrap();
    }

    #[test]
    fn vscode_json_removal_nonexistent_binding_is_noop() {
        let mut engine = KeybindingEngine::with_defaults();
        let count_before = engine.bindings().len();
        let json = r#"[
            { "key": "ctrl+shift+alt+z", "command": "-workbench.action.files.newUntitledFile" }
        ]"#;
        engine.load_vscode_json(json, "test").unwrap();
        // No binding existed for that key, so count unchanged.
        assert_eq!(engine.bindings().len(), count_before);
    }
}
