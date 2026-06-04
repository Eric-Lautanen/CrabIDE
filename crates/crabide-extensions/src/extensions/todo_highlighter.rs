//! Todo/Fixme Highlighter — scans open documents for TODO, FIXME, HACK, XXX,
//! NOTE, and BUG comment tags and collects them into a list shown in the
//! Extensions panel sidebar.

use std::path::PathBuf;

use crate::host::{
    CommandResult, ContentBlock, ExtensionCategory, ExtensionContext, ExtensionManifest,
    ExtensionOutput, NativeExtension, NavigateTarget, PanelLocation, PanelRegistration,
    RegisteredCommand, RowItem,
};

/// Known tag strings, in priority order for display.
const TAGS: &[&str] = &["BUG", "FIXME", "HACK", "XXX", "TODO", "NOTE"];

pub struct TodoHighlighterExtension {
    manifest: ExtensionManifest,
    /// Accumulated todo items for the currently scanned document.
    items: Vec<RowItem>,
    /// Last URI that was scanned.
    last_uri: String,
    dirty: bool,
}

impl TodoHighlighterExtension {
    pub fn new() -> Self {
        Self {
            manifest: ExtensionManifest {
                id: "todo-highlighter".into(),
                name: "Todo/Fixme Highlighter".into(),
                description: "Scans open documents for TODO, FIXME, HACK, XXX, NOTE, and BUG \
                               tags and lists them in the Extensions panel for quick navigation."
                    .into(),
                version: "1.0.0".into(),
                author: "crabide Contributors".into(),
                categories: vec![ExtensionCategory::Productivity],
                is_builtin: false,
            },
            items: Vec::new(),
            last_uri: String::new(),
            dirty: false,
        }
    }

    fn scan(&mut self, uri: &str, text: &str) {
        self.items.clear();
        self.last_uri = uri.to_owned();

        let path = PathBuf::from(uri.strip_prefix("file://").unwrap_or(uri));

        for (line_idx, line) in text.lines().enumerate() {
            let upper = line.to_uppercase();

            for &tag in TAGS {
                let mut start = 0;
                while let Some(pos) = upper[start..].find(tag) {
                    let abs = start + pos;
                    let after = &upper[abs + tag.len()..];
                    let is_tagged = after.starts_with(':')
                        || after.starts_with(' ')
                        || after.starts_with('(')
                        || after.is_empty();

                    if is_tagged {
                        let raw_text = line[abs + tag.len()..]
                            .trim_start_matches([':', ' ', '('])
                            .trim()
                            .to_owned();

                        let icon = match tag {
                            "BUG" => "🐛",
                            "FIXME" => "🔥",
                            "HACK" | "XXX" => "⚠",
                            "NOTE" => "📝",
                            _ => "📌",
                        };
                        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                        let display =
                            format!("{}  {}:{} — {}", icon, filename, line_idx + 1, raw_text);

                        self.items.push(RowItem {
                            icon: icon.to_owned(),
                            text: display,
                            tooltip: Some(raw_text),
                            on_click: Some(NavigateTarget::FileAt {
                                path: path.clone(),
                                line: line_idx as u32,
                            }),
                        });
                        break;
                    }
                    start = abs + 1;
                }
            }
        }

        self.dirty = true;
    }
}

impl NativeExtension for TodoHighlighterExtension {
    fn manifest(&self) -> &ExtensionManifest {
        &self.manifest
    }

    fn panels(&self) -> Vec<PanelRegistration> {
        vec![PanelRegistration {
            id: "todo-highlighter.panel".into(),
            title: "TODO ITEMS".into(),
            location: PanelLocation::Bottom,
            min_size: 80.0,
            default_size: 180.0,
            initially_open: false,
            toggle_command: Some("todo-highlighter.toggle-panel".into()),
        }]
    }

    fn commands(&self) -> Vec<RegisteredCommand> {
        vec![RegisteredCommand {
            id: "todo-highlighter.toggle-panel".into(),
            title: "Todo Highlighter: Toggle TODO Panel".into(),
            default_keybinding: None,
        }]
    }

    fn activate(&mut self, _ctx: &ExtensionContext) {
        log::debug!("todo-highlighter: activated");
    }

    fn deactivate(&mut self) {
        self.items.clear();
        self.dirty = true;
    }

    fn on_document_open(&mut self, uri: &str, _lang: &str, ctx: &ExtensionContext) {
        if let Some(text) = ctx.active_text {
            self.scan(uri, text);
        }
    }

    fn on_document_change(&mut self, uri: &str, ctx: &ExtensionContext) {
        if let Some(text) = ctx.active_text {
            self.scan(uri, text);
        }
    }

    fn on_document_save(&mut self, uri: &str, ctx: &ExtensionContext) {
        self.on_document_change(uri, ctx);
    }

    fn execute_command(&mut self, _command: &str, _args: &[String]) -> CommandResult {
        CommandResult::Ok
    }

    fn poll(&mut self, _ctx: &ExtensionContext) -> Vec<ExtensionOutput> {
        if !self.dirty {
            return Vec::new();
        }
        self.dirty = false;
        vec![ExtensionOutput::PanelContent {
            panel_id: "todo-highlighter.panel".into(),
            blocks: if self.items.is_empty() {
                vec![]
            } else {
                vec![ContentBlock::Rows(self.items.clone())]
            },
        }]
    }
}
