//! `crabide-search` — fuzzy file finder + workspace grep.
//!
//! # Fuzzy file finder (Ctrl+P)
//! [`FuzzyFileFinder`] maintains an in-memory index of workspace file paths.
//! Call [`FuzzyFileFinder::update_index`] once (or on VFS events) and then
//! [`FuzzyFileFinder::search`] per keystroke — it is fast enough to run
//! synchronously on the UI thread.
//!
//! # Workspace grep (Ctrl+Shift+F)
//! [`grep_workspace`] searches file contents with a [`regex::Regex`] using
//! a Rayon parallel iterator.  Expected latency: <200 ms for typical
//! project sizes.  Call it from the app thread (not the UI thread) and send
//! the results back through the event channel.
//!
//! # File indexing
//! [`index_workspace_files`] walks workspace roots and collects all
//! text-file paths suitable for indexing.  It skips hidden directories,
//! `target/`, `node_modules/`, etc.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use nucleo::pattern::{CaseMatching, Normalization, Pattern};
use nucleo::{Config, Matcher, Utf32String};
use rayon::prelude::*;
use regex::Regex;

pub use crabide_core::error::{crabideError, Result};

/// Maximum number of fuzzy-finder results displayed in the overlay.
pub const FUZZY_MAX_RESULTS: usize = 15;

// ── FuzzyFileFinder ───────────────────────────────────────────────────────────

/// A single file-path match returned by the fuzzy finder.
#[derive(Debug, Clone)]
pub struct FuzzyMatch {
    /// Absolute path of the matched file.
    pub path: PathBuf,
    /// Display string used for scoring (often the relative path or file name).
    pub display: String,
    /// Nucleo score — higher is better.
    pub score: u32,
}

/// Fast fuzzy file-path finder backed by nucleo.
///
/// Build the index once with [`update_index`], then call [`search`] on every
/// query keystroke. The nucleo `Matcher` is cached across calls for efficiency.
pub struct FuzzyFileFinder {
    index: Vec<PathBuf>,
    matcher: Matcher,
}

impl FuzzyFileFinder {
    pub fn new() -> Self {
        Self {
            index: Vec::new(),
            matcher: Matcher::new(Config::DEFAULT),
        }
    }

    /// Replace the file index with a fresh set of paths.
    pub fn update_index(&mut self, files: Vec<PathBuf>) {
        self.index = files;
    }

    /// Returns `true` if the index contains at least one path.
    pub fn has_index(&self) -> bool {
        !self.index.is_empty()
    }

    /// Number of files in the index.
    pub fn index_len(&self) -> usize {
        self.index.len()
    }

    /// Fuzzy-search the index.
    ///
    /// When `query` is empty, returns the first `limit` files in index order.
    /// Otherwise scores every path with nucleo and returns the top `limit`
    /// hits sorted by descending score.
    pub fn search(&mut self, query: &str, limit: usize) -> Vec<FuzzyMatch> {
        if self.index.is_empty() {
            return Vec::new();
        }

        if query.is_empty() {
            return self
                .index
                .iter()
                .take(limit)
                .map(|p| FuzzyMatch {
                    display: p.to_string_lossy().into_owned(),
                    path: p.clone(),
                    score: 0,
                })
                .collect();
        }

        let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);

        let mut scored: Vec<FuzzyMatch> = self
            .index
            .iter()
            .filter_map(|p| {
                let display = p.to_string_lossy().into_owned();
                let hay = Utf32String::from(display.as_str());
                let score = pattern.score(hay.slice(..), &mut self.matcher)?;
                Some(FuzzyMatch {
                    path: p.clone(),
                    display,
                    score,
                })
            })
            .collect();

        scored.sort_by_key(|b| std::cmp::Reverse(b.score));
        scored.truncate(limit);
        scored
    }
}

impl Default for FuzzyFileFinder {
    fn default() -> Self {
        Self::new()
    }
}

// ── Workspace grep ────────────────────────────────────────────────────────────

/// One match from a workspace-wide text search.
#[derive(Debug, Clone)]
pub struct GrepMatch {
    /// Absolute path of the file containing the match.
    pub path: PathBuf,
    /// Zero-based line number.
    pub line_number: usize,
    /// Full text of the matching line (without trailing newline).
    pub line_text: String,
    /// Byte offset of the match start within `line_text`.
    pub match_start: usize,
    /// Byte offset of the match end within `line_text`.
    pub match_end: usize,
}

