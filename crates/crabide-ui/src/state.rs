//! `UiState` — the complete mutable display state owned by the UI thread.
//!
//! The app crate creates one `UiState`, updates it when background events
//! arrive (new diagnostics, git diffs, LSP hover, etc.), and passes it by
//! mutable reference into `crabide_ui::render` every egui frame.
//!
//! `UiState` is **pure data** — it has no channels, async handles, or locks.
//! Background-event integration happens in `crabide-app`.

use std::path::PathBuf;
use std::time::Instant;

use crabide_buffer::{CursorSet, SnippetEngine, SnippetTabstop};
use crabide_config::{Action, Color, ColorTheme, KeybindingEngine, WhenContext};
use crabide_core::{
    event::{
        BlameLine, BranchInfo, CommitEntry, ConflictInfo, Diagnostic, DiffHunk, FileStatus,
        FoldingRange, Location, OutputCategory, RemoteInfo, StackFrame, StashEntry, SubmoduleInfo,
        TagInfo, TerminalCell, TerminalColor, TerminalColorScheme, Variable,
    },
    types::{BufferId, DocumentUri, Language, Position, Range},
};
use crabide_dap::LaunchConfig;
use crabide_extensions::{
    ContentBlock, ContextMenuContribution, GutterMarker, InstalledExtension, NavigateTarget,
    PanelRegistration, RegistryExtension, SidebarPaneRegistration, StatusBarAlignment,
};
use crabide_search::{FuzzyFileFinder, GrepAbortHandle, GrepMatch};
use crabide_syntax::HighlightSpan;
use indexmap::IndexMap;

use crate::layout::{default_layout, PaneKind};

// ── Helper ────────────────────────────────────────────────────────────────────

/// Convert a `crabide_config::Color` to an egui `Color32`.
pub fn cfg_to_egui(c: Color) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

// ── FindReplaceState ──────────────────────────────────────────────────────────

/// State for the find/replace overlay bar (Ctrl+F / Ctrl+H).
#[derive(Default)]
pub struct FindReplaceState {
    /// Whether the bar is visible.
    pub visible: bool,
    /// True when the replace row is also shown (opened with Ctrl+H).
    pub replace_visible: bool,
    /// The search query string.
    pub query: String,
    /// The replacement string.
    pub replacement: String,
    /// Use regular expressions for matching.
    pub use_regex: bool,
    /// Case-sensitive matching.
    pub case_sensitive: bool,
    /// Match whole words only.
    pub whole_word: bool,
    /// All match ranges in the current document (recomputed on query change).
    pub match_ranges: Vec<Range>,
    /// Index of the currently selected match within `match_ranges`.
    pub current_match_idx: usize,
    /// The query that was used to compute `match_ranges` (to detect changes).
    pub last_computed_query: String,
    /// Set to true when the bar is first opened so the query field can grab focus.
    pub just_opened: bool,
    /// Set to true after find/prev so the query TextEdit regains focus next frame.
    pub needs_refocus: bool,
}

impl FindReplaceState {
    /// Whether the current query is non-empty and has computed results.
    pub fn has_matches(&self) -> bool {
        !self.query.is_empty() && !self.match_ranges.is_empty()
    }

    /// Navigate to next match (wraps around).
    pub fn next_match(&mut self) {
        if self.match_ranges.is_empty() {
            return;
        }
        self.current_match_idx = (self.current_match_idx + 1) % self.match_ranges.len();
    }

    /// Navigate to previous match (wraps around).
    pub fn prev_match(&mut self) {
        if self.match_ranges.is_empty() {
            return;
        }
        if self.current_match_idx == 0 {
            self.current_match_idx = self.match_ranges.len() - 1;
        } else {
            self.current_match_idx -= 1;
        }
    }

