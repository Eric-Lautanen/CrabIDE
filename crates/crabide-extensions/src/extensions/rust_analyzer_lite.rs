//! Rust Analyzer Lite — scans Rust documents for common anti-patterns and
//! emits hints / warnings without requiring a running language server.
//!
//! Detected patterns:
//!   - `todo!()` / `unimplemented!()` / `unreachable!()` — Information
//!   - `.unwrap()` in non-test contexts — Hint (prefer `?` or `expect`)
//!   - `dbg!()` / `eprintln!()` — Warning (debug prints left in)
//!   - `#[allow(dead_code)]` — Hint (acknowledged suppression)
//!   - `panic!()` — Warning

use crate::host::{
    CommandResult, ExtensionCategory, ExtensionContext, ExtensionDiagnostic, ExtensionManifest,
    ExtensionOutput, ExtensionSeverity, NativeExtension,
};

struct Pattern {
    text: &'static str,
    severity: ExtensionSeverity,
    message: &'static str,
}

const PATTERNS: &[Pattern] = &[
    Pattern {
        text: "todo!()",
        severity: ExtensionSeverity::Information,
        message: "todo!() placeholder — implement before shipping",
    },
    Pattern {
        text: "todo!(\"",
        severity: ExtensionSeverity::Information,
        message: "todo!() placeholder — implement before shipping",
    },
    Pattern {
        text: "unimplemented!()",
        severity: ExtensionSeverity::Information,
        message: "unimplemented!() — this path is not yet implemented",
    },
    Pattern {
        text: "unreachable!()",
        severity: ExtensionSeverity::Hint,
        message: "unreachable!() — consider using `panic!` with a message",
    },
    Pattern {
        text: ".unwrap()",
        severity: ExtensionSeverity::Hint,
        message: "unwrap() panics on None/Err — prefer `?` or `.expect()`",
    },
    Pattern {
        text: "dbg!(",
        severity: ExtensionSeverity::Warning,
        message: "dbg!() debug print left in source",
    },
    Pattern {
        text: "eprintln!(",
        severity: ExtensionSeverity::Warning,
        message: "eprintln!() debug print — remove before release",
    },
    Pattern {
        text: "panic!(",
        severity: ExtensionSeverity::Warning,
        message: "explicit panic!() — consider a recoverable error instead",
    },
    Pattern {
        text: "#[allow(dead_code)]",
        severity: ExtensionSeverity::Hint,
        message: "#[allow(dead_code)] suppresses a lint — remove dead code",
    },
];

pub struct RustAnalyzerLiteExtension {
    manifest: ExtensionManifest,
    /// Cached diagnostics for the last analysed URI.
    cached_uri: String,
    cached_diagnostics: Vec<ExtensionDiagnostic>,
    dirty: bool,
}

impl RustAnalyzerLiteExtension {
    pub fn new() -> Self {
        Self {
            manifest: ExtensionManifest {
                id: "rust-analyzer-lite".into(),
                name: "Rust Analyzer Lite".into(),
                description: "Lightweight Rust diagnostics without a running language server. \
                               Detects todo!(), unwrap(), debug prints, and more."
                    .into(),
                version: "1.0.0".into(),
                author: "crabide Contributors".into(),
                categories: vec![ExtensionCategory::Linters, ExtensionCategory::Languages],
                is_builtin: false,
            },
            cached_uri: String::new(),
            cached_diagnostics: Vec::new(),
            dirty: false,
        }
    }

    fn analyse(&mut self, uri: &str, text: &str) {
        self.cached_uri = uri.to_owned();
        self.cached_diagnostics.clear();

        for (line_idx, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            // Skip comment-only lines (blame the actual code, not comments).
            if trimmed.starts_with("//") {
                continue;
            }

            for pat in PATTERNS {
                if let Some(col) = line.find(pat.text) {
                    self.cached_diagnostics.push(ExtensionDiagnostic {
                        start_line: line_idx as u32,
                        start_col: col as u32,
                        end_line: line_idx as u32,
                        end_col: (col + pat.text.len()) as u32,
                        severity: pat.severity,
                        message: pat.message.into(),
                        source: "rust-analyzer-lite".into(),
                    });
                }
            }
        }

        self.dirty = true;
    }
}

impl NativeExtension for RustAnalyzerLiteExtension {
    fn manifest(&self) -> &ExtensionManifest {
        &self.manifest
    }

    fn activate(&mut self, _ctx: &ExtensionContext) {
        log::debug!("rust-analyzer-lite: activated");
    }

    fn deactivate(&mut self) {
        self.cached_diagnostics.clear();
        self.dirty = true;
    }

    fn on_document_open(&mut self, uri: &str, language_id: &str, ctx: &ExtensionContext) {
        if language_id != "rust" {
            return;
        }
        if let Some(text) = ctx.active_text {
            self.analyse(uri, text);
        }
    }

    fn on_document_change(&mut self, uri: &str, ctx: &ExtensionContext) {
        // Only analyse Rust files.
        if !uri.ends_with(".rs") {
            return;
        }
        if let Some(text) = ctx.active_text {
            self.analyse(uri, text);
        }
    }

    fn on_document_save(&mut self, uri: &str, ctx: &ExtensionContext) {
        self.on_document_change(uri, ctx);
    }

    fn execute_command(&mut self, _command: &str, _args: &[String]) -> CommandResult {
        CommandResult::Ok
    }

    fn poll(&mut self, _ctx: &ExtensionContext) -> Vec<ExtensionOutput> {
        if !self.dirty || self.cached_uri.is_empty() {
            return Vec::new();
        }
        self.dirty = false;

        vec![ExtensionOutput::Diagnostics {
            extension_id: self.manifest.id.clone(),
            uri: self.cached_uri.clone(),
            items: self.cached_diagnostics.clone(),
        }]
    }
}
