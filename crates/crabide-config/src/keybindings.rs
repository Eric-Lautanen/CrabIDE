//! Keybinding engine: `Action` enum, chord sequences, TOML parser.
//!
//! Format of `keybindings.toml`:
//! ```toml
//! [[bindings]]
//! key    = "ctrl+shift+p"
//! action = "commandPalette"
//!
//! [[bindings]]
//! key    = "ctrl+k ctrl+s"   # chord: two presses
//! action = "saveAll"
//! when   = "editorFocused"   # optional context filter
//! ```

use bitflags::bitflags;
use crabide_core::error::{crabideError, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
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

// ── KeybindingEngine ──────────────────────────────────────────────────────────

pub struct KeybindingEngine {
    bindings: Vec<KeyBinding>,
    pending_chord: Option<KeyChord>,
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
        self.load_toml(&content, &path.display().to_string())
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
                self.bindings.push(KeyBinding {
                    chords,
                    action: raw.action,
                    when: raw.when,
                });
            }
        }
        Ok(())
    }

    pub fn press(&mut self, chord: KeyChord) -> Option<Action> {
        if let Some(first) = self.pending_chord.take() {
            if let Some(action) = self.find_two_chord(&first, &chord) {
                return Some(action.clone());
            }
        }
        if let Some(action) = self.find_one_chord(&chord) {
            return Some(action.clone());
        }
        let starts_sequence = self
            .bindings
            .iter()
            .any(|b| b.chords.len() == 2 && b.chords[0] == chord);
        if starts_sequence {
            self.pending_chord = Some(chord);
        }
        None
    }

    pub fn cancel_pending(&mut self) {
        self.pending_chord = None;
    }
    pub fn has_pending_chord(&self) -> bool {
        self.pending_chord.is_some()
    }
    pub fn bindings(&self) -> &[KeyBinding] {
        &self.bindings
    }

    pub fn chords_for_action(&self, action: &Action) -> Vec<&[KeyChord]> {
        self.bindings
            .iter()
            .filter(|b| &b.action == action)
            .map(|b| b.chords.as_slice())
            .collect()
    }

    fn find_one_chord(&self, chord: &KeyChord) -> Option<&Action> {
        self.bindings
            .iter()
            .rev()
            .find(|b| b.chords.len() == 1 && &b.chords[0] == chord)
            .map(|b| &b.action)
    }

    fn find_two_chord(&self, first: &KeyChord, second: &KeyChord) -> Option<&Action> {
        self.bindings
            .iter()
            .rev()
            .find(|b| b.chords.len() == 2 && &b.chords[0] == first && &b.chords[1] == second)
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
            self.bindings.push(KeyBinding {
                chords,
                action,
                when: None,
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
    m.insert(Action::Quit, "File: Quit");
    m.insert(Action::Undo, "Edit: Undo");
    m.insert(Action::Redo, "Edit: Redo");
    m.insert(Action::Cut, "Edit: Cut");
    m.insert(Action::Copy, "Edit: Copy");
    m.insert(Action::Paste, "Edit: Paste");
    m.insert(Action::SelectAll, "Edit: Select All");
    m.insert(Action::ToggleLineComment, "Editor: Toggle Line Comment");
    m.insert(Action::FormatDocument, "Editor: Format Document");
    m.insert(Action::RenameSymbol, "Editor: Rename Symbol");
    m.insert(Action::GotoDefinition, "Go: Go to Definition");
    m.insert(Action::GotoReferences, "Go: Go to References");
    m.insert(Action::GotoSymbol, "Go: Go to Symbol in File...");
    m.insert(Action::GotoLine, "Go: Go to Line...");
    m.insert(Action::Find, "Edit: Find");
    m.insert(Action::FindInFiles, "Edit: Find in Files");
    m.insert(Action::CommandPalette, "View: Command Palette");
    m.insert(Action::FuzzyFindFile, "View: Open File by Name...");
    m.insert(Action::ToggleSidebar, "View: Toggle Sidebar");
    m.insert(Action::ToggleTerminal, "View: Toggle Terminal");
    m.insert(Action::ToggleWordWrap, "View: Toggle Word Wrap");
    m.insert(Action::ZoomIn, "View: Zoom In");
    m.insert(Action::ZoomOut, "View: Zoom Out");
    m.insert(Action::ZoomReset, "View: Reset Zoom");
    m.insert(Action::DuplicateLine, "Edit: Duplicate Line");
    m.insert(Action::DeleteLine, "Edit: Delete Line");
    m.insert(Action::MoveLineUp, "Edit: Move Line Up");
    m.insert(Action::MoveLineDown, "Edit: Move Line Down");
    m.insert(Action::IndentLine, "Edit: Indent Line");
    m.insert(Action::OutdentLine, "Edit: Outdent Line");
    m.insert(Action::ToggleBlockComment, "Editor: Toggle Block Comment");
    m.insert(
        Action::TrimTrailingWhitespace,
        "Editor: Trim Trailing Whitespace",
    );
    m.insert(Action::Find, "Edit: Find");
    m.insert(Action::FindReplace, "Edit: Replace");
    m.insert(Action::FindNext, "Edit: Find Next");
    m.insert(Action::FindPrevious, "Edit: Find Previous");
    m.insert(Action::NextTabstop, "Snippet: Next Tabstop");
    m.insert(Action::PreviousTabstop, "Snippet: Previous Tabstop");
    m.insert(Action::StartDebug, "Debug: Start Debugging");
    m.insert(Action::StopDebug, "Debug: Stop Debugging");
    m.insert(Action::ContinueDebug, "Debug: Continue");
    m.insert(Action::StepOver, "Debug: Step Over");
    m.insert(Action::StepInto, "Debug: Step Into");
    m.insert(Action::StepOut, "Debug: Step Out");
    m.insert(Action::RestartDebug, "Debug: Restart");
    m.insert(Action::ToggleBreakpoint, "Debug: Toggle Breakpoint");
    m.insert(Action::ToggleDebugPanel, "View: Toggle Debug Panel");
    m.insert(Action::ToggleDebug, "Debug: Enable/Disable Debugger");
    m.insert(
        Action::ToggleExtensionsPanel,
        "View: Toggle Extensions Panel",
    );
    m
}