    /// Current match range, if any.
    pub fn current_match(&self) -> Option<Range> {
        self.match_ranges.get(self.current_match_idx).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ── FindReplaceState tests ───────────────────────────────────────────

    #[test]
    fn find_replace_default() {
        let state = FindReplaceState::default();
        assert!(!state.visible);
        assert!(!state.replace_visible);
        assert!(state.query.is_empty());
        assert!(state.match_ranges.is_empty());
        assert_eq!(state.current_match_idx, 0);
        assert!(!state.has_matches());
    }

    #[test]
    fn find_replace_has_matches_empty_query() {
        let state = FindReplaceState {
            query: String::new(),
            match_ranges: vec![Range::new(Position::ZERO, Position::new(0, 1))],
            ..Default::default()
        };
        assert!(!state.has_matches());
    }

    #[test]
    fn find_replace_has_matches_empty_ranges() {
        let state = FindReplaceState {
            query: "test".into(),
            match_ranges: vec![],
            ..Default::default()
        };
        assert!(!state.has_matches());
    }

    #[test]
    fn find_replace_has_matches_ok() {
        let state = FindReplaceState {
            query: "test".into(),
            match_ranges: vec![Range::new(Position::ZERO, Position::new(0, 4))],
            ..Default::default()
        };
        assert!(state.has_matches());
    }

    #[test]
    fn find_replace_next_match_wraps() {
        let mut state = FindReplaceState {
            query: "a".into(),
            match_ranges: vec![
                Range::new(Position::ZERO, Position::new(0, 1)),
                Range::new(Position::new(0, 2), Position::new(0, 3)),
                Range::new(Position::new(0, 5), Position::new(0, 6)),
            ],
            current_match_idx: 2,
            ..Default::default()
        };
        state.next_match();
        assert_eq!(state.current_match_idx, 0);
    }

    #[test]
    fn find_replace_prev_match_wraps() {
        let mut state = FindReplaceState {
            query: "a".into(),
            match_ranges: vec![
                Range::new(Position::ZERO, Position::new(0, 1)),
                Range::new(Position::new(0, 2), Position::new(0, 3)),
            ],
            current_match_idx: 0,
            ..Default::default()
        };
        state.prev_match();
        assert_eq!(state.current_match_idx, 1);
    }

    #[test]
    fn find_replace_next_match_empty_noop() {
        let mut state = FindReplaceState::default();
        state.next_match();
        assert_eq!(state.current_match_idx, 0);
    }

    #[test]
    fn find_replace_current_match() {
        let state = FindReplaceState {
            query: "a".into(),
            match_ranges: vec![Range::new(Position::ZERO, Position::new(0, 1))],
            ..Default::default()
        };
        assert_eq!(
            state.current_match(),
            Some(Range::new(Position::ZERO, Position::new(0, 1)))
        );
    }

    #[test]
    fn find_replace_current_match_none() {
        let state = FindReplaceState::default();
        assert_eq!(state.current_match(), None);
    }

    // ── FuzzyFinderState tests ──────────────────────────────────────────

    #[test]
    fn fuzzy_finder_default() {
        let state = FuzzyFinderState::default();
        assert!(!state.visible);
        assert!(state.query.is_empty());
        assert!(state.results.is_empty());
        assert_eq!(state.selected_idx, 0);
        assert!(!state.index_stale);
    }

    #[test]
    fn fuzzy_finder_open_resets() {
        let mut state = FuzzyFinderState {
            query: "old".into(),
            selected_idx: 5,
            results: vec![PathBuf::from("test.rs")],
            result_labels: vec!["test.rs".into()],
            ..Default::default()
        };
        state.open();
        assert!(state.visible);
        assert!(state.query.is_empty());
        assert_eq!(state.selected_idx, 0);
        assert!(state.results.is_empty());
        assert!(state.result_labels.is_empty());
    }

    #[test]
    fn fuzzy_finder_close_resets() {
        let mut state = FuzzyFinderState {
            visible: true,
            query: "test".into(),
            selected_idx: 2,
            results: vec![PathBuf::from("main.rs")],
            result_labels: vec!["main.rs".into()],
            ..Default::default()
        };
        state.close();
        assert!(!state.visible);
        assert!(state.query.is_empty());
        assert_eq!(state.selected_idx, 0);
        assert!(state.results.is_empty());
        assert!(state.result_labels.is_empty());
    }

    // ── GotoLineState tests ─────────────────────────────────────────────

    #[test]
    fn goto_line_default() {
        let state = GotoLineState::default();
        assert!(!state.visible);
        assert!(state.query.is_empty());
    }

    #[test]
    fn goto_line_target_line_ok() {
        let state = GotoLineState {
            query: "3".into(),
            ..Default::default()
        };
        assert_eq!(state.target_line(10), Some(2)); // 0-based
    }

    #[test]
    fn goto_line_target_line_zero() {
        let state = GotoLineState {
            query: "0".into(),
            ..Default::default()
        };
        assert_eq!(state.target_line(10), None);
    }

    #[test]
    fn goto_line_target_line_out_of_range() {
        let state = GotoLineState {
            query: "20".into(),
            ..Default::default()
        };
        assert_eq!(state.target_line(10), None);
    }

    #[test]
    fn goto_line_target_line_not_a_number() {
        let state = GotoLineState {
            query: "abc".into(),
            ..Default::default()
        };
        assert_eq!(state.target_line(10), None);
    }

    #[test]
    fn goto_line_target_line_trimmed() {
        let state = GotoLineState {
            query: "  5  ".into(),
            ..Default::default()
        };
        assert_eq!(state.target_line(10), Some(4));
    }

    #[test]
    fn goto_line_target_line_empty() {
        let state = GotoLineState::default();
        assert_eq!(state.target_line(10), None);
    }

    // ── SymbolOutlineState tests ────────────────────────────────────────

    #[test]
    fn symbol_outline_default() {
        let state = SymbolOutlineState::default();
        assert!(!state.visible);
        assert!(state.query.is_empty());
        assert!(state.entries.is_empty());
        assert_eq!(state.selected_idx, 0);
    }

    // ── DapPanelState tests ─────────────────────────────────────────────

    #[test]
    fn dap_panel_default() {
        let state = DapPanelState::default();
        assert!(!state.visible);
        assert!(!state.enabled);
        assert!(!state.session_active);
        assert!(!state.paused);
        assert!(state.call_stack.is_empty());
        assert!(state.variables.is_empty());
        assert!(state.watch_expressions.is_empty());
        assert!(state.console_lines.is_empty());
        assert!(state.pending_launch == false);
    }

    #[test]
    fn dap_panel_append_console_capped() {
        let mut state = DapPanelState::default();
        for i in 0..2000 {
            state.append_console(OutputCategory::Stdout, format!("line {i}"));
        }
        assert_eq!(state.console_lines.len(), 2000);
        state.append_console(OutputCategory::Stdout, "overflow".into());
        assert_eq!(state.console_lines.len(), 2000);
        assert_eq!(state.console_lines[0].1, "line 1");
        assert_eq!(state.console_lines[1999].1, "overflow");
    }

    #[test]
    fn dap_panel_append_console_splits_newlines() {
        let mut state = DapPanelState::default();
        state.append_console(OutputCategory::Stdout, "line1\nline2\nline3".into());
        assert_eq!(state.console_lines.len(), 3);
        assert_eq!(state.console_lines[0].1, "line1");
        assert_eq!(state.console_lines[1].1, "line2");
        assert_eq!(state.console_lines[2].1, "line3");
    }

    #[test]
    fn dap_panel_append_console_sets_scroll_flag() {
        let mut state = DapPanelState::default();
        state.console_scroll_to_bottom = false;
        state.append_console(OutputCategory::Stdout, "hello".into());
        assert!(state.console_scroll_to_bottom);
    }

    #[test]
    fn dap_panel_reset_session() {
        let mut state = DapPanelState {
            session_active: true,
            paused: true,
            paused_thread_id: Some(42),
            stop_reason: Some("breakpoint".into()),
            call_stack: vec![crabide_core::event::StackFrame {
                id: 1,
                name: "main".into(),
                source_path: None,
                line: 10,
                column: 5,
            }],
            active_frame_id: Some(1),
            expanded_var_refs: std::collections::HashSet::from([100]),
            breakpoint_states: vec![crabide_core::event::BreakpointState {
                id: Some(1),
                verified: true,
                message: None,
                source_path: None,
                line: None,
                column: None,
            }],
            ..Default::default()
        };
        state.reset_session();
        assert!(!state.session_active);
        assert!(!state.paused);
        assert!(state.paused_thread_id.is_none());
        assert!(state.stop_reason.is_none());
        assert!(state.call_stack.is_empty());
        assert!(state.active_frame_id.is_none());
        assert!(state.variables.is_empty());
        assert!(state.expanded_var_refs.is_empty());
        assert!(state.breakpoint_states.is_empty());
    }

    // ── TerminalInstance tests ──────────────────────────────────────────

    #[test]
    fn terminal_instance_new() {
        let inst = TerminalInstance::new(42, 80, 24, TerminalColorScheme::dark());
        assert_eq!(inst.id, 42);
        assert_eq!(inst.title, "Terminal 42");
        assert_eq!(inst.cols, 80);
        assert_eq!(inst.grid_rows, 24);
        assert_eq!(inst.rows.len(), 24);
        assert_eq!(inst.rows[0].len(), 80);
        assert!(inst.cwd.is_none());
        assert!(!inst.exited);
    }

    #[test]
    fn terminal_instance_apply_delta() {
        let mut inst = TerminalInstance::new(1, 80, 24, TerminalColorScheme::dark());
        let delta = crabide_core::event::TerminalGridDelta {
            cursor_col: 5,
            cursor_row: 10,
            scroll_top: 0,
            cursor_visible: true,
            bracketed_paste: false,
            mouse_x10: false,
            mouse_normal: false,
            mouse_button_event: false,
            mouse_sgr: false,
            rows: vec![crabide_core::event::ChangedRow {
                row: 5,
                cells: vec![
                    TerminalCell {
                        ch: 'H',
                        fg: TerminalColor::Default,
                        bg: TerminalColor::Default,
                        attrs: crabide_core::event::CellAttrs::empty(),
                        hyperlink: None,
                    },
                    TerminalCell {
                        ch: 'i',
                        fg: TerminalColor::Default,
                        bg: TerminalColor::Default,
                        attrs: crabide_core::event::CellAttrs::empty(),
                        hyperlink: None,
                    },
                ],
            }],
        };
        inst.apply_delta(&delta);
        assert_eq!(inst.cursor_col, 5);
        assert_eq!(inst.cursor_row, 10);
        assert_eq!(inst.rows[5][0].ch, 'H');
        assert_eq!(inst.rows[5][1].ch, 'i');
    }

    #[test]
    fn terminal_instance_resize() {
        let mut inst = TerminalInstance::new(1, 80, 24, TerminalColorScheme::dark());
        inst.resize(120, 30);
        assert_eq!(inst.cols, 120);
        assert_eq!(inst.grid_rows, 30);
        assert_eq!(inst.rows.len(), 30);
        assert_eq!(inst.rows[0].len(), 120);
    }

    // ── TerminalPanelState tests ────────────────────────────────────────

    #[test]
    fn terminal_panel_default() {
        let state = TerminalPanelState::default();
        assert!(!state.visible);
        assert!(state.instances.is_empty());
        assert_eq!(state.active_idx, 0);
        assert!(!state.has_focus);
    }

    #[test]
    fn terminal_panel_active_none_when_empty() {
        let state = TerminalPanelState::default();
        assert!(state.active().is_none());
    }

    #[test]
    fn terminal_panel_active_mut_none_when_empty() {
        let mut state = TerminalPanelState::default();
        assert!(state.active_mut().is_none());
    }

    #[test]
    fn terminal_panel_active_with_instance() {
        let mut state = TerminalPanelState::default();
        state.instances.push(TerminalInstance::new(
            1,
            80,
            24,
            TerminalColorScheme::dark(),
        ));
        assert!(state.active().is_some());
        assert_eq!(state.active().unwrap().id, 1);
        assert!(state.active_mut().is_some());
    }

    #[test]
    fn terminal_panel_by_id_mut() {
        let mut state = TerminalPanelState::default();
        state.instances.push(TerminalInstance::new(
            1,
            80,
            24,
            TerminalColorScheme::dark(),
        ));
        state.instances.push(TerminalInstance::new(
            2,
            80,
            24,
            TerminalColorScheme::dark(),
        ));
        let found = state.by_id_mut(2);
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, 2);
        assert!(state.by_id_mut(99).is_none());
    }

    #[test]
    fn terminal_panel_remove_by_id() {
        let mut state = TerminalPanelState::default();
        state.instances.push(TerminalInstance::new(
            1,
            80,
            24,
            TerminalColorScheme::dark(),
        ));
        state.instances.push(TerminalInstance::new(
            2,
            80,
            24,
            TerminalColorScheme::dark(),
        ));
        state.active_idx = 1;
        state.remove_by_id(1);
        assert_eq!(state.instances.len(), 1);
        assert_eq!(state.instances[0].id, 2);
    }

    #[test]
    fn terminal_panel_remove_by_id_adjusts_active() {
        let mut state = TerminalPanelState::default();
        state.instances.push(TerminalInstance::new(
            1,
            80,
            24,
            TerminalColorScheme::dark(),
        ));
        state.remove_by_id(1);
        assert!(state.instances.is_empty());
        assert_eq!(state.active_idx, 0);
    }

    // ── EditorTab tests ─────────────────────────────────────────────────

    fn make_uri(name: &str) -> DocumentUri {
        DocumentUri::from_file_path(if cfg!(windows) {
            format!(r"C:\{name}")
        } else {
            format!("/tmp/{name}")
        })
        .unwrap()
    }

    fn make_test_tab() -> EditorTab {
        EditorTab::new(
            BufferId::new(),
            "test.rs".into(),
            make_uri("test.rs"),
            Language::RUST,
        )
    }

    #[test]
    fn editor_tab_new() {
        let tab = make_test_tab();
        assert_eq!(tab.title, "test.rs");
        assert_eq!(tab.language, Language::RUST);
        assert!(!tab.is_dirty);
        assert!(tab.lines.is_empty());
        assert!(tab.diagnostics.is_empty());
        assert!(tab.breakpoints.is_empty());
        assert!(tab.active_tabstop().is_none());
    }

    #[test]
    fn editor_tab_active_tabstop_no_snippet() {
        let tab = make_test_tab();
        assert!(tab.active_tabstop().is_none());
    }

    // ── UiState tests ───────────────────────────────────────────────────

    fn make_theme() -> ColorTheme {
        ColorTheme {
            id: "test".into(),
            name: "Test".into(),
            theme_type: crabide_config::ThemeType::Dark,
            ui_colors: IndexMap::new(),
            token_colors: Vec::new(),
        }
    }

    #[test]
    fn ui_state_new() {
        let theme = make_theme();
        let keybindings = crabide_config::KeybindingEngine::with_defaults();
        let state = UiState::new(theme, keybindings);
        assert!(state.editor_groups[0].tabs.is_empty());
        assert!(state.editor_groups[0].active_tab.is_none());
        assert_eq!(state.font_size, 14.0);
        assert!(!state.word_wrap);
        assert!(!state.git_enabled);
        assert_eq!(state.sidebar_tab, SidebarTab::Explorer);
        assert!(state.git_branch.is_none());
    }

