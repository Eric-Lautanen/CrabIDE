//! Unit tests for `crabide-extensions`.

use crabide_extensions::{
    is_output_allowed, CommandResult, CompletionItem, CompletionKind, ContentBlock,
    ContextMenuContext, ContextMenuContribution, ExtensionCapabilities, ExtensionCategory,
    ExtensionContext, ExtensionDiagnostic, ExtensionHost, ExtensionManifest, ExtensionOutput,
    ExtensionSeverity, ExtensionSource, GutterMarker, HoverResult, InstalledExtension,
    NavigateTarget, PanelLocation, PanelRegistration, RegisteredCommand, RegistryClient, RowItem,
    SidebarPaneRegistration, StatusBarAlignment,
};
use std::path::PathBuf;

// â”€â”€ ExtensionCategory â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn category_label() {
    assert_eq!(ExtensionCategory::Git.label(), "Git");
    assert_eq!(ExtensionCategory::Languages.label(), "Languages");
    assert_eq!(ExtensionCategory::Linters.label(), "Linters");
    assert_eq!(ExtensionCategory::Themes.label(), "Themes");
    assert_eq!(ExtensionCategory::Productivity.label(), "Productivity");
    assert_eq!(ExtensionCategory::Debuggers.label(), "Debuggers");
    assert_eq!(ExtensionCategory::Formatters.label(), "Formatters");
    assert_eq!(ExtensionCategory::Other.label(), "Other");
}

#[test]
fn category_color() {
    let (r, g, b) = ExtensionCategory::Git.color();
    assert_eq!(r, 0xf1);
    assert_eq!(g, 0x50);
    assert_eq!(b, 0x2f);
}

#[test]
fn category_equality() {
    assert_eq!(ExtensionCategory::Git, ExtensionCategory::Git);
    assert_ne!(ExtensionCategory::Git, ExtensionCategory::Languages);
}

// â”€â”€ ExtensionManifest â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn manifest_construction() {
    let m = ExtensionManifest {
        id: "test-ext".into(),
        name: "Test Extension".into(),
        description: "A test extension.".into(),
        version: "1.0.0".into(),
        author: "Test Author".into(),
        categories: vec![ExtensionCategory::Productivity],
        is_builtin: false,
    };
    assert_eq!(m.id, "test-ext");
    assert_eq!(m.name, "Test Extension");
    assert!(!m.is_builtin);
}

// â”€â”€ InstalledExtension â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn installed_extension_defaults() {
    let m = ExtensionManifest {
        id: "ext".into(),
        name: "Ext".into(),
        description: "".into(),
        version: "0.1.0".into(),
        author: "".into(),
        categories: vec![],
        is_builtin: true,
    };
    let ie = InstalledExtension {
        manifest: m,
        enabled: true,
        source: ExtensionSource::Builtin,
    };
    assert!(ie.enabled);
    assert!(ie.manifest.is_builtin);
}

// â”€â”€ ExtensionCapabilities â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn capabilities_default() {
    let caps = ExtensionCapabilities::default();
    assert!(!caps.file_read);
    assert!(!caps.file_write);
    assert!(!caps.terminal);
    assert!(!caps.network);
}

#[test]
fn capabilities_custom() {
    let caps = ExtensionCapabilities {
        file_read: true,
        file_write: false,
        terminal: true,
        network: false,
    };
    assert!(caps.file_read);
    assert!(caps.terminal);
    assert!(!caps.network);
}

// â”€â”€ StatusBarAlignment â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn status_bar_alignment_default() {
    let align: StatusBarAlignment = Default::default();
    assert_eq!(align, StatusBarAlignment::Left);
}

// â”€â”€ ExtensionSource â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn extension_source_builtin() {
    let src = ExtensionSource::Builtin;
    match src {
        ExtensionSource::Builtin => {}
        _ => panic!("expected Builtin"),
    }
}

#[test]
fn extension_source_local() {
    let src = ExtensionSource::Local(PathBuf::from("/ext.wasm"));
    match src {
        ExtensionSource::Local(p) => assert_eq!(p, PathBuf::from("/ext.wasm")),
        _ => panic!("expected Local"),
    }
}

#[test]
fn extension_source_registry() {
    let src = ExtensionSource::Registry {
        download_url: "https://registry.example.com/ext.wasm".into(),
    };
    match src {
        ExtensionSource::Registry { download_url } => {
            assert!(download_url.contains("registry.example.com"));
        }
        _ => panic!("expected Registry"),
    }
}

