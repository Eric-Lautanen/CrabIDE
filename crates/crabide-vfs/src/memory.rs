//! In-memory VFS implementation for testing.

use async_trait::async_trait;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crabide_core::error::{Result, crabideError};
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

        let mut seen_dirs = std::collections::HashSet::new();
        for (file_path, data) in files.iter() {
            // Use Path::strip_prefix for platform-correct prefix matching
            let Ok(relative) = file_path.strip_prefix(&path) else {
                continue;
            };
            // Skip the directory itself
            if relative.as_os_str().is_empty() {
                continue;
            }
            let components: Vec<_> = relative.components().collect();
            let first_component = components[0].as_os_str().to_string_lossy().into_owned();
            if first_component.is_empty() {
                continue;
            }
            if components.len() > 1 {
                // This file is in a subdirectory; report the subdirectory.
                if seen_dirs.insert(first_component.clone()) {
                    let dir_path = path.join(&first_component);
                    entries.push(DirEntry {
                        uri: path_to_uri(&dir_path)?,
                        name: first_component,
                        kind: DirEntryKind::Directory,
                        size: None,
                    });
                }
            } else {
                entries.push(DirEntry {
                    uri: path_to_uri(file_path)?,
                    name: first_component,
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crabide_core::traits::VirtualFileSystem;

    /// Helper to build a file:// URI from a relative path under the current directory.
    /// This ensures the path is valid on any platform (Windows drive letters, Unix absolute).
    fn child_uri(name: &str) -> DocumentUri {
        let mut base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/tmp"));
        base.push(name);
        DocumentUri::from_file_path(&base).unwrap()
    }

    /// Return a directory URI under the current directory.
    fn dir_uri(subdir: &str) -> DocumentUri {
        let mut base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/tmp"));
        base.push(subdir);
        // Ensure trailing separator for directory detection
        DocumentUri::from_file_path(&base).unwrap()
    }

    #[tokio::test]
    async fn memory_vfs_read_write_roundtrip() {
        let vfs = MemoryVfs::new();
        let uri = child_uri("test_hello.txt");
        vfs.write_file(&uri, b"Hello, World!").await.unwrap();
        let contents = vfs.read_file(&uri).await.unwrap();
        assert_eq!(contents, b"Hello, World!");
    }

    #[tokio::test]
    async fn memory_vfs_read_nonexistent() {
        let vfs = MemoryVfs::new();
        let uri = child_uri("nonexistent_file_xyz");
        let err = vfs.read_file(&uri).await.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("not found") || msg.contains("No such file"),
            "got: {msg}"
        );
    }

    #[tokio::test]
    async fn memory_vfs_delete() {
        let vfs = MemoryVfs::new();
        let uri = child_uri("delete_me_test");
        vfs.write_file(&uri, b"to be deleted").await.unwrap();
        assert!(vfs.exists(&uri).await.unwrap());
        vfs.delete(&uri, false).await.unwrap();
        assert!(!vfs.exists(&uri).await.unwrap());
    }

    #[tokio::test]
    async fn memory_vfs_delete_nonexistent() {
        let vfs = MemoryVfs::new();
        let uri = child_uri("nonexistent_del");
        let err = vfs.delete(&uri, false).await.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("not found"), "got: {msg}");
    }

    #[tokio::test]
    async fn memory_vfs_rename() {
        let vfs = MemoryVfs::new();
        let from = child_uri("old_name_test");
        let to = child_uri("new_name_test");
        vfs.write_file(&from, b"content").await.unwrap();
        vfs.rename(&from, &to).await.unwrap();
        assert!(!vfs.exists(&from).await.unwrap());
        assert!(vfs.exists(&to).await.unwrap());
        assert_eq!(vfs.read_file(&to).await.unwrap(), b"content");
    }

    #[tokio::test]
    async fn memory_vfs_rename_nonexistent() {
        let vfs = MemoryVfs::new();
        let from = child_uri("nonexistent_rename_src");
        let to = child_uri("dest");
        let err = vfs.rename(&from, &to).await.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("not found"), "got: {msg}");
    }

    #[tokio::test]
    async fn memory_vfs_create_dir_noop() {
        let vfs = MemoryVfs::new();
        let uri = dir_uri("some_dir_test");
        vfs.create_dir(&uri).await.unwrap();
        // create_dir is a no-op for memory VFS
    }

    #[tokio::test]
    async fn memory_vfs_exists() {
        let vfs = MemoryVfs::new();
        let uri = child_uri("exists_test");
        assert!(!vfs.exists(&uri).await.unwrap());
        vfs.write_file(&uri, b"yes").await.unwrap();
        assert!(vfs.exists(&uri).await.unwrap());
    }

    #[tokio::test]
    async fn memory_vfs_read_dir() {
        let vfs = MemoryVfs::new();
        let root_uri = dir_uri("read_dir_test_root");
        let file1 = child_uri("read_dir_test_root/main.rs");
        let file2 = child_uri("read_dir_test_root/lib.rs");
        vfs.write_file(&file1, b"fn main() {}").await.unwrap();
        vfs.write_file(&file2, b"pub fn helper() {}").await.unwrap();

        let entries = vfs.read_dir(&root_uri).await.unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|e| e.name == "main.rs"));
        assert!(entries.iter().any(|e| e.name == "lib.rs"));
    }

    #[tokio::test]
    async fn memory_vfs_read_dir_with_subdir() {
        let vfs = MemoryVfs::new();
        let root_uri = dir_uri("project_subdir_test");
        let src_file = child_uri("project_subdir_test/src/main.rs");
        let readme = child_uri("project_subdir_test/README.md");
        vfs.write_file(&src_file, b"fn main() {}").await.unwrap();
        vfs.write_file(&readme, b"# Project").await.unwrap();

        let entries = vfs.read_dir(&root_uri).await.unwrap();
        assert_eq!(entries.len(), 2);
        assert!(
            entries
                .iter()
                .any(|e| e.name == "src" && e.kind == DirEntryKind::Directory)
        );
        assert!(
            entries
                .iter()
                .any(|e| e.name == "README.md" && e.kind == DirEntryKind::File)
        );
    }

    #[tokio::test]
    async fn memory_vfs_canonical_uri() {
        let vfs = MemoryVfs::new();
        let uri = child_uri("canonical_test");
        let canon = vfs.canonical_uri(&uri).unwrap();
        assert_eq!(canon, uri);
    }

    #[tokio::test]
    async fn memory_vfs_insert_and_read() {
        let vfs = MemoryVfs::new();
        let uri = child_uri("insert_test_file");
        let path = uri_to_path(&uri).unwrap();
        vfs.insert(path.clone(), b"preloaded data".to_vec());
        assert!(vfs.exists(&uri).await.unwrap());
        assert_eq!(vfs.read_file(&uri).await.unwrap(), b"preloaded data");
    }

    #[tokio::test]
    async fn memory_vfs_overwrite() {
        let vfs = MemoryVfs::new();
        let uri = child_uri("overwrite_test");
        vfs.write_file(&uri, b"version 1").await.unwrap();
        vfs.write_file(&uri, b"version 2").await.unwrap();
        assert_eq!(vfs.read_file(&uri).await.unwrap(), b"version 2");
    }
}
