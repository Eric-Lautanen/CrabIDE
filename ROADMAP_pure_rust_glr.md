# Pure Rust GLR Parser — Roadmap

**Goal**: Replace the C-based tree-sitter parser with a fully idiomatic Rust
implementation — no `cc`, no `build.rs` C compilation, no `extern "C"` FFI.
Suitable for embedding in editors, language servers, and other latency-sensitive
tools.

## How to use this document

This roadmap is designed for an AI agent or human engineer to execute
**sequentially by phase**. Each section lists concrete deliverables,
testable criteria, and ship gates. Do not skip phases.

**Web search is required throughout.** Many details (tree-sitter source layout,
grammar JSON schema, query file format, existing external scanner C sources)
are not fully specified here — the agent must fetch the latest canonical
information from:

- <https://github.com/tree-sitter/tree-sitter> — source code, issue tracker
- <https://tree-sitter.github.io/tree-sitter/> — official docs
- <https://crates.io/crates/tree-sitter> — Rust crate API
- Individual grammar repos under <https://github.com/tree-sitter/> — grammar
  JSON, scanner C sources, query files
- <https://github.com/softdevteam/grmtools/> — grmtools/lrpar, the closest
  existing Rust GLR implementation

Before starting any phase, search for relevant tree-sitter issue discussions
and existing crates to avoid duplicating effort.

---

## Phase 0 — Foundation (3–4 months)

### 0.1 Understand the target
Study tree-sitter's C source exhaustively:
- `lib/src/parser.c` — the GLR engine core (~800 lines, the crucial piece)
- `lib/src/language.c` — language instantiation & serialization
- `lib/src/lexer.c` — external scanner integration
- `lib/src/node.c` — tree cursor & node API
- `lib/src/query.c` — query engine (separate concern, can wait)

**Deliverable**: Internal design doc mapping every C struct, function, and state
machine to Rust equivalents.

### 0.2 Core data structures
Define the in-memory grammar format:

```rust
// The compiled grammar — what a .so/.dll currently provides
struct Grammar {
    version: u32,
    symbol_count: u32,
    alias_count: u32,
    token_count: u32,
    state_count: u32,
    // Parse table: the big one
    parse_table: Vec<ParseTableEntry>,
    // … aliases, field mapping, metadata
}

enum ParseTableEntry {
    Shift { state: StateId },
    Reduce { production: ProductionId },
    Accept,
    Error,
}
```

**Key insight**: tree-sitter's parse table is a compact 2D array
`[state][symbol] → action`. The Rust version should be a flat `Vec<Action>` with
lookup via `state * symbol_count + symbol`.

**Deliverable**: Crate `glr-core` with `Grammar`, `ParseTable`, `Symbol`,
`StateId`, `Production` — all `#[no_std]` compatible + `serde` for caching.

### 0.3 The GLR engine
Implement the core `Parser::parse()` loop:

```
Input:  Grammar + source bytes
Output: MutableTree (write-only during parse)

1. Create a "fork stack" — the GLR *graph-structured stack*
2. For each token:
   a. Lex the next token (see Phase 1)
   b. For each active GLR "fork":
      - Lookup action in parse_table[current_state][token]
      - Shift   → push new state, consume token
      - Reduce  → pop N states, goto continuation state, push reduction
      - Accept  → done
      - Error   → try other forks; if all dead, report error
3. Handle ambiguity: when two forks converge on the same state + stack
   prefix, *merge* them (the "G" in GLR — graph-structured stack sharing)
```

The trickiest part is the **merge** logic: tree-sitter avoids full GSS overhead
by exploiting that most LR conflicts are local. The Rust version can start
simple (keep all forks separate) and add GSS merging as an optimization.

**Deliverable**: `Parser::parse(&self, grammar: &Grammar, source: &[u8]) -> Tree`
producing correct output for unambiguous grammars.

### 0.3.1 Test: GLR engine correctness

Validate the engine on hand-crafted ambiguous and unambiguous grammars before
moving on:

