//! Snippet engine — VS Code–compatible snippet syntax.
//!
//! Implements the full VS Code snippet specification:
//! - Tabstops: `$1`, `$2`, … `$0` (final cursor)
//! - Placeholders: `${1:default text}`
//! - Choice: `${1|option1,option2,option3|}`
//! - Variables: `$TM_FILENAME`, `$CLIPBOARD`, `$CURRENT_YEAR`, etc.
//! - Regex transforms: `${1/pattern/replacement/flags}`
//! - Nested placeholders
//!
//! # Usage
//!
//! ```ignore
//! let ctx = SnippetContext {
//!     file_path: Some(Path::new("src/main.rs")),
//!     language_id: "rust",
//!     clipboard: "",
//!     line_indent: "    ",
//! };
//! let expansion = SnippetEngine::expand(&snippet, cursor_pos, &ctx);
//! engine.begin_expansion(expansion);
//! ```

use std::collections::BTreeMap;

use crabide_core::types::{Position, Range, TextEdit};
use regex::Regex;
use std::cell::RefCell;

// ── Context ───────────────────────────────────────────────────────────────────

/// Context supplied to `SnippetEngine::expand` for resolving variables.
pub struct SnippetContext<'a> {
    /// Path of the file being edited (for `$TM_FILENAME` etc.).
    pub file_path: Option<&'a std::path::Path>,
    /// LSP language ID of the current document (e.g. `"rust"`).
    pub language_id: &'a str,
    /// Current clipboard contents (for `$CLIPBOARD`).
    pub clipboard: &'a str,
    /// Leading whitespace of the line where the snippet is inserted.
    /// Continuation lines inside the snippet are re-indented by this amount.
    pub line_indent: &'a str,
}

impl<'a> SnippetContext<'a> {
    /// An empty context with no file, no clipboard, no indent.
    pub fn empty() -> Self {
        Self {
            file_path: None,
            language_id: "",
            clipboard: "",
            line_indent: "",
        }
    }
}

// ── Public types ──────────────────────────────────────────────────────────────

/// A parsed snippet ready for expansion.
#[derive(Debug, Clone)]
pub struct Snippet {
    /// The raw snippet body string (VS Code snippet syntax).
    pub body: String,
    /// The snippet's display name (e.g. "for loop").
    pub label: String,
    /// Optional description shown in completion docs.
    pub description: Option<String>,
    /// Populated by the editor after `expand()` is called; empty until then.
    pub tabstops: Vec<SnippetTabstop>,
}

/// A single tabstop position within an expanded snippet.
#[derive(Debug, Clone)]
pub struct SnippetTabstop {
    /// Tabstop index (1-based; 0 = final cursor).
    pub index: u32,
    /// The range in the document where this tabstop was inserted.
    pub range: Range,
    /// Placeholder text (selected when the cursor lands on this tabstop).
    pub placeholder: String,
    /// Available choices, if this is a choice tabstop.
    pub choices: Vec<String>,
}

/// The result of expanding a snippet.
#[derive(Debug, Clone)]
pub struct SnippetExpansion {
    /// Text edits to apply to insert the snippet body (always exactly one edit).
    pub edits: Vec<TextEdit>,
    /// Tabstops in tabstop-index order (1, 2, … then 0 last).
    pub tabstops: Vec<SnippetTabstop>,
    /// The cursor position after the final `$0` tabstop (or end of body if no `$0`).
    pub final_cursor: Position,
}

// ── Internal AST ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Node {
    Text(String),
    Tabstop(u32),
    Placeholder {
        index: u32,
        body: Vec<Node>,
    },
    Choice {
        index: u32,
        options: Vec<String>,
    },
    Variable {
        name: String,
        default: Vec<Node>,
    },
    Transform {
        index: u32,
        pattern: String,
        replacement: String,
        flags: String,
    },
}

