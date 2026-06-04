//! Theme Switcher Quick — a status-bar extension that shows the current theme
//! name and lets the user cycle through themes without opening the settings.
//!
//! Commands:
//!   `theme-switcher.next-theme` — cycles to the next theme.
//!
//! The extension communicates a theme cycle request to the app via
//! [`ExtensionOutput::CycleTheme`]; the app calls `apply_theme_cycle()` in response.

use crate::host::{
    CommandResult, ExtensionCategory, ExtensionContext, ExtensionManifest, ExtensionOutput,
    NativeExtension, RegisteredCommand, StatusBarAlignment,
};

pub struct ThemeSwitcherExtension {
    manifest: ExtensionManifest,
    cycle_pending: bool,
}

impl ThemeSwitcherExtension {
    pub fn new() -> Self {
        Self {
            manifest: ExtensionManifest {
                id: "theme-switcher".into(),
                name: "Theme Switcher Quick".into(),
                description: "Status-bar toggle to cycle between dark and light themes. \
                               Respects the system dark-mode preference."
                    .into(),
                version: "1.0.0".into(),
                author: "crabide Contributors".into(),
                categories: vec![ExtensionCategory::Themes],
                is_builtin: false,
            },
            cycle_pending: false,
        }
    }

    fn theme_display_name(theme_id: &str) -> &'static str {
        match theme_id {
            "crabide-dark" => "🌙 Dark",
            "crabide-light" => "☀ Light",
            _ => "◑ Theme",
        }
    }
}

impl NativeExtension for ThemeSwitcherExtension {
    fn manifest(&self) -> &ExtensionManifest {
        &self.manifest
    }

    fn commands(&self) -> Vec<RegisteredCommand> {
        vec![RegisteredCommand {
            id: "theme-switcher.next-theme".into(),
            title: "Theme Switcher: Cycle to Next Theme".into(),
            default_keybinding: None,
        }]
    }

    fn activate(&mut self, _ctx: &ExtensionContext) {
        log::debug!("theme-switcher: activated");
        self.cycle_pending = false;
    }

    fn deactivate(&mut self) {
        log::debug!("theme-switcher: deactivated");
    }

    fn on_document_open(&mut self, _uri: &str, _lang: &str, _ctx: &ExtensionContext) {}
    fn on_document_change(&mut self, _uri: &str, _ctx: &ExtensionContext) {}
    fn on_document_save(&mut self, _uri: &str, _ctx: &ExtensionContext) {}

    fn execute_command(&mut self, command: &str, _args: &[String]) -> CommandResult {
        if command == "theme-switcher.next-theme" {
            self.cycle_pending = true;
            CommandResult::Ok
        } else {
            CommandResult::Error(format!("Unknown command: {command}"))
        }
    }

    fn poll(&mut self, ctx: &ExtensionContext) -> Vec<ExtensionOutput> {
        let mut out = Vec::new();

        if self.cycle_pending {
            self.cycle_pending = false;
            out.push(ExtensionOutput::CycleTheme);
        }

        // Derive display name from the actual current theme id — always in sync.
        let display = Self::theme_display_name(ctx.current_theme_id);
        out.push(ExtensionOutput::StatusBarText {
            extension_id: self.manifest.id.clone(),
            text: display.to_owned(),
            tooltip: Some("Click to toggle light/dark theme".into()),
            command: Some("theme-switcher.next-theme".into()),
            alignment: StatusBarAlignment::Right,
        });

        out
    }
}
