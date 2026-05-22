use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::core::types::{ObservedListRow, ObservedListState, ObservedState};
use crate::shell::fs::Fs;
use crate::shell::jj::Jj;

pub fn observe<J: Jj, F: Fs>(
  jj: &J,
  fs: &F,
  start_dir: &Path,
  target_name: Option<&str>,
) -> Result<ObservedState> {
  let repo_root = match jj.repo_root(start_dir) {
    Ok(r) => r,
    Err(_) => {
      return Ok(ObservedState {
        repo_root: start_dir.to_path_buf(),
        is_jj_repo: false,
        workspaces: vec![],
        target_path_exists: false,
        target_workspace_dirty: false,
        target_bookmark_merged: false,
        target_bookmark_exists: false,
        target_resolved_workspace: None,
      });
    }
  };

  let workspaces = jj.workspace_list(&repo_root)?;

  let mut target_path_exists = false;
  let mut target_workspace_dirty = false;
  let mut target_bookmark_merged = false;
  let mut target_bookmark_exists = false;
  let mut target_resolved_workspace = None;

  if let Some(name) = target_name {
    let target_path = repo_root.join(".worktrees").join(name);
    target_path_exists = fs.exists(&target_path);

    if workspaces.iter().any(|w| w.name == name) {
      target_workspace_dirty = jj.workspace_is_dirty(&repo_root, name)?;
    } else if let Ok(Some(trunk)) = jj.trunk_bookmark(&repo_root) {
      if trunk == name && workspaces.iter().any(|w| w.name == "default") {
        target_resolved_workspace = Some("default".to_string());
      }
    }

    target_bookmark_exists = jj.bookmark_exists(&repo_root, name)?;
    if target_bookmark_exists {
      target_bookmark_merged = jj.bookmark_is_merged_into_trunk(&repo_root, name)?;
    }
  }

  Ok(ObservedState {
    repo_root,
    is_jj_repo: true,
    workspaces,
    target_path_exists,
    target_workspace_dirty,
    target_bookmark_merged,
    target_bookmark_exists,
    target_resolved_workspace,
  })
}

/// Gather everything needed for `jjwt list`. One pass; sequential per-workspace
/// `jj` calls. For typical workspace counts (N ≤ ~10) the latency is fine.
pub fn observe_list<J: Jj, F: Fs>(jj: &J, _fs: &F, start_dir: &Path) -> Result<ObservedListState> {
  let repo_root = match jj.repo_root(start_dir) {
    Ok(r) => r,
    Err(_) => {
      return Ok(ObservedListState {
        repo_root: start_dir.to_path_buf(),
        is_jj_repo: false,
        current_workspace: None,
        rows: vec![],
      });
    }
  };

  let workspaces = jj.workspace_list(&repo_root)?;
  let cwd_canon = std::fs::canonicalize(start_dir).unwrap_or_else(|_| start_dir.to_path_buf());

  let current_workspace = pick_current_workspace(&cwd_canon, &workspaces);
  let remote_set = jj.bookmarks_with_remote(&repo_root).unwrap_or_default();
  let mut rows = Vec::with_capacity(workspaces.len());

  for w in &workspaces {
    let details = jj.workspace_details(&repo_root, &w.name)?;
    let (ahead, behind) = jj.workspace_ahead_behind_trunk(&repo_root, &w.name)?;
    let has_remote_bookmark = remote_set.contains(&w.name);

    rows.push(ObservedListRow {
      workspace: w.clone(),
      details,
      ahead,
      behind,
      has_remote_bookmark,
    });
  }

  Ok(ObservedListState {
    repo_root,
    is_jj_repo: true,
    current_workspace,
    rows,
  })
}

/// Pick the workspace whose canonical path is an ancestor of `cwd` with the
/// deepest match. Returns the workspace name, or `None` if cwd is not inside
/// any known workspace.
fn pick_current_workspace(
  cwd: &Path,
  workspaces: &[crate::core::types::Workspace],
) -> Option<String> {
  let mut best: Option<(usize, String)> = None;

  for w in workspaces {
    let ws_canon: PathBuf = std::fs::canonicalize(&w.path).unwrap_or_else(|_| w.path.clone());

    if cwd.starts_with(&ws_canon) {
      let depth = ws_canon.components().count();

      if best.as_ref().map(|(d, _)| depth > *d).unwrap_or(true) {
        best = Some((depth, w.name.clone()));
      }
    }
  }

  best.map(|(_, n)| n)
}