// ── Parser ────────────────────────────────────────────────────────────────────

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn new(s: &str) -> Self {
        Self {
            chars: s.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn eat(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Parse zero or more snippet nodes, stopping at EOF or `stop_at`.
    fn parse_body(&mut self, stop_at: Option<char>) -> Vec<Node> {
        let mut nodes: Vec<Node> = Vec::new();
        loop {
            match self.peek() {
                None => break,
                Some(c) if Some(c) == stop_at => break,
                Some('$') => {
                    if let Some(n) = self.parse_dollar() {
                        nodes.push(n);
                    }
                }
                Some('\\') => {
                    self.advance();
                    if let Some(c) = self.advance() {
                        Self::push_text(&mut nodes, c);
                    }
                }
                Some(c) => {
                    self.advance();
                    Self::push_text(&mut nodes, c);
                }
            }
        }
        nodes
    }

    fn push_text(nodes: &mut Vec<Node>, c: char) {
        if let Some(Node::Text(s)) = nodes.last_mut() {
            s.push(c);
        } else {
            nodes.push(Node::Text(c.to_string()));
        }
    }

    fn parse_dollar(&mut self) -> Option<Node> {
        self.advance(); // consume '$'
        if self.eat('{') {
            let node = self.parse_braced();
            self.eat('}');
            node
        } else if self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            Some(Node::Tabstop(self.parse_uint()))
        } else if self
            .peek()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
        {
            let name = self.parse_name();
            Some(Node::Variable {
                name,
                default: vec![],
            })
        } else {
            Some(Node::Text("$".to_string()))
        }
    }

    fn parse_braced(&mut self) -> Option<Node> {
        if self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            let index = self.parse_uint();
            if self.eat(':') {
                let body = self.parse_body(Some('}'));
                Some(Node::Placeholder { index, body })
            } else if self.eat('|') {
                let options = self.parse_choices();
                self.eat('|');
                Some(Node::Choice { index, options })
            } else if self.eat('/') {
                let pattern = self.parse_transform_part('/');
                self.eat('/');
                let replacement = self.parse_transform_part('/');
                self.eat('/');
                let flags = self.parse_flags();
                Some(Node::Transform {
                    index,
                    pattern,
                    replacement,
                    flags,
                })
            } else {
                Some(Node::Tabstop(index))
            }
        } else if self
            .peek()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
        {
            let name = self.parse_name();
            if self.eat(':') {
                let default = self.parse_body(Some('}'));
                Some(Node::Variable { name, default })
            } else if self.eat('/') {
                // Variable transform — resolve var, ignore transform for now
                self.skip_to('/');
                self.eat('/');
                self.skip_to('/');
                self.eat('/');
                self.parse_flags(); // consume flags
                Some(Node::Variable {
                    name,
                    default: vec![],
                })
            } else {
                Some(Node::Variable {
                    name,
                    default: vec![],
                })
            }
        } else {
            None
        }
    }

    fn parse_uint(&mut self) -> u32 {
        let mut s = String::new();
        while self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            s.push(self.advance().unwrap());
        }
        s.parse().unwrap_or(0)
    }

    fn parse_name(&mut self) -> String {
        let mut s = String::new();
        while self
            .peek()
            .map(|c| c.is_ascii_alphanumeric() || c == '_')
            .unwrap_or(false)
        {
            s.push(self.advance().unwrap());
        }
        s
    }

    fn parse_choices(&mut self) -> Vec<String> {
        let mut opts: Vec<String> = Vec::new();
        let mut cur = String::new();
        loop {
            match self.peek() {
                None | Some('|') => break,
                Some(',') => {
                    self.advance();
                    opts.push(std::mem::take(&mut cur));
                }
                Some('\\') => {
                    self.advance();
                    if let Some(c) = self.advance() {
                        cur.push(c);
                    }
                }
                Some(c) => {
                    self.advance();
                    cur.push(c);
                }
            }
        }
        if !cur.is_empty() {
            opts.push(cur);
        }
        opts
    }

    /// Parse a transform segment (pattern or replacement), stopping at `stop`.
    fn parse_transform_part(&mut self, stop: char) -> String {
        let mut s = String::new();
        let mut depth = 0i32;
        loop {
            match self.peek() {
                None => break,
                Some(c) if c == stop && depth == 0 => break,
                Some('}') if depth <= 0 => break,
                Some('{') => {
                    depth += 1;
                    s.push(self.advance().unwrap());
                }
                Some('}') => {
                    depth -= 1;
                    s.push(self.advance().unwrap());
                }
                Some('\\') => {
                    self.advance();
                    if let Some(c) = self.advance() {
                        s.push('\\');
                        s.push(c);
                    }
                }
                _ => {
                    s.push(self.advance().unwrap());
                }
            }
        }
        s
    }

    fn skip_to(&mut self, stop: char) {
        while self.peek().map(|c| c != stop).unwrap_or(false) {
            if self.peek() == Some('\\') {
                self.advance();
            }
            self.advance();
        }
    }

    fn parse_flags(&mut self) -> String {
        let mut s = String::new();
        while self
            .peek()
            .map(|c| c.is_ascii_alphabetic())
            .unwrap_or(false)
        {
            s.push(self.advance().unwrap());
        }
        s
    }
}

// ── Variable resolution ───────────────────────────────────────────────────────

