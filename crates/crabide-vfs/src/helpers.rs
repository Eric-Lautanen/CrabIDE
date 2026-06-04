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
