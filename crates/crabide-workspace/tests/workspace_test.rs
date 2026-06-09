//! Integration tests for `crabide-workspace`.
//!
//! Uses `MemoryVfs` which stores files by `PathBuf` extracted from `file://` URIs.
//! On Windows, the path must include a drive letter (e.g., `C:\`).

use std::path::PathBuf;
use std::sync::Arc;

use crabide_core::{
    traits::{TextBuffer, VirtualFileSystem},
    types::{BufferId, DocumentUri, Language, Position, Range, TextEdit},
};
use crabide_vfs::MemoryVfs;
use crabide_workspace::{CloseResult, Workspace};

/// Create a `file://` URI from a platform-appropriate absolute path.
fn file_uri(path: &str) -> DocumentUri {
    // On Windows, file:///C:/... is needed; on Unix, file:///... works.
    let uri_str = if cfg!(windows) && !path.contains(':') {
        // Add C: drive prefix for absolute-looking paths on Windows
        format!("file:///C:{}", path)
    } else {
        format!("file://{path}")
    };
    DocumentUri::parse(&uri_str).expect("valid file URI")
}

// ── Workspace construction ─────────────────────────────────────────────────

#[test]
fn workspace_new_is_empty() {
    let vfs = Arc::new(MemoryVfs::new());
    let ws = Workspace::new(vfs);
    assert!(ws.open_buffer_ids().is_empty());
    assert!(ws.roots().is_empty());
}

#[test]
fn workspace_roots() {
    let vfs = Arc::new(MemoryVfs::new());
    let ws = Workspace::new(vfs);
    ws.add_root(PathBuf::from("/project"));
    ws.add_root(PathBuf::from("/lib"));
    assert_eq!(ws.roots().len(), 2);
    ws.remove_root(&PathBuf::from("/project"));
    assert_eq!(ws.roots().len(), 1);
    assert_eq!(ws.roots()[0], PathBuf::from("/lib"));
}

// ── Opening files ──────────────────────────────────────────────────────────

#[tokio::test]
async fn workspace_open_file() {
    let vfs = Arc::new(MemoryVfs::new());
    let uri = file_uri("/tmp/test.txt");
    // Insert directly into MemoryVfs to bypass URI conversion
    let path = uri.to_file_path().expect("valid file path");
    vfs.insert(path, b"hello world".to_vec());
    let ws = Workspace::new(vfs);
    let id = ws.open_file(uri.clone()).await.unwrap();
    assert_eq!(ws.open_buffer_ids().len(), 1);
    assert_eq!(ws.uri(id).unwrap(), uri);
    assert!(!ws.is_dirty(id));
}

#[tokio::test]
async fn workspace_open_file_already_open() {
    let vfs = Arc::new(MemoryVfs::new());
    let uri = file_uri("/tmp/test.txt");
    let path = uri.to_file_path().expect("valid file path");
    vfs.insert(path, b"content".to_vec());
    let ws = Workspace::new(vfs);
    let id1 = ws.open_file(uri.clone()).await.unwrap();
    let id2 = ws.open_file(uri.clone()).await.unwrap();
    assert_eq!(id1, id2, "opening same file should return same id");
}

#[tokio::test]
async fn workspace_open_file_nonexistent() {
    let vfs = Arc::new(MemoryVfs::new());
    let ws = Workspace::new(vfs);
    let uri = file_uri("/tmp/nonexistent.rs");
    let result = ws.open_file(uri).await;
    assert!(result.is_err());
}

// ── Open or create ─────────────────────────────────────────────────────────

#[tokio::test]
async fn workspace_open_or_create_new() {
    let vfs = Arc::new(MemoryVfs::new());
    let ws = Workspace::new(vfs);
    let uri = file_uri("/tmp/newfile.rs");
    let _id = ws.open_or_create(uri).await.unwrap();
    assert_eq!(ws.open_buffer_ids().len(), 1);
}

#[tokio::test]
async fn workspace_open_or_create_existing() {
    let vfs = Arc::new(MemoryVfs::new());
    let uri = file_uri("/tmp/existing.rs");
    let path = uri.to_file_path().expect("valid file path");
    vfs.insert(path, b"code".to_vec());
    let ws = Workspace::new(vfs);
    let id1 = ws.open_or_create(uri.clone()).await.unwrap();
    let id2 = ws.open_or_create(uri).await.unwrap();
    assert_eq!(id1, id2);
}

// ── Untitled buffers ───────────────────────────────────────────────────────

#[test]
fn workspace_new_untitled() {
    let vfs = Arc::new(MemoryVfs::new());
    let ws = Workspace::new(vfs);
    let id = ws.new_untitled(None);
    let uri = ws.uri(id).unwrap();
    assert!(uri.as_str().contains("Untitled"));
    assert_eq!(ws.language(id), Some(Language::PLAIN_TEXT));
}

