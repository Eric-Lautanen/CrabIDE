//! VFS resolver: selects the appropriate VFS implementation based on URI scheme.

use crabide_core::error::{crabideError, Result};
use crabide_core::traits::VirtualFileSystem;
use crabide_core::types::DocumentUri;

use crate::local::LocalVfs;
use crate::memory::MemoryVfs;
use crate::read_only::ReadOnlyVfs;

/// Factory that resolves a [`VirtualFileSystem`] implementation based on URI scheme.
///
/// Supported schemes:
/// - `file://` → [`LocalVfs`]
/// - `memory://` → [`MemoryVfs`]
/// - Any other scheme → error
///
/// The resolver can optionally wrap the resolved VFS in [`ReadOnlyVfs`].
#[derive(Debug, Clone, Default)]
pub struct VfsResolver {
    memory: MemoryVfs,
}

impl VfsResolver {
    pub fn new() -> Self {
        Self {
            memory: MemoryVfs::new(),
        }
    }

    /// Resolve the appropriate VFS for the given URI's scheme.
    pub fn resolve(&self, uri: &DocumentUri) -> Result<VfsKind<'_>> {
        match uri.as_url().scheme() {
            "file" => Ok(VfsKind::Local(LocalVfs)),
            "memory" => Ok(VfsKind::Memory(&self.memory)),
            "untitled" => Ok(VfsKind::Memory(&self.memory)),
            scheme => Err(crabideError::Other(format!(
                "unsupported VFS scheme: {scheme}"
            ))),
        }
    }

    /// Get a reference to the shared memory VFS (for pre-populating test data).
    pub fn memory_vfs(&self) -> &MemoryVfs {
        &self.memory
    }

    /// Wrap any VFS in a read-only wrapper.
    pub fn read_only<T: VirtualFileSystem + Sync>(inner: T) -> ReadOnlyVfs<T> {
        ReadOnlyVfs::new(inner)
    }
}

/// The kind of VFS returned by the resolver.
#[derive(Debug)]
pub enum VfsKind<'a> {
    Local(LocalVfs),
    Memory(&'a MemoryVfs),
}
