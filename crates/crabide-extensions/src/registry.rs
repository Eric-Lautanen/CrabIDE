//! Extension registry client — search, browse, and download extensions.
//!
//! The registry HTTP layer uses `ureq` (sync, lightweight).  All network
//! calls happen on a background Tokio-spawned blocking thread so the UI
//! thread is never blocked.
//!
//! Until a public registry URL is configured, the client returns a curated
//! hardcoded list of recommended extensions.

use sha2::Digest;

/// A single extension record from the registry search results.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
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
    /// Registry download URL for the `.wasm` binary.
    pub download_url: String,
    /// Expected SHA-256 hex digest of the `.wasm` binary.
    /// Populated from registry metadata; used for checksum verification after
    /// download.  An empty string means skip verification.
    pub sha256_hex: String,
}

/// Result of a successful extension download.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DownloadResult {
    /// Raw `.wasm` binary bytes.
    pub bytes: Vec<u8>,
    /// SHA-256 hex digest that was verified (empty if verification was skipped).
    pub sha256_hex: String,
}

/// Registry HTTP client (sync via ureq, called from a blocking thread).
#[derive(Default)]
pub struct RegistryClient {
    /// Base URL for the extension registry API.
    /// `None` until a public registry is stood up.
    base_url: Option<String>,
}

impl RegistryClient {
    pub fn new() -> Self {
        Self { base_url: None }
    }

    /// Set the registry base URL.  Once set, `search()` and `download()` will
    /// make actual HTTP requests instead of returning stub data.
    pub fn set_base_url(&mut self, url: String) {
        self.base_url = Some(url);
    }

    /// Search the registry for extensions matching `query`.
    ///
    /// When `base_url` is set this performs an HTTP GET to
    /// `{base_url}/api/search?q={query}` and expects a JSON array of
    /// [`RegistryExtension`] objects.  Otherwise returns results from the
    /// curated builtin catalogue filtered by the query string.
    pub fn search(&self, query: &str) -> Vec<RegistryExtension> {
        let Some(base) = &self.base_url else {
            let catalogue = Self::catalogue();
            if query.is_empty() {
                return catalogue;
            }
            let q = query.to_lowercase();
            return catalogue
                .into_iter()
                .filter(|e| {
                    e.name.to_lowercase().contains(&q)
                        || e.description.to_lowercase().contains(&q)
                        || e.category.to_lowercase().contains(&q)
                        || e.author.to_lowercase().contains(&q)
                })
                .collect();
        };

        let url = format!("{base}/api/search?q={}", urlencoding(query));
        match ureq::get(&url).call() {
            Ok(resp) => {
                let body = match resp.into_body().read_to_vec() {
                    Ok(b) => b,
                    Err(e) => {
                        log::warn!("Registry search: failed to read response body: {e}");
                        return Vec::new();
                    }
                };
                match serde_json::from_slice::<Vec<RegistryExtension>>(&body) {
                    Ok(exts) => exts,
                    Err(e) => {
                        log::warn!("Registry search: invalid JSON: {e}");
                        Vec::new()
                    }
                }
            }
            Err(e) => {
                log::warn!("Registry search request failed: {e}");
                Vec::new()
            }
        }
    }