| Test | Grammar | Input | Expected behavior |
|------|---------|-------|-------------------|
| Simple arithmetic | `E → E + E \| int` | `"1 + 2 + 3"` | Right-associative parse tree (or left, per precedence) |
| Dangling else | `S → if E then S \| if E then S else S \| …` | `"if a then if b then c else d"` | else binds to innermost if |
| Empty production | `S → A \| ε ; A → "a"` | `""` | Single reduction to S |
| Long chain | `S → A+ ; A → "a"` | `"a" × 1000` | 1000 A nodes, no stack overflow |
| Ambiguous expression | `E → E + E \| E * E \| int` | `"1 + 2 * 3"` | Both parses produced, GLR forks merged |
| Conflicted production | `S → "a" S \| "a"` | `"a" × 10` | Terminates, accepts, no infinite loop |

Each test asserts:
- Parse succeeds or fails as expected
- Tree structure matches expected node count, depth, and symbol kinds
- No panics, no assertion failures
- With `cfg(debug_assertions)`, internal invariants hold (fork count ≤ limit,
  stack depth ≥ reduction pop count)

Run as `cargo test --test glr_engine` — must pass before Phase 0 ships.

### 0.4 Tree construction
Build a mutable tree during reduce actions:

```rust
struct MutableTree {
    nodes: Vec<InternalNode>,
    // parent pointers, sibling links — built incrementally
}

struct InternalNode {
    kind: Symbol,
    start_byte: u32,
    end_byte: u32,
    child_count: u32,
    parent: Option<u32>,
}
```

Freeze to `Tree` (immutable, `Arc<[Node]>`) on completion.

**Deliverable**: Immutable `Tree` with cursor API matching tree-sitter's.

### 0.5 Error recovery (ERROR nodes)

For editor use, the parser must never reject input — it must produce a tree
with ERROR nodes wherever syntax is invalid. Tree-sitter's approach:

1. When no valid action exists for the current state + token, create an ERROR
   node that spans the problematic region
2. Skip tokens until a state is found that can continue (a sync strategy)
3. ERROR nodes are real tree nodes with a `kind() == "ERROR"` — queries can
   match them, the editor can style them

The simplest recovery: when stuck, consume tokens one at a time, checking after
each if the current state can shift or reduce. Once resync succeeds, resume
normal parsing.

**Deliverable**: `Parser::parse()` always returns a `Tree`, never fails. ERROR
nodes correctly bracket invalid ranges. Test with malformed inputs.

---

## Phase 1 — Lexer (2–3 months)

### 1.1 Built-in lexer
Replace tree-sitter's hand-written C lexer with a generic, table-driven lexer:

```rust
trait Lexer {
    fn next_token(&mut self, source: &[u8], cursor: &mut usize) -> Option<Token>;
}

/// Table-driven lexer generated from grammar `.json`
struct BuiltinLexer {
    dfa: Vec<DFAState>,
    // states, transitions, accept symbols
}
```

### 1.2 External scanner API
Most real grammars need hand-written "external scanners" for indentation
(Python), heredocs (Ruby, Bash), template strings (JavaScript), etc.

```rust
trait ExternalScanner {
    fn scan(&mut self, source: &[u8], cursor: &mut usize) -> Option<Symbol>;
    fn serialize(&self) -> Vec<u8>;   // for incremental reparse
    fn deserialize(&mut self, state: &[u8]);
}
```

tree-sitter passes `valid_symbols` (which tokens are legal at this point) — the
Rust version should too, so scanners can make informed decisions.

### 1.2.1 Test: Lexer & scanner coverage

Each lexer mode (built-in DFA, external scanner) gets:

| Test | What it validates |
|------|-------------------|
| Token-by-token comparison | Lex `"if x == 3 then return"` via library and via our lexer; every token kind, span, and value must match |
| External scanner integration | Python scanner on `"if x:\n  pass"` — produces INDENT/DEDENT at correct positions |
| EOF behavior | Empty input → single EOF token. Input ending mid‑token → error |
| Multi-byte UTF-8 | `"// 日本語\nlet x = 1"` — comment span covers all bytes, not just ASCII |
| Maximum token length | 10 MB string literal — lexer doesn't OOM, produces single string token |
| Scanner serialization | Round-trip: scan → serialize → deserialize → scan again from deserialized state, identical token stream |

