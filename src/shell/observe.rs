#![cfg(not(tarpaulin_include))]

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::core::types::{
  CiStatus, ListOptions, ObservedListRow, ObservedListState, ObservedState, Workspace,
  WorkspaceDetails,
};
use crate::shell::fs::Fs;
use crate::shell::jj::Jj;

/// Gather the repo state needed for a `switch` or `remove` operation.
pub fn observe<J: Jj, F: Fs>(
  jj: &J,
  fs: &F,
  start_dir: &Path,
  target_name: Option<&str>,
  worktree_path_template: &str,
) -> Result<ObservedState> {
  let repo_root = match jj.repo_root(start_dir) {
    Ok(r) => r,
    Err(_) => {
      return Ok(ObservedState {
        repo_root: start_dir.to_path_buf(),
        is_jj_repo: false,
        workspaces: vec![],
        current_workspace: None,
        target_path_exists: false,
        target_workspace_dirty: false,
        target_bookmark_merged: false,
        target_bookmark_exists: false,
        target_resolved_workspace: None,
        trunk_bookmark: None,
      });
    }
  };

  let workspaces = jj.workspace_list(&repo_root)?;
  let cwd_canon = std::fs::canonicalize(start_dir).unwrap_or_else(|_| start_dir.to_path_buf());
  let current_workspace = pick_current_workspace(&cwd_canon, &workspaces);

  let mut target_path_exists = false;
  let mut target_workspace_dirty = false;
  let mut target_bookmark_merged = false;
  let mut target_bookmark_exists = false;
  let mut target_resolved_workspace = None;

  if let Some(name) = target_name {
    let target_path = {
      let ctx = crate::core::types::RenderContext {
        branch: name.into(),
        repo: repo_root
          .file_name()
          .map(|n| n.to_string_lossy().into_owned()),
        repo_path: Some(repo_root.clone()),
        ..Default::default()
      };
      let rendered = crate::core::template::render(worktree_path_template, &ctx)?;

      repo_root.join(rendered)
    };

    target_path_exists = fs.exists(&target_path);

    if workspaces.iter().any(|w| w.name == name) {
      target_workspace_dirty = jj.workspace_is_dirty(&repo_root, name)?;
    } else if let Ok(Some(trunk)) = jj.trunk_bookmark(&repo_root)
      && trunk == name
      && workspaces.iter().any(|w| w.name == "default")
    {
      target_resolved_workspace = Some("default".to_string());
    }

    target_bookmark_exists = jj.bookmark_exists(&repo_root, name)?;
    if target_bookmark_exists {
      target_bookmark_merged = jj.bookmark_is_merged_into_trunk(&repo_root, name)?;
    }
  }

  let trunk_bookmark = jj.trunk_bookmark(&repo_root).unwrap_or(None);

  Ok(ObservedState {
    repo_root,
    is_jj_repo: true,
    workspaces,
    current_workspace,
    target_path_exists,
    target_workspace_dirty,
    target_bookmark_merged,
    target_bookmark_exists,
    target_resolved_workspace,
    trunk_bookmark,
  })
}

/// Gather everything needed for `jjwt list`. One pass; sequential per-workspace
/// `jj` calls. For typical workspace counts (N ≤ ~10) the latency is fine.
///
/// `opts.include_bookmarks` triggers a `jj bookmark list` to collect names
/// of local bookmarks that don't have a corresponding workspace.
/// `opts.include_remotes` collects remote-only bookmarks (local doesn't
/// exist; we already know `bookmarks_with_remote()` returns the local
/// names that do have a remote variant — the *remote-only* set is the
/// difference of `--all-remotes` ∖ `local`).
pub fn observe_list<J: Jj + Sync, F: Fs>(
  jj: &J,
  _fs: &F,
  start_dir: &Path,
  opts: crate::core::types::ListOptions,
) -> Result<ObservedListState> {
  let repo_root = match jj.repo_root(start_dir) {
    Ok(r) => r,
    Err(_) => {
      return Ok(ObservedListState {
        repo_root: start_dir.to_path_buf(),
        is_jj_repo: false,
        ..Default::default()
      });
    }
  };

  let workspaces = jj.workspace_list(&repo_root)?;
  let cwd_canon = std::fs::canonicalize(start_dir).unwrap_or_else(|_| start_dir.to_path_buf());

  let current_workspace = pick_current_workspace(&cwd_canon, &workspaces);
  let ws_names: Vec<String> = workspaces.iter().map(|w| w.name.clone()).collect();

  // Run three independent batch queries in parallel:
  // 1. bookmark_sets (one jj call)
  // 2. workspace_commit_info_batch (one jj call — commit metadata + diff stats + conflicts)
  // 3. workspace_ahead_behind_batch (two jj calls — ahead + behind)
  // Plus per-workspace status queries (one jj call each, for modified/untracked).
  let (bookmark_result, commit_result, ahead_behind_result, status_results) =
    std::thread::scope(|s| {
      let repo = &repo_root;
      let names = &ws_names;

      let bm_handle = s.spawn(move || jj.bookmark_sets(repo));
      let ci_handle = s.spawn(move || jj.workspace_commit_info_batch(repo, names));
      let ab_handle = s.spawn(move || jj.workspace_ahead_behind_batch(repo, names));

      // Per-workspace status queries in parallel.
      let status_handles: Vec<_> = workspaces
        .iter()
        .map(|w| s.spawn(move || (w.name.clone(), jj.workspace_status(repo, &w.name))))
        .collect();

      let status_results: Vec<_> = status_handles
        .into_iter()
        .map(|h| h.join().expect("status query thread panicked"))
        .collect();

      (
        bm_handle.join().expect("bookmark query thread panicked"),
        ci_handle.join().expect("commit info query thread panicked"),
        ab_handle
          .join()
          .expect("ahead-behind query thread panicked"),
        status_results,
      )
    });

  let (all_local, remote_set) = bookmark_result.unwrap_or_default();
  let commit_infos = commit_result?;
  let ahead_behinds = ahead_behind_result?;

  let rows = build_list_rows(
    &workspaces,
    &commit_infos,
    &ahead_behinds,
    &status_results,
    &remote_set,
  );

  let (extra_bookmark_names, extra_remote_only_names) =
    collect_extra_bookmarks(&opts, &all_local, &remote_set, &workspaces);

  Ok(ObservedListState {
    repo_root,
    is_jj_repo: true,
    current_workspace,
    rows,
    extra_bookmark_names,
    extra_remote_only_names,
    full: opts.full,
  })
}