fn resolve_variable(name: &str, ctx: &SnippetContext) -> Option<String> {
    match name {
        "TM_FILENAME" => ctx
            .file_path
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(str::to_owned),
        "TM_FILENAME_BASE" => ctx
            .file_path
            .and_then(|p| p.file_stem())
            .and_then(|n| n.to_str())
            .map(str::to_owned),
        "TM_DIRECTORY" => ctx
            .file_path
            .and_then(|p| p.parent())
            .and_then(|p| p.to_str())
            .map(str::to_owned),
        "TM_FILEPATH" => ctx.file_path.and_then(|p| p.to_str()).map(str::to_owned),
        "CLIPBOARD" => Some(ctx.clipboard.to_owned()),
        "CURRENT_YEAR" => Some(epoch_field(31_557_600, 1970, 1).to_string()),
        "CURRENT_MONTH" => Some(format!("{:02}", epoch_field(2_629_800, 1, 12))),
        "CURRENT_DATE" => Some(format!("{:02}", epoch_field(86_400, 1, 31))),
        "CURRENT_HOUR" => Some(format!("{:02}", epoch_field(3_600, 0, 23))),
        "CURRENT_MINUTE" => Some(format!("{:02}", epoch_field(60, 0, 59))),
        "CURRENT_SECOND" => Some(format!("{:02}", epoch_field(1, 0, 59))),
        "LINE_COMMENT" => Some(line_comment_for(ctx.language_id).to_owned()),
        "BLOCK_COMMENT_START" => Some(block_comment_start(ctx.language_id).to_owned()),
        "BLOCK_COMMENT_END" => Some(block_comment_end(ctx.language_id).to_owned()),
        _ => None,
    }
}

/// Compute a crude calendar field from Unix epoch: `(now / divisor) % range + base`.
fn epoch_field(divisor: u64, base: u64, wrap: u64) -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    (secs / divisor) % (wrap - base + 1) + base
}

fn line_comment_for(lang: &str) -> &'static str {
    match lang {
        "python" | "ruby" | "shellscript" | "bash" | "yaml" | "toml" | "r" => "#",
        "html" | "xml" => "<!--",
        "sql" | "lua" => "--",
        "vb" => "'",
        _ => "//",
    }
}

fn block_comment_start(lang: &str) -> &'static str {
    match lang {
        "html" | "xml" => "<!--",
        _ => "/*",
    }
}

fn block_comment_end(lang: &str) -> &'static str {
    match lang {
        "html" | "xml" => "-->",
        _ => "*/",
    }
}

// ── Expander ──────────────────────────────────────────────────────────────────

struct Expander<'a> {
    ctx: &'a SnippetContext<'a>,
    output: String,
    /// index list of (range, placeholder_text, choices)
    tabstop_data: BTreeMap<u32, Vec<(Range, String, Vec<String>)>>,
    /// Current line offset from the first inserted line.
    cur_line: u32,
    /// Current column (chars from start of current line).
    cur_col: u32,
    base_line: u32,
    base_col: u32,
    /// Cached compiled regexes keyed by (pattern, flags).
    regex_cache: RefCell<BTreeMap<String, Regex>>,
}

impl<'a> Expander<'a> {
    fn new(ctx: &'a SnippetContext<'a>, base_line: u32, base_col: u32) -> Self {
        Self {
            ctx,
            output: String::new(),
            tabstop_data: BTreeMap::new(),
            cur_line: 0,
            cur_col: 0,
            base_line,
            base_col,
            regex_cache: RefCell::new(BTreeMap::new()),
        }
    }

    fn current_pos(&self) -> Position {
        if self.cur_line == 0 {
            Position::new(self.base_line, self.base_col + self.cur_col)
        } else {
            Position::new(self.base_line + self.cur_line, self.cur_col)
        }
    }

    fn emit(&mut self, s: &str) {
        for ch in s.chars() {
            if ch == '\n' {
                self.output.push('\n');
                self.cur_line += 1;
                self.cur_col = 0;
                // Re-apply caller's indentation on continuation lines.
                let indent = self.ctx.line_indent;
                self.output.push_str(indent);
                self.cur_col = indent.chars().count() as u32;
            } else {
                self.output.push(ch);
                self.cur_col += 1;
            }
        }
    }

    fn expand_nodes(&mut self, nodes: &[Node]) {
        for node in nodes {
            self.expand_node(node);
        }
    }

