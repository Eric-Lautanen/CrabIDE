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