/// Build one `ObservedListRow` per workspace from the parallel query results.
fn build_list_rows(
  workspaces: &[Workspace],
  commit_infos: &std::collections::HashMap<String, crate::core::types::CommitInfo>,
  ahead_behinds: &std::collections::HashMap<String, (u32, u32)>,
  status_results: &[(String, Result<(bool, bool)>)],
  remote_set: &std::collections::HashSet<String>,
) -> Vec<ObservedListRow> {
  let mut rows = Vec::with_capacity(workspaces.len());

  for (i, w) in workspaces.iter().enumerate() {
    let ci = commit_infos.get(&w.name).cloned().unwrap_or_default();
    let (ahead, behind) = ahead_behinds.get(&w.name).copied().unwrap_or((0, 0));
    let (modified, untracked) = match &status_results[i].1 {
      Ok(s) => *s,
      Err(_) => (false, false),
    };
    let has_remote_bookmark = remote_set.contains(&w.name);

    rows.push(ObservedListRow {
      workspace: w.clone(),
      details: WorkspaceDetails {
        modified,
        untracked,
        conflicts: ci.conflicts,
        commit_short: ci.commit_short,
        age_seconds: ci.age_seconds,
        message_first_line: ci.message_first_line,
        head_added: ci.head_added,
        head_removed: ci.head_removed,
      },
      ahead,
      behind,
      has_remote_bookmark,
      ci_status: CiStatus::None,
      summary: String::new(),
    });
  }

  rows
}

/// Collect extra bookmark and remote-only names that don't correspond to any
/// workspace. Returns `(extra_bookmark_names, extra_remote_only_names)`.
fn collect_extra_bookmarks(
  opts: &ListOptions,
  all_local: &[String],
  remote_set: &std::collections::HashSet<String>,
  workspaces: &[Workspace],
) -> (Vec<String>, Vec<String>) {
  let ws_name_set: std::collections::HashSet<&str> =
    workspaces.iter().map(|w| w.name.as_str()).collect();

  let extra_bookmark_names = if opts.include_bookmarks {
    all_local
      .iter()
      .filter(|n| !ws_name_set.contains(n.as_str()))
      .cloned()
      .collect()
  } else {
    Vec::new()
  };

  let extra_remote_only_names = if opts.include_remotes {
    let local_set: std::collections::HashSet<&str> = all_local.iter().map(|s| s.as_str()).collect();

    remote_set
      .iter()
      .filter(|n| !local_set.contains(n.as_str()))
      .cloned()
      .collect()
  } else {
    Vec::new()
  };

  (extra_bookmark_names, extra_remote_only_names)
}

/// Gather workspace states needed for the `prune` command.
pub fn observe_prune<J: Jj + Sync, F: Fs>(
  jj: &J,
  _fs: &F,
  start_dir: &Path,
) -> Result<crate::core::types::ObservedPruneState> {
  use crate::core::types::ObservedPruneState;

  let repo_root = match jj.repo_root(start_dir) {
    Ok(r) => r,
    Err(_) => {
      return Ok(ObservedPruneState {
        repo_root: start_dir.to_path_buf(),
        is_jj_repo: false,
        ..Default::default()
      });
    }
  };

  let workspaces = jj.workspace_list(&repo_root)?;
  let cwd_canon = std::fs::canonicalize(start_dir).unwrap_or_else(|_| start_dir.to_path_buf());
  let current_workspace = pick_current_workspace(&cwd_canon, &workspaces);

  // Per-workspace prune queries in parallel (mirrors observe_list pattern).
  let workspace_status = std::thread::scope(|s| {
    let repo = &repo_root;

    let handles: Vec<_> = workspaces
      .iter()
      .map(|w| {
        s.spawn(move || {
          let bm_exists = jj.bookmark_exists(repo, &w.name)?;

          let bm_merged = if bm_exists {
            jj.bookmark_is_merged_into_trunk(repo, &w.name)?
          } else {
            false
          };

          let dirty = jj.workspace_is_dirty(repo, &w.name)?;

          Ok::<_, anyhow::Error>((w.name.clone(), bm_exists, bm_merged, dirty))
        })
      })
      .collect();

    handles
      .into_iter()
      .map(|h| h.join().expect("prune query thread panicked"))
      .collect::<Result<Vec<_>>>()
  })?;

  Ok(ObservedPruneState {
    repo_root,
    is_jj_repo: true,
    current_workspace,
    workspaces,
    workspace_status,
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

      if best.as_ref().is_none_or(|(d, _)| depth > *d) {
        best = Some((depth, w.name.clone()));
      }
    }
  }

  best.map(|(_, n)| n)
}
