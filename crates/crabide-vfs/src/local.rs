//! `LocalVfs` — implements `VirtualFileSystem` using `tokio::fs`.

use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;

use crabide_core::error::{Result, crabideError};
use crabide_core::traits::{DirEntry, DirEntryKind, VirtualFileSystem};
use crabide_core::types::DocumentUri;

use crate::helpers::{canonical_uri, path_to_uri, uri_to_path};

/// Local filesystem implementation of [`VirtualFileSystem`].
#[derive(Debug, Clone, Default)]
pub struct LocalVfs;

#[async_trait]
impl VirtualFileSystem for LocalVfs {
    async fn read_file(&self, uri: &DocumentUri) -> Result<Vec<u8>> {
        let path = uri_to_path(uri)?;
        fs::read(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                crabideError::DocumentNotFound {
                    uri: uri.to_string(),
                }
            } else {
                crabideError::Io(e)
            }
        })
    }

    async fn write_file(&self, uri: &DocumentUri, contents: &[u8]) -> Result<()> {
        let path = uri_to_path(uri)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        // Atomic write: write to temp file then rename.
        let temp_path = path.with_extension("crabide-tmp");
        fs::write(&temp_path, contents).await?;
        fs::rename(&temp_path, &path).await?;
        Ok(())
    }

    async fn delete(&self, uri: &DocumentUri, recursive: bool) -> Result<()> {
        let path = uri_to_path(uri)?;
        let meta = fs::metadata(&path).await.map_err(crabideError::Io)?;
        if meta.is_dir() {
            if recursive {
                fs::remove_dir_all(&path).await?;
            } else {
                fs::remove_dir(&path).await?;
            }
        } else {
            fs::remove_file(&path).await?;
        }
        Ok(())
    }

    async fn rename(&self, from: &DocumentUri, to: &DocumentUri) -> Result<()> {
        let from_path = uri_to_path(from)?;
        let to_path = uri_to_path(to)?;
        if let Some(parent) = to_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::rename(&from_path, &to_path).await?;
        Ok(())
    }

    async fn create_dir(&self, uri: &DocumentUri) -> Result<()> {
        let path = uri_to_path(uri)?;
        fs::create_dir_all(&path).await?;
        Ok(())
    }

    async fn read_dir(&self, uri: &DocumentUri) -> Result<Vec<DirEntry>> {
        let path = uri_to_path(uri)?;
        let mut entries: Vec<DirEntry> = Vec::new();
        let mut reader = fs::read_dir(&path).await.map_err(crabideError::Io)?;

        while let Some(entry) = reader.next_entry().await.map_err(crabideError::Io)? {
            let entry_path: PathBuf = entry.path();
            let meta = match entry.metadata().await {
                Ok(m) => m,
                Err(e) => {
                    log::debug!("VFS read_dir: skipping {}: {e}", entry_path.display());
                    continue;
                }
            };
            let kind = if meta.is_dir() {
                DirEntryKind::Directory
            } else if meta.is_symlink() {
                DirEntryKind::Symlink
            } else if meta.is_file() {
                DirEntryKind::File
            } else {
                DirEntryKind::Other
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            let size = if meta.is_file() {
                Some(meta.len())
            } else {
                None
            };
            entries.push(DirEntry {
                uri: path_to_uri(&entry_path)?,
                name,
                kind,
                size,
            });
        }

        // Directories first, then files — both alphabetically (case-insensitive)
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
        fs::try_exists(&path).await.map_err(crabideError::Io)
    }

    fn canonical_uri(&self, uri: &DocumentUri) -> Result<DocumentUri> {
        canonical_uri(uri)
    }
}
