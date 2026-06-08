//! `crabide-git` — libgit2-backed git service for the editor.
//!
//! All git2 operations are blocking and run on a dedicated OS thread so the
//! UI thread and Tokio pool are never stalled.
//!
//! # Architecture
//! ```text
//!  crabideApp                     git worker thread
//!  ──────────────────             ──────────────────────────────────────
//!  GitService::start()  ────────► spawn thread (owns Repository)
//!  svc.refresh()        ──cmd──► recv GitCommand → run git2 op
//!  svc.stage_file()     ──cmd──► recv GitCommand → run git2 op
//!  …                             send EditorEvent::Git(…) back to UI
//! ```
//!
//! # Feature gate
//! This crate compiles without any git2 dependency unless the `git-support`
//! feature is enabled.  When disabled `GitService::start` always returns `None`.

use std::path::PathBuf;

use crossbeam_channel::Sender;

use crabide_core::event::EditorEvent;
use crabide_core::types::DocumentUri;

// Re-export core error types for downstream convenience.
pub use crabide_core::error::{crabideError, Result};

// ── GitService public API (always compiled) ───────────────────────────────────

/// Handle to the background git worker.
///
/// Dropping this sends `Shutdown` to the worker thread automatically.
/// When the `git-support` feature is not enabled this type is a zero-size
/// no-op and `start` always returns `None`.
pub struct GitService {
    #[cfg(feature = "git-support")]
    cmd_tx: crossbeam_channel::Sender<GitCommand>,
}

impl GitService {
    /// Try to discover a git repository starting from `repo_path` and launch
    /// the background worker.  Returns `None` if no repository is found or if
    /// the `git-support` feature is disabled.
    pub fn start(repo_path: PathBuf, event_tx: Sender<EditorEvent>) -> Option<Self> {
        #[cfg(feature = "git-support")]
        {
            git_support::start_service(repo_path, event_tx)
        }
        #[cfg(not(feature = "git-support"))]
        {
            let _ = (repo_path, event_tx);
            None
        }
    }

    /// Refresh HEAD info and full repository status.
    pub fn refresh(&self) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::Refresh);
    }

    /// Request diff hunks for `path` (sent back as `DiffHunksUpdated`).
    pub fn request_diff_hunks(&self, uri: DocumentUri, path: PathBuf) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::DiffHunks { uri, path });
        #[cfg(not(feature = "git-support"))]
        let _ = (uri, path);
    }

    /// Request blame for `path` (sent back as `BlameUpdated`).
    pub fn request_blame(&self, uri: DocumentUri, path: PathBuf) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::Blame { uri, path });
        #[cfg(not(feature = "git-support"))]
        let _ = (uri, path);
    }

    /// Stage a single file.
    pub fn stage_file(&self, path: PathBuf) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::StageFile(path));
        #[cfg(not(feature = "git-support"))]
        let _ = path;
    }

    /// Unstage a single file.
    pub fn unstage_file(&self, path: PathBuf) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::UnstageFile(path));
        #[cfg(not(feature = "git-support"))]
        let _ = path;
    }

    /// Stage all modified / untracked files.
    pub fn stage_all(&self) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::StageAll);
    }

    /// Unstage all staged files.
    pub fn unstage_all(&self) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::UnstageAll);
    }

    /// Commit staged changes with `message`.
    pub fn commit(&self, message: String) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::Commit { message });
        #[cfg(not(feature = "git-support"))]
        let _ = message;
    }

    /// Checkout an existing local branch by name.
    pub fn checkout_branch(&self, name: String) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::CheckoutBranch(name));
        #[cfg(not(feature = "git-support"))]
        let _ = name;
    }

    /// Create a new branch from HEAD and check it out.
    pub fn create_branch(&self, name: String) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::CreateBranch(name));
        #[cfg(not(feature = "git-support"))]
        let _ = name;
    }

    /// Discard working-tree changes to `path` (restore from HEAD).
    pub fn discard_file(&self, path: PathBuf) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::DiscardFile(path));
        #[cfg(not(feature = "git-support"))]
        let _ = path;
    }

    /// Request staged diff hunks for `path` (sent back as `DiffStagedUpdated`).
    pub fn request_diff_staged(&self, uri: DocumentUri, path: PathBuf) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::DiffStaged { uri, path });
        #[cfg(not(feature = "git-support"))]
        let _ = (uri, path);
    }

    /// Request listing of all local and remote branches (sent back as `BranchesListed`).
    pub fn list_branches(&self) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::ListBranches);
    }

    /// Delete a local branch by name.
    pub fn delete_branch(&self, name: String) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::DeleteBranch(name));
        #[cfg(not(feature = "git-support"))]
        let _ = name;
    }
}