#[test]
fn workspace_new_untitled_with_language() {
    let vfs = Arc::new(MemoryVfs::new());
    let ws = Workspace::new(vfs);
    let id = ws.new_untitled(Some(Language::RUST));
    assert_eq!(ws.language(id), Some(Language::RUST));
}

#[test]
fn workspace_untitled_counter_increments() {
    let vfs = Arc::new(MemoryVfs::new());
    let ws = Workspace::new(vfs);
    let id1 = ws.new_untitled(None);
    let id2 = ws.new_untitled(None);
    let uri1 = ws.uri(id1).unwrap();
    let uri2 = ws.uri(id2).unwrap();
    assert_ne!(uri1, uri2);
}

// ── Close ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn workspace_close_clean() {
    let vfs = Arc::new(MemoryVfs::new());
    let uri = file_uri("/tmp/test.txt");
    let path = uri.to_file_path().expect("valid file path");
    vfs.insert(path, b"data".to_vec());
    let ws = Workspace::new(vfs);
    let id = ws.open_file(uri).await.unwrap();
    assert_eq!(ws.close(id, false).unwrap(), CloseResult::Closed);
    assert!(ws.open_buffer_ids().is_empty());
}

#[tokio::test]
async fn workspace_close_with_unsaved_changes() {
    let vfs = Arc::new(MemoryVfs::new());
    let uri = file_uri("/tmp/test.txt");
    let path = uri.to_file_path().expect("valid file path");
    vfs.insert(path, b"data".to_vec());
    let ws = Workspace::new(vfs);
    let id = ws.open_file(uri).await.unwrap();
    // Make an edit to make it dirty
    ws.apply_edit(
        id,
        TextEdit {
            range: Range::new(Position::ZERO, Position::ZERO),
            new_text: "modified".into(),
        },
        "edit",
    )
    .unwrap();
    assert!(ws.is_dirty(id));
    assert_eq!(ws.close(id, false).unwrap(), CloseResult::UnsavedChanges);
    // Force close
    assert_eq!(ws.close(id, true).unwrap(), CloseResult::Closed);
}

#[test]
fn workspace_close_nonexistent() {
    let vfs = Arc::new(MemoryVfs::new());
    let ws = Workspace::new(vfs);
    let result = ws.close(BufferId::new(), true);
    assert!(result.is_err());
}

// ── Document queries ───────────────────────────────────────────────────────

#[tokio::test]
async fn workspace_get_buffer_id() {
    let vfs = Arc::new(MemoryVfs::new());
    let uri = file_uri("/tmp/file.rs");
    let path = uri.to_file_path().expect("valid file path");
    vfs.insert(path, b"fn main() {}".to_vec());
    let ws = Workspace::new(vfs);
    assert!(ws.get_buffer_id(&uri).is_none());
    let id = ws.open_file(uri.clone()).await.unwrap();
    assert_eq!(ws.get_buffer_id(&uri), Some(id));
}

#[tokio::test]
async fn workspace_language_from_extension() {
    let vfs = Arc::new(MemoryVfs::new());
    let uri = file_uri("/tmp/main.rs");
    let path = uri.to_file_path().expect("valid file path");
    vfs.insert(path, b"fn main() {}".to_vec());
    let ws = Workspace::new(vfs);
    let id = ws.open_file(uri).await.unwrap();
    assert_eq!(ws.language(id), Some(Language::RUST));
}

// ── Edits ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn workspace_apply_edit() {
    let vfs = Arc::new(MemoryVfs::new());
    let uri = file_uri("/tmp/test.txt");
    let path = uri.to_file_path().expect("valid file path");
    vfs.insert(path, b"hello".to_vec());
    let ws = Workspace::new(vfs);
    let id = ws.open_file(uri).await.unwrap();

    ws.apply_edit(
        id,
        TextEdit {
            range: Range::new(Position::new(0, 5), Position::new(0, 5)),
            new_text: " world".into(),
        },
        "insert",
    )
    .unwrap();
    assert!(ws.is_dirty(id));

    let lines = ws.get_lines(id).unwrap();
    assert_eq!(lines[0], "hello world");
}

// ── Undo / Redo ────────────────────────────────────────────────────────────

