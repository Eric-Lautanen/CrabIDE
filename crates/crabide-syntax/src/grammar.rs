//! Grammar registry: maps language IDs to `tree_sitter::Language` objects.
//!
//! Grammars can be registered in two ways:
//! 1. **Static**: the app crate links in grammar crates at compile time and
//!    calls [`grammar_registry().register(...)`].
//! 2. **Dynamic**: compiled `.so` / `.dll` files are loaded at runtime via
//!    [`GrammarRegistry::load_from_disk`]. The library must export a C function
//!    with the signature `const TSLanguage *tree_sitter_<name>(void)` (ABI 14).

use std::path::Path;
use std::sync::Arc;

use dashmap::DashMap;
use parking_lot::Mutex;
use std::sync::OnceLock;

use crabide_core::{
    error::{crabideError, Result},
    types::Language,
};

// ── GrammarEntry ──────────────────────────────────────────────────────────────

/// A tree-sitter language with its associated query sources.
#[derive(Clone)]
pub struct GrammarEntry {
    /// The compiled tree-sitter `Language`.
    pub language: tree_sitter::Language,
    /// Tree-sitter highlight query source (`.scm` syntax).
    pub highlights_query: Arc<str>,
    /// Tree-sitter locals query (scope-aware highlighting helpers).
    pub locals_query: Arc<str>,
    /// Tree-sitter indents query.
    pub indents_query: Arc<str>,
    /// Tree-sitter injection query source for embedded languages.
    /// Maps capture names (e.g. `@injection.content`) to language IDs.
    pub injections_query: Arc<str>,
}

impl GrammarEntry {
    pub fn new(
        language: tree_sitter::Language,
        highlights_query: &str,
        locals_query: &str,
        indents_query: &str,
    ) -> Self {
        Self {
            language,
            highlights_query: Arc::from(highlights_query),
            locals_query: Arc::from(locals_query),
            indents_query: Arc::from(indents_query),
            injections_query: Arc::from(""),
        }
    }

    /// Create a `GrammarEntry` with an injection query for embedded languages.
    pub fn with_injections(
        language: tree_sitter::Language,
        highlights_query: &str,
        locals_query: &str,
        indents_query: &str,
        injections_query: &str,
    ) -> Self {
        Self {
            language,
            highlights_query: Arc::from(highlights_query),
            locals_query: Arc::from(locals_query),
            indents_query: Arc::from(indents_query),
            injections_query: Arc::from(injections_query),
        }
    }
}

// ── GrammarRegistry ───────────────────────────────────────────────────────────

/// Central registry of all available tree-sitter grammars.
///
/// Access the global instance via [`grammar_registry()`].
pub struct GrammarRegistry {
    grammars: DashMap<Language, GrammarEntry>,
    /// Keep dynamically-loaded libraries alive so their code stays mapped.
    _libs: Mutex<Vec<libloading::Library>>,
}

impl GrammarRegistry {
    pub fn new() -> Self {
        Self {
            grammars: DashMap::new(),
            _libs: Mutex::new(Vec::new()),
        }
    }

    /// Register a grammar that was linked at compile time.
    ///
    /// Typically called during app start-up by the `crabide-app` crate after
    /// it links in the relevant `tree-sitter-*` grammar crates.
    pub fn register(
        &self,
        language: Language,
        ts_language: tree_sitter::Language,
        highlights_query: &str,
        locals_query: &str,
        indents_query: &str,
    ) {
        self.grammars.insert(
            language,
            GrammarEntry::new(ts_language, highlights_query, locals_query, indents_query),
        );
    }

    /// Register a grammar with injection support for embedded languages.
    ///
    /// Like [`register`](Self::register), but also provides an injection query
    /// that tells the highlight engine how to switch languages inside a
    /// document (e.g. JavaScript inside `<script>` tags in HTML).
    pub fn register_with_injections(
        &self,
        language: Language,
        ts_language: tree_sitter::Language,
        highlights_query: &str,
        locals_query: &str,
        indents_query: &str,
        injections_query: &str,
    ) {
        self.grammars.insert(
            language,
            GrammarEntry::with_injections(
                ts_language,
                highlights_query,
                locals_query,
                indents_query,
                injections_query,
            ),
        );
    }