#[cfg(feature = "git-support")]
impl Drop for GitService {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(GitCommand::Shutdown);
    }
}

// ── Full git2-backed implementation (feature = "git-support") ─────────────────

#[cfg(feature = "git-support")]
use crossbeam_channel::{bounded, Receiver};

#[cfg(feature = "git-support")]
use log::{debug, warn};

#[cfg(feature = "git-support")]
use std::collections::HashMap;
#[cfg(feature = "git-support")]
use std::path::Path;
#[cfg(feature = "git-support")]
use std::thread;

#[cfg(feature = "git-support")]
use crabide_core::event::{BlameLine, BranchInfo, DiffHunk, FileStatus, HunkKind, StatusKind};

/// Commands sent to the git background worker thread.
#[cfg(feature = "git-support")]
enum GitCommand {
    Refresh,
    DiffHunks { uri: DocumentUri, path: PathBuf },
    DiffStaged { uri: DocumentUri, path: PathBuf },
    Blame { uri: DocumentUri, path: PathBuf },
    StageFile(PathBuf),
    UnstageFile(PathBuf),
    StageAll,
    UnstageAll,
    Commit { message: String },
    CheckoutBranch(String),
    CreateBranch(String),
    ListBranches,
    DeleteBranch(String),
    DiscardFile(PathBuf),
    Shutdown,
}

#[cfg(feature = "git-support")]
mod git_support {
    use super::*;

    pub fn start_service(repo_path: PathBuf, event_tx: Sender<EditorEvent>) -> Option<GitService> {
        let repo = git2::Repository::discover(&repo_path)
            .map_err(|e| debug!("no git repo at {}: {}", repo_path.display(), e.message()))
            .ok()?;

        let workdir = repo.workdir()?.to_owned();
        let (cmd_tx, cmd_rx) = bounded::<GitCommand>(256);

        thread::Builder::new()
            .name("crabide-git-worker".into())
            .spawn(move || git_worker(repo, workdir, event_tx, cmd_rx))
            .map_err(|e| warn!("failed to spawn git worker: {e}"))
            .ok()?;

        Some(GitService { cmd_tx })
    }

    fn git_worker(
        repo: git2::Repository,
        workdir: PathBuf,
        event_tx: Sender<EditorEvent>,
        cmd_rx: Receiver<GitCommand>,
    ) {
        // Limit libgit2's pack-file mmap usage.
        // Default mwindow_mapped_limit on 64-bit is 8 GB (!), which causes libgit2
        // to keep large portions of pack files memory-mapped at idle.  Cap it to
        // 32 MB so the editor's idle RSS stays reasonable.
        // Safety: these are process-wide libgit2 settings with no data races.
        unsafe {
            let _ = git2::opts::set_mwindow_mapped_limit(32 * 1024 * 1024);
            let _ = git2::opts::set_mwindow_size(1024 * 1024);
        }

        // Send initial HEAD state immediately on startup.
        send_head_info(&repo, &event_tx);
        send_status(&repo, &event_tx);

        for cmd in &cmd_rx {
            match cmd {
                GitCommand::Refresh => {
                    send_head_info(&repo, &event_tx);
                    send_status(&repo, &event_tx);
                }

                GitCommand::DiffHunks { uri, path } => {
                    send_diff_hunks(&repo, &workdir, uri, &path, &event_tx);
                }

                GitCommand::DiffStaged { uri, path } => {
                    send_diff_staged(&repo, &workdir, uri, &path, &event_tx);
                }

                GitCommand::Blame { uri, path } => {
                    send_blame(&repo, &workdir, uri, &path, &event_tx);
                }

                GitCommand::StageFile(path) => {
                    run_op(&event_tx, format!("stage {}", path.display()), || {
                        stage_file_impl(&repo, &workdir, &path)
                    });
                    send_status(&repo, &event_tx);
                }

                GitCommand::UnstageFile(path) => {
                    run_op(&event_tx, format!("unstage {}", path.display()), || {
                        unstage_file_impl(&repo, &workdir, &path)
                    });
                    send_status(&repo, &event_tx);
                }

                GitCommand::StageAll => {
                    run_op(&event_tx, "stage all".into(), || stage_all_impl(&repo));
                    send_status(&repo, &event_tx);
                }

                GitCommand::UnstageAll => {
                    run_op(&event_tx, "unstage all".into(), || unstage_all_impl(&repo));
                    send_status(&repo, &event_tx);
                }

                GitCommand::Commit { message } => {
                    run_op(&event_tx, "commit".into(), || commit_impl(&repo, &message));
                    send_head_info(&repo, &event_tx);
                    send_status(&repo, &event_tx);
                }

                GitCommand::CheckoutBranch(name) => {
                    run_op(&event_tx, format!("checkout {name}"), || {
                        checkout_branch_impl(&repo, &name)
                    });
                    send_head_info(&repo, &event_tx);
                    send_status(&repo, &event_tx);
                }

                GitCommand::CreateBranch(name) => {
                    run_op(&event_tx, format!("create branch {name}"), || {
                        create_branch_impl(&repo, &name)
                    });
                    send_head_info(&repo, &event_tx);
                }

                GitCommand::ListBranches => {
                    send_branch_list(&repo, &event_tx);
                }

                GitCommand::DeleteBranch(name) => {
                    run_op(&event_tx, format!("delete branch {name}"), || {
                        delete_branch_impl(&repo, &name)
                    });
                }

                GitCommand::DiscardFile(path) => {
                    run_op(&event_tx, format!("discard {}", path.display()), || {
                        discard_file_impl(&repo, &workdir, &path)
                    });
                    send_status(&repo, &event_tx);
                }

                GitCommand::Shutdown => break,
            }
        }
    }

