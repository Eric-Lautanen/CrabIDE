//! Git Blame Inline — shows the git blame for the cursor line in the status bar
//! and provides a hover-style blame summary in the Extensions panel.

use crate::host::{
    CommandResult, ExtensionCategory, ExtensionContext, ExtensionManifest, ExtensionOutput,
    NativeExtension,
};

pub struct GitBlameExtension {
    manifest: ExtensionManifest,
    /// Last blame text we emitted (to avoid redundant updates).
    last_blame_text: String,
}

impl GitBlameExtension {
    pub fn new() -> Self {
        Self {
            manifest: ExtensionManifest {
                id: "git-blame-inline".into(),
                name: "Git Blame Inline".into(),
                description: "Shows git blame for the current line in the status bar, \
                               with author, commit hash, and summary."
                    .into(),
                version: "1.0.0".into(),
                author: "crabide Contributors".into(),
                categories: vec![ExtensionCategory::Git],
                is_builtin: false,
            },
            last_blame_text: String::new(),
        }
    }
}

impl NativeExtension for GitBlameExtension {
    fn manifest(&self) -> &ExtensionManifest {
        &self.manifest
    }

    fn activate(&mut self, _ctx: &ExtensionContext) {
        log::debug!("git-blame-inline: activated");
    }

    fn deactivate(&mut self) {
        log::debug!("git-blame-inline: deactivated");
    }

    fn on_document_open(&mut self, _uri: &str, _lang: &str, _ctx: &ExtensionContext) {
        self.last_blame_text.clear();
    }

    fn on_document_change(&mut self, _uri: &str, _ctx: &ExtensionContext) {
        self.last_blame_text.clear();
    }

    fn on_document_save(&mut self, _uri: &str, _ctx: &ExtensionContext) {}

    fn execute_command(&mut self, _command: &str, _args: &[String]) -> CommandResult {
        CommandResult::Ok
    }

    fn poll(&mut self, ctx: &ExtensionContext) -> Vec<ExtensionOutput> {
        let blame_text = if ctx.blame_lines.is_empty() {
            String::new()
        } else {
            // Find the blame entry for the cursor line.
            ctx.blame_lines
                .iter()
                .find(|(line, _)| *line == ctx.cursor_line)
                .or_else(|| ctx.blame_lines.first())
                .map(|(_, text)| text.clone())
                .unwrap_or_default()
        };

        if blame_text == self.last_blame_text {
            return Vec::new();
        }
        self.last_blame_text = blame_text.clone();

        let (text, tooltip) = if blame_text.is_empty() {
            (String::new(), None)
        } else {
            let short = if blame_text.len() > 60 {
                format!("{}…", &blame_text[..60])
            } else {
                blame_text.clone()
            };
            (format!("$(git-commit) {short}"), Some(blame_text))
        };

        vec![ExtensionOutput::StatusBarText {
            extension_id: self.manifest.id.clone(),
            text,
            tooltip,
            command: None,
            alignment: crate::host::StatusBarAlignment::Left,
        }]
    }
}