#[tokio::test]
async fn workspace_undo_redo() {
    let vfs = Arc::new(MemoryVfs::new());
    let uri = file_uri("/tmp/test.txt");
    let path = uri.to_file_path().expect("valid file path");
    vfs.insert(path, b"start".to_vec());
    let ws = Workspace::new(vfs);
    let id = ws.open_file(uri).await.unwrap();

    assert!(!ws.can_undo(id));
    assert!(!ws.can_redo(id));

    ws.apply_edit(
        id,
        TextEdit {
            range: Range::new(Position::ZERO, Position::ZERO),
            new_text: "modified".into(),
        },
        "edit",
    )
    .unwrap();

    assert!(ws.can_undo(id));
    assert!(!ws.can_redo(id));

    assert!(ws.undo(id).unwrap());
    assert!(!ws.can_undo(id));
    assert!(ws.can_redo(id));

    assert!(ws.redo(id).unwrap());
    assert!(ws.can_undo(id));
    assert!(!ws.can_redo(id));
}

// ── Save ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn workspace_save() {
    let vfs = Arc::new(MemoryVfs::new());
    let uri = file_uri("/tmp/test.txt");
    let path = uri.to_file_path().expect("valid file path");
    vfs.insert(path, b"original".to_vec());
    let ws = Workspace::new(vfs.clone());
    let id = ws.open_file(uri.clone()).await.unwrap();

    ws.apply_edit(
        id,
        TextEdit {
            range: Range::new(Position::ZERO, Position::new(0, 8)),
            new_text: "updated".into(),
        },
        "edit",
    )
    .unwrap();
    assert!(ws.is_dirty(id));

    ws.save(id).await.unwrap();
    assert!(!ws.is_dirty(id));

    let saved = vfs.read_file(&uri).await.unwrap();
    assert_eq!(saved.as_slice(), b"updated");
}

#[tokio::test]
async fn workspace_save_as() {
    let vfs = Arc::new(MemoryVfs::new());
    let old_uri = file_uri("/tmp/old.txt");
    let new_uri = file_uri("/tmp/new.txt");
    let old_path = old_uri.to_file_path().expect("valid file path");
    vfs.insert(old_path, b"content".to_vec());
    let ws = Workspace::new(vfs.clone());
    let id = ws.open_file(old_uri.clone()).await.unwrap();

    ws.save_as(id, new_uri.clone()).await.unwrap();
    assert!(!ws.is_dirty(id));
    assert_eq!(ws.uri(id).unwrap(), new_uri);
    assert!(ws.get_buffer_id(&old_uri).is_none());
    assert_eq!(ws.get_buffer_id(&new_uri), Some(id));
}

// ── with_document / with_document_mut ──────────────────────────────────────

#[tokio::test]
async fn workspace_with_document() {
    let vfs = Arc::new(MemoryVfs::new());
    let uri = file_uri("/tmp/test.txt");
    let path = uri.to_file_path().expect("valid file path");
    vfs.insert(path, b"hello".to_vec());
    let ws = Workspace::new(vfs);
    let id = ws.open_file(uri).await.unwrap();

    let text = ws.with_document(id, |e| e.document.text_content()).unwrap();
    assert_eq!(text, "hello");
}

#[tokio::test]
async fn workspace_with_document_mut() {
    let vfs = Arc::new(MemoryVfs::new());
    let uri = file_uri("/tmp/test.txt");
    let path = uri.to_file_path().expect("valid file path");
    vfs.insert(path, b"hello".to_vec());
    let ws = Workspace::new(vfs);
    let id = ws.open_file(uri).await.unwrap();

    ws.with_document_mut(id, |e| {
        let rope = e.document.rope_snapshot();
        e.history.push(rope, "snapshot", vec![]);
    })
    .unwrap();
    assert!(ws.can_undo(id));
}

#[test]
fn workspace_with_document_nonexistent() {
    let vfs = Arc::new(MemoryVfs::new());
    let ws = Workspace::new(vfs);
    let result = ws.with_document(BufferId::new(), |_| ());
    assert!(result.is_err());
}

// ── get_lines ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn workspace_get_lines() {
    let vfs = Arc::new(MemoryVfs::new());
    let uri = file_uri("/tmp/multi.txt");
    let path = uri.to_file_path().expect("valid file path");
    vfs.insert(path, b"line1\nline2\nline3".to_vec());
    let ws = Workspace::new(vfs);
    let id = ws.open_file(uri).await.unwrap();
    let lines = ws.get_lines(id).unwrap();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "line1");
    assert_eq!(lines[1], "line2");
    assert_eq!(lines[2], "line3");
}

#[test]
fn workspace_get_lines_nonexistent() {
    let vfs = Arc::new(MemoryVfs::new());
    let ws = Workspace::new(vfs);
    let result = ws.get_lines(BufferId::new());
    assert!(result.is_err());
}

// ── Register document ──────────────────────────────────────────────────────

#[test]
fn workspace_register_document() {
    let vfs = Arc::new(MemoryVfs::new());
    let ws = Workspace::new(vfs);
    let doc = crabide_workspace::Document::new_untitled(Language::RUST);
    let _id = ws.register_document(doc);
    assert_eq!(ws.open_buffer_ids().len(), 1);
}

