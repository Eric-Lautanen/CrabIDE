//! `crabide-syntax` — tree-sitter integration: parsing, highlighting, folding, symbols.
//!
//! # Overview
//!
//! This crate wraps tree-sitter to provide:
//! - **Grammar registry** ([`grammar`]) — register or dynamically load compiled grammars.
//! - **Embedded queries** ([`queries`]) — built-in highlight queries for 10 languages.
//! - **Highlight spans** ([`highlight`]) — `Vec<HighlightSpan>` from a parsed tree.
//! - **Folding ranges** ([`fold`]) — collapsible regions from the AST.
//! - **Symbol outline** ([`outline`]) — named symbols for breadcrumbs / go-to-symbol.
//! - **Syntax engine** ([`engine`]) — the main service that ties everything together.
//!
//! # Quick start
//!
//! ```no_run
//! use crabide_syntax::{SyntaxEngine, grammar::{grammar_registry, GrammarEntry}};
//! use crabide_core::types::{BufferId, Language};
//!
//! // 1. Register a grammar (app crate links in tree-sitter-rust, for example):
//! //    grammar_registry().register(Language::RUST, ts_lang, HIGHLIGHTS, "", "");
//!
//! // 2. Create the engine.
//! let engine = SyntaxEngine::new();
//!
//! // 3. Parse a document.
//! let id = BufferId::new();
//! engine.parse_document(id, &Language::RUST, "fn main() {}", 1);
//!
//! // 4. Query results.
//! let spans  = engine.highlights(id);
//! let folds  = engine.folding_ranges(id);
//! let outline = engine.outline(id);
//! ```

pub mod engine;
pub mod fold;
pub mod grammar;
pub mod highlight;
pub mod outline;
pub mod queries;

// ── Convenient re-exports ─────────────────────────────────────────────────────

pub use engine::SyntaxEngine;
pub use fold::{FoldKind, FoldingRange};
pub use grammar::{grammar_registry, GrammarEntry, GrammarRegistry, REGISTRY};
pub use highlight::{scope_to_vscode, HighlightEngine, HighlightSpan};
pub use outline::{SymbolKind, SymbolOutline};
pub use queries::highlights_query_for;

pub use crabide_core::error::{crabideError, Result};
pub use crabide_core::types::{BufferId, Language};