// â”€â”€ RegistryClient â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn registry_client_search_empty() {
    let client = RegistryClient::new();
    let results = client.search("");
    assert!(!results.is_empty(), "catalogue should have entries");
}

#[test]
fn registry_client_search_query() {
    let client = RegistryClient::new();
    let results = client.search("rust");
    assert!(!results.is_empty());
    assert!(results.iter().any(|e| e.id.contains("rust")));
}

#[test]
fn registry_client_search_no_match() {
    let client = RegistryClient::new();
    let results = client.search("xyznonexistent12345");
    assert!(results.is_empty());
}

#[test]
fn registry_client_recommended() {
    let client = RegistryClient::new();
    let rec = client.recommended();
    assert_eq!(rec.len(), 6);
    // Recommended should be sorted by downloads descending.
    for i in 1..rec.len() {
        assert!(rec[i - 1].downloads >= rec[i].downloads);
    }
}

#[test]
fn registry_client_download_fails_without_base_url() {
    let client = RegistryClient::new();
    let ext = client.search("rust").into_iter().next().unwrap();
    let result = client.download(&ext);
    assert!(result.is_err());
}

// â”€â”€ ExtensionHost â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn host_new_has_builtins() {
    let host = ExtensionHost::new();
    assert!(!host.installed().is_empty());
}

#[test]
fn host_list_installed() {
    let host = ExtensionHost::new();
    let list = host.installed();
    // Should contain all 5 built-in extensions.
    assert!(list.len() >= 5);
    assert!(list.iter().any(|e| e.manifest.id == "git-blame-inline"));
}

#[test]
fn host_registered_commands() {
    let host = ExtensionHost::new();
    let cmds = host.registered_commands();
    // Built-in extensions register commands.
    assert!(!cmds.is_empty());
}

#[test]
fn host_registered_panels() {
    let host = ExtensionHost::new();
    let panels = host.registered_panels();
    assert!(!panels.is_empty());
    // Markdown preview registers a panel.
    assert!(panels.iter().any(|p| p.id.contains("markdown-preview")));
}

#[test]
fn host_enable_disable_extension() {
    let mut host = ExtensionHost::new();
    let id = "todo-highlighter";
    // Check initial state
    let installed = host.installed().iter().find(|e| e.manifest.id == id);
    assert!(installed.is_some());
    assert!(installed.unwrap().enabled);

    let ctx = ExtensionContext {
        active_text: None,
        active_uri: None,
        active_language: "",
        workspace_roots: &[],
        blame_lines: &[],
        cursor_line: 0,
        cursor_col: 0,
        selection: None,
        current_theme_id: "dark",
    };
    host.set_enabled(id, false, &ctx);
    let installed = host.installed().iter().find(|e| e.manifest.id == id);
    assert!(!installed.unwrap().enabled);

    host.set_enabled(id, true, &ctx);
    let installed = host.installed().iter().find(|e| e.manifest.id == id);
    assert!(installed.unwrap().enabled);
}

#[test]
fn host_enable_unknown_extension() {
    let mut host = ExtensionHost::new();
    let ctx = ExtensionContext {
        active_text: None,
        active_uri: None,
        active_language: "",
        workspace_roots: &[],
        blame_lines: &[],
        cursor_line: 0,
        cursor_col: 0,
        selection: None,
        current_theme_id: "dark",
    };
    host.set_enabled("nonexistent", false, &ctx); // Should not panic.
}