    fn run_op<F>(event_tx: &Sender<EditorEvent>, operation: String, f: F)
    where
        F: FnOnce() -> std::result::Result<(), git2::Error>,
    {
        use crabide_core::event::GitEvent;
        match f() {
            Ok(()) => {
                let _ = event_tx.send(EditorEvent::Git(GitEvent::OperationCompleted { operation }));
            }
            Err(e) => {
                warn!("git op '{operation}' failed: {}", e.message());
                let _ = event_tx.send(EditorEvent::Git(GitEvent::OperationFailed {
                    operation,
                    error: e.message().to_owned(),
                }));
            }
        }
    }

    fn send_head_info(repo: &git2::Repository, event_tx: &Sender<EditorEvent>) {
        use crabide_core::event::GitEvent;
        let (branch, commit) = match repo.head() {
            Ok(head) => {
                let branch = if head.is_branch() {
                    head.shorthand().ok().map(|s| s.to_owned())
                } else {
                    None
                };
                let commit = head
                    .peel_to_commit()
                    .map(|c| c.id().to_string())
                    .unwrap_or_else(|_| "0000000".into());
                (branch, commit)
            }
            Err(_) => {
                let default = repo
                    .config()
                    .ok()
                    .and_then(|cfg| cfg.get_string("init.defaultBranch").ok())
                    .unwrap_or_else(|| "main".into());
                (Some(default), "0000000".into())
            }
        };
        let _ = event_tx.send(EditorEvent::Git(GitEvent::HeadChanged { branch, commit }));
    }

    fn send_status(repo: &git2::Repository, event_tx: &Sender<EditorEvent>) {
        use crabide_core::event::GitEvent;
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true)
            .include_ignored(false)
            .recurse_untracked_dirs(false);

        let entries = match repo.statuses(Some(&mut opts)) {
            Ok(s) => s,
            Err(e) => {
                warn!("git status: {}", e.message());
                return;
            }
        };

        let statuses: Vec<FileStatus> = entries
            .iter()
            .filter_map(|entry| {
                let path = PathBuf::from(entry.path().ok()?);
                let flags = entry.status();
                Some(FileStatus {
                    path,
                    index_status: git2_index_status(flags),
                    worktree_status: git2_worktree_status(flags),
                })
            })
            .collect();

