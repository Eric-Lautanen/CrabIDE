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