// ── Additional edge-case tests ────────────────────────────────────────────────

#[tokio::test]
async fn workspace_apply_edits_batch() {
    let vfs = Arc::new(MemoryVfs::new());
    let uri = file_uri("/tmp/batch.txt");
    let path = uri.to_file_path().expect("valid file path");
    vfs.insert(path, b"abcdef".to_vec());
    let ws = Workspace::new(vfs);
    let id = ws.open_file(uri).await.unwrap();

    // Apply edits in reverse order (back-to-front)
    let edits = vec![
        TextEdit::replace(
            Range::new(Position::new(0, 3), Position::new(0, 6)),
            "DEF".to_string(),
        ),
        TextEdit::replace(
            Range::new(Position::new(0, 0), Position::new(0, 3)),
            "ABC".to_string(),
        ),
    ];
    ws.apply_edits(id, &edits, "batch").unwrap();
    let lines = ws.get_lines(id).unwrap();
    assert_eq!(lines[0], "ABCDEF");
}

#[tokio::test]
async fn workspace_undo_and_redo_nonexistent_document() {
    let vfs = Arc::new(MemoryVfs::new());
    let ws = Workspace::new(vfs);
    let id = BufferId::new();
    assert!(ws.undo(id).is_err());
    assert!(ws.redo(id).is_err());
}

#[tokio::test]
async fn workspace_edit_nonexistent_document() {
    let vfs = Arc::new(MemoryVfs::new());
    let ws = Workspace::new(vfs);
    let id = BufferId::new();
    let edit = TextEdit::insert(Position::ZERO, "text".to_string());
    assert!(ws.apply_edit(id, edit, "test").is_err());
}

#[tokio::test]
async fn workspace_open_multiple_close_one() {
    let vfs = Arc::new(MemoryVfs::new());
    let uri1 = file_uri("/tmp/a.txt");
    let uri2 = file_uri("/tmp/b.txt");
    let p1 = uri1.to_file_path().expect("valid path");
    let p2 = uri2.to_file_path().expect("valid path");
    vfs.insert(p1, b"file a".to_vec());
    vfs.insert(p2, b"file b".to_vec());
    let ws = Workspace::new(vfs);
    let id1 = ws.open_file(uri1).await.unwrap();
    let id2 = ws.open_file(uri2).await.unwrap();
    assert_eq!(ws.open_buffer_ids().len(), 2);

    // Close first, verify second still accessible
    ws.close(id1, true).unwrap();
    assert_eq!(ws.open_buffer_ids().len(), 1);
    assert!(ws.with_document(id2, |_| ()).is_ok());
    assert!(ws.with_document(id1, |_| ()).is_err());
}

#[tokio::test]
async fn workspace_remove_root_not_added() {
    let vfs = Arc::new(MemoryVfs::new());
    let ws = Workspace::new(vfs);
    // Removing a root that was never added should be a no-op
    ws.remove_root(&PathBuf::from("/nonexistent"));
    assert_eq!(ws.roots().len(), 0);
}

#[tokio::test]
async fn workspace_save_as_to_already_open_uri() {
    let vfs = Arc::new(MemoryVfs::new());
    let uri_a = file_uri("/tmp/a.txt");
    let uri_b = file_uri("/tmp/b.txt");
    let pa = uri_a.to_file_path().expect("valid path");
    let pb = uri_b.to_file_path().expect("valid path");
    vfs.insert(pa, b"content a".to_vec());
    vfs.insert(pb, b"content b".to_vec());
    let ws = Workspace::new(vfs.clone());

    let id_a = ws.open_file(uri_a.clone()).await.unwrap();
    let _id_b = ws.open_file(uri_b.clone()).await.unwrap();

    // Save id_a under uri_b which is already open → should succeed, and old buffer becomes untracked
    ws.save_as(id_a, uri_b.clone()).await.unwrap();
    assert_eq!(ws.uri(id_a).unwrap(), uri_b);
    // The old b-tracking id should still reference a valid buffer
    assert!(!ws.open_buffer_ids().is_empty());
}

#[test]
fn workspace_register_document_twice() {
    let vfs = Arc::new(MemoryVfs::new());
    let ws = Workspace::new(vfs);
    let doc1 = crabide_workspace::Document::new_untitled(Language::RUST);
    let doc2 = crabide_workspace::Document::new_untitled(Language::PYTHON);
    let id1 = ws.register_document(doc1);
    let id2 = ws.register_document(doc2);
    assert_eq!(ws.open_buffer_ids().len(), 2);
    assert_eq!(ws.language(id1), Some(Language::RUST));
    assert_eq!(ws.language(id2), Some(Language::PYTHON));
}