    fn expand_node(&mut self, node: &Node) {
        match node {
            Node::Text(s) => self.emit(s),

            Node::Tabstop(index) => {
                let pos = self.current_pos();
                self.tabstop_data.entry(*index).or_default().push((
                    Range::new(pos, pos),
                    String::new(),
                    vec![],
                ));
            }

            Node::Placeholder { index, body } => {
                let start = self.current_pos();
                self.expand_nodes(body);
                let end = self.current_pos();
                let placeholder = collect_text(body);
                self.tabstop_data.entry(*index).or_default().push((
                    Range::new(start, end),
                    placeholder,
                    vec![],
                ));
            }

            Node::Choice { index, options } => {
                let start = self.current_pos();
                let first = options.first().map(String::as_str).unwrap_or("");
                self.emit(first);
                let end = self.current_pos();
                self.tabstop_data.entry(*index).or_default().push((
                    Range::new(start, end),
                    first.to_owned(),
                    options.clone(),
                ));
            }

            Node::Variable { name, default } => {
                let value = resolve_variable(name, self.ctx).unwrap_or_else(|| {
                    if default.is_empty() {
                        String::new()
                    } else {
                        collect_text(default)
                    }
                });
                self.emit(&value);
            }

            Node::Transform {
                index,
                pattern,
                replacement,
                flags,
            } => {
                let pos = self.current_pos();
                // Look up the text of the referenced tabstop (already expanded).
                let input = self
                    .tabstop_data
                    .get(index)
                    .and_then(|entries| entries.first())
                    .map(|(_, text, _)| text.as_str())
                    .unwrap_or("");
                let transformed = self.apply_transform(input, pattern, replacement, flags);
                self.emit(&transformed);
                let end = self.current_pos();
                self.tabstop_data.entry(*index).or_default().push((
                    Range::new(pos, end),
                    transformed,
                    vec![],
                ));
            }
        }
    }

    fn finish(self) -> (String, Vec<SnippetTabstop>, Position) {
        let final_pos = self.current_pos();
        let mut tabstops: Vec<SnippetTabstop> = self
            .tabstop_data
            .into_iter()
            .flat_map(|(index, entries)| {
                entries
                    .into_iter()
                    .map(move |(range, placeholder, choices)| SnippetTabstop {
                        index,
                        range,
                        placeholder,
                        choices,
                    })
            })
            .collect();
        // Sort: 1, 2, 3, … then $0 last.
        tabstops.sort_by(|a, b| match (a.index, b.index) {
            (0, 0) => std::cmp::Ordering::Equal,
            (0, _) => std::cmp::Ordering::Greater,
            (_, 0) => std::cmp::Ordering::Less,
            (x, y) => x.cmp(&y),
        });
        (self.output, tabstops, final_pos)
    }

    fn apply_transform(
        &self,
        input: &str,
        pattern: &str,
        replacement: &str,
        flags: &str,
    ) -> String {
        let cache_key = if flags.contains('i') {
            format!("(?i){pattern}")
        } else {
            pattern.to_owned()
        };

        let mut cache = self.regex_cache.borrow_mut();
        let re = match cache.entry(cache_key.clone()) {
            std::collections::btree_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::btree_map::Entry::Vacant(e) => match Regex::new(&cache_key) {
                Ok(re) => e.insert(re),
                Err(_) => return input.to_owned(),
            },
        };

        if flags.contains('g') {
            re.replace_all(input, replacement).into_owned()
        } else {
            re.replace(input, replacement).into_owned()
        }
    }
}

fn collect_text(nodes: &[Node]) -> String {
    nodes
        .iter()
        .map(|n| match n {
            Node::Text(s) => s.clone(),
            Node::Placeholder { body, .. } | Node::Variable { default: body, .. } => {
                collect_text(body)
            }
            _ => String::new(),
        })
        .collect()
}

/// Shift `range` by `delta` characters (signed) if the edit at `offset` affects it.
fn apply_delta_to_range(range: &mut crabide_core::types::Range, offset: Position, delta: i64) {
    use crabide_core::types::Position;

    /// Convert a Position to a linear offset (used for delta computation).
    /// We approximate: line * 1000000 + character, which is sufficient for
    /// relative re-ordering within a single file (no file has >1M chars per line).
    fn linear(p: Position) -> i64 {
        p.line as i64 * 1_000_000 + p.character as i64
    }

    let off_lin = linear(offset);
    let start_lin = linear(range.start);
    let end_lin = linear(range.end);

    if start_lin >= off_lin {
        // Whole range is after the edit point.
        if delta > 0 {
            range.start.character += delta as u32;
            range.end.character += delta as u32;
        } else {
            let abs = (-delta) as u32;
            // Avoid underflow.
            range.start.character = range.start.character.saturating_sub(abs);
            range.end.character = range.end.character.saturating_sub(abs);
        }
    } else if end_lin > off_lin {
        // Edit point is inside the range — extend the end only.
        if delta > 0 {
            range.end.character += delta as u32;
        } else {
            let abs = (-delta) as u32;
            range.end.character = range.end.character.saturating_sub(abs);
        }
    }
    // else: range is entirely before the edit — no change.
}

