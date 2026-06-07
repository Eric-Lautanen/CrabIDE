//! Read-only VFS wrapper that prevents write operations.

use async_trait::async_trait;

use crabide_core::error::{crabideError, Result};
use crabide_core::traits::{DirEntry, VirtualFileSystem};
use crabide_core::types::DocumentUri;

/// A read-only wrapper around any [`VirtualFileSystem`] implementation.
///
/// All write operations (`write_file`, `delete`, `rename`, `create_dir`)
/// return an error. Read operations are forwarded to the inner VFS.
#[derive(Debug, Clone)]
pub struct ReadOnlyVfs<T> {
    inner: T,
}

impl<T> ReadOnlyVfs<T> {
    pub fn new(inner: T) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }
}

#[async_trait]
impl<T: VirtualFileSystem + Sync> VirtualFileSystem for ReadOnlyVfs<T> {
    async fn read_file(&self, uri: &DocumentUri) -> Result<Vec<u8>> {
        self.inner.read_file(uri).await
    }

    async fn write_file(&self, _uri: &DocumentUri, _contents: &[u8]) -> Result<()> {
        Err(crabideError::Other(
            "read-only VFS: write not allowed".into(),
        ))
    }

    async fn delete(&self, _uri: &DocumentUri, _recursive: bool) -> Result<()> {
        Err(crabideError::Other(
            "read-only VFS: delete not allowed".into(),
        ))
    }

    async fn rename(&self, _from: &DocumentUri, _to: &DocumentUri) -> Result<()> {
        Err(crabideError::Other(
            "read-only VFS: rename not allowed".into(),
        ))
    }

    async fn create_dir(&self, _uri: &DocumentUri) -> Result<()> {
        Err(crabideError::Other(
            "read-only VFS: create_dir not allowed".into(),
        ))
    }

    async fn read_dir(&self, uri: &DocumentUri) -> Result<Vec<DirEntry>> {
        self.inner.read_dir(uri).await
    }

    async fn exists(&self, uri: &DocumentUri) -> Result<bool> {
        self.inner.exists(uri).await
    }

    fn canonical_uri(&self, uri: &DocumentUri) -> Result<DocumentUri> {
        self.inner.canonical_uri(uri)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryVfs;
    use crabide_core::traits::VirtualFileSystem;

    fn child_uri(name: &str) -> DocumentUri {
        let mut base = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/tmp"));
        base.push(name);
        DocumentUri::from_file_path(&base).unwrap()
    }

    fn dir_uri(subdir: &str) -> DocumentUri {
        let mut base = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/tmp"));
        base.push(subdir);
        DocumentUri::from_file_path(&base).unwrap()
    }

    #[tokio::test]
    async fn read_only_forwards_read() {
        let inner = MemoryVfs::new();
        let uri = child_uri("ro_readable.txt");
        inner.write_file(&uri, b"content").await.unwrap();
        let wrapper = ReadOnlyVfs::new(inner);
        assert_eq!(wrapper.read_file(&uri).await.unwrap(), b"content");
    }

    #[tokio::test]
    async fn read_only_blocks_write() {
        let inner = MemoryVfs::new();
        let wrapper = ReadOnlyVfs::new(inner);
        let uri = child_uri("ro_blocked_write.txt");
        let err = wrapper.write_file(&uri, b"data").await.unwrap_err();
        assert!(format!("{err}").contains("not allowed"));
    }

    #[tokio::test]
    async fn read_only_blocks_delete() {
        let inner = MemoryVfs::new();
        let wrapper = ReadOnlyVfs::new(inner);
        let uri = child_uri("ro_blocked_del.txt");
        let err = wrapper.delete(&uri, false).await.unwrap_err();
        assert!(format!("{err}").contains("not allowed"));
    }

    #[tokio::test]
    async fn read_only_blocks_rename() {
        let inner = MemoryVfs::new();
        let wrapper = ReadOnlyVfs::new(inner);
        let from = child_uri("ro_from.txt");
        let to = child_uri("ro_to.txt");
        let err = wrapper.rename(&from, &to).await.unwrap_err();
        assert!(format!("{err}").contains("not allowed"));
    }

    #[tokio::test]
    async fn read_only_blocks_create_dir() {
        let inner = MemoryVfs::new();
        let wrapper = ReadOnlyVfs::new(inner);
        let uri = dir_uri("ro_newdir");
        let err = wrapper.create_dir(&uri).await.unwrap_err();
        assert!(format!("{err}").contains("not allowed"));
    }

    #[tokio::test]
    async fn read_only_forwards_exists() {
        let inner = MemoryVfs::new();
        let uri = child_uri("ro_existing.txt");
        inner.write_file(&uri, b"x").await.unwrap();
        let wrapper = ReadOnlyVfs::new(inner);
        assert!(wrapper.exists(&uri).await.unwrap());
    }

    #[tokio::test]
    async fn read_only_forwards_canonical_uri() {
        let inner = MemoryVfs::new();
        let wrapper = ReadOnlyVfs::new(inner);
        let uri = child_uri("ro_canonical.txt");
        assert_eq!(wrapper.canonical_uri(&uri).unwrap(), uri);
    }

    #[tokio::test]
    async fn read_only_forwards_read_dir() {
        let inner = MemoryVfs::new();
        let root_uri = dir_uri("ro_read_dir");
        let file_uri = child_uri("ro_read_dir/a.rs");
        inner.create_dir(&root_uri).await.unwrap();
        inner.write_file(&file_uri, b"fn a() {}").await.unwrap();
        let wrapper = ReadOnlyVfs::new(inner);
        let entries = wrapper.read_dir(&root_uri).await.unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn read_only_inner() {
        let inner = MemoryVfs::new();
        let wrapper = ReadOnlyVfs::new(inner.clone());
        let _ref: &MemoryVfs = wrapper.inner();
    }
}
