#![cfg(not(tarpaulin_include))]

use anyhow::Result;
use std::path::Path;

use crate::core::types::{CommitInfo, Workspace};

/// Abstraction over jj operations for testability.
pub trait Jj {
  /// Detect the repo root (parent of `.jj/`); errors if not in a jj repo.
  fn repo_root(&self, start: &Path) -> Result<std::path::PathBuf>;
  /// Enumerate workspaces with name, path, and stale flag.
  fn workspace_list(&self, repo_root: &Path) -> Result<Vec<Workspace>>;
  /// `jj workspace add --name <name> <path>`, optionally checking out a
  /// specific revision instead of the root changeset. When `edit_in_place`
  /// is true, the new workspace's `@` is set directly onto `revision`
  /// (jj edit) rather than a new empty child commit on top (jj new).
  fn workspace_add(
    &self,
    repo_root: &Path,
    name: &str,
    path: &Path,
    revision: Option<&str>,
    edit_in_place: bool,
  ) -> Result<()>;
  /// `jj util snapshot` in the workspace at `path`, after
  /// `jj workspace update-stale` when `stale`. A missing directory is a
  /// no-op; any jj failure is an error.
  fn workspace_snapshot(&self, path: &Path, stale: bool) -> Result<()>;
  /// `jj workspace forget <name>`
  fn workspace_forget(&self, repo_root: &Path, name: &str) -> Result<()>;
  /// `jj workspace update-stale` for the named workspace
  fn workspace_update_stale(&self, repo_root: &Path, name: &str) -> Result<()>;
  /// True if a bookmark with this name exists.
  fn bookmark_exists(&self, repo_root: &Path, name: &str) -> Result<bool>;
  /// For a bookmark candidate for in-place adoption, report
  /// `(is_empty, is_occupied)`: whether its target commit is empty, and
  /// whether that commit is already some workspace's working copy.
  /// `(false, false)` if the bookmark is absent.
  fn bookmark_commit_state(&self, repo_root: &Path, name: &str) -> Result<(bool, bool)>;
  /// Per-workspace status flags (modified, untracked) for list rendering.
  /// Only collects data that cannot be obtained from
  /// [`Jj::workspace_commit_info_batch`] (which handles commit metadata,
  /// conflicts, and diff stats).
  fn workspace_status(&self, repo_root: &Path, workspace: &str) -> Result<(bool, bool)>;
  /// Batch-fetch commit metadata, conflict status, and diff stats for all
  /// named workspaces in a single `jj log` call. Returns a map keyed by
  /// workspace name. Uses `\x1e` record separator to handle multi-line
  /// `diff.stat()` output.
  fn workspace_commit_info_batch(
    &self,
    repo_root: &Path,
    workspaces: &[String],
  ) -> Result<std::collections::HashMap<String, CommitInfo>>;
  /// Commits ahead/behind trunk for the given workspace's `@`.
  /// Returns `(ahead, behind)`.
  fn workspace_ahead_behind_trunk(&self, repo_root: &Path, workspace: &str) -> Result<(u32, u32)>;
  /// Batch-fetch ahead/behind counts for all named workspaces in two
  /// `jj log` calls (one for ahead, one for behind). Returns a map keyed
  /// by workspace name → `(ahead, behind)`.
  fn workspace_ahead_behind_batch(
    &self,
    repo_root: &Path,
    workspaces: &[String],
  ) -> Result<std::collections::HashMap<String, (u32, u32)>>;
  /// Local bookmarks in `trunk()..@` for each named workspace, nearest `@`
  /// first. Empty for every workspace when `trunk()` is `root()`.
  fn workspace_bookmarks_batch(
    &self,
    repo_root: &Path,
    workspaces: &[String],
  ) -> Result<std::collections::HashMap<String, Vec<crate::core::types::WorkspaceBookmark>>>;
  /// All local bookmark names (one entry per bookmark, no `@<remote>`
  /// suffix). Used for shell completion.
  fn bookmarks_local(&self, repo_root: &Path) -> Result<Vec<String>>;
  /// What `trunk()` resolves to, or `None` when it is `root()`. Its name
  /// lets `switch <default-branch>` route to the default workspace.
  fn trunk(&self, repo_root: &Path) -> Result<Option<crate::core::types::Trunk>>;
  /// Run `jj git fetch` to update remote refs.
  fn git_fetch(&self, repo_root: &Path) -> Result<()>;
  /// Rename a workspace.
  fn workspace_rename(&self, repo_root: &Path, old: &str, new: &str) -> Result<()>;
}

/// Walk up from `start` to find the nearest directory containing `.jj/`.
/// Returns `None` if no `.jj/` directory is found.
pub(crate) fn find_nearest_jj_dir(start: &Path) -> Option<std::path::PathBuf> {
  let mut p = start.to_path_buf();

  loop {
    if p.join(".jj").is_dir() {
      return Some(p);
    }

    if !p.pop() {
      return None;
    }
  }
}

/// Walk up from `start` to find the repo root (parent of `.jj/`).
/// When inside a non-default workspace, follows the `.jj/repo` pointer
/// to return the main repo root.
pub(crate) fn find_repo_root(start: &Path) -> Result<std::path::PathBuf> {
  let p = find_nearest_jj_dir(start)
    .ok_or_else(|| anyhow::anyhow!("not inside a jj repo (no .jj/ found above {start:?})"))?;

  let jj_dir = p.join(".jj");
  let marker = jj_dir.join("repo");

  // In a non-default workspace, `.jj/repo` is a file whose
  // contents point to the main repo's `.jj/repo` directory.
  // In the main repo, `.jj/repo` is itself a directory. Follow
  // the pointer so callers always get the main repo root.
  if marker.is_file() {
    let content = std::fs::read_to_string(&marker)
      .map_err(|e| anyhow::anyhow!("failed to read {marker:?}: {e}"))?;
    let target = std::path::PathBuf::from(content.trim());

    let resolved = if target.is_absolute() {
      target
    } else {
      jj_dir.join(target)
    };

    let canonical = std::fs::canonicalize(&resolved)
      .map_err(|e| anyhow::anyhow!("failed to resolve {resolved:?}: {e}"))?;
    let main_root = canonical
      .parent()
      .and_then(|p| p.parent())
      .ok_or_else(|| anyhow::anyhow!("invalid repo pointer in {marker:?}"))?
      .to_path_buf();

    return Ok(main_root);
  }

  Ok(p)
}

/// Compute the path to a workspace directory by rendering the worktree-path
/// template. For "default", this is always `repo_root`.
pub(crate) fn workspace_dir(repo_root: &Path, name: &str, template: &str) -> std::path::PathBuf {
  if name == "default" {
    return repo_root.to_path_buf();
  }

  let ctx = crate::core::types::RenderContext {
    branch: name.into(),
    repo: repo_root
      .file_name()
      .map(|n| n.to_string_lossy().into_owned()),
    repo_path: Some(repo_root.to_path_buf()),
    ..Default::default()
  };

  crate::core::template::render(template, &ctx)
    .map(|rendered| repo_root.join(rendered))
    .unwrap_or_else(|_| repo_root.join(name))
}
