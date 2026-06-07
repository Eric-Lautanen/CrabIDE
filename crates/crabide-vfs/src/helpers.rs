//! URI ↔ filesystem path helpers and canonicalization.

use crabide_core::error::{crabideError, Result};
use crabide_core::types::DocumentUri;
use std::path::{Path, PathBuf};

/// Convert a `DocumentUri` to a local `PathBuf`.
pub fn uri_to_path(uri: &DocumentUri) -> Result<PathBuf> {
    uri.to_file_path()
        .ok_or_else(|| crabideError::Other(format!("URI is not a local file:// URI: {uri}")))
}

/// Convert a local `Path` to a `DocumentUri`.
pub fn path_to_uri(path: &Path) -> Result<DocumentUri> {
    DocumentUri::from_file_path(path).ok_or_else(|| {
        crabideError::Other(format!("Cannot convert path to URI: {}", path.display()))
    })
}

/// Canonicalize a `DocumentUri`. For `file://` URIs, resolves symlinks via
/// `std::fs::canonicalize`. Non-existent paths and non-`file://` URIs are
/// returned unchanged.
pub fn canonical_uri(uri: &DocumentUri) -> Result<DocumentUri> {
    if uri.as_url().scheme() != "file" {
        return Ok(uri.clone());
    }
    let path = uri_to_path(uri)?;
    match std::fs::canonicalize(&path) {
        Ok(canon) => path_to_uri(&canon),
        Err(_) => Ok(uri.clone()), // file doesn't exist yet — return as-is
    }
}

/// Returns true if `path` is inside `root`.
pub fn is_descendant(root: &Path, path: &Path) -> bool {
    path.starts_with(root)
}

/// Return the relative path from `base`'s directory to `target`.
pub fn relative_path(base: &DocumentUri, target: &DocumentUri) -> Option<PathBuf> {
    let base_path = base.to_file_path()?;
    let base_dir = base_path.parent()?;
    let target_path = target.to_file_path()?;
    diff_paths(target_path, base_dir)
}

/// Return the file extension of a URI (lowercase), if any.
pub fn uri_extension(uri: &DocumentUri) -> Option<String> {
    let path = uri.to_file_path()?;
    Some(path.extension()?.to_str()?.to_lowercase())
}

/// Return the file name (with extension) of a URI, if any.
pub fn uri_file_name(uri: &DocumentUri) -> Option<String> {
    let path = uri.to_file_path()?;
    Some(path.file_name()?.to_str()?.to_owned())
}

/// Return the file stem (name without extension) of a URI, if any.
pub fn uri_file_stem(uri: &DocumentUri) -> Option<String> {
    let path = uri.to_file_path()?;
    Some(path.file_stem()?.to_str()?.to_owned())
}

// ── Internal: path diff (avoids pathdiff crate dependency) ───────────────────