    #[test]
    fn ui_state_open_tab_activates_existing() {
        let theme = make_theme();
        let keybindings = crabide_config::KeybindingEngine::with_defaults();
        let mut state = UiState::new(theme, keybindings);
        let buf_id = BufferId::new();
        let tab1 = EditorTab::new(buf_id, "a.rs".into(), make_uri("a.rs"), Language::RUST);
        let tab2 = EditorTab::new(
            BufferId::new(),
            "b.rs".into(),
            make_uri("b.rs"),
            Language::RUST,
        );
        state.open_tab(tab1);
        state.open_tab(tab2);
        assert_eq!(state.editor_groups[0].tabs.len(), 2);

        let tab_dup = EditorTab::new(buf_id, "a.rs".into(), make_uri("a.rs"), Language::RUST);
        state.open_tab(tab_dup);
        assert_eq!(state.editor_groups[0].tabs.len(), 2);
        assert_eq!(state.editor_groups[0].active_tab, Some(0));
    }

    #[test]
    fn ui_state_open_tab_appends_new() {
        let theme = make_theme();
        let keybindings = crabide_config::KeybindingEngine::with_defaults();
        let mut state = UiState::new(theme, keybindings);
        let tab = EditorTab::new(
            BufferId::new(),
            "new.rs".into(),
            make_uri("new.rs"),
            Language::RUST,
        );
        state.open_tab(tab);
        assert_eq!(state.editor_groups[0].tabs.len(), 1);
        assert_eq!(state.editor_groups[0].active_tab, Some(0));
    }

    #[test]
    fn ui_state_close_tab() {
        let theme = make_theme();
        let keybindings = crabide_config::KeybindingEngine::with_defaults();
        let mut state = UiState::new(theme, keybindings);
        let bid = BufferId::new();
        let tab = EditorTab::new(bid, "t.rs".into(), make_uri("t.rs"), Language::RUST);
        state.open_tab(tab);
        let closed = state.close_tab(0);
        assert_eq!(closed, Some(bid));
        assert!(state.editor_groups[0].tabs.is_empty());
        assert!(state.editor_groups[0].active_tab.is_none());
    }

    #[test]
    fn ui_state_active_tab_mut_none() {
        let theme = make_theme();
        let keybindings = crabide_config::KeybindingEngine::with_defaults();
        let mut state = UiState::new(theme, keybindings);
        assert!(state.active_tab_mut().is_none());
        assert!(state.active_tab_ref().is_none());
    }

    #[test]
    fn ui_state_set_status() {
        let theme = make_theme();
        let keybindings = crabide_config::KeybindingEngine::with_defaults();
        let mut state = UiState::new(theme, keybindings);
        assert!(state.status_message.is_none());
        state.set_status("hello");
        assert!(state.status_message.is_some());
        assert_eq!(state.status_message.as_ref().unwrap().0, "hello");
    }

    #[test]
    fn ui_state_expire_status_fresh() {
        let theme = make_theme();
        let keybindings = crabide_config::KeybindingEngine::with_defaults();
        let mut state = UiState::new(theme, keybindings);
        state.set_status("fresh");
        state.expire_status();
        assert!(state.status_message.is_some());
    }

    #[test]
    fn ui_state_tick_caret_toggles() {
        let theme = make_theme();
        let keybindings = crabide_config::KeybindingEngine::with_defaults();
        let mut state = UiState::new(theme, keybindings);
        assert!(state.caret_visible);
        let toggled = state.tick_caret(1.0);
        assert!(toggled);
        assert!(!state.caret_visible);
    }

    #[test]
    fn ui_state_tick_caret_no_toggle_below_threshold() {
        let theme = make_theme();
        let keybindings = crabide_config::KeybindingEngine::with_defaults();
        let mut state = UiState::new(theme, keybindings);
        state.last_blink_toggle = 0.0;
        let toggled = state.tick_caret(0.1);
        assert!(!toggled);
        assert!(state.caret_visible);
    }

    // ── cfg_to_egui tests ───────────────────────────────────────────────

    #[test]
    fn cfg_to_egui_converts_color() {
        let c = Color::rgba(0x12, 0x34, 0x56, 0xff);
        let e = cfg_to_egui(c);
        assert_eq!(e.r(), 0x12);
        assert_eq!(e.g(), 0x34);
        assert_eq!(e.b(), 0x56);
        assert_eq!(e.a(), 0xff);
    }

    // ── DisplayCell tests ───────────────────────────────────────────────

    #[test]
    fn display_cell_blank_constant() {
        let cell = DisplayCell::BLANK;
        assert_eq!(cell.ch, ' ');
        assert_eq!(cell.fg, TerminalColor::Default);
        assert_eq!(cell.bg, TerminalColor::Default);
    }

    #[test]
    fn display_cell_from_terminal_cell() {
        let tc = TerminalCell {
            ch: 'X',
            fg: TerminalColor::Rgb(255, 0, 0),
            bg: TerminalColor::Rgb(0, 0, 0),
            attrs: crabide_core::event::CellAttrs::empty(),
            hyperlink: None,
        };
        let dc = DisplayCell::from(tc);
        assert_eq!(dc.ch, 'X');
        assert_eq!(dc.fg, TerminalColor::Rgb(255, 0, 0));
    }

    // ── SymbolOutlineEntry tests ────────────────────────────────────────

    #[test]
    fn symbol_outline_entry_fields() {
        let entry = SymbolOutlineEntry {
            name: "main".into(),
            kind: "function".into(),
            line: 42,
        };
        assert_eq!(entry.name, "main");
        assert_eq!(entry.kind, "function");
        assert_eq!(entry.line, 42);
    }

    // ── FileExplorerState tests ─────────────────────────────────────────

    #[test]
    fn file_explorer_state_default() {
        let state = FileExplorerState::default();
        assert!(state.roots.is_empty());
    }

    // ── GitPanelState tests ─────────────────────────────────────────────

    #[test]
    fn git_panel_state_default() {
        let state = GitPanelState::default();
        assert!(!state.visible);
        assert!(state.staged_files.is_empty());
        assert!(state.unstaged_files.is_empty());
        assert!(state.commit_message.is_empty());
        assert!(state.blame_lines.is_empty());
    }

    // ── ExtensionsPanelState tests ──────────────────────────────────────

    #[test]
    fn extensions_panel_state_default() {
        let state = ExtensionsPanelState::default();
        assert_eq!(state.active_tab, ExtensionsPanelTab::Installed);
        assert!(state.selected_id.is_none());
        assert!(state.search_query.is_empty());
        assert!(state.search_results.is_empty());
        assert!(!state.is_searching);
    }

    // ── SidebarTab tests ────────────────────────────────────────────────

    #[test]
    fn sidebar_tab_default_is_explorer() {
        assert_eq!(SidebarTab::default(), SidebarTab::Explorer);
    }

    #[test]
    fn sidebar_tab_extension_pane_equality() {
        let a = SidebarTab::ExtensionPane("p1".into());
        let b = SidebarTab::ExtensionPane("p1".into());
        let c = SidebarTab::ExtensionPane("p2".into());
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    // ── ExtensionsPanelTab tests ────────────────────────────────────────

    #[test]
    fn extensions_panel_tab_default_is_installed() {
        assert_eq!(ExtensionsPanelTab::default(), ExtensionsPanelTab::Installed);
    }

    // ── LspStatus tests ─────────────────────────────────────────────────

    #[test]
    fn lsp_status_derives() {
        assert_ne!(LspStatus::Starting, LspStatus::Ready);
        assert_ne!(LspStatus::Ready, LspStatus::Error);
    }

    // ── GitDecoration tests ─────────────────────────────────────────────

    #[test]
    fn git_decoration_variants() {
        assert_ne!(GitDecoration::Modified, GitDecoration::Added);
        assert_ne!(GitDecoration::Added, GitDecoration::Deleted);
        assert_ne!(GitDecoration::Deleted, GitDecoration::Untracked);
        assert_ne!(GitDecoration::Untracked, GitDecoration::Conflicted);
    }

    // ── FileNode tests ──────────────────────────────────────────────────

    #[test]
    fn file_node_fields() {
        let node = FileNode {
            name: "src".into(),
            path: PathBuf::from("src"),
            is_dir: true,
            children: vec![],
            expanded: false,
            git_status: None,
        };
        assert!(node.is_dir);
        assert!(!node.expanded);
        assert!(node.git_status.is_none());
    }

    // ── CommandPaletteState tests ───────────────────────────────────────

    #[test]
    fn command_palette_default() {
        let state = CommandPaletteState::default();
        assert!(!state.visible);
        assert!(state.query.is_empty());
        assert!(state.entries.is_empty());
        assert_eq!(state.selected_idx, 0);
    }

    // ── WorkspaceSearchState tests ──────────────────────────────────────

    #[test]
    fn workspace_search_default() {
        let state = WorkspaceSearchState::default();
        assert!(!state.visible);
        assert!(state.query.is_empty());
        assert!(!state.use_regex);
        assert!(!state.case_sensitive);
        assert!(state.results.is_empty());
        assert_eq!(state.selected_idx, 0);
        assert!(!state.is_searching);
    }
}

// ── FuzzyFinderState ──────────────────────────────────────────────────────────

/// State for the Ctrl+P fuzzy file finder overlay.
#[derive(Default)]
pub struct FuzzyFinderState {
    /// Whether the overlay is visible.
    pub visible: bool,
    /// Current query typed by the user.
    pub query: String,
    /// Persistent fuzzy finder (holds the file index; rebuilt only on workspace changes).
    pub finder: FuzzyFileFinder,
    /// Matching paths after fuzzy scoring.
    pub results: Vec<PathBuf>,
    /// Index of the currently highlighted result row.
    pub selected_idx: usize,
    /// Display strings parallel to `results` (e.g. relative paths).
    pub result_labels: Vec<String>,
    /// Set to `true` when VFS file changes occur; the index should be rebuilt
    /// on the next `open()` call.
    pub index_stale: bool,
}

impl FuzzyFinderState {
    /// Open (or re-open) the finder and clear the previous query/results.
    pub fn open(&mut self) {
        self.visible = true;
        self.query = String::new();
        self.selected_idx = 0;
        self.results.clear();
        self.result_labels.clear();
        // If the file index is stale due to VFS changes, rebuild it on next open.
        // The caller (app) should call `self.finder.update_index(...)` before
        // or after `open()` if `self.index_stale` is true.
    }

