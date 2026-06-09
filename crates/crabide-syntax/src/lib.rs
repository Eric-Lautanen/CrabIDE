#![warn(clippy::pedantic)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::doc_markdown,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::struct_excessive_bools,
    clippy::similar_names,
    clippy::assigning_clones,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    clippy::collapsible_else_if,
    clippy::default_trait_access,
    clippy::explicit_iter_loop,
    clippy::float_cmp,
    clippy::fn_params_excessive_bools,
    clippy::format_collect,
    clippy::format_push_string,
    clippy::if_not_else,
    clippy::items_after_statements,
    clippy::manual_let_else,
    clippy::many_single_char_names,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::match_wildcard_for_single_variants,
    clippy::needless_continue,
    clippy::needless_pass_by_value,
    clippy::redundant_closure,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_else,
    clippy::return_self_not_must_use,
    clippy::semicolon_if_nothing_returned,
    clippy::too_many_lines,
    clippy::trivially_copy_pass_by_ref,
    clippy::uninlined_format_args,
    clippy::unnecessary_debug_formatting,
    clippy::unnecessary_map_or,
    clippy::unnecessary_wraps,
    clippy::unused_self,
    clippy::used_underscore_binding,
    clippy::wildcard_imports
)]
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
pub mod indent;
pub mod locals;
pub mod outline;
pub mod queries;

// ── Convenient re-exports ─────────────────────────────────────────────────────

pub use engine::SyntaxEngine;
pub use fold::{FoldKind, FoldingRange};
pub use grammar::{GrammarEntry, GrammarRegistry, REGISTRY, grammar_registry};
pub use highlight::{HighlightEngine, HighlightSpan, scope_to_vscode};
pub use indent::{IndentEngine, LineIndent};
pub use locals::{LocalScopeInfo, LocalsEngine, ResolvedScope};
pub use outline::{SymbolKind, SymbolOutline};
pub use queries::highlights_query_for;

pub use crabide_core::error::{Result, crabideError};
pub use crabide_core::types::{BufferId, Language};
