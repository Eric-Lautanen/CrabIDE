//! Built-in native extensions shipped with the editor.

mod git_blame;
mod markdown_preview;
mod rust_analyzer_lite;
mod theme_switcher;
mod todo_highlighter;

use crate::host::NativeExtension;

/// Construct all five built-in extensions.
///
/// Called once by [`ExtensionHost::new`] during startup.
pub fn builtin_extensions() -> Vec<Box<dyn NativeExtension>> {
    vec![
        Box::new(git_blame::GitBlameExtension::new()),
        Box::new(rust_analyzer_lite::RustAnalyzerLiteExtension::new()),
        Box::new(markdown_preview::MarkdownPreviewExtension::new()),
        Box::new(todo_highlighter::TodoHighlighterExtension::new()),
        Box::new(theme_switcher::ThemeSwitcherExtension::new()),
    ]
}