    /// Close and reset the finder.
    pub fn close(&mut self) {
        self.visible = false;
        self.query = String::new();
        self.selected_idx = 0;
        self.results.clear();
        self.result_labels.clear();
    }
}

// ── WorkspaceSearchState ──────────────────────────────────────────────────────

/// State for the Ctrl+Shift+F workspace-grep panel.
#[derive(Default)]
pub struct WorkspaceSearchState {
    /// Whether the panel is visible.
    pub visible: bool,
    /// Query string.
    pub query: String,
    /// Use regular expressions.
    pub use_regex: bool,
    /// Case-sensitive search.
    pub case_sensitive: bool,
    /// Grep results populated by the app.
    pub results: Vec<GrepMatch>,
    /// Index of the currently highlighted result.
    pub selected_idx: usize,
    /// Set to `true` while the app is running the grep in the background.
    pub is_searching: bool,
    /// Set to true when the panel first opens so the query field grabs focus.
    pub just_opened: bool,
    /// Abort handle for the currently running grep (if any).
    pub abort_handle: GrepAbortHandle,
    /// Timestamp of the last query change (for debounce). `None` if unchanged since last search.
    pub last_change: Option<std::time::Instant>,
}

// ── GotoLineState ─────────────────────────────────────────────────────────────

/// State for the Ctrl+G go-to-line dialog.
#[derive(Default)]
pub struct GotoLineState {
    /// Whether the dialog is visible.
    pub visible: bool,
    /// Raw user input (a line number string).
    pub query: String,
}

impl GotoLineState {
    /// Parse `query` as a 1-based line number and return the 0-based index,
    /// or `None` if unparseable or out of range.
    pub fn target_line(&self, max_lines: usize) -> Option<usize> {
        let n: usize = self.query.trim().parse().ok()?;
        if n == 0 {
            return None;
        }
        let zero_based = n - 1;
        if zero_based < max_lines {
            Some(zero_based)
        } else {
            None
        }
    }
}

/// A single segment in the breadcrumb bar above the editor.
#[derive(Debug, Clone)]
pub struct BreadcrumbSegment {
    /// Display name (e.g. "main", "MyStruct", "impl Foo").
    pub name: String,
    /// Symbol kind label (e.g. "function", "struct", "module").
    pub kind: String,
    /// 0-based line to jump to when this segment is clicked.
    pub line: u32,
}

/// A search result for the symbol outline overlay.
#[derive(Debug, Clone)]
pub struct SymbolOutlineEntry {
    pub name: String,
    pub kind: String,
    pub line: u32,
}

/// State for the Go-to-symbol (Ctrl+Shift+O) overlay.
#[derive(Default)]
pub struct SymbolOutlineState {
    pub visible: bool,
    pub query: String,
    pub entries: Vec<SymbolOutlineEntry>,
    pub selected_idx: usize,
}

// ── DapPanelState ─────────────────────────────────────────────────────────────

/// All state for the integrated debugger bottom panel.
pub struct DapPanelState {
    /// Whether the debug panel is visible.
    pub visible: bool,
    /// Whether the debugger feature is enabled (user-toggled).
    pub enabled: bool,

    // ── Session state ─────────────────────────────────────────────────────────
    /// Whether a debug session is currently active.
    pub session_active: bool,
    /// Whether execution is currently paused.
    pub paused: bool,
    /// The thread that triggered the last stop.
    pub paused_thread_id: Option<u64>,
    /// Human-readable reason for the last stop ("breakpoint", "step", …).
    pub stop_reason: Option<String>,

    // ── Launch configuration ──────────────────────────────────────────────────
    /// Available launch configurations parsed from launch.json.
    pub launch_configs: Vec<LaunchConfig>,
    /// Index into `launch_configs` for the selected config.
    pub selected_config_idx: usize,

    // ── Call stack ────────────────────────────────────────────────────────────
    pub call_stack: Vec<StackFrame>,
    /// The currently selected (active) stack frame id.
    pub active_frame_id: Option<u64>,

    // ── Variables (scope ref → variables) ────────────────────────────────────
    pub variables: IndexMap<u32, Vec<Variable>>,
    /// Variable references that have been expanded to show children.
    pub expanded_var_refs: std::collections::HashSet<u64>,

    // ── Watch expressions ─────────────────────────────────────────────────────
    pub watch_expressions: Vec<String>,
    /// Input buffer for adding a new watch expression.
    pub watch_input: String,

    // ── Debug console output ──────────────────────────────────────────────────
    pub console_lines: Vec<(OutputCategory, String)>,
    /// Whether the console should auto-scroll to the bottom.
    pub console_scroll_to_bottom: bool,

    // ── Active sub-tab (0=call stack, 1=variables, 2=watch, 3=console) ────────
    pub active_tab: usize,

    // ── Breakpoint verification status (id → state) ───────────────────────────
    /// Verified/unverified state for breakpoints returned by the adapter.
    pub breakpoint_states: Vec<crabide_core::event::BreakpointState>,

    // ── Pending actions drained by the app each frame ─────────────────────────
    /// Start a debug session with the selected launch config.
    pub pending_launch: bool,
    pub pending_continue: bool,
    pub pending_step_over: bool,
    pub pending_step_in: bool,
    pub pending_step_out: bool,
    pub pending_stop: bool,
    pub pending_restart: bool,
    pub pending_pause: bool,
    /// Request stack trace for the paused thread.
    pub pending_stack_trace: bool,
    /// Expand a variable reference (fetch its children).
    pub pending_expand_var: Option<u64>,
    /// Set/clear breakpoints for these files (path → 0-based line list).
    pub pending_set_breakpoints: Vec<(std::path::PathBuf, Vec<u32>)>,

    // ── Threads ────────────────────────────────────────────────────────────
    /// Thread list from the adapter.
    pub threads: Vec<crabide_core::event::DapThread>,

    // ── Function breakpoints ───────────────────────────────────────────────
    /// Verified function breakpoint states.
    pub function_breakpoints: Vec<crabide_core::event::BreakpointState>,

    // ── Exception info ─────────────────────────────────────────────────────
    /// Last exception description string.
    pub last_exception: Option<String>,

    // ── Goto targets ───────────────────────────────────────────────────────
    /// Available goto targets for "run to cursor".
    pub goto_targets: Vec<crabide_core::event::GotoTarget>,

    // ── Modules ────────────────────────────────────────────────────────────
    /// Loaded modules from the debuggee.
    pub modules: Vec<crabide_core::event::DapModule>,

    // ── Evaluate ───────────────────────────────────────────────────────────
    /// Last evaluate result.
    pub last_evaluate_result: Option<crabide_core::event::EvaluateResult>,

    // ── Progress ───────────────────────────────────────────────────────────
    /// Active progress indicators (progress_id → (title, message, percentage)).
    pub progress: std::collections::HashMap<String, (String, Option<String>, Option<f64>)>,
}

impl Default for DapPanelState {
    fn default() -> Self {
        Self {
            visible: false,
            enabled: false,
            session_active: false,
            paused: false,
            paused_thread_id: None,
            stop_reason: None,
            launch_configs: Vec::new(),
            selected_config_idx: 0,
            call_stack: Vec::new(),
            active_frame_id: None,
            variables: IndexMap::new(),
            expanded_var_refs: std::collections::HashSet::new(),
            watch_expressions: Vec::new(),
            watch_input: String::new(),
            console_lines: Vec::new(),
            console_scroll_to_bottom: true,
            active_tab: 0,
            breakpoint_states: Vec::new(),
            pending_launch: false,
            pending_continue: false,
            pending_step_over: false,
            pending_step_in: false,
            pending_step_out: false,
            pending_stop: false,
            pending_restart: false,
            pending_pause: false,
            pending_stack_trace: false,
            pending_expand_var: None,
            pending_set_breakpoints: Vec::new(),
            threads: Vec::new(),
            function_breakpoints: Vec::new(),
            last_exception: None,
            goto_targets: Vec::new(),
            modules: Vec::new(),
            last_evaluate_result: None,
            progress: std::collections::HashMap::new(),
        }
    }
}

impl DapPanelState {
    /// Append a line to the debug console (capped at 2000 entries).
    pub fn append_console(&mut self, category: OutputCategory, text: String) {
        // Split on newlines so each logical output line is separate.
        for line in text.lines() {
            if self.console_lines.len() >= 2000 {
                self.console_lines.remove(0);
            }
            self.console_lines.push((category.clone(), line.to_owned()));
        }
        self.console_scroll_to_bottom = true;
    }