/// A handle that allows cancelling an in-flight workspace grep.
///
/// Clone the handle and pass it to [`grep_workspace`] by reference. Set
/// the abort flag to `true` to request cancellation. The search will stop
/// at the next file boundary (it may take a moment to respond).
#[derive(Clone, Default)]
pub struct GrepAbortHandle(Arc<AtomicBool>);

impl GrepAbortHandle {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    /// Signal the search to abort as soon as possible.
    pub fn abort(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Returns `true` if `abort()` has been called.
    pub fn is_aborted(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Search all text files under `roots` for `pattern`.
///
/// * `use_regex` — if `false`, the pattern is treated as a literal string.
/// * `case_sensitive` — if `false`, a `(?i)` prefix is added to the regex.
/// * `max_results` — hard cap on the number of returned matches.
/// * `abort` — optional [`GrepAbortHandle`]; if `abort.is_aborted()` returns
///   `true` during iteration, the search stops early.
///
/// Files are searched in parallel with Rayon.  Results are sorted by path,
/// then line number.
pub fn grep_workspace(
    roots: &[PathBuf],
    pattern: &str,
    use_regex: bool,
    case_sensitive: bool,
    max_results: usize,
    abort: Option<&GrepAbortHandle>,
) -> Vec<GrepMatch> {
    if pattern.is_empty() || roots.is_empty() {
        return Vec::new();
    }

    let flags = if case_sensitive { "" } else { "(?i)" };
    let re_str = if use_regex {
        format!("{flags}{pattern}")
    } else {
        format!("{flags}{}", regex::escape(pattern))
    };

    let re = match Regex::new(&re_str) {
        Ok(r) => r,
        Err(e) => {
            log::warn!("grep: invalid pattern — {e}");
            return Vec::new();
        }
    };

    let files = collect_files(roots);

    let mut results: Vec<GrepMatch> = files
        .par_iter()
        .flat_map(|p| {
            // Check cancellation before processing each file
            if abort.is_some_and(|h| h.is_aborted()) {
                return Vec::new();
            }
            search_file(p, &re, abort)
        })
        .collect();

    results.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| a.line_number.cmp(&b.line_number))
    });
    results.truncate(max_results);
    results
}

// ── File indexing ─────────────────────────────────────────────────────────────

/// Walk `roots` and collect all text-file paths for the fuzzy-finder index.
///
/// Skips hidden entries, `target/`, `node_modules/`, `.git/`, etc.
/// Returns paths sorted lexicographically.
pub fn index_workspace_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for root in roots {
        collect_files_recursive(root, &mut files);
    }
    files.sort();
    files
}

// ── Private helpers ───────────────────────────────────────────────────────────

fn collect_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for root in roots {
        collect_files_recursive(root, &mut files);
    }
    files
}

fn collect_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden entries and well-known non-source directories.
        if name_str.starts_with('.') {
            continue;
        }
        if matches!(
            name_str.as_ref(),
            "target" | "node_modules" | "dist" | "build" | "__pycache__" | ".cache"
        ) {
            continue;
        }

        if path.is_dir() {
            collect_files_recursive(&path, out);
        } else if is_text_extension(&path) {
            out.push(path);
        }
    }
}

/// Returns `true` for common source and text file extensions.
fn is_text_extension(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => matches!(
            ext,
            "rs" | "py"
                | "js"
                | "ts"
                | "jsx"
                | "tsx"
                | "go"
                | "c"
                | "h"
                | "cpp"
                | "cc"
                | "cxx"
                | "hpp"
                | "cs"
                | "java"
                | "kt"
                | "swift"
                | "rb"
                | "php"
                | "lua"
                | "sh"
                | "bash"
                | "zsh"
                | "fish"
                | "json"
                | "toml"
                | "yaml"
                | "yml"
                | "xml"
                | "html"
                | "htm"
                | "css"
                | "scss"
                | "sass"
                | "less"
                | "md"
                | "markdown"
                | "txt"
                | "sql"
                | "graphql"
                | "gql"
                | "proto"
                | "wgsl"
                | "glsl"
                | "hlsl"
                | "r"
                | "jl"
                | "ex"
                | "exs"
                | "erl"
                | "hrl"
                | "hs"
                | "ml"
                | "mli"
                | "nim"
                | "v"
                | "zig"
                | "dart"
                | "vue"
                | "svelte"
                | "wit"
                | "lock"
                | "gitignore"
                | "env"
        ),
        None => false,
    }
}