// â”€â”€ ExtensionOutput â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn extension_output_status_bar_text() {
    let output = ExtensionOutput::StatusBarText {
        extension_id: "test".into(),
        text: "Ready".into(),
        tooltip: Some("All good".into()),
        command: None,
        alignment: StatusBarAlignment::Left,
    };
    match output {
        ExtensionOutput::StatusBarText {
            extension_id,
            text,
            tooltip,
            command,
            alignment,
        } => {
            assert_eq!(extension_id, "test");
            assert_eq!(text, "Ready");
            assert_eq!(tooltip, Some("All good".into()));
            assert_eq!(command, None);
            assert_eq!(alignment, StatusBarAlignment::Left);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn extension_output_diagnostics() {
    let diag = ExtensionDiagnostic {
        start_line: 1,
        start_col: 0,
        end_line: 1,
        end_col: 5,
        severity: ExtensionSeverity::Warning,
        message: "unused variable".into(),
        source: "lint".into(),
    };
    let output = ExtensionOutput::Diagnostics {
        extension_id: "lint".into(),
        uri: "file:///test.rs".into(),
        items: vec![diag],
    };
    match output {
        ExtensionOutput::Diagnostics {
            extension_id,
            items,
            ..
        } => {
            assert_eq!(extension_id, "lint");
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].message, "unused variable");
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn extension_output_panel_content() {
    let output = ExtensionOutput::PanelContent {
        panel_id: "test-panel".into(),
        blocks: vec![
            ContentBlock::Heading("Test".into()),
            ContentBlock::Paragraph("Some content".into()),
            ContentBlock::Separator,
            ContentBlock::Rows(vec![RowItem {
                icon: "â–¶".into(),
                text: "Run".into(),
                tooltip: None,
                on_click: Some(NavigateTarget::Command("run.test".into())),
            }]),
        ],
    };
    match output {
        ExtensionOutput::PanelContent { panel_id, blocks } => {
            assert_eq!(panel_id, "test-panel");
            assert_eq!(blocks.len(), 4);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn extension_output_notification() {
    let output = ExtensionOutput::Notification {
        message: "Hello".into(),
        is_error: false,
    };
    match output {
        ExtensionOutput::Notification { message, is_error } => {
            assert_eq!(message, "Hello");
            assert!(!is_error);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn extension_output_gutter_markers() {
    let output = ExtensionOutput::GutterMarkers {
        extension_id: "test".into(),
        uri: "file:///test.rs".into(),
        markers: vec![GutterMarker {
            line: 0,
            icon: "â—".into(),
            tooltip: Some("issue".into()),
            severity: Some(ExtensionSeverity::Error),
            command: Some("fix".into()),
        }],
    };
    match output {
        ExtensionOutput::GutterMarkers {
            extension_id,
            markers,
            ..
        } => {
            assert_eq!(extension_id, "test");
            assert_eq!(markers.len(), 1);
            assert_eq!(markers[0].icon, "â—");
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn extension_output_cycle_theme() {
    let output = ExtensionOutput::CycleTheme;
    match output {
        ExtensionOutput::CycleTheme => {}
        _ => panic!("wrong variant"),
    }
}

#[test]
fn extension_output_write_file() {
    let output = ExtensionOutput::WriteFile {
        path: PathBuf::from("/tmp/test.txt"),
        content: "hello".into(),
    };
    match output {
        ExtensionOutput::WriteFile { path, content } => {
            assert_eq!(path, PathBuf::from("/tmp/test.txt"));
            assert_eq!(content, "hello");
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn extension_output_send_to_terminal() {
    let output = ExtensionOutput::SendToTerminal {
        terminal_id: 1,
        data: vec![0x41],
    };
    match output {
        ExtensionOutput::SendToTerminal { terminal_id, data } => {
            assert_eq!(terminal_id, 1);
            assert_eq!(data, vec![0x41]);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn extension_output_open_terminal() {
    let output = ExtensionOutput::OpenTerminal {
        title: "Build".into(),
        command: Some("cargo build".into()),
    };
    match output {
        ExtensionOutput::OpenTerminal { title, command } => {
            assert_eq!(title, "Build");
            assert_eq!(command, Some("cargo build".into()));
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn extension_output_show_hide_panel() {
    let show = ExtensionOutput::ShowPanel {
        panel_id: "terminal".into(),
    };
    let hide = ExtensionOutput::HidePanel {
        panel_id: "terminal".into(),
    };
    match show {
        ExtensionOutput::ShowPanel { panel_id } => assert_eq!(panel_id, "terminal"),
        _ => panic!("wrong variant"),
    }
    match hide {
        ExtensionOutput::HidePanel { panel_id } => assert_eq!(panel_id, "terminal"),
        _ => panic!("wrong variant"),
    }
}

// â”€â”€ ContentBlock variants â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn content_block_paragraph() {
    let block = ContentBlock::Paragraph("hello".into());
    match block {
        ContentBlock::Paragraph(s) => assert_eq!(s, "hello"),
        _ => panic!("wrong variant"),
    }
}

#[test]
fn content_block_preformatted() {
    let block = ContentBlock::Preformatted("code".into());
    match block {
        ContentBlock::Preformatted(s) => assert_eq!(s, "code"),
        _ => panic!("wrong variant"),
    }
}

#[test]
fn content_block_heading() {
    let block = ContentBlock::Heading("Title".into());
    match block {
        ContentBlock::Heading(s) => assert_eq!(s, "Title"),
        _ => panic!("wrong variant"),
    }
}

#[test]
fn content_block_separator() {
    let block = ContentBlock::Separator;
    match block {
        ContentBlock::Separator => {}
        _ => panic!("wrong variant"),
    }
}

#[test]
fn content_block_rows() {
    let block = ContentBlock::Rows(vec![RowItem {
        icon: "ðŸ”".into(),
        text: "Search".into(),
        tooltip: Some("Find files".into()),
        on_click: Some(NavigateTarget::Command("search".into())),
    }]);
    match block {
        ContentBlock::Rows(rows) => {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].text, "Search");
        }
        _ => panic!("wrong variant"),
    }
}

// â”€â”€ NavigateTarget â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn navigate_target_file_at() {
    let target = NavigateTarget::FileAt {
        path: PathBuf::from("/src/main.rs"),
        line: 42,
    };
    match target {
        NavigateTarget::FileAt { path, line } => {
            assert_eq!(path, PathBuf::from("/src/main.rs"));
            assert_eq!(line, 42);
        }
        _ => panic!("wrong variant"),
    }
}

#[test]
fn navigate_target_command() {
    let target = NavigateTarget::Command("ext.command".into());
    match target {
        NavigateTarget::Command(cmd) => assert_eq!(cmd, "ext.command"),
        _ => panic!("wrong variant"),
    }
}

// â”€â”€ ContextMenuContribution â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn context_menu_contribution() {
    let item = ContextMenuContribution {
        context: ContextMenuContext::Editor,
        id: "ext.fix".into(),
        label: "Quick Fix".into(),
        command: "ext.fix".into(),
    };
    assert_eq!(item.context, ContextMenuContext::Editor);
    assert_eq!(item.label, "Quick Fix");
}

#[test]
fn context_menu_context_equality() {
    assert_eq!(ContextMenuContext::Editor, ContextMenuContext::Editor);
    assert_ne!(ContextMenuContext::Editor, ContextMenuContext::FileExplorer);
}

// â”€â”€ PanelRegistration â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn panel_registration() {
    let reg = PanelRegistration {
        id: "test-panel".into(),
        title: "Test".into(),
        location: PanelLocation::Right,
        min_size: 100.0,
        default_size: 300.0,
        initially_open: false,
        toggle_command: Some("test.toggle".into()),
    };
    assert_eq!(reg.id, "test-panel");
    assert_eq!(reg.location, PanelLocation::Right);
    assert_eq!(reg.toggle_command.as_deref(), Some("test.toggle"));
}

// â”€â”€ SidebarPaneRegistration â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn sidebar_pane_registration() {
    let reg = SidebarPaneRegistration {
        id: "ext-pane".into(),
        title: "Ext Pane".into(),
        icon: "ðŸ§©".into(),
        toggle_command: Some("ext.toggle".into()),
    };
    assert_eq!(reg.id, "ext-pane");
    assert_eq!(reg.icon, "ðŸ§©");
}

// â”€â”€ CompletionItem â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn completion_item() {
    let item = CompletionItem {
        label: "foo".into(),
        detail: Some("fn".into()),
        insert_text: "foo()".into(),
        kind: CompletionKind::Function,
    };
    assert_eq!(item.label, "foo");
    assert_eq!(item.kind, CompletionKind::Function);
}

#[test]
fn completion_kind_equality() {
    assert_eq!(CompletionKind::Method, CompletionKind::Method);
    assert_ne!(CompletionKind::Method, CompletionKind::Function);
}

// â”€â”€ HoverResult â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn hover_result() {
    let h = HoverResult {
        start_line: 1,
        start_col: 0,
        end_line: 1,
        end_col: 5,
        contents: "**bold** text".into(),
    };
    assert_eq!(h.start_line, 1);
    assert_eq!(h.contents, "**bold** text");
    assert_eq!(h.end_col, 5);
}

// â”€â”€ GutterMarker â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn gutter_marker() {
    let m = GutterMarker {
        line: 5,
        icon: "âš ".into(),
        tooltip: Some("Warning".into()),
        severity: Some(ExtensionSeverity::Warning),
        command: Some("fix".into()),
    };
    assert_eq!(m.line, 5);
    assert_eq!(m.icon, "âš ");
    assert_eq!(m.tooltip.as_deref(), Some("Warning"));
}

// â”€â”€ ExtensionSeverity â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn extension_severity() {
    let sev = ExtensionSeverity::Error;
    match sev {
        ExtensionSeverity::Error => {}
        _ => panic!("wrong variant"),
    }
}

// â”€â”€ CommandResult â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn command_result_ok() {
    let r = CommandResult::Ok;
    match r {
        CommandResult::Ok => {}
        _ => panic!("wrong variant"),
    }
}

#[test]
fn command_result_error() {
    let r = CommandResult::Error("oops".into());
    match r {
        CommandResult::Error(msg) => assert_eq!(msg, "oops"),
        _ => panic!("wrong variant"),
    }
}

// â”€â”€ PanelLocation â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn panel_location_equality() {
    assert_eq!(PanelLocation::Bottom, PanelLocation::Bottom);
    assert_ne!(PanelLocation::Bottom, PanelLocation::Right);
}

// â”€â”€ RegisteredCommand â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn registered_command() {
    let cmd = RegisteredCommand {
        id: "ext.cmd".into(),
        title: "Execute Command".into(),
        default_keybinding: Some("ctrl+shift+x".into()),
    };
    assert_eq!(cmd.id, "ext.cmd");
    assert_eq!(cmd.default_keybinding.as_deref(), Some("ctrl+shift+x"));
}

// â”€â”€ ExtensionContext construction â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn extension_context_fields() {
    let text = "fn main() {}";
    let roots = vec![PathBuf::from("/workspace")];
    let blame = vec![(0u32, "author".to_owned())];
    let ctx = ExtensionContext {
        active_text: Some(text),
        active_uri: Some("file:///main.rs"),
        active_language: "rust",
        workspace_roots: &roots,
        blame_lines: &blame,
        cursor_line: 0,
        cursor_col: 0,
        selection: None,
        current_theme_id: "crabide-dark",
    };
    assert_eq!(ctx.active_text, Some(text));
    assert_eq!(ctx.active_language, "rust");
    assert_eq!(ctx.cursor_line, 0);
    assert!(ctx.selection.is_none());
}

// â”€â”€ ExtensionDiagnostic â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn extension_diagnostic() {
    let d = ExtensionDiagnostic {
        start_line: 1,
        start_col: 0,
        end_line: 1,
        end_col: 10,
        severity: ExtensionSeverity::Hint,
        message: "consider using `let`".into(),
        source: "clippy".into(),
    };
    assert_eq!(d.message, "consider using `let`");
    assert_eq!(d.severity as i32, ExtensionSeverity::Hint as i32);
}

// â”€â”€ Capability enforcement â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[test]
fn is_output_allowed_status_bar() {
    let caps = ExtensionCapabilities::default();
    let out = ExtensionOutput::StatusBarText {
        extension_id: "test".into(),
        text: "hello".into(),
        tooltip: None,
        command: None,
        alignment: StatusBarAlignment::Left,
    };
    assert!(is_output_allowed(&out, &caps));
}

#[test]
fn is_output_allowed_write_file_requires_file_write() {
    let caps_denied = ExtensionCapabilities::default();
    let caps_allowed = ExtensionCapabilities {
        file_write: true,
        ..Default::default()
    };
    let out = ExtensionOutput::WriteFile {
        path: PathBuf::from("/tmp/test.txt"),
        content: "data".into(),
    };
    assert!(!is_output_allowed(&out, &caps_denied));
    assert!(is_output_allowed(&out, &caps_allowed));
}

#[test]
fn is_output_allowed_terminal_requires_terminal() {
    let caps_denied = ExtensionCapabilities::default();
    let caps_allowed = ExtensionCapabilities {
        terminal: true,
        ..Default::default()
    };
    let out = ExtensionOutput::SendToTerminal {
        terminal_id: 0,
        data: vec![1, 2, 3],
    };
    assert!(!is_output_allowed(&out, &caps_denied));
    assert!(is_output_allowed(&out, &caps_allowed));

    let open = ExtensionOutput::OpenTerminal {
        title: "test".into(),
        command: None,
    };
    assert!(!is_output_allowed(&open, &caps_denied));
    assert!(is_output_allowed(&open, &caps_allowed));
}

#[test]
fn is_output_allowed_apply_edits_requires_file_write() {
    let caps_denied = ExtensionCapabilities::default();
    let caps_allowed = ExtensionCapabilities {
        file_write: true,
        ..Default::default()
    };
    let out = ExtensionOutput::ApplyEdits {
        uri: "file:///test.rs".into(),
        edits: vec![],
    };
    assert!(!is_output_allowed(&out, &caps_denied));
    assert!(is_output_allowed(&out, &caps_allowed));
}

#[test]
fn is_output_allowed_insert_at_cursor_requires_file_write() {
    let caps_denied = ExtensionCapabilities::default();
    let caps_allowed = ExtensionCapabilities {
        file_write: true,
        ..Default::default()
    };
    let out = ExtensionOutput::InsertAtCursor {
        text: "hello".into(),
    };
    assert!(!is_output_allowed(&out, &caps_denied));
    assert!(is_output_allowed(&out, &caps_allowed));
}

#[test]
fn is_output_allowed_set_cursor_requires_file_write() {
    let caps_denied = ExtensionCapabilities::default();
    let caps_allowed = ExtensionCapabilities {
        file_write: true,
        ..Default::default()
    };
    let out = ExtensionOutput::SetCursorPosition {
        line: 0,
        character: 5,
    };
    assert!(!is_output_allowed(&out, &caps_denied));
    assert!(is_output_allowed(&out, &caps_allowed));
}

#[test]
fn is_output_allowed_notification() {
    let caps = ExtensionCapabilities::default();
    let out = ExtensionOutput::Notification {
        message: "hello".into(),
        is_error: false,
    };
    assert!(is_output_allowed(&out, &caps));
}

#[test]
fn is_output_allowed_gutter_markers() {
    let caps = ExtensionCapabilities::default();
    let out = ExtensionOutput::GutterMarkers {
        extension_id: "test".into(),
        uri: "file:///test.rs".into(),
        markers: vec![],
    };
    assert!(is_output_allowed(&out, &caps));
}

#[test]
fn is_output_allowed_diagnostics() {
    let caps = ExtensionCapabilities::default();
    let out = ExtensionOutput::Diagnostics {
        extension_id: "test".into(),
        uri: "file:///test.rs".into(),
        items: vec![],
    };
    assert!(is_output_allowed(&out, &caps));
}

#[test]
fn is_output_allowed_panel_content() {
    let caps = ExtensionCapabilities::default();
    let out = ExtensionOutput::PanelContent {
        panel_id: "test.panel".into(),
        blocks: vec![],
    };
    assert!(is_output_allowed(&out, &caps));
}

#[test]
fn is_output_allowed_cycle_theme() {
    let caps = ExtensionCapabilities::default();
    let out = ExtensionOutput::CycleTheme;
    assert!(is_output_allowed(&out, &caps));
}

#[test]
fn is_output_allowed_show_hide_panel() {
    let caps = ExtensionCapabilities::default();
    let show = ExtensionOutput::ShowPanel {
        panel_id: "test".into(),
    };
    let hide = ExtensionOutput::HidePanel {
        panel_id: "test".into(),
    };
    assert!(is_output_allowed(&show, &caps));
    assert!(is_output_allowed(&hide, &caps));
}

#[test]
fn is_output_allowed_sidebar_pane() {
    let caps = ExtensionCapabilities::default();
    let out = ExtensionOutput::SidebarPaneContent {
        pane_id: "test".into(),
        blocks: vec![],
    };
    assert!(is_output_allowed(&out, &caps));
}

#[test]
fn is_output_allowed_status_bar_visible() {
    let caps = ExtensionCapabilities::default();
    let out = ExtensionOutput::StatusBarVisible {
        extension_id: "test".into(),
        visible: true,
    };
    assert!(is_output_allowed(&out, &caps));
}

#[test]
fn poll_all_filters_write_file_without_capability() {
    // Verify that is_output_allowed correctly filters WriteFile when
    // file_write capability is not declared.
    let write_out = ExtensionOutput::WriteFile {
        path: PathBuf::from("/tmp/test.txt"),
        content: "data".into(),
    };
    let safe_out = ExtensionOutput::StatusBarText {
        extension_id: "mock-no-write".into(),
        text: "hello".into(),
        tooltip: None,
        command: None,
        alignment: StatusBarAlignment::Left,
    };

    let caps = ExtensionCapabilities::default();
    assert!(is_output_allowed(&safe_out, &caps));
    assert!(!is_output_allowed(&write_out, &caps));
}

#[test]
fn poll_all_allows_write_file_with_capability() {
    let caps = ExtensionCapabilities {
        file_write: true,
        ..Default::default()
    };
    let write_out = ExtensionOutput::WriteFile {
        path: PathBuf::from("/tmp/test.txt"),
        content: "data".into(),
    };
    assert!(is_output_allowed(&write_out, &caps));
}