    /// Download a `.wasm` extension binary from the registry with checksum
    /// verification.
    ///
    /// 1. Fetches the binary from `ext.download_url` (or constructs the URL
    ///    from the registry base).
    /// 2. If `ext.sha256_hex` is non-empty, computes the SHA-256 digest of the
    ///    downloaded bytes and verifies it matches.
    /// 3. Returns [`DownloadResult`] with the verified bytes.
    pub fn download(&self, ext: &RegistryExtension) -> Result<DownloadResult, String> {
        let url = if ext.download_url.is_empty() {
            let Some(base) = &self.base_url else {
                return Err("Extension registry not yet available — check back soon!".into());
            };
            format!("{base}/api/download/{}/{}", ext.id, ext.version)
        } else {
            ext.download_url.clone()
        };

        log::info!("Downloading extension from {url}");

        let resp = ureq::get(&url)
            .call()
            .map_err(|e| format!("Failed to download extension: {e}"))?;

        let bytes = resp
            .into_body()
            .read_to_vec()
            .map_err(|e| format!("Failed to read response body: {e}"))?;

        // Verify SHA-256 checksum if provided.
        if !ext.sha256_hex.is_empty() {
            let actual_hex = hex_encode(sha2::Sha256::digest(&bytes));
            if !constant_time_eq(&actual_hex, &ext.sha256_hex) {
                return Err(format!(
                    "Checksum mismatch for '{}': expected {}, got {}",
                    ext.id, ext.sha256_hex, actual_hex
                ));
            }
            log::info!("Checksum verified for '{}' ({})", ext.id, ext.sha256_hex);
        } else {
            log::warn!(
                "No SHA-256 checksum available for '{}' — skipping verification",
                ext.id
            );
        }

        Ok(DownloadResult {
            bytes,
            sha256_hex: ext.sha256_hex.clone(),
        })
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
                sha256_hex: "".into(),
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
                sha256_hex: "".into(),
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
                sha256_hex: "".into(),
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
                sha256_hex: "".into(),
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
                sha256_hex: "".into(),
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
                sha256_hex: "".into(),
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
                sha256_hex: "".into(),
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
                sha256_hex: "".into(),
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
                sha256_hex: "".into(),
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
                sha256_hex: "".into(),
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

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Minimal URL-encoding for search query parameters.
fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push_str("%20"),
            _ => {
                out.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    out
}

/// Hex-encode a byte slice to lowercase hex string.
fn hex_encode(bytes: impl AsRef<[u8]>) -> String {
    let bytes = bytes.as_ref();
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Constant-time equality comparison to avoid timing side-channels.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result: u8 = 0;
    for (ca, cb) in a.bytes().zip(b.bytes()) {
        result |= ca ^ cb;
    }
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_client_search_empty() {
        let client = RegistryClient::new();
        let results = client.search("");
        assert!(!results.is_empty(), "catalogue should have entries");
    }

    #[test]
    fn registry_client_search_query() {
        let client = RegistryClient::new();
        let results = client.search("rust");
        assert!(!results.is_empty());
        assert!(results.iter().any(|e| e.id.contains("rust")));
    }

    #[test]
    fn registry_client_search_no_match() {
        let client = RegistryClient::new();
        let results = client.search("xyznonexistent12345");
        assert!(results.is_empty());
    }

    #[test]
    fn registry_client_recommended() {
        let client = RegistryClient::new();
        let rec = client.recommended();
        assert_eq!(rec.len(), 6);
        for i in 1..rec.len() {
            assert!(rec[i - 1].downloads >= rec[i].downloads);
        }
    }

    #[test]
    fn registry_client_download_fails_without_base_url() {
        let client = RegistryClient::new();
        let ext = client.search("rust").into_iter().next().unwrap();
        let result = client.download(&ext);
        assert!(result.is_err());
    }

    #[test]
    fn registry_extension_sha256_field() {
        let ext = RegistryExtension {
            id: "test".into(),
            name: "Test".into(),
            description: "".into(),
            version: "1.0.0".into(),
            author: "".into(),
            category: "Other".into(),
            downloads: 0,
            rating: 0.0,
            download_url: "".into(),
            sha256_hex: "abcdef1234567890".into(),
        };
        assert_eq!(ext.sha256_hex, "abcdef1234567890");
    }

    #[test]
    fn download_result_construction() {
        let result = DownloadResult {
            bytes: vec![0, 1, 2, 3],
            sha256_hex: "deadbeef".into(),
        };
        assert_eq!(result.bytes.len(), 4);
        assert_eq!(result.sha256_hex, "deadbeef");
    }

    #[test]
    fn urlencoding_basic() {
        assert_eq!(urlencoding("hello"), "hello");
        assert_eq!(urlencoding("rust analyzer"), "rust%20analyzer");
        assert_eq!(urlencoding("a&b=c"), "a%26b%3Dc");
    }

    #[test]
    fn hex_encode_basic() {
        assert_eq!(hex_encode([0x00, 0xFF, 0xab]), "00ffab");
        assert_eq!(hex_encode([]), "");
    }

    #[test]
    fn constant_time_eq_matches() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "abcd"));
        assert!(constant_time_eq("", ""));
    }

    #[test]
    fn checksum_verify_success() {
        let data = b"hello world";
        let expected = hex_encode(sha2::Sha256::digest(data));
        assert!(constant_time_eq(&expected, &expected));
    }

    #[test]
    fn checksum_verify_failure() {
        let data = b"hello world";
        let expected = hex_encode(sha2::Sha256::digest(data));
        // tamper a byte
        let tampered = b"hello worlD";
        let actual = hex_encode(sha2::Sha256::digest(tampered));
        assert_ne!(expected, actual);
    }
}