    /// Reset all session-specific state after termination.
    pub fn reset_session(&mut self) {
        self.session_active = false;
        self.paused = false;
        self.paused_thread_id = None;
        self.stop_reason = None;
        self.call_stack.clear();
        self.active_frame_id = None;
        self.variables.clear();
        self.expanded_var_refs.clear();
        self.breakpoint_states.clear();
        self.threads.clear();
        self.function_breakpoints.clear();
        self.goto_targets.clear();
        self.modules.clear();
        self.last_evaluate_result = None;
        self.last_exception = None;
        self.progress.clear();
    }
}

// ── EditorTab ─────────────────────────────────────────────────────────────────

/// All display state for one open document tab.
pub struct EditorTab {
    pub buffer_id: BufferId,
    pub title: String,
    pub uri: DocumentUri,
    pub language: Language,
    pub is_dirty: bool,
    /// Snapshot of document lines — updated by the app on every edit.
    pub lines: Vec<String>,
    /// Syntax highlight spans for the current snapshot, sorted by start.
    pub highlight_spans: Vec<HighlightSpan>,
    /// LSP diagnostics for this file.
    pub diagnostics: Vec<Diagnostic>,
    /// Git diff hunks for gutter markers.
    pub git_hunks: Vec<DiffHunk>,
    /// Git staged diff hunks (index vs HEAD) for the staged review view.
    pub git_staged_hunks: Vec<DiffHunk>,
    /// Breakpoints set in this file (0-based line numbers).
    pub breakpoints: Vec<u32>,
    /// Gutter markers contributed by extensions for this document.
    pub extension_gutter_markers: Vec<GutterMarker>,
    /// Per-tab cursor / selection state (owned by UI).
    pub cursors: CursorSet,
    /// Snippet engine for tabstop cycling (owned by UI).
    pub snippet_engine: SnippetEngine,
    /// Matching bracket pair for the cursor position: (open_range, close_range).
    pub bracket_match: Option<(Range, Range)>,
    /// Scroll state id used by egui's `ScrollArea` to persist scroll position.
    pub scroll_id: egui::Id,
    /// Anchor position when the user is drag-selecting with the mouse.
    /// `None` when no drag is in progress.
    pub drag_anchor: Option<Position>,
    /// Anchor position when the user is performing a column (box) selection
    /// with Shift+Alt+drag.  Set on press; cleared on release.
    pub column_select_anchor: Option<Position>,
    /// Timestamp (egui time) of the most recent primary-button press in this tab.
    pub last_click_time: f64,
    /// Document position of the most recent primary-button press.
    pub last_click_pos: Option<Position>,
    /// Consecutive click count at the same position (1 = single, 2 = double, 3+ = triple).
    pub click_count: u32,
    /// LSP inlay hints (parameter names, type hints) rendered inline.
    pub inlay_hints: Vec<crabide_core::event::InlayHint>,
    /// LSP semantic tokens for syntax highlighting.
    pub semantic_tokens: Vec<crabide_core::event::SemanticToken>,
    /// LSP code lens items (clickable links above functions).
    pub code_lens: Vec<crabide_core::event::CodeLens>,
    /// Folding ranges from syntax engine or LSP (sorted by start_line).
    pub folding_ranges: Vec<FoldingRange>,
    /// Bit-set tracking which folding ranges are collapsed (indices into folding_ranges).
    pub collapsed_folds: Vec<usize>,
    /// Breadcrumb segments for the cursor position (file → module → struct → function).
    pub breadcrumbs: Vec<BreadcrumbSegment>,
}

impl EditorTab {
    pub fn new(buffer_id: BufferId, title: String, uri: DocumentUri, language: Language) -> Self {
        let scroll_id = egui::Id::new(("tab_scroll", buffer_id));
        Self {
            buffer_id,
            title,
            uri,
            language,
            is_dirty: false,
            lines: Vec::new(),
            highlight_spans: Vec::new(),
            diagnostics: Vec::new(),
            git_hunks: Vec::new(),
            git_staged_hunks: Vec::new(),
            breakpoints: Vec::new(),
            extension_gutter_markers: Vec::new(),
            cursors: CursorSet::new(),
            snippet_engine: SnippetEngine::new(),
            bracket_match: None,
            scroll_id,
            drag_anchor: None,
            column_select_anchor: None,
            last_click_time: 0.0,
            last_click_pos: None,
            click_count: 0,
            inlay_hints: Vec::new(),
            semantic_tokens: Vec::new(),
            code_lens: Vec::new(),
            folding_ranges: Vec::new(),
            collapsed_folds: Vec::new(),
            breadcrumbs: Vec::new(),
        }
    }

    /// Returns a copy of the current tabstop being edited, if any.
    pub fn active_tabstop(&self) -> Option<SnippetTabstop> {
        self.snippet_engine.current_tabstop().cloned()
    }
}

// ── FileExplorer ──────────────────────────────────────────────────────────────

/// Decoration from git status shown on a file tree node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitDecoration {
    Modified,
    Added,
    Deleted,
    Untracked,
    Conflicted,
}

/// A node in the file-explorer tree.
#[derive(Debug, Clone)]
pub struct FileNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    /// Only non-empty for directories after expansion.
    pub children: Vec<FileNode>,
    pub expanded: bool,
    pub git_status: Option<GitDecoration>,
}

/// State of the file-explorer sidebar panel.
#[derive(Debug, Default)]
pub struct FileExplorerState {
    /// Top-level workspace roots (populated by the app).
    pub roots: Vec<FileNode>,
}

// ── GitPanelState ─────────────────────────────────────────────────────────────

/// All state for the Source Control git panel.
pub struct GitPanelState {
    /// Whether the git panel is visible as a bottom strip.
    pub visible: bool,

    /// Files whose index (staged) status is non-trivial.
    pub staged_files: Vec<FileStatus>,
    /// Files whose worktree (unstaged) status is non-trivial.
    pub unstaged_files: Vec<FileStatus>,

    /// Commit message typed by the user.
    pub commit_message: String,

    /// Blame lines keyed by absolute path, populated on request.
    pub blame_lines: IndexMap<PathBuf, Vec<BlameLine>>,

    /// List of local and remote branches.
    pub branches: Vec<BranchInfo>,

    /// Stash entries (most recent first).
    pub stash_entries: Vec<StashEntry>,

    /// Commit log entries (most recent first).
    pub log_entries: Vec<CommitEntry>,

    /// List of tags (lightweight and annotated).
    pub tags: Vec<TagInfo>,

    /// List of remotes.
    pub remotes: Vec<RemoteInfo>,

    /// List of submodules.
    pub submodules: Vec<SubmoduleInfo>,

    /// List of conflicted files during merge/rebase.
    pub conflicts: Vec<ConflictInfo>,

    // ── Pending actions drained by the app each frame ─────────────────────────
    pub pending_stage_file: Option<PathBuf>,
    pub pending_unstage_file: Option<PathBuf>,
    pub pending_stage_all: bool,
    pub pending_unstage_all: bool,
    /// When true the app should call `git_service.commit(commit_message)`.
    pub pending_commit: bool,
    pub pending_blame_request: Option<PathBuf>,
    pub pending_discard_file: Option<PathBuf>,
}

impl Default for GitPanelState {
    fn default() -> Self {
        Self {
            visible: false,
            staged_files: Vec::new(),
            unstaged_files: Vec::new(),
            commit_message: String::new(),
            blame_lines: IndexMap::new(),
            branches: Vec::new(),
            stash_entries: Vec::new(),
            log_entries: Vec::new(),
            tags: Vec::new(),
            remotes: Vec::new(),
            submodules: Vec::new(),
            conflicts: Vec::new(),
            pending_stage_file: None,
            pending_unstage_file: None,
            pending_stage_all: false,
            pending_unstage_all: false,
            pending_commit: false,
            pending_blame_request: None,
            pending_discard_file: None,
        }
    }
}

// ── Terminal ──────────────────────────────────────────────────────────────────

/// A single cell in the terminal display grid (UI copy).
#[derive(Clone)]
pub struct DisplayCell {
    pub ch: char,
    pub fg: TerminalColor,
    pub bg: TerminalColor,
    pub attrs: crabide_core::event::CellAttrs,
    /// OSC 8 hyperlink URL, if this cell is part of a clickable hyperlink.
    pub hyperlink: Option<String>,
}

impl DisplayCell {
    pub const BLANK: Self = Self {
        ch: ' ',
        fg: TerminalColor::Default,
        bg: TerminalColor::Default,
        attrs: crabide_core::event::CellAttrs::empty(),
        hyperlink: None,
    };
}

impl From<TerminalCell> for DisplayCell {
    fn from(c: TerminalCell) -> Self {
        Self {
            ch: c.ch,
            fg: c.fg,
            bg: c.bg,
            attrs: c.attrs,
            hyperlink: c.hyperlink,
        }
    }
}

/// UI-side state for one running terminal instance.
pub struct TerminalInstance {
    pub id: u32,
    pub title: String,
    pub cwd: Option<PathBuf>,
    /// Visible grid rows (row-major: `rows[row][col]`).
    pub rows: Vec<Vec<DisplayCell>>,
    pub cols: u16,
    pub grid_rows: u16,
    /// Cursor position in the visible grid.
    pub cursor_col: u16,
    pub cursor_row: u16,
    /// Whether the terminal application wants the cursor visible (DECSET 25).
    pub cursor_visible: bool,
    /// How far the user has scrolled up into scrollback (0 = bottom).
    pub scroll_offset: u32,
    /// Total scrollback rows available (updated from delta).
    pub scrollback_len: u32,
    /// When true the terminal process has exited.
    pub exited: bool,
    /// Whether bracketed paste mode is active (DECSET 2004).
    pub bracketed_paste: bool,
    /// Whether X10 mouse reporting is active (DECSET 1000).
    pub mouse_x10: bool,
    /// Whether normal mouse tracking is active (DECSET 1002).
    pub mouse_normal: bool,
    /// Whether button-event mouse tracking is active (DECSET 1003).
    pub mouse_button_event: bool,
    /// Whether SGR extended mouse mode is active (DECSET 1006).
    pub mouse_sgr: bool,
    /// Color scheme for rendering this terminal.
    pub color_scheme: TerminalColorScheme,
}
impl TerminalInstance {
    pub fn new(id: u32, cols: u16, grid_rows: u16, color_scheme: TerminalColorScheme) -> Self {
        let blank_row = vec![DisplayCell::BLANK; cols as usize];
        Self {
            id,
            title: format!("Terminal {id}"),
            cwd: None,
            rows: vec![blank_row; grid_rows as usize],
            cols,
            grid_rows,
            cursor_col: 0,
            cursor_row: 0,
            cursor_visible: true,
            scroll_offset: 0,
            scrollback_len: 0,
            exited: false,
            bracketed_paste: false,
            mouse_x10: false,
            mouse_normal: false,
            mouse_button_event: false,
            mouse_sgr: false,
            color_scheme,
        }
    }
}