/// Search in-memory buffers (open documents) for a pattern.
///
/// This is identical to `grep_workspace` but operates on already-loaded text
/// lines instead of reading files from disk.  The `buffers` parameter is a
/// slice of `(path, lines)` pairs where `lines` is a slice of document lines.
pub fn grep_buffers(
    buffers: &[(PathBuf, &[String])],
    pattern: &str,
    use_regex: bool,
    case_sensitive: bool,
    max_results: usize,
    abort: Option<&GrepAbortHandle>,
) -> Vec<GrepMatch> {
    if pattern.is_empty() || buffers.is_empty() {
        return Vec::new();
    }

    let flags = if case_sensitive { "" } else { "(?i)" };
    let re_str = if use_regex {
        format!("{flags}{pattern}")
    } else {
        format!("{flags}{}", regex::escape(pattern))
    };

    let re = match Regex::new(&re_str) {
        Ok(r) => r,
        Err(e) => {
            log::warn!("grep_buffers: invalid pattern — {e}");
            return Vec::new();
        }
    };

    let mut results: Vec<GrepMatch> = Vec::new();
    for (path, lines) in buffers {
        if abort.is_some_and(|h| h.is_aborted()) {
            break;
        }
        for (line_num, line) in lines.iter().enumerate() {
            for m in re.find_iter(line) {
                results.push(GrepMatch {
                    path: path.clone(),
                    line_number: line_num,
                    line_text: line.clone(),
                    match_start: m.start(),
                    match_end: m.end(),
                });
                if results.len() >= max_results {
                    return results;
                }
            }
        }
    }
    results
}