Build a **lexer fuzz harness** (`cargo fuzz --target lexer`) that feeds random
byte strings and asserts:
- No panics
- Token kinds are valid per the grammar
- Spans are contiguous and non-overlapping
- Spans cover the entire input (end of last span = input length)

### 1.3 Incremental re-parse
Core selling point of tree-sitter. When source changes:
1. Find the first changed byte
2. Re-lex from there
3. Re-parse using the *previous tree* as a hint:
   - Tree-sitter's method: track `ts_node_has_changes()`, repopulate
     the lexer cache from unchanged nodes, re-parse from the first
     changed node downward
4. During reduce, reuse unchanged subtrees from the old tree

This is ~500 lines of subtle state management. tree-sitter's incremental
re-parse is its killer feature — must get this right.

**Deliverable**: `Parser::parse_incremental(&mut self, old_tree: &Tree, source: &[u8]) -> Tree`

---

## Phase 2 — Grammar compilation (3–4 months)

### 2.1 Grammar DSL
Define a Rust-native DSL for writing grammars:

```rust
glr_grammar! {
    language: "javascript",

    tokens: {
        "identifier" = /[a-zA-Z_$][\w$]*/,
        "number"     = /\d+(\.\d+)?/,
        "string"     = /"[^"]*"/,
        "+"          = "+",
        ";"          = ";",
        "{"          = "{",
        "}"          = "}",
    },

    rules: {
        Program       = { Statement* },
        Statement     = { Expression ";" },
        Expression    = { "identifier" }
                      | { "number" }
                      | { Expression "+" Expression }
                      | { "{" Expression "}" },
    },
}
```

This generates the LR parse table + lexer DFA at compile time via a proc-macro.

### 2.2 LR table generation from `.json`
Alternatively (interop): read tree-sitter's `grammar.json`, run our own
LR(1)/GLR table generator, produce `Grammar` struct.
This lets us consume existing tree-sitter grammar repos without rewriting them.

Implementation:
1. Parse `grammar.json` → internal `GrammarAst`
2. Compute FIRST/FOLLOW sets
3. Build LR(1) items → state machine
4. Resolve conflicts (prefer shift over reduce, declared precedence)
5. Emit compressed parse table

**Deliverable**: `cargo run -- compile path/to/grammar.json -o grammar.bin`

---

## Phase 3 — Query engine (2–3 months)

### 3.1 Pattern matching

tree-sitter's query system is a mini-language for finding syntax patterns.
Port `lib/src/query.c` (~2000 lines): compile query strings into a state
machine, execute against a `Tree`.

