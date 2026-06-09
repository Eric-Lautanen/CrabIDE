//! Markdown Preview — renders a plain-text approximation of the active Markdown
//! document and sends it to the Extensions panel preview area.
//!
//! Full HTML rendering via a WebView is the long-term goal (`webview` feature).
//! This implementation does a lightweight text-only pass that preserves structure:
//!   - ATX headings (`# …`) shown in ALL CAPS with a separator rule
//!   - Bold / italic stripped to plain text
//!   - Fenced code blocks preserved with a `│` gutter
//!   - Unordered list bullets `*/-/+` → `•`
//!   - Ordered lists preserved with their numbers
//!   - Horizontal rules → a line of `─`
//!   - Blank lines preserved for readability

use crate::host::{
    CommandResult, ContentBlock, ExtensionCategory, ExtensionContext, ExtensionManifest,
    ExtensionOutput, NativeExtension, PanelLocation, PanelRegistration, RegisteredCommand,
};

pub struct MarkdownPreviewExtension {
    manifest: ExtensionManifest,
    /// Whether the split preview is turned on.
    pub preview_on: bool,
    last_uri: String,
    last_rendered: String,
    dirty: bool,
}

impl MarkdownPreviewExtension {
    pub fn new() -> Self {
        Self {
            manifest: ExtensionManifest {
                id: "markdown-preview".into(),
                name: "Markdown Preview".into(),
                description:
                    "Live plain-text preview of Markdown documents in the Extensions panel.".into(),
                version: "1.0.0".into(),
                author: "crabide Contributors".into(),
                categories: vec![
                    ExtensionCategory::Languages,
                    ExtensionCategory::Productivity,
                ],
                is_builtin: false,
            },
            preview_on: true,
            last_uri: String::new(),
            last_rendered: String::new(),
            dirty: false,
        }
    }

    fn render(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut in_fence = false;
        let mut fence_ch = ' ';

        for raw_line in text.lines() {
            // ── Fenced code blocks ─────────────────────────────────────────
            if in_fence {
                let end_marker: String = std::iter::repeat_n(fence_ch, 3).collect();
                if raw_line.starts_with(&end_marker) {
                    in_fence = false;
                    out.push_str("└────────────────────────\n");
                    continue;
                }
                out.push_str("│ ");
                out.push_str(raw_line);
                out.push('\n');
                continue;
            } else if raw_line.starts_with("```") || raw_line.starts_with("~~~") {
                in_fence = true;
                fence_ch = raw_line.chars().next().unwrap_or('`');
                let lang = raw_line.trim_start_matches(['`', '~']).trim();
                if lang.is_empty() {
                    out.push_str("┌ code ──────────────────\n");
                } else {
                    out.push_str(&format!("┌ {lang} ─────────────────\n"));
                }
                continue;
            }

            let line = raw_line.trim_end();

            // ── Horizontal rule ───────────────────────────────────────────
            let stripped = line.trim().replace(['-', '*', '_'], "");
            if line.len() >= 3 && stripped.trim().is_empty() {
                out.push_str("────────────────────────\n");
                continue;
            }

            // ── ATX headings ──────────────────────────────────────────────
            if line.starts_with('#') {
                let level = line.chars().take_while(|&c| c == '#').count();
                let content = line[level..].trim();
                match level {
                    1 => {
                        let up = content.to_uppercase();
                        out.push_str(&up);
                        out.push('\n');
                        out.push_str(&"═".repeat(up.chars().count().min(40)));
                        out.push('\n');
                    }
                    2 => {
                        out.push_str(content);
                        out.push('\n');
                        out.push_str(&"─".repeat(content.chars().count().min(40)));
                        out.push('\n');
                    }
                    _ => {
                        out.push_str(&"  ".repeat(level.saturating_sub(3)));
                        out.push_str("▸ ");
                        out.push_str(content);
                        out.push('\n');
                    }
                }
                continue;
            }

            // ── Unordered lists ───────────────────────────────────────────
            if let Some(rest) = line
                .strip_prefix("* ")
                .or_else(|| line.strip_prefix("- "))
                .or_else(|| line.strip_prefix("+ "))
            {
                out.push_str("  • ");
                out.push_str(&strip_inline(rest));
                out.push('\n');
                continue;
            }

            // ── Ordered lists ─────────────────────────────────────────────
            let first_dot = line.find(". ");
            if let Some(dot) = first_dot {
                let prefix = &line[..dot];
                if prefix.chars().all(|c| c.is_ascii_digit()) {
                    out.push_str(&format!("  {prefix}. "));
                    out.push_str(&strip_inline(&line[dot + 2..]));
                    out.push('\n');
                    continue;
                }
            }

            // ── Blank line ────────────────────────────────────────────────
            if line.is_empty() {
                out.push('\n');
                continue;
            }

            // ── Paragraph / inline ────────────────────────────────────────
            out.push_str(&strip_inline(line));
            out.push('\n');
        }

        out
    }
}

