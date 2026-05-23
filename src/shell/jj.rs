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
  /// `jj workspace add --name <name> <path>`
  fn workspace_add(&self, repo_root: &Path, name: &str, path: &Path) -> Result<()>;
  /// `jj workspace forget <name>`
  fn workspace_forget(&self, repo_root: &Path, name: &str) -> Result<()>;
  /// `jj workspace update-stale` for the named workspace
  fn workspace_update_stale(&self, repo_root: &Path, name: &str) -> Result<()>;
  /// Create a bookmark at the named workspace's `@`.
  fn bookmark_create(&self, repo_root: &Path, name: &str, workspace: &str) -> Result<()>;
  /// `jj bookmark delete <name>`
  fn bookmark_delete(&self, repo_root: &Path, name: &str) -> Result<()>;
  /// True if a bookmark with this name exists.
  fn bookmark_exists(&self, repo_root: &Path, name: &str) -> Result<bool>;
  /// True if the bookmark's target is an ancestor of trunk.
  fn bookmark_is_merged_into_trunk(&self, repo_root: &Path, name: &str) -> Result<bool>;
  /// True if `jj status` for the workspace shows any uncommitted changes.
  fn workspace_is_dirty(&self, repo_root: &Path, workspace: &str) -> Result<bool>;
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
  /// Set of local bookmark names that have at least one remote-tracking
  /// variant (e.g. `name@origin`). Used to decide whether to render the
  /// "tracks remote" glyph in the list view.
  fn bookmarks_with_remote(&self, repo_root: &Path) -> Result<std::collections::HashSet<String>>;
  /// All local bookmark names (one entry per bookmark, no `@<remote>`
  /// suffix). Used by `list --bookmarks` and `--remotes` to discover
  /// bookmarks that don't have an associated workspace.
  fn bookmarks_local(&self, repo_root: &Path) -> Result<Vec<String>>;
  /// All bookmark sets in one call. Returns `(all_local, with_remote)`.
  /// `all_local` is every local bookmark name; `with_remote` is the subset
  /// that has at least one remote-tracking variant.
  fn bookmark_sets(
    &self,
    repo_root: &Path,
  ) -> Result<(Vec<String>, std::collections::HashSet<String>)>;
  /// Name of the bookmark at `trunk()`, if any (typically "main" or "master").
  /// Used so `switch <default-branch>` routes to the default workspace.
  fn trunk_bookmark(&self, repo_root: &Path) -> Result<Option<String>>;
  /// Run `jj git fetch` to update remote refs.
  fn git_fetch(&self, repo_root: &Path) -> Result<()>;
  /// Rename a workspace.
  fn workspace_rename(&self, repo_root: &Path, old: &str, new: &str) -> Result<()>;
  /// Rename a bookmark (create new at old's target, then delete old).
  fn bookmark_rename(&self, repo_root: &Path, old: &str, new: &str) -> Result<()>;
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

/// Compute the path to a workspace directory. For "default", this is repo_root;
/// for other workspaces, it is repo_root/.worktrees/<name>.
pub(crate) fn workspace_dir(repo_root: &Path, name: &str) -> std::path::PathBuf {
  if name == "default" {
    repo_root.to_path_buf()
  } else {
    repo_root.join(".worktrees").join(name)
  }
}
