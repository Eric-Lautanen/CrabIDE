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
pub use crabide_core::error::{Result, crabideError};

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

    /// Fetch from a remote. If `branch` is None, fetches all branches.
    pub fn fetch(&self, remote: String, branch: Option<String>) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::Fetch { remote, branch });
        #[cfg(not(feature = "git-support"))]
        let _ = (remote, branch);
    }

    /// Pull (fetch + merge) from `remote`/`branch`. If `rebase` is true, use rebase instead of merge.
    pub fn pull(&self, remote: String, branch: String, rebase: bool) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::Pull {
            remote,
            branch,
            rebase,
        });
        #[cfg(not(feature = "git-support"))]
        let _ = (remote, branch, rebase);
    }

    /// Push to `remote`. If `branch` is None, push the current branch.
    /// If `force` is true, force-push (--force).
    pub fn push(&self, remote: String, branch: Option<String>, force: bool) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::Push {
            remote,
            branch,
            force,
        });
        #[cfg(not(feature = "git-support"))]
        let _ = (remote, branch, force);
    }

    /// Merge the named branch into the current branch.
    pub fn merge(&self, branch: String) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::Merge { branch });
        #[cfg(not(feature = "git-support"))]
        let _ = branch;
    }

    /// Rebase the current branch onto the named branch.
    pub fn rebase(&self, branch: String) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::Rebase { branch });
        #[cfg(not(feature = "git-support"))]
        let _ = branch;
    }

    /// Push a stash onto the stack with an optional message.
    pub fn stash_push(&self, message: Option<String>) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::StashPush { message });
        #[cfg(not(feature = "git-support"))]
        let _ = message;
    }

    /// Pop a stash from the stack (optionally by index).
    pub fn stash_pop(&self, index: Option<usize>) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::StashPop { index });
        #[cfg(not(feature = "git-support"))]
        let _ = index;
    }

    /// List all stashes on the stack.
    pub fn stash_list(&self) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::StashList);
    }

    /// Drop a stash by index.
    pub fn stash_drop(&self, index: usize) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::StashDrop { index });
        #[cfg(not(feature = "git-support"))]
        let _ = index;
    }

    /// Request commit log history. If `branch` is None, show all refs.
    /// `limit` caps entries (0 = unlimited).
    pub fn log(&self, branch: Option<String>, limit: usize) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::Log { branch, limit });
        #[cfg(not(feature = "git-support"))]
        let _ = (branch, limit);
    }

    /// List all tags (lightweight and annotated).
    pub fn list_tags(&self) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::ListTags);
    }

    /// Create a tag. If `message` is set, creates an annotated tag.
    /// If `target` is None, tags HEAD.
    pub fn create_tag(&self, name: String, target: Option<String>, message: Option<String>) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::CreateTag {
            name,
            target,
            message,
        });
        #[cfg(not(feature = "git-support"))]
        let _ = (name, target, message);
    }

    /// Delete a tag by name.
    pub fn delete_tag(&self, name: String) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::DeleteTag { name });
        #[cfg(not(feature = "git-support"))]
        let _ = name;
    }

    /// List all remotes.
    pub fn list_remotes(&self) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::ListRemotes);
    }

    /// Add a remote with the given URL.
    pub fn add_remote(&self, name: String, url: String) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::AddRemote { name, url });
        #[cfg(not(feature = "git-support"))]
        let _ = (name, url);
    }

    /// Remove a remote by name.
    pub fn remove_remote(&self, name: String) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::RemoveRemote { name });
        #[cfg(not(feature = "git-support"))]
        let _ = name;
    }

    /// List all submodules and their status.
    pub fn list_submodules(&self) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::ListSubmodules);
    }

    /// Add a submodule at the given URL and path.
    pub fn submodule_add(&self, url: String, path: String, branch: Option<String>) {
        #[cfg(feature = "git-support")]
        let _ = self
            .cmd_tx
            .send(GitCommand::SubmoduleAdd { url, path, branch });
        #[cfg(not(feature = "git-support"))]
        let _ = (url, path, branch);
    }

    /// Update/init submodule(s). If `path` is None, update all submodules.
    /// `init` controls whether uninitialized submodules are initialized first.
    /// `recursive` controls whether nested submodules are also updated.
    pub fn submodule_update(&self, path: Option<String>, init: bool, recursive: bool) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::SubmoduleUpdate {
            path,
            init,
            recursive,
        });
        #[cfg(not(feature = "git-support"))]
        let _ = (path, init, recursive);
    }

    /// Sync submodule URL(s). If `path` is None, sync all submodules.
    pub fn submodule_sync(&self, path: Option<String>) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::SubmoduleSync { path });
        #[cfg(not(feature = "git-support"))]
        let _ = path;
    }

    /// List all conflicted files in the index.
    pub fn list_conflicts(&self) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::ListConflicts);
    }

    /// Resolve a conflict by taking our side (stage 2 / HEAD version).
    pub fn resolve_ours(&self, path: String) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::ResolveOurs { path });
        #[cfg(not(feature = "git-support"))]
        let _ = path;
    }

    /// Resolve a conflict by taking their side (stage 3 / merge source version).
    pub fn resolve_theirs(&self, path: String) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::ResolveTheirs { path });
        #[cfg(not(feature = "git-support"))]
        let _ = path;
    }

    /// Mark a conflict as resolved after manual editing.
    pub fn mark_resolved(&self, path: String) {
        #[cfg(feature = "git-support")]
        let _ = self.cmd_tx.send(GitCommand::MarkResolved { path });
        #[cfg(not(feature = "git-support"))]
        let _ = path;
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
use crossbeam_channel::{Receiver, bounded};

#[cfg(feature = "git-support")]
use log::{debug, warn};

#[cfg(feature = "git-support")]
use std::collections::HashMap;
#[cfg(feature = "git-support")]
use std::path::Path;
#[cfg(feature = "git-support")]
use std::thread;

#[cfg(feature = "git-support")]
use crabide_core::event::{
    BlameLine, BranchInfo, DiffHunk, FileStatus, HunkKind, StashEntry, StatusKind,
};

/// Commands sent to the git background worker thread.
#[cfg(feature = "git-support")]
enum GitCommand {
    Refresh,
    DiffHunks {
        uri: DocumentUri,
        path: PathBuf,
    },
    DiffStaged {
        uri: DocumentUri,
        path: PathBuf,
    },
    Blame {
        uri: DocumentUri,
        path: PathBuf,
    },
    StageFile(PathBuf),
    UnstageFile(PathBuf),
    StageAll,
    UnstageAll,
    Commit {
        message: String,
    },
    CheckoutBranch(String),
    CreateBranch(String),
    ListBranches,
    DeleteBranch(String),
    DiscardFile(PathBuf),
    Shutdown,
    /// Fetch from remote (optionally a specific branch).
    Fetch {
        remote: String,
        branch: Option<String>,
    },
    /// Pull (fetch + merge) from remote/branch. If `rebase` is true, rebase instead of merge.
    Pull {
        remote: String,
        branch: String,
        rebase: bool,
    },
    /// Push to remote. If `force`, use force push.
    Push {
        remote: String,
        branch: Option<String>,
        force: bool,
    },
    /// Merge the named branch into the current branch.
    Merge {
        branch: String,
    },
    /// Rebase the current branch onto the named branch.
    Rebase {
        branch: String,
    },
    /// Push a stash onto the stack.
    StashPush {
        message: Option<String>,
    },
    /// Pop a stash from the stack.
    StashPop {
        index: Option<usize>,
    },
    /// List all stashes.
    StashList,
    /// Drop a stash by index.
    StashDrop {
        index: usize,
    },
    /// Request commit log. If `branch` is Some, log for that branch; otherwise all refs.
    /// `limit` caps the number of entries (0 = no limit).
    Log {
        branch: Option<String>,
        limit: usize,
    },
    /// List all tags (lightweight and annotated).
    ListTags,
    /// Create a tag.
    CreateTag {
        name: String,
        /// Commit to tag (None = HEAD).
        target: Option<String>,
        /// Tag message (None = lightweight tag, Some = annotated tag).
        message: Option<String>,
    },
    /// Delete a tag.
    DeleteTag {
        name: String,
    },
    /// List all remotes.
    ListRemotes,
    /// Add a remote.
    AddRemote {
        name: String,
        url: String,
    },
    /// Remove a remote.
    RemoveRemote {
        name: String,
    },
    /// List all submodules.
    ListSubmodules,
    /// Add a submodule.
    SubmoduleAdd {
        url: String,
        path: String,
        branch: Option<String>,
    },
    /// Update/init submodule(s).
    SubmoduleUpdate {
        path: Option<String>,
        init: bool,
        recursive: bool,
    },
    /// Sync submodule URL(s).
    SubmoduleSync {
        path: Option<String>,
    },
    /// List conflicted files in the index.
    ListConflicts,
    /// Resolve a conflict by taking our side (stage 2 / HEAD).
    ResolveOurs {
        path: String,
    },
    /// Resolve a conflict by taking their side (stage 3 / merge source).
    ResolveTheirs {
        path: String,
    },
    /// Mark a conflict as resolved after manual editing (removes conflict entries from index).
    MarkResolved {
        path: String,
    },
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
        mut repo: git2::Repository,
        workdir: PathBuf,
        event_tx: Sender<EditorEvent>,
        cmd_rx: Receiver<GitCommand>,
    ) {
        // Limit libgit2's pack-file mmap usage.
        // Default mwindow_mapped_limit on 64-bit is 8 GB (!), which causes libgit2
        // to keep large portions of pack files memory-mapped at idle.  Cap it to
        // 32 MB so the editor's idle RSS stays reasonable.
        // SAFETY: these are process-wide libgit2 settings with no data races.
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

                GitCommand::Fetch { remote, branch } => {
                    use crabide_core::event::GitEvent;
                    let op_name = format!(
                        "fetch {}{}",
                        remote,
                        branch
                            .as_deref()
                            .map(|b| format!(" {b}"))
                            .unwrap_or_default()
                    );
                    match fetch_impl(&repo, &remote, branch.as_deref()) {
                        Ok(msg) => {
                            let _ = event_tx.send(EditorEvent::Git(GitEvent::FetchCompleted {
                                remote: remote.clone(),
                                branch: branch.clone(),
                                message: msg,
                            }));
                            send_status(&repo, &event_tx);
                        }
                        Err(e) => {
                            warn!("git fetch failed: {}", e.message());
                            let _ = event_tx.send(EditorEvent::Git(GitEvent::OperationFailed {
                                operation: op_name,
                                error: e.message().to_owned(),
                            }));
                        }
                    }
                }

                GitCommand::Pull {
                    remote,
                    branch,
                    rebase,
                } => {
                    use crabide_core::event::GitEvent;
                    let op_name = format!("pull {remote} {branch}");
                    if rebase {
                        match pull_rebase_impl(&repo, &remote, &branch) {
                            Ok(()) => {
                                let _ =
                                    event_tx.send(EditorEvent::Git(GitEvent::OperationCompleted {
                                        operation: op_name,
                                    }));
                                send_head_info(&repo, &event_tx);
                                send_status(&repo, &event_tx);
                            }
                            Err(e) => {
                                warn!("git pull --rebase failed: {}", e.message());
                                let _ =
                                    event_tx.send(EditorEvent::Git(GitEvent::OperationFailed {
                                        operation: op_name,
                                        error: e.message().to_owned(),
                                    }));
                            }
                        }
                    } else {
                        match pull_merge_impl(&repo, &remote, &branch) {
                            Ok(()) => {
                                let _ =
                                    event_tx.send(EditorEvent::Git(GitEvent::OperationCompleted {
                                        operation: op_name,
                                    }));
                                send_head_info(&repo, &event_tx);
                                send_status(&repo, &event_tx);
                            }
                            Err(e) => {
                                warn!("git pull failed: {}", e.message());
                                let _ =
                                    event_tx.send(EditorEvent::Git(GitEvent::OperationFailed {
                                        operation: op_name,
                                        error: e.message().to_owned(),
                                    }));
                            }
                        }
                    }
                }

                GitCommand::Push {
                    remote,
                    branch,
                    force,
                } => {
                    use crabide_core::event::GitEvent;
                    let op_name = format!(
                        "push {}{}",
                        remote,
                        branch
                            .as_deref()
                            .map(|b| format!(" {b}"))
                            .unwrap_or_default()
                    );
                    match push_impl(&repo, &remote, branch.as_deref(), force) {
                        Ok(pushed) => {
                            let _ = event_tx.send(EditorEvent::Git(GitEvent::PushCompleted {
                                remote: remote.clone(),
                                branch: branch.clone(),
                                pushed,
                            }));
                            send_status(&repo, &event_tx);
                        }
                        Err(e) => {
                            warn!("git push failed: {}", e.message());
                            let _ = event_tx.send(EditorEvent::Git(GitEvent::OperationFailed {
                                operation: op_name,
                                error: e.message().to_owned(),
                            }));
                        }
                    }
                }

                GitCommand::Merge { branch } => {
                    run_op(&event_tx, format!("merge {branch}"), || {
                        merge_impl(&repo, &branch)
                    });
                    send_head_info(&repo, &event_tx);
                    send_status(&repo, &event_tx);
                }

                GitCommand::Rebase { branch } => {
                    run_op(&event_tx, format!("rebase onto {branch}"), || {
                        rebase_impl(&repo, &branch)
                    });
                    send_head_info(&repo, &event_tx);
                    send_status(&repo, &event_tx);
                }

                GitCommand::StashPush { message } => {
                    let op = message
                        .as_deref()
                        .map(|m| format!("stash push {m}"))
                        .unwrap_or_else(|| "stash push".into());
                    run_op(&event_tx, op, || {
                        stash_push_impl(&mut repo, message.as_deref())
                    });
                    send_status(&repo, &event_tx);
                }

                GitCommand::StashPop { index } => {
                    let op = index
                        .map(|i| format!("stash pop @{{{i}}}"))
                        .unwrap_or_else(|| "stash pop".into());
                    run_op(&event_tx, op, || stash_pop_impl(&mut repo, index));
                    send_head_info(&repo, &event_tx);
                    send_status(&repo, &event_tx);
                }

                GitCommand::StashList => {
                    use crabide_core::event::GitEvent;
                    let stashes = stash_list_impl(&mut repo);
                    let _ = event_tx.send(EditorEvent::Git(GitEvent::StashListUpdated { stashes }));
                }

                GitCommand::StashDrop { index } => {
                    run_op(&event_tx, format!("stash drop @{{{index}}}"), || {
                        stash_drop_impl(&mut repo, index)
                    });
                }

                GitCommand::Log { branch, limit } => {
                    send_log(&repo, &workdir, &event_tx, branch.as_deref(), limit);
                }

                GitCommand::ListTags => {
                    send_tag_list(&repo, &event_tx);
                }

                GitCommand::CreateTag {
                    name,
                    target,
                    message,
                } => {
                    create_tag_impl(
                        &repo,
                        &event_tx,
                        &name,
                        target.as_deref(),
                        message.as_deref(),
                    );
                }

                GitCommand::DeleteTag { name } => {
                    run_op(&event_tx, format!("delete tag {name}"), || {
                        delete_tag_impl(&repo, &name)
                    });
                }

                GitCommand::ListRemotes => {
                    send_remote_list(&repo, &event_tx);
                }

                GitCommand::AddRemote { name, url } => {
                    run_op(&event_tx, format!("add remote {name}"), || {
                        add_remote_impl(&repo, &name, &url)
                    });
                }

                GitCommand::RemoveRemote { name } => {
                    run_op(&event_tx, format!("remove remote {name}"), || {
                        remove_remote_impl(&repo, &name)
                    });
                }

                GitCommand::ListSubmodules => {
                    send_submodule_list(&repo, &event_tx);
                }

                GitCommand::SubmoduleAdd { url, path, branch } => {
                    use crabide_core::event::GitEvent;
                    match submodule_add_impl(&mut repo, &url, &path, branch.as_deref()) {
                        Ok(()) => {
                            let _ =
                                event_tx.send(EditorEvent::Git(GitEvent::SubmoduleAdded { path }));
                            send_status(&repo, &event_tx);
                        }
                        Err(e) => {
                            warn!("submodule add: {}", e.message());
                            let _ = event_tx.send(EditorEvent::Git(GitEvent::OperationFailed {
                                operation: format!("submodule add {path}"),
                                error: e.message().to_owned(),
                            }));
                        }
                    }
                }

                GitCommand::SubmoduleUpdate {
                    path,
                    init,
                    recursive,
                } => {
                    use crabide_core::event::GitEvent;
                    match submodule_update_impl(&repo, path.as_deref(), init, recursive) {
                        Ok(paths) => {
                            for p in paths {
                                let _ = event_tx
                                    .send(EditorEvent::Git(GitEvent::SubmoduleUpdated { path: p }));
                            }
                            send_status(&repo, &event_tx);
                        }
                        Err(e) => {
                            warn!("submodule update: {}", e.message());
                            let _ = event_tx.send(EditorEvent::Git(GitEvent::OperationFailed {
                                operation: "submodule update".into(),
                                error: e.message().to_owned(),
                            }));
                        }
                    }
                }

                GitCommand::SubmoduleSync { path } => {
                    use crabide_core::event::GitEvent;
                    match submodule_sync_impl(&repo, path.as_deref()) {
                        Ok(paths) => {
                            for p in paths {
                                let _ = event_tx
                                    .send(EditorEvent::Git(GitEvent::SubmoduleSynced { path: p }));
                            }
                        }
                        Err(e) => {
                            warn!("submodule sync: {}", e.message());
                            let _ = event_tx.send(EditorEvent::Git(GitEvent::OperationFailed {
                                operation: "submodule sync".into(),
                                error: e.message().to_owned(),
                            }));
                        }
                    }
                }

                GitCommand::ListConflicts => {
                    use crabide_core::event::GitEvent;
                    let conflicts = list_conflicts_impl(&repo);
                    let _ =
                        event_tx.send(EditorEvent::Git(GitEvent::ConflictsDetected { conflicts }));
                }

                GitCommand::ResolveOurs { path } => {
                    run_op(&event_tx, format!("resolve ours: {path}"), || {
                        resolve_ours_impl(&repo, &path)
                    });
                    send_status(&repo, &event_tx);
                }

                GitCommand::ResolveTheirs { path } => {
                    run_op(&event_tx, format!("resolve theirs: {path}"), || {
                        resolve_theirs_impl(&repo, &path)
                    });
                    send_status(&repo, &event_tx);
                }

                GitCommand::MarkResolved { path } => {
                    run_op(&event_tx, format!("mark resolved: {path}"), || {
                        mark_resolved_impl(&repo, &path)
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
                let info = make_branch_info(repo, &branch, true, current_head.as_deref());
                branches.push(info);
            }
        }

        // Gather remote branches.
        if let Ok(remotes) = repo.branches(Some(git2::BranchType::Remote)) {
            for branch_result in remotes.flatten() {
                let (branch, _type) = branch_result;
                let info = make_branch_info(repo, &branch, false, current_head.as_deref());
                branches.push(info);
            }
        }

        let _ = event_tx.send(EditorEvent::Git(GitEvent::BranchesListed { branches }));
    }

    fn make_branch_info(
        repo: &git2::Repository,
        branch: &git2::Branch<'_>,
        is_local: bool,
        current_head: Option<&str>,
    ) -> BranchInfo {
        let ref_name = branch.get().name().unwrap_or("").to_owned();
        let shorthand = branch.get().shorthand().unwrap_or("").to_owned();
        let is_current = is_local && Some(shorthand.as_str()) == current_head;

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

    // ── fetch / pull / push / merge / rebase ─────────────────────────────

    fn fetch_impl(
        repo: &git2::Repository,
        remote_name: &str,
        branch: Option<&str>,
    ) -> std::result::Result<String, git2::Error> {
        let mut remote = repo.find_remote(remote_name)?;
        let refspecs: Vec<String> = if let Some(b) = branch {
            vec![format!("+refs/heads/{b}:refs/remotes/{remote_name}/{b}")]
        } else {
            // Fetch all branches — collect refspecs from remote config
            let mut refs = Vec::new();
            for rs in remote.fetch_refspecs()?.iter() {
                if let Ok(Some(s)) = rs {
                    refs.push(s.to_owned());
                }
            }
            refs
        };

        let mut fetch_opts = git2::FetchOptions::new();
        fetch_opts.download_tags(git2::AutotagOption::All);

        let mut callbacks = git2::RemoteCallbacks::new();
        callbacks.transfer_progress(|stats| {
            debug!(
                "fetch: {} bytes, {} objects",
                stats.received_bytes(),
                stats.total_objects()
            );
            true
        });
        fetch_opts.remote_callbacks(callbacks);

        remote.fetch(&refspecs, Some(&mut fetch_opts), None)?;

        let stats = remote.stats();
        Ok(format!(
            "{} objects received, {} bytes transferred",
            stats.total_objects(),
            stats.received_bytes()
        ))
    }

    fn pull_merge_impl(
        repo: &git2::Repository,
        remote_name: &str,
        branch: &str,
    ) -> std::result::Result<(), git2::Error> {
        // First fetch
        fetch_impl(repo, remote_name, Some(branch))?;

        // Find the remote branch commit
        let remote_ref_name = format!("refs/remotes/{remote_name}/{branch}");
        let remote_oid = repo.refname_to_id(&remote_ref_name)?;
        let remote_annotated = repo.find_annotated_commit(remote_oid)?;

        // Find current HEAD commit
        let head_commit = repo.head()?.peel_to_commit()?;

        // Find the merge base
        let merge_base_oid = repo.merge_base(head_commit.id(), remote_oid)?;

        // Check for fast-forward
        if merge_base_oid == head_commit.id() {
            // Fast-forward
            let head_ref = repo.head()?;
            let refname = head_ref
                .name()
                .ok()
                .ok_or_else(|| git2::Error::from_str("cannot get HEAD refname"))?;
            let mut found_ref = repo.find_reference(refname)?;
            found_ref.set_target(remote_oid, "pull: fast-forward")?;
            repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
            return Ok(());
        }

        // Perform merge
        let merge_preference = repo.merge_analysis(&[&remote_annotated])?;
        if merge_preference
            .0
            .contains(git2::MergeAnalysis::ANALYSIS_NORMAL)
        {
            repo.merge(&[&remote_annotated], None, None)?;

            // Check for conflicts
            let mut index = repo.index()?;
            if index.has_conflicts() {
                return Err(git2::Error::from_str(
                    "merge conflicts detected — resolve conflicts and commit manually",
                ));
            }

            // Create merge commit
            let sig = repo.signature()?;
            let tree_oid = index.write_tree()?;
            let tree = repo.find_tree(tree_oid)?;

            let parent_commits: Vec<git2::Commit<'_>> =
                vec![head_commit, repo.find_commit(remote_oid)?];
            let parents: Vec<&git2::Commit<'_>> = parent_commits.iter().collect();
            repo.commit(
                Some("HEAD"),
                &sig,
                &sig,
                &format!("Merge branch '{branch}' of {remote_name}"),
                &tree,
                &parents,
            )?;
        }

        Ok(())
    }

    fn pull_rebase_impl(
        repo: &git2::Repository,
        remote_name: &str,
        branch: &str,
    ) -> std::result::Result<(), git2::Error> {
        // First fetch
        fetch_impl(repo, remote_name, Some(branch))?;

        // Find the remote tracking branch
        let remote_ref_name = format!("refs/remotes/{remote_name}/{branch}");
        let onto_oid = repo.refname_to_id(&remote_ref_name)?;
        let onto_annotated = repo.find_annotated_commit(onto_oid)?;

        // Perform rebase
        let head_annotated = repo
            .head()?
            .peel_to_commit()
            .and_then(|c| repo.find_annotated_commit(c.id()))?;

        let mut rebase = repo.rebase(Some(&head_annotated), Some(&onto_annotated), None, None)?;

        let sig = repo.signature()?;
        while let Some(op_result) = rebase.next() {
            let _op = op_result?;
            let idx = rebase.inmemory_index()?;
            if idx.has_conflicts() {
                return Err(git2::Error::from_str(
                    "rebase conflicts detected — resolve conflicts and continue manually",
                ));
            }
            rebase.commit(None, &sig, None)?;
        }

        rebase.finish(None)?;
        Ok(())
    }

    fn push_impl(
        repo: &git2::Repository,
        remote_name: &str,
        branch: Option<&str>,
        force: bool,
    ) -> std::result::Result<usize, git2::Error> {
        let mut remote = repo.find_remote(remote_name)?;

        // Determine what refspec to push
        let head_branch = repo.head()?.shorthand().ok().map(|s| s.to_owned());
        let branch_name = branch.or(head_branch.as_deref()).unwrap_or("main");

        let prefix = if force { "+" } else { "" };
        let refspec = format!("{prefix}refs/heads/{branch_name}:refs/heads/{branch_name}");

        let mut callbacks = git2::RemoteCallbacks::new();
        callbacks.push_update_reference(|refname, status| {
            if let Some(msg) = status {
                warn!("push ref {refname}: {msg}");
            }
            Ok(())
        });
        let mut push_opts = git2::PushOptions::new();
        push_opts.remote_callbacks(callbacks);

        remote.push(&[&refspec], Some(&mut push_opts))?;

        Ok(1)
    }

    fn merge_impl(repo: &git2::Repository, branch: &str) -> std::result::Result<(), git2::Error> {
        // Find the branch to merge
        let branch_ref = repo.find_branch(branch, git2::BranchType::Local)?;
        let branch_oid = branch_ref
            .get()
            .target()
            .ok_or_else(|| git2::Error::from_str("branch has no target"))?;
        let branch_annotated = repo.find_annotated_commit(branch_oid)?;

        // Find current HEAD commit
        let head_commit = repo.head()?.peel_to_commit()?;

        // Check for fast-forward
        let merge_base_oid = repo.merge_base(head_commit.id(), branch_oid)?;
        if merge_base_oid == head_commit.id() {
            // Fast-forward: just move HEAD
            let head_ref = repo.head()?;
            let refname = head_ref
                .name()
                .ok()
                .ok_or_else(|| git2::Error::from_str("cannot get HEAD refname"))?;
            let mut found_ref = repo.find_reference(refname)?;
            found_ref.set_target(branch_oid, &format!("merge {branch}: fast-forward"))?;
            repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
            return Ok(());
        }

        // Perform merge
        repo.merge(&[&branch_annotated], None, None)?;

        // Check for conflicts
        let mut index = repo.index()?;
        if index.has_conflicts() {
            return Err(git2::Error::from_str(
                "merge conflicts detected — resolve conflicts and commit manually",
            ));
        }

        // Create merge commit
        let sig = repo.signature()?;
        let tree_oid = index.write_tree()?;
        let tree = repo.find_tree(tree_oid)?;

        let branch_commit = repo.find_commit(branch_oid)?;
        let parent_commits: Vec<git2::Commit<'_>> = vec![head_commit, branch_commit];
        let parents: Vec<&git2::Commit<'_>> = parent_commits.iter().collect();
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &format!("Merge branch '{branch}'"),
            &tree,
            &parents,
        )?;

        Ok(())
    }

    fn rebase_impl(repo: &git2::Repository, branch: &str) -> std::result::Result<(), git2::Error> {
        // Find the branch to rebase onto
        let onto_branch = repo.find_branch(branch, git2::BranchType::Local)?;
        let onto_oid = onto_branch
            .get()
            .target()
            .ok_or_else(|| git2::Error::from_str("branch has no target"))?;
        let onto_annotated = repo.find_annotated_commit(onto_oid)?;

        // Find upstream: the merge base of current HEAD and the onto branch
        let head_commit = repo.head()?.peel_to_commit()?;
        let upstream_oid = repo.merge_base(head_commit.id(), onto_oid)?;
        let upstream_annotated = repo.find_annotated_commit(upstream_oid)?;

        let head_annotated = repo.find_annotated_commit(head_commit.id())?;

        // Initialize rebase: replay commits from head (excluding upstream) onto onto
        let mut rebase = repo.rebase(
            Some(&head_annotated),
            Some(&onto_annotated),
            Some(&upstream_annotated),
            None,
        )?;

        let sig = repo.signature()?;
        while let Some(op_result) = rebase.next() {
            let _op = op_result?;
            let idx = rebase.inmemory_index()?;
            if idx.has_conflicts() {
                return Err(git2::Error::from_str(
                    "rebase conflicts detected — resolve conflicts and continue manually",
                ));
            }
            rebase.commit(None, &sig, None)?;
        }

        rebase.finish(None)?;
        Ok(())
    }

    // ── stash operations ────────────────────────────────────────────────

    fn stash_push_impl(
        repo: &mut git2::Repository,
        message: Option<&str>,
    ) -> std::result::Result<(), git2::Error> {
        let sig = repo.signature()?;
        let msg = message.unwrap_or("WIP");
        repo.stash_save(&sig, msg, None)?;
        Ok(())
    }

    fn stash_pop_impl(
        repo: &mut git2::Repository,
        index: Option<usize>,
    ) -> std::result::Result<(), git2::Error> {
        let idx = index.unwrap_or(0);
        repo.stash_pop(idx, None::<&mut git2::StashApplyOptions>)?;
        Ok(())
    }

    fn stash_list_impl(repo: &mut git2::Repository) -> Vec<crabide_core::event::StashEntry> {
        let mut raw: Vec<(usize, String, git2::Oid)> = Vec::new();
        let _ = repo.stash_foreach(|index, message, stash_oid| {
            raw.push((index, message.to_owned(), *stash_oid));
            true
        });

        raw.into_iter()
            .map(|(index, message, oid)| {
                let branch = repo
                    .find_commit(oid)
                    .ok()
                    .and_then(|c| {
                        let msg = match c.message() {
                            Ok(m) => m,
                            Err(_) => return None,
                        };
                        for prefix in &["WIP on ", "On "] {
                            if let Some(pos) = msg.find(prefix) {
                                let rest = &msg[pos + prefix.len()..];
                                let branch = if let Some(end) = rest.find(':') {
                                    rest[..end].to_owned()
                                } else {
                                    rest.to_owned()
                                };
                                return Some(branch);
                            }
                        }
                        None
                    })
                    .unwrap_or_else(|| "unknown".into());
                StashEntry {
                    index,
                    message,
                    branch,
                }
            })
            .collect::<Vec<_>>()
    }

    fn stash_drop_impl(
        repo: &mut git2::Repository,
        index: usize,
    ) -> std::result::Result<(), git2::Error> {
        repo.stash_drop(index)?;
        Ok(())
    }

    fn send_log(
        repo: &git2::Repository,
        _workdir: &Path,
        event_tx: &Sender<EditorEvent>,
        branch: Option<&str>,
        limit: usize,
    ) {
        use crabide_core::event::{CommitEntry, GitEvent};

        // Build a revwalk
        let mut revwalk = match repo.revwalk() {
            Ok(w) => w,
            Err(e) => {
                warn!("revwalk: {}", e.message());
                return;
            }
        };

        if revwalk
            .set_sorting(git2::Sort::TIME | git2::Sort::TOPOLOGICAL)
            .is_err()
        {
            warn!("revwalk set_sorting failed");
            return;
        }

        if let Some(branch_name) = branch {
            // Walk only the named branch's history
            let ref_spec = format!("refs/heads/{branch_name}");
            if revwalk.push_ref(&ref_spec).is_err() {
                // Try remote ref
                let remote_spec = format!("refs/remotes/{branch_name}");
                if revwalk.push_ref(&remote_spec).is_err() {
                    warn!("log: branch '{branch_name}' not found");
                    return;
                }
            }
        } else {
            // Walk all refs (reachable commits)
            if revwalk.push_glob("*").is_err() {
                warn!("log: no refs to walk");
                return;
            }
        }

        // Collect decorations (ref names) for each commit
        let mut decorations: HashMap<git2::Oid, Vec<String>> = HashMap::new();
        if let Ok(refdb) = repo.references() {
            for ref_result in refdb.flatten() {
                let name = ref_result.name().unwrap_or("").to_owned();
                if let Some(target) = ref_result.target() {
                    decorations.entry(target).or_default().push(name);
                }
            }
        }

        let mut entries: Vec<CommitEntry> = Vec::new();
        for oid_result in revwalk {
            let oid = match oid_result {
                Ok(o) => o,
                Err(e) => {
                    warn!("revwalk iteration: {}", e.message());
                    continue;
                }
            };

            let commit = match repo.find_commit(oid) {
                Ok(c) => c,
                Err(e) => {
                    warn!("find_commit {}: {}", oid, e.message());
                    continue;
                }
            };

            let hash = oid.to_string();
            let author = commit.author();
            let message = commit
                .summary()
                .ok()
                .flatten()
                .unwrap_or("(no message)")
                .to_owned();
            let parents: Vec<String> = commit.parents().map(|p| p.id().to_string()).collect();

            let ref_names = decorations.remove(&oid).unwrap_or_default();

            entries.push(CommitEntry {
                hash,
                author: author.name().unwrap_or("Unknown").to_owned(),
                author_email: author.email().unwrap_or("").to_owned(),
                author_time: author.when().seconds(),
                message,
                parents,
                ref_names,
            });

            if limit > 0 && entries.len() >= limit {
                break;
            }
        }

        let _ = event_tx.send(EditorEvent::Git(GitEvent::LogReady { entries }));
    }

    // ── Tag operations ─────────────────────────────────────────────────────

    fn send_tag_list(repo: &git2::Repository, event_tx: &Sender<EditorEvent>) {
        use crabide_core::event::{GitEvent, TagInfo};
        let tags = match repo.tag_names(None) {
            Ok(names) => names
                .iter()
                .filter_map(|name_res| {
                    let name = name_res.ok()??;
                    let oid = repo.refname_to_id(&format!("refs/tags/{name}")).ok()?;
                    let obj = repo.find_object(oid, None).ok()?;
                    let (message, tagger) = if let Some(tag) = obj.as_tag() {
                        (
                            tag.message().ok().flatten().map(|m| m.to_owned()),
                            tag.tagger().map(|t| t.name().unwrap_or("").to_owned()),
                        )
                    } else {
                        (None, None)
                    };
                    Some(TagInfo {
                        name: name.to_owned(),
                        commit: oid.to_string(),
                        message,
                        annotated: obj.as_tag().is_some(),
                        tagger,
                    })
                })
                .collect::<Vec<_>>(),
            Err(e) => {
                warn!("list tags: {}", e.message());
                Vec::new()
            }
        };
        let _ = event_tx.send(EditorEvent::Git(GitEvent::TagListed { tags }));
    }

    fn create_tag_impl(
        repo: &git2::Repository,
        event_tx: &Sender<EditorEvent>,
        name: &str,
        target: Option<&str>,
        message: Option<&str>,
    ) {
        use crabide_core::event::GitEvent;
        // Resolve target commit.
        let target_oid = if let Some(t) = target {
            match repo.revparse_single(t) {
                Ok(obj) => obj.id(),
                Err(e) => {
                    warn!("create tag: revparse {t}: {}", e.message());
                    let _ = event_tx.send(EditorEvent::Git(GitEvent::OperationFailed {
                        operation: format!("create tag {name}"),
                        error: e.message().to_owned(),
                    }));
                    return;
                }
            }
        } else {
            match repo.head() {
                Ok(head) => head.target().expect("HEAD should have a target Oid"),
                _ => {
                    let _ = event_tx.send(EditorEvent::Git(GitEvent::OperationFailed {
                        operation: format!("create tag {name}"),
                        error: "no HEAD to tag".into(),
                    }));
                    return;
                }
            }
        };

        let target_obj = match repo.find_object(target_oid, None) {
            Ok(o) => o,
            Err(e) => {
                warn!("create tag: find_object: {}", e.message());
                let _ = event_tx.send(EditorEvent::Git(GitEvent::OperationFailed {
                    operation: format!("create tag {name}"),
                    error: e.message().to_owned(),
                }));
                return;
            }
        };

        match message {
            Some(msg) => {
                let sig = match repo.signature() {
                    Ok(s) => s,
                    Err(e) => {
                        warn!("create tag: signature: {}", e.message());
                        let _ = event_tx.send(EditorEvent::Git(GitEvent::OperationFailed {
                            operation: format!("create tag {name}"),
                            error: e.message().to_owned(),
                        }));
                        return;
                    }
                };
                match repo.tag(name, &target_obj, &sig, msg, false) {
                    Ok(_) => {
                        let _ = event_tx
                            .send(EditorEvent::Git(GitEvent::TagCreated { name: name.into() }));
                    }
                    Err(e) => {
                        warn!("create annotated tag: {}", e.message());
                        let _ = event_tx.send(EditorEvent::Git(GitEvent::OperationFailed {
                            operation: format!("create tag {name}"),
                            error: e.message().to_owned(),
                        }));
                    }
                }
            }
            None => match repo.tag_lightweight(name, &target_obj, false) {
                Ok(_) => {
                    let _ =
                        event_tx.send(EditorEvent::Git(GitEvent::TagCreated { name: name.into() }));
                }
                Err(e) => {
                    warn!("create lightweight tag: {}", e.message());
                    let _ = event_tx.send(EditorEvent::Git(GitEvent::OperationFailed {
                        operation: format!("create tag {name}"),
                        error: e.message().to_owned(),
                    }));
                }
            },
        }
    }

    fn delete_tag_impl(
        repo: &git2::Repository,
        name: &str,
    ) -> std::result::Result<(), git2::Error> {
        let ref_name = format!("refs/tags/{name}");
        // Delete by writing null OID (the standard way to delete a reference in libgit2).
        let null_oid = git2::Oid::ZERO_SHA1;
        repo.reference(&ref_name, null_oid, true, "delete tag")?;
        Ok(())
    }

    // ── Remote operations ───────────────────────────────────────────────────

    fn send_remote_list(repo: &git2::Repository, event_tx: &Sender<EditorEvent>) {
        use crabide_core::event::{GitEvent, RemoteInfo};
        let remotes = match repo.remotes() {
            Ok(names) => names
                .iter()
                .filter_map(|name_res| {
                    let name = name_res.ok()??;
                    let remote = repo.find_remote(name).ok()?;
                    let url = remote.url().unwrap_or("").to_owned();
                    let push_url = remote.pushurl().ok().flatten().map(|u| u.to_owned());
                    Some(RemoteInfo {
                        name: name.to_owned(),
                        url,
                        push_url,
                    })
                })
                .collect::<Vec<_>>(),
            Err(e) => {
                warn!("list remotes: {}", e.message());
                Vec::new()
            }
        };
        let _ = event_tx.send(EditorEvent::Git(GitEvent::RemotesListed { remotes }));
    }

    fn add_remote_impl(
        repo: &git2::Repository,
        name: &str,
        url: &str,
    ) -> std::result::Result<(), git2::Error> {
        repo.remote(name, url)?;
        Ok(())
    }

    fn remove_remote_impl(
        repo: &git2::Repository,
        name: &str,
    ) -> std::result::Result<(), git2::Error> {
        repo.remote_delete(name)?;
        Ok(())
    }

    // ── Submodule operations ───────────────────────────────────────────

    fn send_submodule_list(repo: &git2::Repository, event_tx: &Sender<EditorEvent>) {
        use crabide_core::event::{GitEvent, SubmoduleInfo};
        use git2::SubmoduleIgnore;
        let sms: Vec<SubmoduleInfo> = match repo.submodules() {
            Ok(subs) => subs
                .iter()
                .filter_map(|sm| {
                    let path = sm.path().to_str()?.to_owned();
                    let name = sm.name().ok()?.to_owned();
                    let url = sm.url().ok()?.unwrap_or("").to_owned();
                    let branch = sm.branch().ok()?.map(|s| s.to_owned());
                    let commit = sm.head_id().map(|oid| oid.to_string()).unwrap_or_default();
                    // Get status via repo
                    let status = repo
                        .submodule_status(&name, SubmoduleIgnore::Unspecified)
                        .ok()
                        .unwrap_or(git2::SubmoduleStatus::empty());
                    let initialized = status.contains(git2::SubmoduleStatus::IN_HEAD)
                        || status.contains(git2::SubmoduleStatus::IN_INDEX)
                        || status.contains(git2::SubmoduleStatus::IN_CONFIG);
                    let has_changes = status.contains(git2::SubmoduleStatus::WD_MODIFIED)
                        || status.contains(git2::SubmoduleStatus::WD_INDEX_MODIFIED);
                    let cloned = status.contains(git2::SubmoduleStatus::IN_WD);
                    Some(SubmoduleInfo {
                        path,
                        url,
                        branch,
                        commit,
                        initialized,
                        has_changes,
                        cloned,
                    })
                })
                .collect(),
            Err(e) => {
                warn!("list submodules: {}", e.message());
                Vec::new()
            }
        };
        let _ = event_tx.send(EditorEvent::Git(GitEvent::SubmodulesListed {
            submodules: sms,
        }));
    }

    fn submodule_add_impl(
        repo: &mut git2::Repository,
        url: &str,
        path_str: &str,
        branch: Option<&str>,
    ) -> std::result::Result<(), git2::Error> {
        let mut sm = repo.submodule(url, path_str.as_ref(), false)?;
        if let Some(b) = branch {
            // Get the submodule name before mutating repo
            let sm_name = sm.name()?.to_owned();
            drop(sm); // release immutable borrow
            repo.submodule_set_branch(&sm_name, b)?;
            // Re-acquire the submodule
            let mut tmp = repo.find_submodule(&sm_name)?;
            let mut opts = git2::SubmoduleUpdateOptions::new();
            opts.allow_fetch(true);
            tmp.update(true, Some(&mut opts))?;
            tmp.add_to_index(true)?;
            tmp.add_finalize()?;
        } else {
            let mut opts = git2::SubmoduleUpdateOptions::new();
            opts.allow_fetch(true);
            sm.update(true, Some(&mut opts))?;
            sm.add_to_index(true)?;
            sm.add_finalize()?;
        }
        Ok(())
    }
    fn submodule_update_impl(
        repo: &git2::Repository,
        path: Option<&str>,
        init: bool,
        _recursive: bool,
    ) -> std::result::Result<Vec<String>, git2::Error> {
        let mut updated = Vec::new();
        if let Some(p) = path {
            let mut sm = repo.find_submodule(p)?;
            if init {
                sm.init(false)?;
            }
            let mut opts = git2::SubmoduleUpdateOptions::new();
            opts.allow_fetch(true);
            sm.update(false, Some(&mut opts))?;
            updated.push(p.to_owned());
        } else {
            // Update all submodules
            for sm_result in repo.submodules()? {
                let sm_path = sm_result.path().to_str().unwrap_or("").to_owned();
                let mut sm = repo.find_submodule(&sm_path)?;
                if init {
                    sm.init(false)?;
                }
                let mut opts = git2::SubmoduleUpdateOptions::new();
                opts.allow_fetch(true);
                sm.update(false, Some(&mut opts))?;
                updated.push(sm_path);
            }
        }
        Ok(updated)
    }

    fn submodule_sync_impl(
        repo: &git2::Repository,
        path: Option<&str>,
    ) -> std::result::Result<Vec<String>, git2::Error> {
        let mut synced = Vec::new();
        if let Some(p) = path {
            let mut sm = repo.find_submodule(p)?;
            sm.sync()?;
            synced.push(p.to_owned());
        } else {
            for sm_result in repo.submodules()? {
                let sm_path = sm_result.path().to_str().unwrap_or("").to_owned();
                let mut sm = repo.find_submodule(&sm_path)?;
                sm.sync()?;
                synced.push(sm_path);
            }
        }
        Ok(synced)
    }

    // ── Conflict resolution ─────────────────────────────────────────────

    fn list_conflicts_impl(repo: &git2::Repository) -> Vec<crabide_core::event::ConflictInfo> {
        use crabide_core::event::ConflictInfo;
        let index = match repo.index() {
            Ok(idx) => idx,
            Err(e) => {
                warn!("list_conflicts: index: {}", e.message());
                return Vec::new();
            }
        };

        if !index.has_conflicts() {
            return Vec::new();
        }

        let mut conflicts: Vec<ConflictInfo> = Vec::new();
        if let Ok(iter) = index.conflicts() {
            for entry_result in iter {
                let conflict = match entry_result {
                    Ok(c) => c,
                    Err(e) => {
                        warn!("conflict entry: {}", e.message());
                        continue;
                    }
                };
                // Get the path from whichever stage is present.
                let path = conflict
                    .ancestor
                    .as_ref()
                    .or(conflict.our.as_ref())
                    .or(conflict.their.as_ref())
                    .map(|e| std::str::from_utf8(&e.path).unwrap_or("").to_owned())
                    .unwrap_or_default();
                if path.is_empty() {
                    continue;
                }
                conflicts.push(ConflictInfo {
                    path,
                    ancestor_oid: conflict.ancestor.as_ref().map(|e| e.id.to_string()),
                    ours_oid: conflict.our.as_ref().map(|e| e.id.to_string()),
                    theirs_oid: conflict.their.as_ref().map(|e| e.id.to_string()),
                });
            }
        }
        conflicts
    }

    fn resolve_ours_impl(
        repo: &git2::Repository,
        path: &str,
    ) -> std::result::Result<(), git2::Error> {
        // Checkout our version (stage 2) from the index
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.force().path(std::path::Path::new(path));
        // Use checkout with conflict style to write our version
        repo.checkout_head(Some(&mut checkout))?;

        // Remove conflict entries from index
        let mut index = repo.index()?;
        index.conflict_remove(std::path::Path::new(path))?;
        // Re-add the file as staged (ours version)
        index.add_path(std::path::Path::new(path))?;
        index.write()?;
        Ok(())
    }

    fn resolve_theirs_impl(
        repo: &git2::Repository,
        path: &str,
    ) -> std::result::Result<(), git2::Error> {
        // Write the theirs blob content to the working tree
        checkout_theirs_blob(repo, path)?;
        // Remove conflict entries and stage the result
        let mut index = repo.index()?;
        index.conflict_remove(std::path::Path::new(path))?;
        index.add_path(std::path::Path::new(path))?;
        index.write()?;
        Ok(())
    }

    fn mark_resolved_impl(
        repo: &git2::Repository,
        path: &str,
    ) -> std::result::Result<(), git2::Error> {
        let mut index = repo.index()?;
        index.conflict_remove(std::path::Path::new(path))?;
        // Re-add the working tree version (the user's manually resolved version)
        index.add_path(std::path::Path::new(path))?;
        index.write()?;
        Ok(())
    }

    /// Helper: write the stage 3 (theirs) blob content to the working tree.
    fn checkout_theirs_blob(
        repo: &git2::Repository,
        path: &str,
    ) -> std::result::Result<(), git2::Error> {
        let index = repo.index()?;
        // Find the theirs entry (stage 3)
        let conflict_entries: Vec<_> = index.conflicts()?.filter_map(|r| r.ok()).collect();
        for conflict in &conflict_entries {
            let entry_path = conflict
                .ancestor
                .as_ref()
                .or(conflict.our.as_ref())
                .or(conflict.their.as_ref())
                .map(|e| std::str::from_utf8(&e.path).unwrap_or(""))
                .unwrap_or("");
            if entry_path != path {
                continue;
            }
            if let Some(theirs_entry) = &conflict.their {
                let blob = repo.find_blob(theirs_entry.id)?;
                if blob.is_binary() {
                    return Err(git2::Error::from_str("cannot checkout binary blob"));
                }
                let content = blob.content();
                let workdir = repo
                    .workdir()
                    .ok_or_else(|| git2::Error::from_str("no workdir"))?;
                let full_path = workdir.join(path);
                if let Some(parent) = full_path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| git2::Error::from_str(&format!("create dir: {e}")))?;
                }
                std::fs::write(&full_path, content)
                    .map_err(|e| git2::Error::from_str(&format!("write file: {e}")))?;
                return Ok(());
            }
        }
        Err(git2::Error::from_str("no theirs entry found for path"))
    }
}