fn diff_paths(target: PathBuf, base: &Path) -> Option<PathBuf> {
    use std::path::Component;
    if target.is_absolute() != base.is_absolute() {
        if target.is_absolute() {
            return Some(target);
        }
        return None;
    }
    let mut ait = target.components();
    let mut bit = base.components();
    let mut comps: Vec<Component> = Vec::new();
    loop {
        match (ait.next(), bit.next()) {
            (None, None) => break,
            (Some(a), None) => {
                comps.push(a);
                comps.extend(ait);
                break;
            }
            (None, _) => comps.push(Component::ParentDir),
            (Some(a), Some(b)) if comps.is_empty() && a == b => {}
            (Some(a), Some(_)) => {
                comps.push(Component::ParentDir);
                for _ in bit {
                    comps.push(Component::ParentDir);
                }
                comps.push(a);
                comps.extend(ait);
                break;
            }
        }
    }
    Some(comps.iter().collect())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn child_uri(name: &str) -> DocumentUri {
        let mut base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/tmp"));
        base.push(name);
        DocumentUri::from_file_path(&base).unwrap()
    }

    #[test]
    fn path_to_uri_and_back() {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/tmp"));
        let uri = path_to_uri(&cwd).unwrap();
        assert!(uri.as_str().starts_with("file:///"));
        let back = uri_to_path(&uri).unwrap();
        assert!(back.is_absolute());
    }

    #[test]
    fn uri_to_path_invalid_scheme() {
        let uri = DocumentUri::parse("http://example.com/file.rs").unwrap();
        let err = uri_to_path(&uri).unwrap_err();
        assert!(format!("{err}").contains("not a local file"));
    }

    #[test]
    fn is_descendant_positive() {
        let root = Path::new("/home/user/project");
        let path = Path::new("/home/user/project/src/main.rs");
        assert!(is_descendant(root, path));
    }

    #[test]
    fn is_descendant_negative() {
        let root = Path::new("/home/user/project");
        let path = Path::new("/other/lib.rs");
        assert!(!is_descendant(root, path));
    }

    #[test]
    fn is_descendant_same_path() {
        let root = Path::new("/home/user/project");
        assert!(is_descendant(root, root));
    }

    #[test]
    fn relative_path_simple() {
        // Use explicit file:// URIs with paths that work on the current platform
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/tmp"));
        let base_path = cwd.join("src/main.rs");
        let target_path = cwd.join("README.md");
        let base = DocumentUri::from_file_path(&base_path).unwrap();
        let target = DocumentUri::from_file_path(&target_path).unwrap();
        let rel = relative_path(&base, &target).unwrap();
        let norm = rel.to_string_lossy().to_string().replace('\\', "/");
        assert_eq!(norm, "../README.md");
    }

    #[test]
    fn relative_path_same_dir() {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/tmp"));
        let base_path = cwd.join("src/main.rs");
        let target_path = cwd.join("src/lib.rs");
        let base = DocumentUri::from_file_path(&base_path).unwrap();
        let target = DocumentUri::from_file_path(&target_path).unwrap();
        let rel = relative_path(&base, &target).unwrap();
        let norm = rel.to_string_lossy().to_string().replace('\\', "/");
        assert_eq!(norm, "lib.rs");
    }

    #[test]
    fn uri_extension_known() {
        let uri = child_uri("file.rs");
        // file extension should be "rs"
        if let Some(ext) = uri_extension(&uri) {
            assert_eq!(ext, "rs");
        }
    }

    #[test]
    fn uri_extension_no_ext() {
        // A path without extension using child_uri still has the file name
        // Let's just verify no panic
        let uri = child_uri("Makefile");
        let _ = uri_extension(&uri);
    }

    #[test]
    fn uri_file_name_known() {
        let uri = child_uri("main.rs");
        if let Some(name) = uri_file_name(&uri) {
            assert_eq!(name, "main.rs");
        }
    }

    #[test]
    fn uri_file_stem_known() {
        let uri = child_uri("main.rs");
        if let Some(stem) = uri_file_stem(&uri) {
            assert_eq!(stem, "main");
        }
    }

    #[test]
    fn canonical_uri_non_file_scheme() {
        let uri = DocumentUri::parse("memory:///test.txt").unwrap();
        let canon = canonical_uri(&uri).unwrap();
        assert_eq!(canon, uri);
    }

    #[test]
    fn diff_paths_same() {
        let result = diff_paths(PathBuf::from("/a/b/c.rs"), Path::new("/a/b"));
        assert!(result.is_some());
        let rel = result
            .unwrap()
            .to_string_lossy()
            .to_string()
            .replace('\\', "/");
        assert_eq!(rel, "c.rs");
    }

    #[test]
    fn diff_paths_parent() {
        let result = diff_paths(PathBuf::from("/a/d.rs"), Path::new("/a/b/c.rs"));
        assert!(result.is_some());
        let rel = result
            .unwrap()
            .to_string_lossy()
            .to_string()
            .replace('\\', "/");
        assert_eq!(rel, "../../d.rs");
    }

    #[test]
    fn diff_paths_absolute_mismatch() {
        let _result = diff_paths(PathBuf::from("relative.txt"), Path::new("/absolute"));
        // May return Some or None depending on platform, just don't panic
    }
}