        let _ = event_tx.send(EditorEvent::Git(GitEvent::StatusRefreshed { statuses }));
    }

    fn git2_index_status(flags: git2::Status) -> StatusKind {
        if flags.contains(git2::Status::INDEX_NEW) {
            return StatusKind::Added;
        }
        if flags.contains(git2::Status::INDEX_MODIFIED) {
            return StatusKind::Modified;
        }
        if flags.contains(git2::Status::INDEX_DELETED) {
            return StatusKind::Deleted;
        }
        if flags.contains(git2::Status::INDEX_RENAMED) {
            return StatusKind::Renamed;
        }
        if flags.contains(git2::Status::INDEX_TYPECHANGE) {
            return StatusKind::Modified;
        }
        StatusKind::Unmodified
    }

    fn git2_worktree_status(flags: git2::Status) -> StatusKind {
        if flags.contains(git2::Status::WT_NEW) {
            return StatusKind::Untracked;
        }
        if flags.contains(git2::Status::WT_MODIFIED) {
            return StatusKind::Modified;
        }
        if flags.contains(git2::Status::WT_DELETED) {
            return StatusKind::Deleted;
        }
        if flags.contains(git2::Status::WT_RENAMED) {
            return StatusKind::Renamed;
        }
        if flags.contains(git2::Status::WT_TYPECHANGE) {
            return StatusKind::Modified;
        }
        if flags.contains(git2::Status::CONFLICTED) {
            return StatusKind::Conflicted;
        }
        if flags.contains(git2::Status::IGNORED) {
            return StatusKind::Ignored;
        }
        StatusKind::Unmodified
    }

    fn send_diff_hunks(
        repo: &git2::Repository,
        workdir: &Path,
        uri: DocumentUri,
        path: &Path,
        event_tx: &Sender<EditorEvent>,
    ) {
        use crabide_core::event::GitEvent;
        let rel_path = match path.strip_prefix(workdir) {
            Ok(p) => p,
            Err(_) => {
                warn!("diff_hunks: path not under workdir: {}", path.display());
                return;
            }
        };

        let mut opts = git2::DiffOptions::new();
        opts.pathspec(rel_path.to_string_lossy().as_ref());

        let diff = match repo.head().ok().and_then(|h| h.peel_to_commit().ok()) {
            Some(commit) => match commit.tree() {
                Ok(tree) => repo.diff_tree_to_workdir_with_index(Some(&tree), Some(&mut opts)),
                Err(e) => {
                    warn!("diff tree: {}", e.message());
                    return;
                }
            },
            None => repo.diff_index_to_workdir(None, Some(&mut opts)),
        };

        let diff = match diff {
            Ok(d) => d,
            Err(e) => {
                warn!("diff: {}", e.message());
                return;
            }
        };

        let mut hunks: Vec<DiffHunk> = Vec::new();
        let _ = diff.foreach(
            &mut |_, _| true,
            None,
            Some(&mut |_, hunk: git2::DiffHunk<'_>| {
                let old_lines = hunk.old_lines();
                let new_lines = hunk.new_lines();
                let kind = if old_lines == 0 {
                    HunkKind::Added
                } else if new_lines == 0 {
                    HunkKind::Removed
                } else {
                    HunkKind::Modified
                };
                hunks.push(DiffHunk {
                    old_start: hunk.old_start(),
                    old_lines,
                    new_start: hunk.new_start(),
                    new_lines,
                    kind,
                });
                true
            }),
            None,
        );

        let _ = event_tx.send(EditorEvent::Git(GitEvent::DiffHunksUpdated { uri, hunks }));
    }

    fn send_blame(
        repo: &git2::Repository,
        workdir: &Path,
        uri: DocumentUri,
        path: &Path,
        event_tx: &Sender<EditorEvent>,
    ) {
        use crabide_core::event::GitEvent;
        let rel_path = match path.strip_prefix(workdir) {
            Ok(p) => p,
            Err(_) => return,
        };

        let blame = match repo.blame_file(rel_path, None) {
            Ok(b) => b,
            Err(e) => {
                warn!("blame_file: {}", e.message());
                return;
            }
        };

        let mut summaries: HashMap<git2::Oid, String> = HashMap::new();
        let lines: Vec<BlameLine> = blame
            .iter()
            .flat_map(|hunk| {
                let oid = hunk.final_commit_id();
                let sig = hunk.final_signature();
                let author = sig
                    .as_ref()
                    .and_then(|s| s.name().ok())
                    .unwrap_or("Unknown")
                    .to_owned();
                let email = sig
                    .as_ref()
                    .and_then(|s| s.email().ok())
                    .unwrap_or("")
                    .to_owned();
                let time = sig.map(|s| s.when().seconds()).unwrap_or(0);
                let hash = oid.to_string();

                let summary = summaries
                    .entry(oid)
                    .or_insert_with(|| {
                        repo.find_commit(oid)
                            .ok()
                            .and_then(|c| c.summary().ok().flatten().map(|s| s.to_owned()))
                            .unwrap_or_default()
                    })
                    .clone();

                let start = hunk.final_start_line() as u32;
                let count = hunk.lines_in_hunk() as u32;

                (0..count)
                    .map(move |i| BlameLine {
                        line: start + i,
                        commit_hash: hash.clone(),
                        author: author.clone(),
                        author_email: email.clone(),
                        commit_time: time,
                        summary: summary.clone(),
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        let _ = event_tx.send(EditorEvent::Git(GitEvent::BlameUpdated { uri, lines }));
    }

    fn stage_file_impl(
        repo: &git2::Repository,
        workdir: &Path,
        path: &Path,
    ) -> std::result::Result<(), git2::Error> {
        let rel = path
            .strip_prefix(workdir)
            .map_err(|_| git2::Error::from_str("path not in workdir"))?;
        let mut index = repo.index()?;
        index.add_path(rel)?;
        index.write()?;
        Ok(())
    }

    fn unstage_file_impl(
        repo: &git2::Repository,
        workdir: &Path,
        path: &Path,
    ) -> std::result::Result<(), git2::Error> {
        let rel = path
            .strip_prefix(workdir)
            .map_err(|_| git2::Error::from_str("path not in workdir"))?;

        match repo.head() {
            Ok(head) => {
                let commit = head.peel_to_commit()?;
                repo.reset_default(
                    Some(commit.as_object()),
                    std::iter::once(rel.to_string_lossy().into_owned()),
                )?;
            }
            Err(_) => {
                let mut index = repo.index()?;
                index.remove_path(rel)?;
                index.write()?;
            }
        }
        Ok(())
    }

    fn stage_all_impl(repo: &git2::Repository) -> std::result::Result<(), git2::Error> {
        let mut index = repo.index()?;
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
        index.write()?;
        Ok(())
    }

    fn unstage_all_impl(repo: &git2::Repository) -> std::result::Result<(), git2::Error> {
        match repo.head() {
            Ok(head) => {
                let commit = head.peel_to_commit()?;
                repo.reset(commit.as_object(), git2::ResetType::Mixed, None)?;
            }
            Err(_) => {
                let mut index = repo.index()?;
                index.clear()?;
                index.write()?;
            }
        }
        Ok(())
    }

    fn commit_impl(repo: &git2::Repository, message: &str) -> std::result::Result<(), git2::Error> {
        let sig = repo.signature()?;
        let mut index = repo.index()?;
        let tree_oid = index.write_tree()?;
        let tree = repo.find_tree(tree_oid)?;

        let parent_commits: Vec<git2::Commit<'_>> = match repo.head() {
            Ok(head) => vec![head.peel_to_commit()?],
            Err(_) => vec![],
        };
        let parents: Vec<&git2::Commit<'_>> = parent_commits.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)?;
        Ok(())
    }

    fn checkout_branch_impl(
        repo: &git2::Repository,
        name: &str,
    ) -> std::result::Result<(), git2::Error> {
        let branch = repo.find_branch(name, git2::BranchType::Local)?;
        let ref_name = branch.get().name()?.to_owned();
        repo.set_head(&ref_name)?;
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().safe()))?;
        Ok(())
    }

    fn create_branch_impl(
        repo: &git2::Repository,
        name: &str,
    ) -> std::result::Result<(), git2::Error> {
        let head_commit = repo.head()?.peel_to_commit()?;
        let branch = repo.branch(name, &head_commit, false)?;
        let ref_name = branch.get().name()?.to_owned();
        repo.set_head(&ref_name)?;
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().safe()))?;
        Ok(())
    }

    fn discard_file_impl(
        repo: &git2::Repository,
        workdir: &Path,
        path: &Path,
    ) -> std::result::Result<(), git2::Error> {
        let rel = path
            .strip_prefix(workdir)
            .map_err(|_| git2::Error::from_str("path not in workdir"))?;
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.force().path(rel);
        repo.checkout_head(Some(&mut checkout))?;
        Ok(())
    }

    fn send_diff_staged(
        repo: &git2::Repository,
        workdir: &Path,
        uri: DocumentUri,
        path: &Path,
        event_tx: &Sender<EditorEvent>,
    ) {
        use crabide_core::event::GitEvent;
        let rel_path = match path.strip_prefix(workdir) {
            Ok(p) => p,
            Err(_) => {
                warn!("diff_staged: path not under workdir: {}", path.display());
                return;
            }
        };

        let mut opts = git2::DiffOptions::new();
        opts.pathspec(rel_path.to_string_lossy().as_ref());

        // Diff index (staging area) vs HEAD tree.
        let diff = match repo.head().ok().and_then(|h| h.peel_to_commit().ok()) {
            Some(commit) => match commit.tree() {
                Ok(tree) => repo.diff_tree_to_index(Some(&tree), None, Some(&mut opts)),
                Err(e) => {
                    warn!("diff staged tree: {}", e.message());
                    return;
                }
            },
            None => {
                // No HEAD commit — no staged changes to compare against.
                let _ = event_tx.send(EditorEvent::Git(GitEvent::DiffStagedUpdated {
                    uri,
                    hunks: vec![],
                }));
                return;
            }
        };

        let diff = match diff {
            Ok(d) => d,
            Err(e) => {
                warn!("diff staged: {}", e.message());
                return;
            }
        };

        let mut hunks: Vec<DiffHunk> = Vec::new();
        let _ = diff.foreach(
            &mut |_, _| true,
            None,
            Some(&mut |_, hunk: git2::DiffHunk<'_>| {
                let old_lines = hunk.old_lines();
                let new_lines = hunk.new_lines();
                let kind = if old_lines == 0 {
                    HunkKind::Added
                } else if new_lines == 0 {
                    HunkKind::Removed
                } else {
                    HunkKind::Modified
                };
                hunks.push(DiffHunk {
                    old_start: hunk.old_start(),
                    old_lines,
                    new_start: hunk.new_start(),
                    new_lines,
                    kind,
                });
                true
            }),
            None,
        );

        let _ = event_tx.send(EditorEvent::Git(GitEvent::DiffStagedUpdated { uri, hunks }));
    }

    fn send_branch_list(repo: &git2::Repository, event_tx: &Sender<EditorEvent>) {
        use crabide_core::event::GitEvent;

        // Determine current HEAD branch name.
        let current_head = repo.head().ok().and_then(|h| {
            if h.is_branch() {
                h.shorthand().map(|s| s.to_owned()).ok()
            } else {
                None
            }
        });

        let mut branches: Vec<BranchInfo> = Vec::new();

        // Gather local branches.
        if let Ok(local) = repo.branches(Some(git2::BranchType::Local)) {
            for branch_result in local.flatten() {
                let (branch, _type) = branch_result;
                let info = make_branch_info(repo, &branch, true, &current_head);
                branches.push(info);
            }
        }

        // Gather remote branches.
        if let Ok(remotes) = repo.branches(Some(git2::BranchType::Remote)) {
            for branch_result in remotes.flatten() {
                let (branch, _type) = branch_result;
                let info = make_branch_info(repo, &branch, false, &current_head);
                branches.push(info);
            }
        }

        let _ = event_tx.send(EditorEvent::Git(GitEvent::BranchesListed { branches }));
    }

    fn make_branch_info(
        repo: &git2::Repository,
        branch: &git2::Branch<'_>,
        is_local: bool,
        current_head: &Option<String>,
    ) -> BranchInfo {
        let ref_name = branch.get().name().unwrap_or("").to_owned();
        let shorthand = branch.get().shorthand().unwrap_or("").to_owned();
        let is_current = is_local && Some(&shorthand) == current_head.as_ref();

        let commit = branch
            .get()
            .peel_to_commit()
            .map(|c| c.id().to_string())
            .unwrap_or_else(|_| "0000000".into());

        // Upstream tracking info (local branches only).
        let (upstream, ahead, behind) = if is_local {
            match branch.upstream() {
                Ok(up) => {
                    let up_shorthand = up.get().shorthand().ok().map(|s| s.to_owned());
                    let (a, b) = branch
                        .get()
                        .peel_to_commit()
                        .ok()
                        .and_then(|c1| {
                            up.get()
                                .peel_to_commit()
                                .ok()
                                .and_then(|c2| repo.graph_ahead_behind(c1.id(), c2.id()).ok())
                        })
                        .unwrap_or((0, 0));
                    (up_shorthand, a, b)
                }
                Err(_) => (None, 0, 0),
            }
        } else {
            (None, 0, 0)
        };

        BranchInfo {
            ref_name,
            shorthand,
            is_local,
            is_current,
            commit,
            upstream,
            ahead,
            behind,
        }
    }

    fn delete_branch_impl(
        repo: &git2::Repository,
        name: &str,
    ) -> std::result::Result<(), git2::Error> {
        let mut branch = repo.find_branch(name, git2::BranchType::Local)?;
        branch.delete()?;
        Ok(())
    }
}