impl TerminalInstance {
    /// Apply a grid delta received from the PTY reader.
    pub fn apply_delta(&mut self, delta: &crabide_core::event::TerminalGridDelta) {
        self.cursor_col = delta.cursor_col;
        self.cursor_row = delta.cursor_row;
        self.cursor_visible = delta.cursor_visible;
        self.bracketed_paste = delta.bracketed_paste;
        self.mouse_x10 = delta.mouse_x10;
        self.mouse_normal = delta.mouse_normal;
        self.mouse_button_event = delta.mouse_button_event;
        self.mouse_sgr = delta.mouse_sgr;
        self.scrollback_len = delta.scroll_top;

        for changed in &delta.rows {
            let row = changed.row as usize;
            // Grow rows vec if needed (terminal resized larger).
            while self.rows.len() <= row {
                self.rows.push(vec![DisplayCell::BLANK; self.cols as usize]);
            }
            // Ensure each row is wide enough.
            if self.rows[row].len() < changed.cells.len() {
                self.rows[row].resize(changed.cells.len(), DisplayCell::BLANK);
            }
            for (col, cell) in changed.cells.iter().enumerate() {
                if col < self.rows[row].len() {
                    self.rows[row][col] = DisplayCell::from(cell.clone());
                }
            }
        }
    }

    /// Handle a PTY resize: update cols/rows and re-blank.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.grid_rows = rows;
        // Resize each row
        for row in &mut self.rows {
            row.resize(cols as usize, DisplayCell::BLANK);
        }
        // Add or remove rows
        let blank_row = || vec![DisplayCell::BLANK; cols as usize];
        self.rows.resize_with(rows as usize, blank_row);
    }
}

/// State for the integrated terminal bottom panel.
#[derive(Default)]
pub struct TerminalPanelState {
    /// Whether the terminal panel is visible.
    pub visible: bool,
    /// All running terminal instances.
    pub instances: Vec<TerminalInstance>,
    /// Index into `instances` for the focused terminal.
    pub active_idx: usize,
    /// Whether the terminal area has keyboard focus.
    pub has_focus: bool,

    // ── Pending actions drained by the app each frame ─────────────────────────
    /// Request to open a new terminal.
    pub pending_new: bool,
    /// Request to kill terminal by id.
    pub pending_kill: Option<u32>,
    /// Request to send input bytes to a terminal.
    pub pending_input: Vec<(u32, Vec<u8>)>,
    /// Request to resize a terminal (id, cols, rows).
    pub pending_resize: Option<(u32, u16, u16)>,
    // (resize debounce field reserved for future use)
}

impl TerminalPanelState {
    /// The active terminal instance, if any.
    pub fn active(&self) -> Option<&TerminalInstance> {
        self.instances.get(self.active_idx)
    }

    /// The active terminal instance (mutable), if any.
    pub fn active_mut(&mut self) -> Option<&mut TerminalInstance> {
        self.instances.get_mut(self.active_idx)
    }

    /// Find a mutable reference to a terminal by id.
    pub fn by_id_mut(&mut self, id: u32) -> Option<&mut TerminalInstance> {
        self.instances.iter_mut().find(|t| t.id == id)
    }

    /// Remove a terminal by id.
    pub fn remove_by_id(&mut self, id: u32) {
        if let Some(pos) = self.instances.iter().position(|t| t.id == id) {
            self.instances.remove(pos);
            if self.active_idx >= self.instances.len() && !self.instances.is_empty() {
                self.active_idx = self.instances.len() - 1;
            }
        }
    }
}

// ── CommandPalette ────────────────────────────────────────────────────────────

/// A single entry shown in the command palette results list.
#[derive(Clone)]
pub struct PaletteEntry {
    pub action: Action,
    pub label: String,
    /// Formatted keybinding string, e.g. `"Ctrl+Shift+P"`.
    pub shortcut: String,
}

/// State for the command-palette overlay (Ctrl+Shift+P).
#[derive(Default)]
pub struct CommandPaletteState {
    pub visible: bool,
    pub query: String,
    pub entries: Vec<PaletteEntry>,
    pub selected_idx: usize,
}

// ── Sidebar tab ───────────────────────────────────────────────────────────────

/// Which view is shown in the collapsible left sidebar.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SidebarTab {
    #[default]
    Explorer,
    Extensions,
    /// An extension-contributed left-sidebar pane (identified by pane id).
    ExtensionPane(String),
}

// ── ExtensionsPanelState ──────────────────────────────────────────────────────

/// Active sub-tab in the Extensions panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExtensionsPanelTab {
    #[default]
    Installed,
    Search,
    Recommended,
}

/// A single status-bar contribution from an extension.
pub struct StatusBarItem {
    pub text: String,
    pub tooltip: Option<String>,
    /// Extension command to execute when the item is clicked.
    pub command: Option<String>,
    /// Which side of the status bar this item docks to.
    pub alignment: StatusBarAlignment,
}

/// All state for the Extensions manager panel.
pub struct ExtensionsPanelState {
    // ── Panel navigation ──────────────────────────────────────────────────────
    pub active_tab: ExtensionsPanelTab,
    /// Currently selected extension id for detail / action.
    pub selected_id: Option<String>,

    // ── Search tab ────────────────────────────────────────────────────────────
    pub search_query: String,
    pub search_results: Vec<RegistryExtension>,
    pub is_searching: bool,
    /// Focus the search input on next frame.
    pub just_opened_search: bool,

    // ── Recommended tab cache ─────────────────────────────────────────────────
    pub recommended: Vec<RegistryExtension>,

    // ── Installed extension cache (refreshed by app each frame) ──────────────
    /// Snapshot of `ExtensionHost::installed()` so the panel can render the list.
    pub installed: Vec<InstalledExtension>,

    // ── Extension outputs displayed inside this panel ─────────────────────────
    /// Status bar text slots keyed by extension id.
    pub status_bar_items: IndexMap<String, StatusBarItem>,

    // ── Pending actions drained by the app each frame ─────────────────────────
    /// Open the native file picker to load a `.wasm` extension.
    pub pending_install_local: bool,
    /// Toggle enabled state of an extension (id).
    pub pending_toggle: Option<String>,
    /// Uninstall an extension (id).
    pub pending_uninstall: Option<String>,
    /// Search registry with this query string.
    pub pending_search: Option<String>,
    /// Install an extension from the registry (id).
    pub pending_install_registry: Option<String>,
    /// Execute an extension command.
    pub pending_execute_command: Option<(String, Vec<String>)>,
    /// Request the app cycle to the next theme (set by ThemeSwitcher extension).
    pub pending_cycle_theme: bool,
}

impl Default for ExtensionsPanelState {
    fn default() -> Self {
        Self {
            active_tab: ExtensionsPanelTab::Installed,
            selected_id: None,
            installed: Vec::new(),
            search_query: String::new(),
            search_results: Vec::new(),
            is_searching: false,
            just_opened_search: false,
            recommended: Vec::new(),
            status_bar_items: IndexMap::new(),
            pending_install_local: false,
            pending_toggle: None,
            pending_uninstall: None,
            pending_search: None,
            pending_install_registry: None,
            pending_execute_command: None,
            pending_cycle_theme: false,
        }
    }
}

// ── ExtensionPanelUiState ─────────────────────────────────────────────────────

/// Runtime state for one dynamically-registered extension panel.
pub struct ExtensionPanelUiState {
    /// The static registration provided by the extension.
    pub registration: PanelRegistration,
    /// Latest content blocks pushed by the extension's `poll()`.
    pub content: Vec<ContentBlock>,
    /// Whether the panel is currently visible.
    pub open: bool,
}

// ── SidebarPaneUiState ────────────────────────────────────────────────────────

/// Runtime state for one dynamically-registered left-sidebar pane.
pub struct SidebarPaneUiState {
    pub registration: SidebarPaneRegistration,
    pub content: Vec<crabide_extensions::ContentBlock>,
    pub visible: bool,
}

// ── LSP indicator ─────────────────────────────────────────────────────────────

/// Displayed in the status bar per language server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspStatus {
    Starting,
    Ready,
    Error,
}

// ── Context Menu State ────────────────────────────────────────────────────────

/// State for the right-click context menu popup.
#[derive(Default)]
pub struct ContextMenuState {
    /// Whether the context menu is currently visible.
    pub visible: bool,
    /// Screen position where the menu should appear.
    pub pos: egui::Pos2,
    /// Items to display (built-in + extension contributions).
    pub items: Vec<ContextMenuItem>,
    /// Which context triggered the menu.
    pub context: ContextMenuContext,
}

/// A single item in the context menu.
#[derive(Clone)]
pub struct ContextMenuItem {
    pub label: String,
    /// Action or command to execute when clicked.
    pub action: ContextMenuAction,
}

