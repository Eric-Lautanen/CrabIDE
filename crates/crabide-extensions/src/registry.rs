//! Extension registry client — search, browse, and download extensions.
//!
//! The registry HTTP layer uses `ureq` (sync, lightweight).  All network
//! calls happen on a background Tokio-spawned blocking thread so the UI
//! thread is never blocked.
//!
//! Until a public registry URL is configured, the client returns a curated
//! hardcoded list of recommended extensions.

/// A single extension record from the registry search results.
#[derive(Debug, Clone)]
pub struct RegistryExtension {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub category: String,
    pub downloads: u64,
    /// Star rating out of 5.0.
    pub rating: f32,
    /// Registry download URL (stub — real URL provided by the registry API).
    pub download_url: String,
}

/// Registry HTTP client (sync via ureq, called from a blocking thread).
pub struct RegistryClient {
    /// Base URL for the extension registry API.
    /// `None` until a public registry is stood up.
    base_url: Option<String>,
}

impl RegistryClient {
    pub fn new() -> Self {
        Self { base_url: None }
    }

    /// Search the registry for extensions matching `query`.
    ///
    /// Currently returns results from the curated builtin catalogue filtered by
    /// the query string.  When `base_url` is set this will instead perform an
    /// HTTP GET to `{base_url}/search?q={query}`.
    pub fn search(&self, query: &str) -> Vec<RegistryExtension> {
        let catalogue = Self::catalogue();
        if query.is_empty() {
            return catalogue;
        }
        let q = query.to_lowercase();
        catalogue
            .into_iter()
            .filter(|e| {
                e.name.to_lowercase().contains(&q)
                    || e.description.to_lowercase().contains(&q)
                    || e.category.to_lowercase().contains(&q)
                    || e.author.to_lowercase().contains(&q)
            })
            .collect()
    }

    /// Download a `.wasm` extension binary from the registry (stub).
    ///
    /// Returns the raw bytes on success, or an error string.
    pub fn download(&self, ext: &RegistryExtension) -> Result<Vec<u8>, String> {
        let Some(base) = &self.base_url else {
            return Err("Extension registry not yet available — check back soon!".into());
        };
        let url = format!("{base}/download/{}/{}", ext.id, ext.version);
        // TODO: replace stub with ureq call once registry is live:
        // ureq::get(&url)
        //     .call()
        //     .map_err(|e| e.to_string())?
        //     .body_mut()
        //     .read_to_vec()
        //     .map_err(|e| e.to_string())
        let _ = url;
        Err("Extension registry not yet available — check back soon!".into())
    }

    /// Curated catalogue of popular / recommended extensions.
    fn catalogue() -> Vec<RegistryExtension> {
        vec![
            RegistryExtension {
                id: "rust-analyzer".into(),
                name: "rust-analyzer".into(),
                description: "Rich language support for Rust via rust-analyzer LSP.".into(),
                version: "0.4.2001".into(),
                author: "The rust-analyzer team".into(),
                category: "Languages".into(),
                downloads: 48_200_000,
                rating: 5.0,
                download_url: "".into(),
            },
            RegistryExtension {
                id: "prettier".into(),
                name: "Prettier - Code Formatter".into(),
                description: "Opinionated code formatting for JS/TS, CSS, JSON, Markdown.".into(),
                version: "11.0.0".into(),
                author: "Prettier".into(),
                category: "Formatters".into(),
                downloads: 34_900_000,
                rating: 4.8,
                download_url: "".into(),
            },
            RegistryExtension {
                id: "eslint".into(),
                name: "ESLint".into(),
                description: "Integrates ESLint JavaScript linting into the editor.".into(),
                version: "3.0.10".into(),
                author: "Microsoft".into(),
                category: "Linters".into(),
                downloads: 29_100_000,
                rating: 4.7,
                download_url: "".into(),
            },
            RegistryExtension {
                id: "catppuccin".into(),
                name: "Catppuccin Theme".into(),
                description: "Soothing pastel theme for crabide. Four beautiful flavours.".into(),
                version: "3.15.0".into(),
                author: "Catppuccin Org".into(),
                category: "Themes".into(),
                downloads: 11_300_000,
                rating: 4.9,
                download_url: "".into(),
            },
            RegistryExtension {
                id: "git-graph".into(),
                name: "Git Graph".into(),
                description: "Visualize repository history as an interactive branch graph.".into(),
                version: "1.30.0".into(),
                author: "mhutchie".into(),
                category: "Git".into(),
                downloads: 8_400_000,
                rating: 4.8,
                download_url: "".into(),
            },
            RegistryExtension {
                id: "gitlens".into(),
                name: "GitLens — Git Supercharged".into(),
                description: "Supercharges the Git capabilities built into the editor.".into(),
                version: "16.5.1".into(),
                author: "GitKraken".into(),
                category: "Git".into(),
                downloads: 42_700_000,
                rating: 4.6,
                download_url: "".into(),
            },
            RegistryExtension {
                id: "go".into(),
                name: "Go".into(),
                description: "Full Go language support including debugging and testing.".into(),
                version: "0.43.1".into(),
                author: "Go Team at Google".into(),
                category: "Languages".into(),
                downloads: 18_900_000,
                rating: 4.8,
                download_url: "".into(),
            },
            RegistryExtension {
                id: "python".into(),
                name: "Python".into(),
                description: "Python language support with IntelliSense, debugging, and Jupyter."
                    .into(),
                version: "2024.22.2".into(),
                author: "Microsoft".into(),
                category: "Languages".into(),
                downloads: 122_500_000,
                rating: 4.5,
                download_url: "".into(),
            },
            RegistryExtension {
                id: "dracula".into(),
                name: "Dracula Official".into(),
                description: "Dark theme for crabide — easy on the eyes, great contrast.".into(),
                version: "2.25.1".into(),
                author: "Dracula Theme".into(),
                category: "Themes".into(),
                downloads: 14_700_000,
                rating: 4.9,
                download_url: "".into(),
            },
            RegistryExtension {
                id: "spell-right".into(),
                name: "Spell Right".into(),
                description: "Multilingual, offline spell checker using OS dictionaries.".into(),
                version: "3.0.155".into(),
                author: "Bartosz Antosik".into(),
                category: "Productivity".into(),
                downloads: 2_100_000,
                rating: 4.3,
                download_url: "".into(),
            },
        ]
    }

    /// Recommended extensions shown on the Recommended tab (top picks).
    pub fn recommended(&self) -> Vec<RegistryExtension> {
        let mut all = Self::catalogue();
        all.sort_by_key(|b| std::cmp::Reverse(b.downloads));
        all.into_iter().take(6).collect()
    }
}

impl Default for RegistryClient {
    fn default() -> Self {
        Self::new()
    }
}