**Query syntax to support** (from tree-sitter's `.scm` files):

| Feature | Example | Priority |
|---------|---------|----------|
| Node kind match | `(function_definition)` | P0 (required) |
| Anonymous node match | `"return"` | P0 |
| Field match | `body: (block)` | P0 |
| Wildcard | `(_)` | P0 |
| Nested patterns | `(function name: (identifier) @name)` | P0 |
| Capture | `@name` | P0 |
| Quantifiers | `+`, `*`, `?` | P1 |
| Alternation | `(_ "if" _)` / `(_ "else" _)` | P1 |
| Predicates | `(#eq? @name "foo")` | P1 |
| Anchors | `. (program)` (start-of-root) | P2 |
| `#set!` directives | `#set! priority 1` (highlight overrides) | P1 |
| Match negation | `(identifier) @name (#not-eq? @name "a")` | P2 |
| `make-syntax-query`-style | Wildcard field `(_)* @capture` | P2 |

**Implementation sketch:**

```rust
/// Compiled representation of a `.scm` query file.
struct Query {
    /// State machine for matching patterns against a tree cursor.
    states: Vec<QueryState>,
    /// List of capture names in declaration order.
    captures: Vec<String>,
    /// Predicates that must be evaluated after a match.
    predicates: Vec<Predicate>,
}

struct QueryMatch {
    pattern_index: usize,
    captures: Vec<(CaptureName, Node)>,
}

impl Tree {
    fn query(&self, query: &Query) -> QueryMatches<'_>;
}

impl<'tree> Iterator for QueryMatches<'tree> {
    type Item = QueryMatch;
}
```

**Deliverable**: `cargo test --test query` passes all tree-sitter `.scm` test
files from the `tree-sitter-javascript` and `tree-sitter-python` repos.

### 3.2 Highlight integration
Hook queries into the syntax highlighting pipeline. Replace our current
`tree-sitter` + `queries.scm` loading with pure Rust equivalents.

**Deliverable**: Drop-in `syntax::highlight()` that takes source + grammar +
query set, returns styled tokens.

---

## Phase 4 — Migration & polish (3–4 months)

### 4.1 Port all 20 grammars
For each grammar tree-sitter ships:
1. Translate `grammar.json` → our format
2. Re-implement external scanner in Rust (this is the labor: each grammar has a
   hand-written C scanner of varying complexity)
3. Regenerate queries (they're already `.scm` files — no change needed there)
4. Test against known-good parse trees

External scanner portability by grammar:

| Grammar  | Scanner complexity | Rust effort |
|----------|------------------|-------------|
| Python   | indentation, dedent | medium |
| Ruby     | heredocs, regex literals | medium |
| JavaScript| template strings, regex | medium |
| HTML     | self-closing tags | easy |
| CSS      | simple token types | easy |
| Bash     | heredocs, backticks | medium |
| Kotlin   | *none* (no external scanner) | *none* |
| SCSS     | *none* | *none* |
| Less     | *none* | *none* |
| Markdown | fenced code blocks, HTML blocks | medium |
| …        | | |

~10 grammars have no external scanner → trivial. ~10 more have scanners ranging
from 50–500 lines of C → ~1–2 weeks each.

### 4.2 Conformance validation

For each grammar, run through the **Validation strategy** conformance suite
(Tier 2) against the tree-sitter C baseline. No grammar ships until it passes
node-by-node comparison on the full corpus.

### 4.3 Performance benchmarks

Run the criterion benchmarks (Tier 4) for the grammar's language group.
Regression thresholds are release-blocking.

---

## Repository structure

Proposed workspace layout in a single git repository:

```
glr/
├── Cargo.toml              # workspace root
├── glr-core/               # Phase 0 — Grammar, ParseTable, Tree, cursors
├── glr-engine/             # Phase 0 — Parser, GLR loop, error recovery
├── glr-lexer/              # Phase 1 — BuiltinLexer, ExternalScanner trait
├── glr-grammar/            # Phase 2 — grammar JSON → ParseTable compiler
├── glr-query/              # Phase 3 — query compiler + executor
├── glr-syntax/             # Phase 3 — highlight pipeline (consumes queries)
├── glr-conformance/        # Validation Tier 2 — tree-sitter C comparison runner
├── glr-fuzz/               # Validation Tier 3 — cargo-fuzz targets
├── glr-bench/              # Validation Tier 4 — criterion benchmarks
├── grammars/               # Phase 4 — vendored grammar JSON + scanner ports
│   ├── javascript/
│   ├── python/
│   ├── rust/
│   └── ...
└── corpus/                 # Validation Tier 2 — source file corpus (git-lfs)
    ├── javascript/
    ├── python/
    └── ...
```

**Crate dependency order**: `glr-core` ← `glr-grammar` + `glr-lexer` ←
`glr-engine` ← `glr-query` ← `glr-syntax`.

**No circular deps**: `glr-lexer` depends on `glr-core` (for `Symbol` types)
but not on `glr-engine`. The engine orchestrates lex + parse.

**Grammar JSON source**: Each `grammars/<lang>/` directory mirrors the
corresponding upstream tree-sitter grammar repo. The `grammar.json` is copied
from <tt>https://github.com/tree-sitter/tree-sitter-&lt;lang&gt;</tt>. Scanner
C sources are ported to Rust in the same directory.

---

## Staffing estimate

| Phase | Effort | Best person |
|-------|--------|-------------|
| 0. Foundation | 1 person × 3–4 mo | Rust compiler hacker |
| 1. Lexer | 1 person × 2–3 mo | PL/grammar expert |
| 2. Grammar compilation | 1 person × 3–4 mo | PL theory (LR items, FIRST/FOLLOW) |
| 3. Query engine | 1 person × 2–3 mo | Pattern matching enthusiast |
| 4. Migration | 1 person × 3–4 mo (repeat for each grammar) | Diligent generalist |
| **Validation** (horizontal) | **1 person × ongoing** | **Testing/infra engineer** |

The validation role is full-time from Phase 1 onward: writing conformance tests,
running fuzzers, triaging regressions, maintaining the corpus, and operating the
benchmark dashboard. This is not a "QA at the end" position — they participate
in design reviews, write the property tests alongside each feature, and define
the correctness model before implementation starts.

**Total**: ~15–18 person-months for a v1 that supports the top 5 grammars
(JavaScript, Python, Rust, TypeScript, JSON). All 20 grammars → ~24–30
person-months. Plus 1 ongoing FTE for validation across all phases.

---

## Validation strategy (horizontal track)

This is **not a phase** — testing runs throughout the entire project, with
increasing rigor at each milestone. Every phase ship gate requires the test
suite at that tier to pass.

### Correctness model — what does "correct" mean?

Three levels, in order of trust:

1. **Structural parity** — Our `Tree` and tree-sitter's output share the same
   node kind at every byte offset. This is the cheapest and most important
   assertion.
2. **Semantic parity** — For any query, our tree and tree-sitter's tree return
   the same match set over the same corpus.
3. **Stability parity** — The same incremental edits produce the same final
   tree regardless of when they are applied (real-time or batched).

### Tier 1 — Unit & property tests (per-PR)

Runs in CI on every commit. Budget: < 30 s.

```
cargo test
cargo test --features proptest
```

| Suite | Tool | What it checks | When it must pass |
|-------|------|----------------|-------------------|
| GLR engine | `#[test]` | Shift/reduce/accept for hand-crafted grammars (§0.3.1) | Phase 0 |
| Lexer | `#[test]` | Token-by-token parity against tree-sitter library on 20 hand-written cases (§1.2.1) | Phase 1 |
| Tree construction | `#[test]` | Parent links, sibling order, depth invariants | Phase 0 |
| Parse table generation | `#[test]` | Table is deterministic — same grammar `.json` → same table | Phase 2 |
| Query compilation | `#[test]` | Query patterns compile without error | Phase 3 |
| Property: parse identity | `proptest` | `parse(source) = parse(parse(source).to_string())` for any valid AST | Phase 0+ |

**Property-based tests** using `proptest`:

```rust
proptest! {
    // For any expression-like substring of a valid program,
    // parsing it produces a tree rooted at the expected nonterminal.
    fn parses_to_expected_root(src: String) {
        let tree = Parser::new(GRAMMAR).parse(src.as_bytes()).unwrap();
        prop_assert_eq!(tree.root_node().kind(), "expression");
    }

    // No node span exceeds the input length.
    fn spans_within_bounds(src: String) {
        let tree = Parser::new(GRAMMAR).parse(src.as_bytes()).unwrap();
        for node in tree.root_node().walk() {
            prop_assert!(node.end_byte() <= src.len());
        }
    }
}
```

### Tier 2 — Conformance suite (per-release)

A standalone crate that compares our parser against tree-sitter C on a corpus
of real-world source files. Budget: < 5 min.

```rust
// Conformance test runner — pseudo-code
fn test_conformance(language: &str, files: &[PathBuf]) {
    let c_parser = tree_sitter_c::Parser::new();
    c_parser.set_language(&c_language());

    let rs_parser = glr::Parser::new(grammar::load(language));

    for file in files {
        let source = std::fs::read(file).unwrap();

        let c_tree = c_parser.parse(&source, None).unwrap();
        let rs_tree = rs_parser.parse(&source, None).unwrap();

        // Node-by-node comparison
        compare_trees(c_tree.root_node(), rs_tree.root_node(), &source);
    }
}

fn compare_trees(c: Node, rs: Node, source: &[u8]) {
    assert_eq!(c.kind(), rs.kind());
    assert_eq!(c.start_byte(), rs.start_byte());
    assert_eq!(c.end_byte(), rs.end_byte());
    assert_eq!(c.child_count(), rs.child_count());

    let mut c_children = c.children();
    let mut rs_children = rs.children();
    for (c_child, rs_child) in c_children.zip(&mut rs_children) {
        compare_trees(c_child, rs_child, source);
    }
    assert!(c_children.next().is_none());
    assert!(rs_children.next().is_none());
}
```

**Corpus sources:**

| Language | Corpus | Files | Lines |
|----------|--------|-------|-------|
| JavaScript | `mdn/content` (top 200 pages) | 200 | ~50K |
| Python | CPython `Lib/` (first 100 modules) | 100 | ~80K |
| Rust | `rust-analyzer` source | 300 | ~120K |
| TypeScript | `TypeScript/src/compiler/` | 150 | ~200K |
| JSON | `npm` package.json collection | 500 | ~50K |
| HTML | W3C spec samples | 50 | ~30K |
| CSS | Bootstrap + Tailwind source | 20 | ~40K |
| C | `tree-sitter` own source | 50 | ~20K |
| Go | Standard library | 100 | ~200K |
| Ruby | Rails models (10 projects) | 50 | ~10K |

**Regression lockbox**: Any bug found in production is reduced to the smallest
reproduction, added to this suite, and **never allowed to regress**.

### Tier 3 — Fuzz testing (continuous)

Runs 24/7 on a dedicated machine or CI cron. Budget: unlimited.

| Target | Tool | Input | Checks |
|--------|------|-------|--------|
| GLR engine | `cargo fuzz` | Random byte strings from grammar token alphabet | No crash, no OOM, no assertion failure |
| Incremental re-parse | Custom harness | Random edits on real source files | parse(full) = parse_incremental(previous, edit) |
| Lexer | `cargo fuzz` | Arbitrary bytes | No panic, spans are valid |
| Query engine | `cargo fuzz` | Random query strings + random parse trees | No panic, matches are valid |
| Grammar compilation | `cargo fuzz` | Mutated grammar JSON | No crash during table generation |
| Concurrency | `loom` | Thread interleavings on shared tree access | No data races, consistent tree |

**Key fuzz: incremental re-parse identity**. This is the most important test in
the entire project:

```
for _ in 0..N:
    source = random_real_source(language)
    tree_a = parse(source)

    // Apply a random edit
    (start, end, new_text) = random_edit(source)
    source_b = source[..start] + new_text + source[end..]
    tree_b_full = parse(source_b)               // full re-parse (ground truth)
    tree_b_incr = parse_incremental(tree_a, source_b)  // incremental

    assert full_trees_equal(tree_b_full, tree_b_incr)
```

The fuzzer should find the minimal counterexample where `full != incr` —
that's a bug in the incremental logic.

### Tier 4 — Performance benchmarks (per-release)

Criterion benchmarks in a standalone crate, never run in CI (too noisy), gated
on release tagging.

```rust
fn bench_full_parse(c: &mut Criterion) {
    let source = include_str!("corpus/python/large.py");
    c.group("python")
        .bench_function("full_parse", |b| b.iter(|| {
            Parser::new(PYTHON).parse(source.as_bytes())
        }))
        .bench_function("incremental_reparse", |b| b.iter(|| {
            let tree = Parser::new(PYTHON).parse(source.as_bytes()).unwrap();
            let edited = edit_at_line(&source, 42, "    return x + 1\n");
            Parser::new(PYTHON).parse_incremental(&tree, edited.as_bytes())
        }));
}
```

| Metric | Target | Regression threshold |
|--------|--------|---------------------|
| Cold parse, 10K LOC | ≤ tree-sitter C × 1.5 | > 2× → block merge |
| Incremental re-parse, single-line edit | ≤ 100 µs | > 200 µs → block merge |
| Incremental re-parse, 50% file replaced | ≤ cold parse | N/A (monitor only) |
| Query, 20 patterns on 10K LOC | ≤ 5 ms | > 10 ms → flag |
| Peak memory, 10K LOC Python | ≤ 50 MB | > 100 MB → flag |
| Throughput, large JSON (100 MB) | ≥ 100 MB/s | < 50 MB/s → block merge |
| Compile grammar from `.json` | ≤ 200 ms | > 1 s → flag |

All benchmarks run on a reference machine (GitHub Actions `ubuntu-24.04-arm`,
4 vCPU). Results are published to a dashboard.

### Tier 5 — Long-running stability (pre-release)

| Test | Duration | What it validates |
|------|----------|-------------------|
| Memory leak soak | 24 h parse loop on Python stdlib | RSS stable, no growth |
| Editor simulation | 1 h of random edits in a virtual buffer | Incremental re-parse never diverges from full |
| Concurrent read harness | 8 threads querying a shared tree for 1 h | No races, no panics |
| Hanging indent stress | 10K rapid edits in Python file | Lexer indentation stack doesn't drift |

---

## Why this is hard

1. **GLR ambiguity handling** — The merge logic on the graph-structured stack is
   subtle. Tree-sitter gets away with a simplified GLR (fork stacks, limited
   merging) that exploits properties of practical grammars. The Rust version
   must replicate exactly those heuristics or users get spurious parse errors.

2. **Incremental re-parse** — Tree-sitter's incremental algorithm is the result
   of ~5 years of iteration. The edge cases (zero-byte edits, multi-byte
   characters, huge deletions) are numerous.

3. **External scanners** — Python's indentation, Bash's heredocs, Ruby's
   `%w(...)` literals — these are hand-written C that depends on parser
   internals. Porting them to Rust is straightforward but tedious.

4. **Grammar compatibility** — Thousands of existing `.so` parsers and `.scm`
   queries must keep working. Any format change breaks the ecosystem.

5. **Performance** — C's `goto`-based state machine dispatcher is hard to beat.
   Rust's `match` is close, but LLVM inlining thresholds and aliasing analysis
   matter at this level.

---

## Related work — existing Rust parser ecosystem

These projects already exist but **none** cover tree-sitter's niche:

| Project | Type | Stars | Has GLR? | Incremental? | Query engine? | Works with existing grammars? |
|---------|------|-------|----------|-------------|---------------|-------------------------------|
| **lalrpop** | LR(1) proc-macro | 3.5k | LALR(1) opt | No | No | No (own DSL) |
| **pest** | PEG proc-macro | 5.4k | No (PEG) | No | No | No (own `.pest` files) |
| **rust-peg** | PEG proc-macro | 1.6k | No (PEG) | No | No | No (own DSL) |
| **grmtools** | LR/GLR build.rs | 574 | **Yes** | No | No | Yacc `.y` files |
| **tree-sitter** | C GLR library | 25.8k | Yes | **Yes** | **Yes** | Yes (200+ grammars) |

**grmtools / lrpar** is the closest: it has a GLR mode, supports Yacc grammars,
and is pure Rust. But it lacks incremental re-parse, has no query/highlight
system, and doesn't consume tree-sitter grammar JSON. A potential foundation
to build on — or a fork point.

### tree-sitter project's own 1.0 roadmap

tree-sitter's issue [#930](https://github.com/tree-sitter/tree-sitter/issues/930)
lays out their 1.0 goals. Two items are relevant here:

1. **WASM parser loading** — Stretch goal to compile parsers to WASM and load
   them via wasmtime. The parse table stays native, only lexing runs in WASM.
   This is essentially a pragmatic hybrid that the tree-sitter team itself
   considers the right path forward.

2. **CLI ergonomics** — Generate Rust bindings from grammars, structure
   Node.js bindings consistently. No Rust rewrite is planned or discussed.

**Conclusion**: The community (25.8k stars, 115 open issues) has shown zero
interest in a Rust rewrite. The WASM approach is the upstream direction.

---

## Alternatives to a full rewrite

| Approach | Effort | Keeps C? | Rust% of codebase |
|----------|--------|----------|-------------------|
| Full Rust rewrite (from scratch) | 24–30 PM | No | 100% |
| Fork grmtools/lrpar, add incremental + queries | 12–18 PM | No | 100% |
| Vendored C (our current approach) | 1 PM | Yes | ~95% |
| Bindings to tree-sitter .so | 0.5 PM | Yes (runtime) | ~95% |
| WASM-compiled tree-sitter grammars | 2 PM | No (WASM) | ~99% |

The **WASM approach** (upstream's chosen direction): compile tree-sitter C
grammars to `.wasm` with `wasm32-unknown-unknown`, load at runtime via
wasmtime. Parse table stays native (fast), only lexing runs in WASM. No `cc`,
no `build.rs`, no native toolchain. ~10–20% slower from WASM call overhead
on every token.

**Potential hybrid**: use grmtools/lrpar as the GLR engine, implement
incremental re-parse on top (the hard part), and write a tree-sitter JSON →
lrpar Yacc converter. This reuses existing Rust infrastructure and skips
Phase 2 entirely, but still requires the incremental parse algorithm (Phase
1.3) and query engine (Phase 3). Roughly 12–18 PM vs 24–30 for a total
rewrite.

---

## Appendix A: Key tree-sitter source files to study

All files are from the main tree-sitter repository:
<https://github.com/tree-sitter/tree-sitter/tree/master/lib>

| File | Lines | Purpose |
|------|-------|---------|
| `src/parser.c` | ~800 | Core GLR engine |
| `src/language.c` | ~500 | Language loading, serialization |
| `src/lexer.c` | ~600 | Built-in lexer logic |
| `src/node.c` | ~400 | Tree node API |
| `src/query.c` | ~2000 | Query engine |
| `src/alloc.h` | ~50 | Arena allocator |
| `include/tree_sitter/parser.h` | ~200 | API for grammar C code |

## Appendix B: Grammar JSON schema

The `grammar.json` file in each grammar repo follows a well-defined schema.
Before Phase 2, fetch the canonical schema from:

- <https://raw.githubusercontent.com/tree-sitter/tree-sitter/master/cli/src/generate/grammar_schema.json>
- Inspect any grammar repo: <https://github.com/tree-sitter/tree-sitter-javascript/blob/master/grammar.json>

Key fields to understand: `rules`, `precedences`, `conflicts`, `extras`,
`word_token`, `inline`, `supertypes`.

## Appendix C: Glossary

| Term | Definition |
|------|------------|
| **GLR** | Generalized LR — an LR parsing algorithm that handles ambiguous grammars by maintaining multiple parse "forks" |
| **GSS** | Graph-Structured Stack — the data structure GLR uses to share common stack prefixes across forks |
| **Shift** | LR action: consume the current token and push a new state onto the stack |
| **Reduce** | LR action: pop N states (matching the RHS of a production), then push the goto state for the LHS nonterminal |
| **Accept** | LR action: parsing complete |
| **Fork** | A single active parse path in the GLR stack; forks split at shift/reduce conflicts and merge when they converge on the same state |
| **State** | An LR automaton state (from the parse table); identifies a set of possible productions and positions |
| **Symbol** | Either a terminal (token) or nonterminal (rule LHS) |
| **Production** | A grammar rule: `Nonterminal → Symbol₁ Symbol₂ … Symbolₙ` |
| **Parse table** | A table `[state × symbol → action]` that drives the parser |
| **External scanner** | Hand-written code (C in tree-sitter, Rust in our version) that handles lexing tokens that can't be expressed as regex (indentation, heredocs, etc.) |
| **Incremental re-parse** | Re-parsing after an edit by reusing unchanged subtrees from the previous parse, yielding O(edit size) time |
| **ERROR node** | A tree node inserted when the parser encounters invalid syntax, allowing the parse to continue and produce a full tree |
| **Query** | A pattern expression (`.scm` file) that matches nodes in a parse tree, used for syntax highlighting and code analysis |