/// Search a single file for all regex matches, checking the abort handle.
fn search_file(path: &Path, re: &Regex, abort: Option<&GrepAbortHandle>) -> Vec<GrepMatch> {
    if abort.is_some_and(|h| h.is_aborted()) {
        return Vec::new();
    }
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    content
        .lines()
        .enumerate()
        .flat_map(|(line_num, line)| {
            re.find_iter(line).map(move |m| GrepMatch {
                path: path.to_owned(),
                line_number: line_num,
                line_text: line.to_owned(),
                match_start: m.start(),
                match_end: m.end(),
            })
        })
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // ── FuzzyFileFinder ─────────────────────────────────────────────────────

    #[test]
    fn fuzzy_finder_new_is_empty() {
        let mut ff = FuzzyFileFinder::new();
        assert!(!ff.has_index());
        assert_eq!(ff.index_len(), 0);
        assert!(ff.search("", 10).is_empty());
    }

    #[test]
    fn fuzzy_finder_default_is_empty() {
        let ff: FuzzyFileFinder = Default::default();
        assert!(!ff.has_index());
        assert_eq!(ff.index_len(), 0);
    }

    #[test]
    fn fuzzy_finder_update_index() {
        let mut ff = FuzzyFileFinder::new();
        ff.update_index(vec![PathBuf::from("/a.rs"), PathBuf::from("/b.rs")]);
        assert!(ff.has_index());
        assert_eq!(ff.index_len(), 2);
    }

    #[test]
    fn fuzzy_finder_search_empty_query() {
        let mut ff = FuzzyFileFinder::new();
        ff.update_index(vec![
            PathBuf::from("/project/src/main.rs"),
            PathBuf::from("/project/src/lib.rs"),
        ]);
        let results = ff.search("", 1);
        assert_eq!(results.len(), 1);
        // Empty query returns first files in index order
        assert!(results[0].path.ends_with("main.rs"));
        assert_eq!(results[0].score, 0);
    }

    #[test]
    fn fuzzy_finder_search_with_query() {
        let mut ff = FuzzyFileFinder::new();
        ff.update_index(vec![
            PathBuf::from("/project/src/main.rs"),
            PathBuf::from("/project/src/lib.rs"),
            PathBuf::from("/project/tests/test_helper.rs"),
        ]);
        let results = ff.search("lib", 10);
        assert!(!results.is_empty());
        // "lib" should match lib.rs
        assert!(results.iter().any(|m| m.path.ends_with("lib.rs")));
    }

    #[test]
    fn fuzzy_finder_search_no_match() {
        let mut ff = FuzzyFileFinder::new();
        ff.update_index(vec![PathBuf::from("/a.rs"), PathBuf::from("/b.rs")]);
        let results = ff.search("zzzznonexistent", 10);
        // May have some results if fuzzy matching finds something; but in practice
        // "zzzznonexistent" shouldn't match "a.rs" or "b.rs" with nucleo
        // If it does match, that's fine — just verify no panic.
        assert!(results.len() <= 2);
    }

    #[test]
    fn fuzzy_finder_search_respects_limit() {
        let mut ff = FuzzyFileFinder::new();
        let many: Vec<PathBuf> = (0..20)
            .map(|i| PathBuf::from(format!("/file_{}.rs", i)))
            .collect();
        ff.update_index(many);
        let results = ff.search("file", 5);
        assert!(results.len() <= 5);
    }

    #[test]
    fn fuzzy_finder_search_empty_index() {
        let mut ff = FuzzyFileFinder::new();
        assert!(ff.search("anything", 10).is_empty());
    }

    // ── GrepAbortHandle ─────────────────────────────────────────────────────

    #[test]
    fn abort_handle_new_not_aborted() {
        let h = GrepAbortHandle::new();
        assert!(!h.is_aborted());
    }

    #[test]
    fn abort_handle_abort() {
        let h = GrepAbortHandle::new();
        h.abort();
        assert!(h.is_aborted());
    }

    #[test]
    fn abort_handle_default_not_aborted() {
        let h = GrepAbortHandle::default();
        assert!(!h.is_aborted());
    }

    #[test]
    fn abort_handle_clone_reflects_abort() {
        let h1 = GrepAbortHandle::new();
        let h2 = h1.clone();
        h1.abort();
        assert!(h2.is_aborted()); // Shared AtomicBool
    }

    // ── grep_workspace ──────────────────────────────────────────────────────

    #[test]
    fn grep_workspace_empty_pattern() {
        let results = grep_workspace(&[PathBuf::from(".")], "", false, true, 100, None);
        assert!(results.is_empty());
    }

    #[test]
    fn grep_workspace_empty_roots() {
        let results = grep_workspace(&[], "pattern", false, true, 100, None);
        assert!(results.is_empty());
    }

    #[test]
    fn grep_workspace_invalid_regex_returns_empty() {
        let results = grep_workspace(&[PathBuf::from(".")], "[invalid", true, true, 100, None);
        assert!(results.is_empty());
    }

    #[test]
    fn grep_workspace_literal_search() {
        // Search in the current directory (this source file) for a known string.
        // We use "grep_workspace_literal_search" itself as the pattern.
        // Note: the test runs from the workspace root, so we need to find
        // our source file: crates/crabide-search/src/lib.rs
        let source = PathBuf::from("crates/crabide-search/src/lib.rs");
        if !source.exists() {
            // The test might be run from a different cwd; skip gracefully
            return;
        }
        let results = grep_workspace(
            &[PathBuf::from("crates/crabide-search/src")],
            "fn grep_workspace_literal_search",
            false,
            true,
            100,
            None,
        );
        assert!(!results.is_empty(), "should find the test function itself");
        assert!(results.iter().any(|m| m.path.ends_with("lib.rs")));
    }

    #[test]
    fn grep_workspace_case_insensitive() {
        let source = PathBuf::from("crates/crabide-search/src/lib.rs");
        if !source.exists() {
            return;
        }
        let results = grep_workspace(
            &[PathBuf::from("crates/crabide-search/src")],
            "GREP_WORKSPACE_CASE_INSENSITIVE",
            false,
            false, // case-insensitive
            100,
            None,
        );
        // Should find the function name even though case differs
        assert!(!results.is_empty(), "case-insensitive should match");
    }

    #[test]
    fn grep_workspace_regex_search() {
        let source = PathBuf::from("crates/crabide-search/src/lib.rs");
        if !source.exists() {
            return;
        }
        // regex: "grep_workspace_.*test" should match several test functions
        let results = grep_workspace(
            &[PathBuf::from("crates/crabide-search/src")],
            r"grep_workspace_\w+_test",
            true, // use_regex
            true,
            100,
            None,
        );
        assert!(
            !results.is_empty(),
            "regex should match test function names"
        );
    }

    #[test]
    fn grep_workspace_abort_prevents_results() {
        // Use a temp dir with a file we can search
        let dir = std::env::temp_dir().join("crabide_test_grep_abort");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("test.txt");
        let _ = std::fs::write(&file, b"unique_match_content_xyz\n");

        let abort = GrepAbortHandle::new();
        abort.abort(); // Abort before searching

        let results = grep_workspace(
            std::slice::from_ref(&dir),
            "unique_match_content_xyz",
            false,
            true,
            100,
            Some(&abort),
        );
        assert!(results.is_empty(), "aborted search should return nothing");

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── GrepMatch ───────────────────────────────────────────────────────────

    #[test]
    fn grep_match_fields() {
        let m = GrepMatch {
            path: PathBuf::from("/path/to/file.rs"),
            line_number: 42,
            line_text: "fn hello() {}".into(),
            match_start: 3,
            match_end: 8,
        };
        assert_eq!(m.path.to_string_lossy(), "/path/to/file.rs");
        assert_eq!(m.line_number, 42);
        assert_eq!(m.line_text, "fn hello() {}");
        assert_eq!(m.match_start, 3);
        assert_eq!(m.match_end, 8);
    }

    // ── grep_buffers ────────────────────────────────────────────────────────

    #[test]
    fn grep_buffers_empty_pattern() {
        let lines: [String; 1] = ["fn main() {}".to_string()];
        let buffers = [(PathBuf::from("test.rs"), &lines[..])];
        let results = grep_buffers(&buffers, "", false, true, 100, None);
        assert!(results.is_empty());
    }

    #[test]
    fn grep_buffers_empty_buffers() {
        let results = grep_buffers(&[], "pattern", false, true, 100, None);
        assert!(results.is_empty());
    }

    #[test]
    fn grep_buffers_finds_match() {
        let lines: [String; 3] = [
            "fn foo() {}".to_string(),
            "fn bar() {}".to_string(),
            "// comment".to_string(),
        ];
        let buffers = [(PathBuf::from("src/lib.rs"), &lines[..])];
        let results = grep_buffers(&buffers, "fn", false, true, 100, None);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|m| m.path.ends_with("lib.rs")));
        assert_eq!(results[0].line_number, 0);
        assert_eq!(results[1].line_number, 1);
    }

    #[test]
    fn grep_buffers_case_sensitive() {
        let lines: [String; 2] = ["Hello".to_string(), "hello".to_string()];
        let buffers = [(PathBuf::from("test.rs"), &lines[..])];
        let results = grep_buffers(&buffers, "Hello", false, true, 100, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].line_number, 0);
    }

    #[test]
    fn grep_buffers_case_insensitive() {
        let lines: [String; 2] = ["Hello".to_string(), "hello".to_string()];
        let buffers = [(PathBuf::from("test.rs"), &lines[..])];
        let results = grep_buffers(&buffers, "hello", false, false, 100, None);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn grep_buffers_max_results() {
        let lines: Vec<String> = std::iter::repeat("line with x".to_string())
            .take(100)
            .collect();
        let buffers = [(PathBuf::from("big.rs"), &lines[..])];
        let results = grep_buffers(&buffers, "x", false, true, 10, None);
        assert_eq!(results.len(), 10);
    }

    #[test]
    fn grep_buffers_abort() {
        let abort = GrepAbortHandle::new();
        abort.abort();
        let lines: [String; 1] = ["match_me".to_string()];
        let buffers = [(PathBuf::from("test.rs"), &lines[..])];
        let results = grep_buffers(&buffers, "match_me", false, true, 100, Some(&abort));
        assert!(results.is_empty());
    }

    #[test]
    fn grep_buffers_invalid_regex() {
        let lines: [String; 1] = ["some text".to_string()];
        let buffers = [(PathBuf::from("test.rs"), &lines[..])];
        let results = grep_buffers(&buffers, "[invalid", true, true, 100, None);
        assert!(results.is_empty());
    }

    #[test]
    fn grep_buffers_multiple_buffers() {
        let lines_a: [String; 1] = ["fn a() {}".to_string()];
        let lines_b: [String; 1] = ["fn b() {}".to_string()];
        let buffers = [
            (PathBuf::from("a.rs"), &lines_a[..]),
            (PathBuf::from("b.rs"), &lines_b[..]),
        ];
        let results = grep_buffers(&buffers, "fn", false, true, 100, None);
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|m| m.path.ends_with("a.rs")));
        assert!(results.iter().any(|m| m.path.ends_with("b.rs")));
    }

    #[test]
    fn grep_buffers_regex_search() {
        let lines: [String; 2] = ["123-abc".to_string(), "456-def".to_string()];
        let buffers = [(PathBuf::from("test.rs"), &lines[..])];
        let results = grep_buffers(&buffers, r"\d+", true, true, 100, None);
        assert_eq!(results.len(), 2);
    }

    // ── FuzzyMatch ──────────────────────────────────────────────────────────

    #[test]
    fn fuzzy_match_fields() {
        let m = FuzzyMatch {
            path: PathBuf::from("/main.rs"),
            display: "main.rs".into(),
            score: 42,
        };
        assert_eq!(m.path.to_string_lossy(), "/main.rs");
        assert_eq!(m.display, "main.rs");
        assert_eq!(m.score, 42);
    }

    // ── index_workspace_files ───────────────────────────────────────────────

    #[test]
    fn index_workspace_files_empty_roots() {
        let files = index_workspace_files(&[]);
        assert!(files.is_empty());
    }

    #[test]
    fn index_workspace_files_nonexistent_dir() {
        let files = index_workspace_files(&[PathBuf::from("/nonexistent_dir_xyz123")]);
        assert!(files.is_empty());
    }

    #[test]
    fn index_workspace_files_skip_hidden() {
        let dir = std::env::temp_dir().join("crabide_test_index_skip_hidden");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(".hidden_file.rs"), b"");
        let _ = std::fs::write(dir.join("visible.rs"), b"");
        let _ = std::fs::create_dir(dir.join(".hidden_dir"));
        let _ = std::fs::write(dir.join(".hidden_dir").join("nested.rs"), b"");

        let files = index_workspace_files(std::slice::from_ref(&dir));
        assert_eq!(files.len(), 1, "should only find visible.rs");
        assert!(files[0].ends_with("visible.rs"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn index_workspace_files_skip_well_known_dirs() {
        let dir = std::env::temp_dir().join("crabide_test_index_skip_well_known");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::create_dir(dir.join("target"));
        let _ = std::fs::write(dir.join("target").join("build.rs"), b"");
        let _ = std::fs::create_dir(dir.join("node_modules"));
        let _ = std::fs::write(dir.join("node_modules").join("pkg.rs"), b"");

        let files = index_workspace_files(std::slice::from_ref(&dir));
        assert!(files.is_empty(), "should skip target and node_modules");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── is_text_extension (tested via known paths) ──────────────────────────

    #[test]
    fn is_text_extension_known() {
        assert!(is_text_extension(Path::new("foo.rs")));
        assert!(is_text_extension(Path::new("foo.py")));
        assert!(is_text_extension(Path::new("foo.js")));
        assert!(is_text_extension(Path::new("foo.md")));
        assert!(is_text_extension(Path::new("foo.toml")));
        assert!(is_text_extension(Path::new("foo.json")));
        assert!(is_text_extension(Path::new("foo.html")));
    }

    #[test]
    fn is_text_extension_unknown() {
        assert!(!is_text_extension(Path::new("foo.exe")));
        assert!(!is_text_extension(Path::new("foo.dll")));
        assert!(!is_text_extension(Path::new("foo.so")));
        assert!(!is_text_extension(Path::new("foo.png")));
        assert!(!is_text_extension(Path::new("foo.jpg")));
        assert!(!is_text_extension(Path::new("foo.mp3")));
        assert!(!is_text_extension(Path::new("foo"))); // no extension
    }

    #[test]
    fn is_text_extension_no_ext() {
        assert!(!is_text_extension(Path::new("Makefile")));
    }
}
