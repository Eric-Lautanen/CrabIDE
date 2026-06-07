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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crabide_core::traits::VirtualFileSystem;

    fn file_uri(path: &str) -> DocumentUri {
        let mut base = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/tmp"));
        base.push(path);
        DocumentUri::from_file_path(&base).unwrap()
    }

    #[tokio::test]
    async fn resolver_local_scheme() {
        let resolver = VfsResolver::new();
        let uri = file_uri("test_local.txt");
        let kind = resolver.resolve(&uri).unwrap();
        match kind {
            VfsKind::Local(_) => {} // expected
            _ => panic!("expected LocalVfs for file:// scheme"),
        }
    }

    #[test]
    fn resolver_memory_scheme() {
        let resolver = VfsResolver::new();
        let uri = DocumentUri::parse("memory:///test.txt").unwrap();
        let kind = resolver.resolve(&uri).unwrap();
        match kind {
            VfsKind::Memory(_) => {} // expected
            _ => panic!("expected MemoryVfs for memory:// scheme"),
        }
    }

    #[test]
    fn resolver_untitled_scheme() {
        let resolver = VfsResolver::new();
        let uri = DocumentUri::parse("untitled:///Untitled-1").unwrap();
        let kind = resolver.resolve(&uri).unwrap();
        match kind {
            VfsKind::Memory(_) => {} // expected
            _ => panic!("expected MemoryVfs for untitled:// scheme"),
        }
    }

    #[test]
    fn resolver_unsupported_scheme() {
        let resolver = VfsResolver::new();
        let uri = DocumentUri::parse("ssh:///server/path").unwrap();
        let err = resolver.resolve(&uri).unwrap_err();
        assert!(format!("{err}").contains("unsupported VFS scheme"));
    }

    #[test]
    fn resolver_memory_vfs_access() {
        let resolver = VfsResolver::new();
        let mem = resolver.memory_vfs();
        // Use file:// URI to interact with MemoryVfs (since it uses uri_to_path internally)
        let uri = file_uri("resolver_prep.txt");
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(mem.write_file(&uri, b"prep")).unwrap();
        // Verify memory_vfs() returns the same instance
        let mem2 = resolver.memory_vfs();
        let rt2 = tokio::runtime::Runtime::new().unwrap();
        let data = rt2.block_on(mem2.read_file(&uri)).unwrap();
        assert_eq!(data, b"prep");
    }

    #[test]
    fn resolver_read_only_wrapper() {
        let inner = MemoryVfs::new();
        let wrapper = VfsResolver::read_only(inner);
        let uri = file_uri("ro_wrapper.txt");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt.block_on(wrapper.write_file(&uri, b"x")).unwrap_err();
        assert!(format!("{err}").contains("not allowed"));
    }
}