/// What happens when a context menu item is activated.
#[derive(Clone)]
pub enum ContextMenuAction {
    /// A built-in editor action (e.g. Cut, Copy, Paste).
    Action(crabide_config::Action),
    /// An extension command to execute.
    Command(String),
}

/// Which surface triggered the context menu.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ContextMenuContext {
    #[default]
    Editor,
    FileExplorer,
    TabBar,
    Terminal,
}

// ── EditorGroup ──────────────────────────────────────────────────────

/// A group of editor tabs displayed in one editor pane.
///
/// When the editor is not split, there is a single group (index 0).
/// Split operations create additional groups, each with their own tabs
/// and active tab index.
pub struct EditorGroup {
    /// All open tabs in this group.
    pub tabs: Vec<EditorTab>,
    /// Index into `tabs` for the active (focused) tab in this group.
    pub active_tab: Option<usize>,
}

impl Default for EditorGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorGroup {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active_tab: None,
        }
    }

    /// Open a tab for `tab`, or activate an existing one for the same buffer.
    pub fn open_tab(&mut self, tab: EditorTab) {
        if let Some(idx) = self.tabs.iter().position(|t| t.buffer_id == tab.buffer_id) {
            self.active_tab = Some(idx);
        } else {
            self.active_tab = Some(self.tabs.len());
            self.tabs.push(tab);
        }
    }

    /// Close the tab at `idx`.  Returns the closed tab's `BufferId` if any.
    pub fn close_tab(&mut self, idx: usize) -> Option<BufferId> {
        if idx >= self.tabs.len() {
            return None;
        }
        let id = self.tabs[idx].buffer_id;
        self.tabs.remove(idx);
        self.active_tab = if self.tabs.is_empty() {
            None
        } else {
            Some(idx.saturating_sub(1).min(self.tabs.len() - 1))
        };
        Some(id)
    }

    /// Mutable reference to the active tab, if any.
    pub fn active_tab_mut(&mut self) -> Option<&mut EditorTab> {
        self.active_tab.and_then(|i| self.tabs.get_mut(i))
    }

    /// Shared reference to the active tab, if any.
    pub fn active_tab_ref(&self) -> Option<&EditorTab> {
        self.active_tab.and_then(|i| self.tabs.get(i))
    }
}

// ── OutputPanelState ──────────────────────────────────────────────────────────

/// State for the output panel (shows text output from tasks, extensions, etc.).
#[derive(Default)]
pub struct OutputPanelState {
    /// Whether the panel is visible.
    pub visible: bool,
    /// The currently selected output channel name (e.g. "Tasks", "Extension Host").
    pub active_channel: String,
    /// Available channel names.
    pub channels: Vec<String>,
    /// Lines of text for each channel, keyed by channel name.
    pub channel_lines: indexmap::IndexMap<String, Vec<String>>,
    /// When true, auto-scroll to bottom on new output.
    pub auto_scroll: bool,
    /// Set to true to force scroll-to-bottom next frame.
    pub scroll_to_bottom: bool,
}

impl OutputPanelState {
    /// Append a line to a named channel, creating it if needed.
    pub fn append_line(&mut self, channel: &str, line: String) {
        if !self.channels.contains(&channel.to_owned()) {
            self.channels.push(channel.to_owned());
        }
        let lines = self.channel_lines.entry(channel.to_owned()).or_default();
        // Cap to 5000 lines per channel to avoid unbounded memory.
        if lines.len() >= 5000 {
            lines.remove(0);
        }
        lines.push(line);
        if self.auto_scroll {
            self.scroll_to_bottom = true;
        }
    }
}

// ── PeekState ──────────────────────────────────────────────────────────────

/// State for the peek view (inline definition/reference preview).
#[derive(Default)]
pub struct PeekState {
    /// Whether the peek overlay is visible.
    pub visible: bool,
    /// The peek kind (definition, references, implementation, etc.).
    pub kind: Option<PeekKind>,
    /// All locations returned by the LSP server.
    pub locations: Vec<Location>,
    /// Index into `locations` for the currently selected location.
    pub selected_idx: usize,
    /// The URI this peek was triggered from (to track context).
    pub origin_uri: Option<DocumentUri>,
    /// The line/column this peek was triggered from.
    pub origin_pos: Option<Position>,
}

/// The kind of peek view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeekKind {
    Definition,
    Declaration,
    Implementation,
    TypeDefinition,
    References,
}

impl PeekState {
    /// The currently selected location, if any.
    pub fn selected_location(&self) -> Option<&Location> {
        self.locations.get(self.selected_idx)
    }

    /// Navigate to the next location (wraps around).
    pub fn next(&mut self) {
        if !self.locations.is_empty() {
            self.selected_idx = (self.selected_idx + 1) % self.locations.len();
        }
    }

    /// Navigate to the previous location (wraps around).
    pub fn prev(&mut self) {
        if !self.locations.is_empty() {
            self.selected_idx = if self.selected_idx == 0 {
                self.locations.len() - 1
            } else {
                self.selected_idx - 1
            };
        }
    }

    /// Close the peek view and reset state.
    pub fn close(&mut self) {
        self.visible = false;
        self.kind = None;
        self.locations.clear();
        self.selected_idx = 0;
        self.origin_uri = None;
        self.origin_pos = None;
    }

    /// Open a peek view with the given kind and locations.
    pub fn open(
        &mut self,
        kind: PeekKind,
        locations: Vec<Location>,
        origin_uri: DocumentUri,
        origin_pos: Position,
    ) {
        self.visible = true;
        self.kind = Some(kind);
        self.locations = locations;
        self.selected_idx = 0;
        self.origin_uri = Some(origin_uri);
        self.origin_pos = Some(origin_pos);
    }
}

// ── ThemePickerState ───────────────────────────────────────────────────────────

/// State for the theme picker panel.
#[derive(Default)]
pub struct ThemePickerState {
    /// Whether the theme picker panel is visible.
    pub visible: bool,
    /// List of available themes as (id, display_name).
    pub themes: Vec<(String, String)>,
    /// Index of the currently selected theme in the list.
    pub selected_idx: usize,
    /// When set, the app should apply this theme. Drained by app each frame.
    pub pending_theme_id: Option<String>,
}

// ── KeybindingsEditorState ──────────────────────────────────────────────────────

/// State for the keybindings editor panel.
#[derive(Default)]
pub struct KeybindingsEditorState {
    /// Whether the keybindings editor is visible.
    pub visible: bool,
    /// All keybindings as (action_label, key_combo).
    pub bindings: Vec<(String, String)>,
    /// Search query to filter the list.
    pub query: String,
}

// ── SettingsPanelState ──────────────────────────────────────────────────────────

/// State for the settings editor panel.
#[derive(Default)]
pub struct SettingsPanelState {
    /// Whether the settings panel is visible.
    pub visible: bool,
    /// The current settings fields as a list of (group, key, value, field_type).
    /// Populated by the app each frame from `Settings`.
    pub fields: Vec<SettingsField>,
}

/// A single editable field in the settings panel.
#[derive(Clone)]
pub struct SettingsField {
    pub group: String,
    pub key: String,
    pub value: String,
    pub field_type: SettingsFieldType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsFieldType {
    Bool,
    Int,
    Float,
    String,
    Enum(Vec<String>),
}

// ── UiState ───────────────────────────────────────────────────────────────────

/// Complete mutable UI state for the editor, owned by the application.
/// All render functions borrow this mutably so they can update scroll
/// positions, cursor state, palette input, etc. in the same frame they render.
pub struct UiState {
    // ── Theme & bindings ──────────────────────────────────────────────────────
    pub theme: ColorTheme,
    pub keybindings: KeybindingEngine,
    /// Runtime context for evaluating keybinding `when` conditions.
    /// Populated by the app layer before each frame and consumed during key
    /// event processing in the UI layer.
    pub when_context: WhenContext,

    // ── Editor tabs (multi-group) ─────────────────────────────────────────────
    /// All editor groups. The first group (index 0) always exists.
    /// Additional groups are created by split operations.
    pub editor_groups: Vec<EditorGroup>,
    /// Index into `editor_groups` for the currently focused group.
    pub active_group: usize,

    // ── Panel layout (egui_tiles) ─────────────────────────────────────────────
    pub layout: egui_tiles::Tree<PaneKind>,
    pub sidebar_visible: bool,

    // ── File explorer ─────────────────────────────────────────────────────────
    pub file_explorer: FileExplorerState,

    // ── Command palette ───────────────────────────────────────────────────────
    pub command_palette: CommandPaletteState,

    // ── Find / Replace ────────────────────────────────────────────────────────
    pub find_replace: FindReplaceState,

    // ── Fuzzy file finder (Ctrl+P) ────────────────────────────────────────────
    pub fuzzy_finder: FuzzyFinderState,

    // ── Workspace grep (Ctrl+Shift+F) ─────────────────────────────────────────
    pub workspace_search: WorkspaceSearchState,

    // ── Go-to-line (Ctrl+G) ───────────────────────────────────────────────────
    pub goto_line: GotoLineState,

    // ── Symbol outline (Ctrl+Shift+O) ──────────────────────────────────────────
    pub symbol_outline: SymbolOutlineState,

    // ── Terminal panel ────────────────────────────────────────────────────────
    pub terminal: TerminalPanelState,

    // ── Git panel ─────────────────────────────────────────────────────────────
    pub git_panel: GitPanelState,

    // ── Git enable/disable ─────────────────────────────────────────────────────
    /// Whether the git service is enabled. Off by default to avoid the ~100 MB
    /// libgit2 RSS cost for users who don't need source control in every project.
    pub git_enabled: bool,