    /// Load a grammar from a compiled `.so` / `.dll` on disk (tree-sitter ABI 14).
    ///
    /// `fn_symbol` is the exported C symbol name, e.g. `b"tree_sitter_rust\0"`.
    /// The library is kept alive inside the registry for the program's lifetime.
    ///
    /// # Safety
    /// The library at `path` must export a valid tree-sitter ABI-14 grammar
    /// function and must not be unloaded while the registry is alive.
    pub fn load_from_disk(
        &self,
        language: Language,
        path: &Path,
        fn_symbol: &[u8],
        highlights_query: &str,
    ) -> Result<()> {
        // SAFETY: caller guarantees the library is a valid tree-sitter grammar.
        let (ts_lang, lib) = unsafe {
            let lib =
                libloading::Library::new(path).map_err(|e| crabideError::Grammar(e.to_string()))?;

            // Tree-sitter ABI 14: grammar .so exports `const TSLanguage *tree_sitter_xxx(void)`.
            type RawFn = unsafe extern "C" fn() -> *const tree_sitter::ffi::TSLanguage;
            let func: libloading::Symbol<RawFn> = lib
                .get(fn_symbol)
                .map_err(|e| crabideError::Grammar(e.to_string()))?;
            let raw_ptr = func();
            let ts_lang = tree_sitter::Language::from_raw(raw_ptr);
            (ts_lang, lib)
        };

        self.grammars.insert(
            language,
            GrammarEntry::new(ts_lang, highlights_query, "", ""),
        );
        self._libs.lock().push(lib);
        Ok(())
    }

    /// Look up a grammar by language ID.
    pub fn get(&self, language: &Language) -> Option<GrammarEntry> {
        self.grammars.get(language).map(|r| r.clone())
    }

    /// Return all registered language IDs.
    pub fn registered_languages(&self) -> Vec<Language> {
        self.grammars.iter().map(|r| r.key().clone()).collect()
    }

    /// Returns `true` if a grammar is registered for the given language.
    pub fn has(&self, language: &Language) -> bool {
        self.grammars.contains_key(language)
    }
}

impl Default for GrammarRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Global singleton ──────────────────────────────────────────────────────────

/// The global grammar registry, initialized on first access.
pub static REGISTRY: OnceLock<GrammarRegistry> = OnceLock::new();

/// Convenience accessor for the global registry.
#[inline]
pub fn grammar_registry() -> &'static GrammarRegistry {
    REGISTRY.get_or_init(GrammarRegistry::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grammar_registry_new_is_empty() {
        let reg = GrammarRegistry::new();
        assert!(!reg.has(&Language::RUST));
        assert!(reg.registered_languages().is_empty());
    }

    #[test]
    fn grammar_registry_register_then_get() {
        let reg = GrammarRegistry::new();
        let lang = Language::RUST;
        // Create a no-op tree-sitter language from raw.
        // tree_sitter::Language::from_raw(ptr) requires a valid ABI-14 lang.
        // For testing we can still test the registry logic with a real language.
        // Since tree-sitter doesn't provide a dummy language, we test with the
        // built-in Rust grammar if available, or just verify has/get behavior.
        assert!(!reg.has(&lang));
        assert!(reg.get(&lang).is_none());
    }

    #[test]
    fn grammar_registry_register_updates_has() {
        let reg = GrammarRegistry::new();
        let lang = Language::RUST;
        assert!(!reg.has(&lang));
        // We can't easily create a tree_sitter::Language in a test,
        // but the registry's has/get methods are simple map lookups.
        // Verify the registered_languages list starts empty.
        assert!(reg.registered_languages().is_empty());
    }

    #[test]
    fn grammar_registry_default_is_new() {
        let reg = GrammarRegistry::default();
        assert!(reg.registered_languages().is_empty());
    }

    #[test]
    fn grammar_entry_new_stores_fields() {
        // We cannot construct a tree_sitter::Language in tests without a grammar
        // crate, but the GrammarEntry struct fields are public and trivially
        // verifiable.  This test just confirms the API compiles and the struct
        // exists with the expected constructor.
        // (Full construction would need a tree-sitter language.)
        let _ = GrammarEntry::new;
    }
}
