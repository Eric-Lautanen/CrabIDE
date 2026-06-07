//! Tests for `crabide-git`.
//!
//! These tests run with the default feature set (no `git-support`), so the
//! `GitService` methods are all no-ops.  We verify the API compiles and
//! doesn't panic.

use crabide_core::event::EditorEvent;
use crabide_git::GitService;
use crossbeam_channel::unbounded;

// ── GitService without git-support feature ─────────────────────────────────

#[test]
fn git_service_start_returns_none_without_git_repo() {
    let (tx, _rx) = unbounded::<EditorEvent>();
    // Use a temp dir that almost certainly isn't a git repo.
    let tmp = std::env::temp_dir().join("crabide_git_test_nonexistent_repo");
    let svc = GitService::start(tmp, tx);
    // Without a real git repo, start() should return None regardless of feature.
    assert!(svc.is_none());
}

#[test]
fn git_service_api_compiles() {
    // Verify all GitService associated functions compile.
    let _ = GitService::start;
    let _ = GitService::refresh;
    let _ = GitService::request_diff_hunks;
    let _ = GitService::request_blame;
    let _ = GitService::stage_file;
    let _ = GitService::unstage_file;
    let _ = GitService::stage_all;
    let _ = GitService::unstage_all;
    let _ = GitService::commit;
    let _ = GitService::checkout_branch;
    let _ = GitService::create_branch;
    let _ = GitService::discard_file;
}

// ── Public type verification ───────────────────────────────────────────────

#[test]
fn types_are_re_exported() {
    // Verify crabide_git::Result exists and is the same as crabide_core::Result
    let _: crabide_git::Result<()> = Ok(());
}