    // ── Debug panel ───────────────────────────────────────────────────────────
    pub dap_panel: DapPanelState,

    // ── Extensions panel ──────────────────────────────────────────────────────
    pub extensions_panel: ExtensionsPanelState,

    // ── Sidebar active tab ────────────────────────────────────────────────────
    pub sidebar_tab: SidebarTab,

    // ── Context info from background services ─────────────────────────────────
    pub git_branch: Option<String>,
    pub lsp_indicators: IndexMap<String, LspStatus>,

    // ── Timed status message (3-second TTL) ───────────────────────────────────
    pub status_message: Option<(String, Instant)>,

    // ── Caret blink ───────────────────────────────────────────────────────────
    pub(crate) caret_visible: bool,
    pub(crate) last_blink_toggle: f64, // egui time in seconds

    // ── Typography ────────────────────────────────────────────────────────────
    pub font_size: f32,

    // ── Word wrap ─────────────────────────────────────────────────────────────
    pub word_wrap: bool,

    // ── Minimap visibility ──────────────────────────────────────────────────
    pub minimap_visible: bool,

    // ── Context menu state ─────────────────────────────────────────────────
    pub context_menu: ContextMenuState,

    // ── Pending file open (set by file explorer or fuzzy finder) ─────────────
    /// App drains this each frame; it maps to `Action::OpenFile`.
    pub pending_open_path: Option<PathBuf>,

    // ── Pending tab close (set by tab bar close button) ───────────────────────
    /// App drains this each frame after handling `Action::CloseTab`.
    pub pending_close_buffer: Option<BufferId>,

    // ── Pending scroll-to-line (set by goto-line / find-in-files) ────────────
    /// When Some, the editor scrolls to this 0-based line on the next frame.
    pub pending_scroll_line: Option<usize>,

    // ── Problems panel (diagnostics) ──────────────────────────────────────────
    /// True when the Problems bottom panel is visible.
    pub problems_panel_open: bool,

    // ── Output panel (toggle via ToggleOutputPanel) ───────────────────────────
    pub output_panel: OutputPanelState,

    // ── Dynamic extension panels ──────────────────────────────────────────────
    /// All dynamically-registered extension panels, keyed by panel id.
    pub extension_panels: IndexMap<String, ExtensionPanelUiState>,

    // ── Extension navigation request ──────────────────────────────────────────
    /// Set by extension panel row clicks; drained by the app each frame.
    pub pending_navigate: Option<NavigateTarget>,

    // ── Extension commands (for command palette) ───────────────────────────────
    /// Registry of custom actions contributed by extensions.
    pub action_registry: crabide_config::ActionRegistry,

    // ── Extension sidebar panes ───────────────────────────────────────────────
    /// Left-sidebar panes registered by extensions.
    pub sidebar_panes: IndexMap<String, SidebarPaneUiState>,

    // ── Extension context-menu contributions ──────────────────────────────────
    /// Context-menu items contributed by extensions (refreshed each frame).
    pub registered_context_menus: Vec<ContextMenuContribution>,

    // ── LSP hover / completion / code action state ────────────────────────────
    /// Text content of the hover popup (set by LSP HoverReady).
    pub hover_text: Option<String>,
    /// Completion items from the LSP server (set by CompletionReady).
    pub completion_items: Vec<crabide_core::event::CompletionItem>,
    /// Whether the completion popup is visible.
    pub completion_visible: bool,
    /// Code actions from the LSP server (set by CodeActionsReady).
    pub code_actions: Vec<crabide_core::event::CodeAction>,
    /// Whether the code actions popup is visible.
    pub code_actions_visible: bool,
    /// Signature help result from the LSP server (set by SignatureHelpReady).
    pub signature_help: Option<crabide_core::event::SignatureHelp>,

    // ── LSP popup selection state ─────────────────────────────────────────
    /// Currently selected completion item index (for keyboard navigation).
    pub completion_selected_idx: usize,
    /// Currently selected code action index (for keyboard navigation).
    pub code_actions_selected_idx: usize,
    /// Pending completion insert text (set by popup, drained by app).
    pub pending_completion_insert: Option<String>,
    /// Pending code action index (set by popup, drained by app).
    pub pending_code_action_idx: Option<usize>,

    // ── Peek view state ───────────────────────────────────────────────────
    pub peek: PeekState,
    /// When set, the next LocationsReady event will be treated as peek results
    /// for the given LSP method. Drained by apply_lsp_event.
    pub pending_peek_method: Option<String>,

    // ── Theme picker state ───────────────────────────────────────────────
    pub theme_picker: ThemePickerState,
    // ── Keybindings editor state ─────────────────────────────────────────
    pub keybindings_editor: KeybindingsEditorState,
    // ── Settings panel state ─────────────────────────────────────────────
    pub settings_panel: SettingsPanelState,
    // ── Update check state ───────────────────────────────────────────────
    /// When `Some`, contains the latest available version string.
    pub update_available: Option<String>,
}

impl UiState {
    pub fn new(theme: ColorTheme, keybindings: KeybindingEngine) -> Self {
        Self {
            theme,
            keybindings,
            when_context: WhenContext::new(),
            editor_groups: vec![EditorGroup::new()],
            active_group: 0,
            layout: default_layout(),
            sidebar_visible: true,
            file_explorer: FileExplorerState::default(),
            command_palette: CommandPaletteState::default(),
            find_replace: FindReplaceState::default(),
            fuzzy_finder: FuzzyFinderState::default(),
            workspace_search: WorkspaceSearchState::default(),
            goto_line: GotoLineState::default(),
            symbol_outline: SymbolOutlineState::default(),
            terminal: TerminalPanelState::default(),
            git_panel: GitPanelState::default(),
            git_enabled: false,
            dap_panel: DapPanelState::default(),
            extensions_panel: ExtensionsPanelState::default(),
            sidebar_tab: SidebarTab::Explorer,
            git_branch: None,
            lsp_indicators: IndexMap::new(),
            status_message: None,
            caret_visible: true,
            last_blink_toggle: 0.0,
            font_size: 14.0,
            word_wrap: false,
            pending_open_path: None,
            pending_close_buffer: None,
            pending_scroll_line: None,
            minimap_visible: false,
            context_menu: ContextMenuState::default(),
            problems_panel_open: false,
            output_panel: OutputPanelState::default(),
            extension_panels: IndexMap::new(),
            pending_navigate: None,
            action_registry: crabide_config::ActionRegistry::new(),
            sidebar_panes: IndexMap::new(),
            registered_context_menus: Vec::new(),
            hover_text: None,
            completion_items: Vec::new(),
            completion_visible: false,
            code_actions: Vec::new(),
            code_actions_visible: false,
            signature_help: None,
            completion_selected_idx: 0,
            code_actions_selected_idx: 0,
            pending_completion_insert: None,
            pending_code_action_idx: None,
            peek: PeekState::default(),
            pending_peek_method: None,
            theme_picker: ThemePickerState::default(),
            keybindings_editor: KeybindingsEditorState::default(),
            settings_panel: SettingsPanelState::default(),
            update_available: None,
        }
    }

    // ── Editor group helpers ──────────────────────────────────────────────

    /// Shared reference to the active editor group.
    pub fn active_group_ref(&self) -> &EditorGroup {
        &self.editor_groups[self.active_group]
    }

    /// Mutable reference to the active editor group.
    pub fn active_group_mut(&mut self) -> &mut EditorGroup {
        &mut self.editor_groups[self.active_group]
    }

    /// Index of the active tab in the active group, if any.
    pub fn active_tab(&self) -> Option<usize> {
        self.active_group_ref().active_tab
    }

    /// The number of tabs in the active group.
    pub fn tab_count(&self) -> usize {
        self.active_group_ref().tabs.len()
    }

    /// Shared reference to the tabs of the active group.
    pub fn tabs(&self) -> &[EditorTab] {
        &self.active_group_ref().tabs
    }

    /// Mutable reference to the tabs of the active group.
    pub fn tabs_mut(&mut self) -> &mut Vec<EditorTab> {
        &mut self.active_group_mut().tabs
    }

    // ── Tab management (delegates to active group) ─────────────────────────

    /// Open a tab for `tab`, or activate an existing one for the same buffer.
    pub fn open_tab(&mut self, tab: EditorTab) {
        self.active_group_mut().open_tab(tab);
    }

    /// Close the tab at `idx`.  Returns the closed tab's `BufferId` if any.
    pub fn close_tab(&mut self, idx: usize) -> Option<BufferId> {
        self.active_group_mut().close_tab(idx)
    }

    /// Mutable reference to the active tab, if any.
    pub fn active_tab_mut(&mut self) -> Option<&mut EditorTab> {
        self.active_group_mut().active_tab_mut()
    }

    /// Shared reference to the active tab, if any.
    pub fn active_tab_ref(&self) -> Option<&EditorTab> {
        self.active_group_ref().active_tab_ref()
    }

    /// Display a timed status message.
    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some((msg.into(), Instant::now()));
    }

    /// Expire status messages older than 3 seconds.
    pub fn expire_status(&mut self) {
        if let Some((_, ts)) = &self.status_message {
            if ts.elapsed().as_secs() >= 3 {
                self.status_message = None;
            }
        }
    }

    // ── Caret blink ───────────────────────────────────────────────────────────

    /// Advance caret blink state using current egui time.
    /// Returns `true` when a repaint is needed (caret toggled this call).
    pub fn tick_caret(&mut self, now_secs: f64) -> bool {
        if now_secs - self.last_blink_toggle >= 0.530 {
            self.caret_visible = !self.caret_visible;
            self.last_blink_toggle = now_secs;
            true
        } else {
            false
        }
    }
}