/// Strip inline markdown formatting: bold, italic, inline code, links.
fn strip_inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // Bold / italic: consume markers but keep the text content.
            '*' | '_' => {
                let double = chars.peek() == Some(&c);
                if double {
                    chars.next();
                }
                // Read and output text until matching closer.
                let mut closer_found = false;
                while let Some(ic) = chars.next() {
                    if ic == c {
                        if double {
                            if chars.peek() == Some(&c) {
                                chars.next();
                                closer_found = true;
                                break;
                            }
                        } else {
                            closer_found = true;
                            break;
                        }
                    }
                    out.push(ic);
                }
                // If no closer was found, re-emit the opening marker(s).
                if !closer_found {
                    out.push(c);
                    if double {
                        out.push(c);
                    }
                }
            }
            // Inline code: consume until closing backtick, keep content.
            '`' => {
                for ic in chars.by_ref() {
                    if ic == '`' {
                        break;
                    }
                    out.push(ic);
                }
            }
            // Link: [text](url) → text
            '[' => {
                // Collect the label text.
                let mut label = String::new();
                for ic in chars.by_ref() {
                    if ic == ']' {
                        break;
                    }
                    label.push(ic);
                }
                // Consume optional `(url)`.
                if chars.peek() == Some(&'(') {
                    chars.next();
                    for ic in chars.by_ref() {
                        if ic == ')' {
                            break;
                        }
                    }
                }
                out.push_str(&label);
            }
            other => out.push(other),
        }
    }
    out
}

impl NativeExtension for MarkdownPreviewExtension {
    fn manifest(&self) -> &ExtensionManifest {
        &self.manifest
    }

    fn panels(&self) -> Vec<PanelRegistration> {
        vec![PanelRegistration {
            id: "markdown-preview.panel".into(),
            title: "MARKDOWN PREVIEW".into(),
            location: PanelLocation::Right,
            min_size: 260.0,
            default_size: 380.0,
            initially_open: false,
            toggle_command: Some("markdown-preview.toggle".into()),
        }]
    }

    fn commands(&self) -> Vec<RegisteredCommand> {
        vec![RegisteredCommand {
            id: "markdown-preview.toggle".into(),
            title: "Markdown Preview: Toggle Preview".into(),
            default_keybinding: Some("ctrl+shift+v".into()),
        }]
    }

    fn activate(&mut self, _ctx: &ExtensionContext) {
        log::debug!("markdown-preview: activated");
    }

    fn deactivate(&mut self) {
        self.last_rendered.clear();
        self.dirty = true;
    }

    fn on_document_open(&mut self, uri: &str, _lang: &str, ctx: &ExtensionContext) {
        if !uri.ends_with(".md") && !uri.ends_with(".markdown") {
            return;
        }
        if let Some(text) = ctx.active_text {
            self.last_uri = uri.to_owned();
            self.last_rendered = Self::render(text);
            self.dirty = true;
        }
    }

    fn on_document_change(&mut self, uri: &str, ctx: &ExtensionContext) {
        if !uri.ends_with(".md") && !uri.ends_with(".markdown") {
            return;
        }
        if let Some(text) = ctx.active_text {
            self.last_uri = uri.to_owned();
            self.last_rendered = Self::render(text);
            self.dirty = true;
        }
    }

    fn on_document_save(&mut self, uri: &str, ctx: &ExtensionContext) {
        self.on_document_change(uri, ctx);
    }

    fn execute_command(&mut self, command: &str, _args: &[String]) -> CommandResult {
        if command == "markdown-preview.toggle" {
            self.preview_on = !self.preview_on;
            self.dirty = true;
            CommandResult::Ok
        } else {
            CommandResult::Error(format!("Unknown command: {command}"))
        }
    }

    fn poll(&mut self, ctx: &ExtensionContext) -> Vec<ExtensionOutput> {
        // Track active URI changes.
        let uri = ctx.active_uri.unwrap_or("");
        let is_md = uri.ends_with(".md") || uri.ends_with(".markdown");

        if !self.preview_on || !is_md {
            if !self.last_rendered.is_empty() {
                self.last_rendered.clear();
                self.dirty = true;
            }
        } else if self.last_uri != uri {
            if let Some(text) = ctx.active_text {
                self.last_uri = uri.to_owned();
                self.last_rendered = Self::render(text);
                self.dirty = true;
            }
        }

        if !self.dirty {
            return Vec::new();
        }
        self.dirty = false;

        vec![ExtensionOutput::PanelContent {
            panel_id: "markdown-preview.panel".into(),
            blocks: if self.last_rendered.is_empty() {
                vec![]
            } else {
                vec![ContentBlock::Preformatted(self.last_rendered.clone())]
            },
        }]
    }
}
