//! In-memory VFS implementation for testing.

use async_trait::async_trait;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crabide_core::error::{crabideError, Result};
use crabide_core::traits::{DirEntry, DirEntryKind, VirtualFileSystem};
use crabide_core::types::DocumentUri;

use crate::helpers::{path_to_uri, uri_to_path};

type FileMap = HashMap<PathBuf, Vec<u8>>;

/// An in-memory filesystem for testing. All operations happen in memory.
#[derive(Debug, Clone, Default)]
pub struct MemoryVfs {
    files: Arc<RwLock<FileMap>>,
}

impl MemoryVfs {
    pub fn new() -> Self {
        Self {
            files: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Insert a file directly into the in-memory store.
    pub fn insert(&self, path: PathBuf, contents: Vec<u8>) {
        self.files.write().insert(path, contents);
    }
}

#[async_trait]
impl VirtualFileSystem for MemoryVfs {
    async fn read_file(&self, uri: &DocumentUri) -> Result<Vec<u8>> {
        let path = uri_to_path(uri)?;
        self.files
            .read()
            .get(&path)
            .cloned()
            .ok_or_else(|| crabideError::DocumentNotFound {
                uri: uri.to_string(),
            })
    }

    async fn write_file(&self, uri: &DocumentUri, contents: &[u8]) -> Result<()> {
        let path = uri_to_path(uri)?;
        self.files.write().insert(path, contents.to_vec());
        Ok(())
    }

    async fn delete(&self, uri: &DocumentUri, _recursive: bool) -> Result<()> {
        let path = uri_to_path(uri)?;
        self.files
            .write()
            .remove(&path)
            .map(|_| ())
            .ok_or_else(|| crabideError::DocumentNotFound {
                uri: uri.to_string(),
            })
    }

    async fn rename(&self, from: &DocumentUri, to: &DocumentUri) -> Result<()> {
        let from_path = uri_to_path(from)?;
        let to_path = uri_to_path(to)?;
        let mut files = self.files.write();
        let data = files
            .remove(&from_path)
            .ok_or_else(|| crabideError::DocumentNotFound {
                uri: from.to_string(),
            })?;
        files.insert(to_path, data);
        Ok(())
    }

    async fn create_dir(&self, _uri: &DocumentUri) -> Result<()> {
        // No-op in memory VFS; directories are implicit.
        Ok(())
    }

    async fn read_dir(&self, uri: &DocumentUri) -> Result<Vec<DirEntry>> {
        let path = uri_to_path(uri)?;
        let files = self.files.read();
        let mut entries: Vec<DirEntry> = Vec::new();
        let prefix = path.to_string_lossy().to_string();
        let prefix = if prefix.ends_with('/') || prefix.ends_with('\\') {
            prefix
        } else {
            format!("{prefix}/")
        };

        let mut seen_dirs = std::collections::HashSet::new();
        for (file_path, data) in files.iter() {
            let file_str = file_path.to_string_lossy();
            if !file_str.starts_with(&prefix) {
                continue;
            }
            let relative = &file_str[prefix.len()..];
            let first_component = relative.split(&['/', '\\']).next().unwrap_or("");
            if first_component.is_empty() {
                continue;
            }
            if relative.contains('/') || relative.contains('\\') {
                // This file is in a subdirectory; report the subdirectory.
                if seen_dirs.insert(first_component.to_owned()) {
                    let dir_path = PathBuf::from(format!("{prefix}{first_component}"));
                    entries.push(DirEntry {
                        uri: path_to_uri(&dir_path)?,
                        name: first_component.to_owned(),
                        kind: DirEntryKind::Directory,
                        size: None,
                    });
                }
            } else {
                entries.push(DirEntry {
                    uri: path_to_uri(file_path)?,
                    name: first_component.to_owned(),
                    kind: DirEntryKind::File,
                    size: Some(data.len() as u64),
                });
            }
        }

        entries.sort_unstable_by(|a, b| {
            let a_dir = u8::from(a.kind == DirEntryKind::Directory);
            let b_dir = u8::from(b.kind == DirEntryKind::Directory);
            b_dir
                .cmp(&a_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        Ok(entries)
    }

    async fn exists(&self, uri: &DocumentUri) -> Result<bool> {
        let path = uri_to_path(uri)?;
        Ok(self.files.read().contains_key(&path))
    }

    fn canonical_uri(&self, uri: &DocumentUri) -> Result<DocumentUri> {
        Ok(uri.clone())
    }
}