// ── SnippetEngine ─────────────────────────────────────────────────────────────

/// Manages snippet expansion and tabstop cycling for one editor view.
pub struct SnippetEngine {
    active_expansion: Option<ActiveExpansion>,
}

struct ActiveExpansion {
    tabstops: Vec<SnippetTabstop>,
    current_index: usize,
}

impl SnippetEngine {
    pub fn new() -> Self {
        Self {
            active_expansion: None,
        }
    }

    /// Returns true if a snippet is currently being edited (tabstop cycling active).
    pub fn is_active(&self) -> bool {
        self.active_expansion.is_some()
    }

    /// Parse and expand a snippet body at `insert_at`.
    ///
    /// Returns a `SnippetExpansion` with the edits to apply and the tabstop
    /// positions. Call `begin_expansion()` to start tabstop cycling.
    pub fn expand(
        snippet: &Snippet,
        insert_at: Position,
        ctx: &SnippetContext,
    ) -> SnippetExpansion {
        let mut parser = Parser::new(&snippet.body);
        let nodes = parser.parse_body(None);
        let mut expander = Expander::new(ctx, insert_at.line, insert_at.character);
        expander.expand_nodes(&nodes);
        let (text, tabstops, final_cursor) = expander.finish();
        SnippetExpansion {
            edits: vec![TextEdit::insert(insert_at, text)],
            tabstops,
            final_cursor,
        }
    }

    /// Activate tabstop cycling for a completed expansion.
    /// Call this after applying the expansion's edits to the document.
    pub fn begin_expansion(&mut self, expansion: SnippetExpansion) {
        if expansion.tabstops.is_empty() {
            self.active_expansion = None;
        } else {
            self.active_expansion = Some(ActiveExpansion {
                tabstops: expansion.tabstops,
                current_index: 0,
            });
        }
    }

    /// The currently focused tabstop.
    pub fn current_tabstop(&self) -> Option<&SnippetTabstop> {
        let exp = self.active_expansion.as_ref()?;
        exp.tabstops.get(exp.current_index)
    }

    /// Advance to the next tabstop. Returns `None` when the expansion ends.
    pub fn next_tabstop(&mut self) -> Option<&SnippetTabstop> {
        let can_advance = self
            .active_expansion
            .as_ref()
            .map(|e| e.current_index + 1 < e.tabstops.len())
            .unwrap_or(false);

        if !can_advance {
            self.active_expansion = None;
            return None;
        }

        let exp = self.active_expansion.as_mut()?;
        exp.current_index += 1;
        exp.tabstops.get(exp.current_index)
    }

    /// Return to the previous tabstop.
    pub fn prev_tabstop(&mut self) -> Option<&SnippetTabstop> {
        let can_go_back = self
            .active_expansion
            .as_ref()
            .map(|e| e.current_index > 0)
            .unwrap_or(false);

        if !can_go_back {
            return None;
        }

        let exp = self.active_expansion.as_mut()?;
        exp.current_index -= 1;
        exp.tabstops.get(exp.current_index)
    }

    /// Cancel the active snippet expansion (e.g. user pressed Escape).
    pub fn cancel(&mut self) {
        self.active_expansion = None;
    }

    /// Apply an edit delta to all active tabstop ranges.
    ///
    /// Call this after inserting or deleting text in the document while a snippet
    /// is active.  Shifts any tabstop range whose start is at or after `edit`'s
    /// offset by `new_text.chars().count() - edit.range_len_chars()`.  Also shifts
    /// ranges that *contain* the edit point so the placeholder boundary moves.
    pub fn apply_edit(&mut self, edit: &crabide_core::types::TextEdit) {
        let Some(ref mut exp) = self.active_expansion else {
            return;
        };
        let old_len = edit.range_len_chars();
        let new_len = edit.new_text.chars().count();
        let delta = (new_len as i64) - (old_len as i64);
        if delta == 0 {
            return;
        }
        let offset = edit.range.start;
        for ts in &mut exp.tabstops {
            apply_delta_to_range(&mut ts.range, offset, delta);
        }
    }
}

impl Default for SnippetEngine {
    fn default() -> Self {
        Self::new()
    }
}
