use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use futures::StreamExt as _;
use jj_lib::backend::CommitId;
use jj_lib::commit::Commit;
use jj_lib::config::{ConfigLayer, ConfigSource, StackedConfig};
use jj_lib::matchers::EverythingMatcher;
use jj_lib::op_store::RefTarget;
use jj_lib::ref_name::{RefName, WorkspaceName, WorkspaceNameBuf};
use jj_lib::repo::{ReadonlyRepo, Repo as _, StoreFactories};
use jj_lib::settings::UserSettings;
use jj_lib::workspace::{Workspace, default_working_copy_factories};

use crate::core::types::{self, CommitInfo};
use crate::shell::jj::Jj;

/// In-process jj backend using jj-lib. Loads the repo once and answers all
/// queries from memory — no subprocess spawning.
pub struct JjLib {
  repo: RwLock<Arc<ReadonlyRepo>>,
  repo_root: PathBuf,
}

impl JjLib {
  pub fn new(start: &Path) -> Result<Self> {
    let repo_root = find_repo_root(start)?;
    let settings = minimal_settings()?;
    let store_factories = StoreFactories::default();
    let wc_factories = default_working_copy_factories();

    let workspace = Workspace::load(&settings, &repo_root, &store_factories, &wc_factories)
      .map_err(|e| anyhow::anyhow!("failed to load workspace: {e}"))?;

    // Trigger a working-copy snapshot for every workspace so the repo
    // reflects current disk state. Every jj command does this for the
    // current workspace; we do all of them since `list` reads from all.
    {
      let pre_repo = pollster::block_on(workspace.repo_loader().load_at_head())
        .map_err(|e| anyhow::anyhow!("failed to load repo: {e}"))?;
      let ws_dirs: Vec<PathBuf> = pre_repo
        .view()
        .wc_commit_ids()
        .keys()
        .map(|ws| workspace_dir(&repo_root, ws.as_str()))
        .collect();

      for dir in &ws_dirs {
        if dir.is_dir() {
          trigger_snapshot(dir);
        }
      }
    }

    // Load repo after snapshot — the op head may have changed.
    let repo = pollster::block_on(workspace.repo_loader().load_at_head())
      .map_err(|e| anyhow::anyhow!("failed to load repo: {e}"))?;

    Ok(Self {
      repo: RwLock::new(repo),
      repo_root,
    })
  }

  /// Borrow the current repo snapshot.
  fn repo(&self) -> Arc<ReadonlyRepo> {
    self.repo.read().unwrap().clone()
  }

  /// Find the trunk bookmark name by scanning for main/master/trunk with
  /// a remote-tracking ref. Returns None if no match found.
  fn resolve_trunk(&self) -> Option<String> {
    let repo = self.repo();
    let view = repo.view();

    // Collect names that have at least one remote-tracking ref.
    let remote_names: HashSet<String> = view
      .all_remote_bookmarks()
      .map(|(sym, _)| sym.name.as_str().to_string())
      .collect();

    // Priority order matches jj's default `trunk()` alias.
    for candidate in &["main", "master", "trunk"] {
      let name = RefName::new(candidate);

      if view.get_local_bookmark(name).is_present() && remote_names.contains(*candidate) {
        return Some(candidate.to_string());
      }
    }

    // Fallback: any bookmark with a remote-tracking ref.
    for (name, target) in view.local_bookmarks() {
      if target.is_present() && remote_names.contains(name.as_str()) {
        return Some(name.as_str().to_string());
      }
    }

    None
  }

  /// Map an external workspace name (which may be the trunk bookmark name
  /// for the default workspace) back to jj's internal workspace name.
  fn internal_ws_name(&self, name: &str) -> String {
    // If this name matches the trunk bookmark, it's the default workspace.
    if let Some(trunk) = self.resolve_trunk() {
      if name == trunk {
        return "default".to_string();
      }
    }

    name.to_string()
  }

  /// Resolve a workspace name to its working-copy CommitId.
  fn wc_commit_id(&self, workspace: &str) -> Result<CommitId> {
    let repo = self.repo();
    let internal = self.internal_ws_name(workspace);
    let ws_name = WorkspaceName::new(&internal);

    repo
      .view()
      .get_wc_commit_id(ws_name)
      .cloned()
      .ok_or_else(|| anyhow::anyhow!("workspace '{workspace}' not found"))
  }

  /// Load a Commit by its CommitId.
  fn get_commit(&self, id: &CommitId) -> Result<Commit> {
    let repo = self.repo();

    repo
      .store()
      .get_commit(id)
      .map_err(|e| anyhow::anyhow!("failed to load commit: {e}"))
  }

  /// Resolve the trunk bookmark to a CommitId. Returns None if no trunk.
  fn trunk_commit_id(&self) -> Option<CommitId> {
    let trunk_name = self.resolve_trunk()?;
    let repo = self.repo();
    let name = RefName::new(&trunk_name);

    repo.view().get_local_bookmark(name).as_normal().cloned()
  }

  /// Count commits in `roots..heads` using revset walk_revs.
  fn count_between(&self, roots: &[CommitId], heads: &[CommitId]) -> Result<u32> {
    let repo = self.repo();

    let revset = jj_lib::revset::walk_revs(&*repo, heads, roots)
      .map_err(|e| anyhow::anyhow!("walk_revs failed: {e}"))?;

    let stream = revset.stream();

    futures::pin_mut!(stream);

    let mut count = 0u32;

    while let Some(item) = pollster::block_on(stream.next()) {
      let _ = item.map_err(|e| anyhow::anyhow!("revset stream error: {e}"))?;
      count += 1;
    }

    Ok(count)
  }

  /// Replace the stored repo after a write transaction.
  fn swap_repo(&self, new_repo: Arc<ReadonlyRepo>) {
    let mut guard = self.repo.write().unwrap();

    *guard = new_repo;
  }
}

impl Jj for JjLib {
  fn repo_root(&self, _start: &Path) -> Result<PathBuf> {
    Ok(self.repo_root.clone())
  }

  fn workspace_list(&self, _repo_root: &Path) -> Result<Vec<types::Workspace>> {
    let repo = self.repo();
    let wc_ids = repo.view().wc_commit_ids();
    let trunk_name = self.resolve_trunk();
    let mut workspaces = Vec::with_capacity(wc_ids.len());

    for (ws_name, _commit_id) in wc_ids {
      let internal_name = ws_name.as_str().to_string();
      let path = workspace_dir(&self.repo_root, &internal_name);

      // Display the default workspace using the trunk bookmark name
      // (e.g. "master" or "main") to match worktrunk's behavior.
      let display_name = if internal_name == "default" {
        trunk_name.clone().unwrap_or(internal_name)
      } else {
        internal_name
      };

      workspaces.push(types::Workspace {
        name: display_name,
        path,
        stale: false,
      });
    }

    Ok(workspaces)
  }

  fn workspace_add(&self, _repo_root: &Path, name: &str, path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)
      .map_err(|e| anyhow::anyhow!("failed to create workspace dir: {e}"))?;

    let repo = self.repo();
    let repo_path = self.repo_root.join(".jj").join("repo");

    let (_new_ws, new_repo) = pollster::block_on(Workspace::init_workspace_with_existing_repo(
      path,
      &repo_path,
      &repo,
      &jj_lib::local_working_copy::LocalWorkingCopyFactory {},
      WorkspaceNameBuf::from(name),
    ))
    .map_err(|e| anyhow::anyhow!("workspace add failed: {e}"))?;

    self.swap_repo(new_repo);

    Ok(())
  }

  fn workspace_forget(&self, _repo_root: &Path, name: &str) -> Result<()> {
    let repo = self.repo();
    let mut tx = repo.start_transaction();
    let internal = self.internal_ws_name(name);
    let ws_name = WorkspaceNameBuf::from(internal.as_str());

    pollster::block_on(tx.repo_mut().remove_wc_commit(&ws_name))
      .map_err(|e| anyhow::anyhow!("workspace forget failed: {e}"))?;

    let new_repo = pollster::block_on(tx.commit(format!("forget workspace {name}")))
      .map_err(|e| anyhow::anyhow!("transaction commit failed: {e}"))?;

    self.swap_repo(new_repo);

    Ok(())
  }

  fn workspace_update_stale(&self, repo_root: &Path, name: &str) -> Result<()> {
    // Complex jj-internal logic. Fall back to subprocess.
    let jj_path = which::which("jj").map_err(|e| anyhow::anyhow!("jj not found: {e}"))?;
    let ws_path = workspace_dir(repo_root, name);

    let out = std::process::Command::new(&jj_path)
      .current_dir(&ws_path)
      .arg("workspace")
      .arg("update-stale")
      .output()
      .map_err(|e| anyhow::anyhow!("spawn failed: {e}"))?;

    if !out.status.success() {
      return Err(anyhow::anyhow!(
        "jj workspace update-stale failed: {}",
        String::from_utf8_lossy(&out.stderr)
      ));
    }

    Ok(())
  }

  fn bookmark_create(&self, _repo_root: &Path, name: &str, workspace: &str) -> Result<()> {
    let commit_id = self.wc_commit_id(workspace)?;
    let repo = self.repo();
    let mut tx = repo.start_transaction();
    let ref_name = RefName::new(name);

    tx.repo_mut()
      .set_local_bookmark_target(ref_name, RefTarget::normal(commit_id));

    let new_repo = pollster::block_on(tx.commit(format!("create bookmark {name}")))
      .map_err(|e| anyhow::anyhow!("transaction commit failed: {e}"))?;

    self.swap_repo(new_repo);

    Ok(())
  }

  fn bookmark_delete(&self, _repo_root: &Path, name: &str) -> Result<()> {
    let repo = self.repo();
    let mut tx = repo.start_transaction();
    let ref_name = RefName::new(name);

    tx.repo_mut()
      .set_local_bookmark_target(ref_name, RefTarget::absent());

    let new_repo = pollster::block_on(tx.commit(format!("delete bookmark {name}")))
      .map_err(|e| anyhow::anyhow!("transaction commit failed: {e}"))?;

    self.swap_repo(new_repo);

    Ok(())
  }

  fn bookmark_exists(&self, _repo_root: &Path, name: &str) -> Result<bool> {
    let repo = self.repo();
    let ref_name = RefName::new(name);

    Ok(repo.view().get_local_bookmark(ref_name).is_present())
  }

  fn bookmark_is_merged_into_trunk(&self, _repo_root: &Path, name: &str) -> Result<bool> {
    let repo = self.repo();
    let ref_name = RefName::new(name);
    let bookmark_target = repo.view().get_local_bookmark(ref_name);

    let bookmark_id = match bookmark_target.as_normal() {
      Some(id) => id,
      None => return Ok(false),
    };

    let trunk_id = match self.trunk_commit_id() {
      Some(id) => id,
      None => return Ok(false),
    };

    repo
      .index()
      .is_ancestor(bookmark_id, &trunk_id)
      .map_err(|e| anyhow::anyhow!("index error: {e}"))
  }

  fn workspace_is_dirty(&self, _repo_root: &Path, workspace: &str) -> Result<bool> {
    let commit_id = self.wc_commit_id(workspace)?;
    let commit = self.get_commit(&commit_id)?;
    let repo = self.repo();

    let is_empty = pollster::block_on(commit.is_empty(&*repo))?;

    Ok(!is_empty)
  }

  fn workspace_status(&self, _repo_root: &Path, workspace: &str) -> Result<(bool, bool)> {
    let commit_id = self.wc_commit_id(workspace)?;
    let commit = self.get_commit(&commit_id)?;
    let repo = self.repo();
    let is_empty = pollster::block_on(commit.is_empty(&*repo))?;

    Ok((!is_empty, false))
  }

  fn workspace_commit_info_batch(
    &self,
    _repo_root: &Path,
    workspaces: &[String],
  ) -> Result<HashMap<String, CommitInfo>> {
    let repo = self.repo();
    let now = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .map(|d| d.as_secs() as i64)
      .unwrap_or(0);

    let mut result = HashMap::with_capacity(workspaces.len());

    for ws_name in workspaces {
      let internal = self.internal_ws_name(ws_name);
      let ws = WorkspaceName::new(&internal);

      let Some(commit_id) = repo.view().get_wc_commit_id(ws) else {
        continue;
      };

      let commit = repo
        .store()
        .get_commit(commit_id)
        .map_err(|e| anyhow::anyhow!("failed to load commit for {ws_name}: {e}"))?;

      let change_id = commit.change_id();
      let commit_short = change_id.reverse_hex();
      let commit_short = if commit_short.len() > 8 {
        commit_short[..8].to_string()
      } else {
        commit_short
      };

      let message_first_line = commit
        .description()
        .lines()
        .next()
        .unwrap_or("")
        .to_string();

      let ts_millis = commit.committer().timestamp.timestamp.0;
      let ts_seconds = ts_millis / 1000;
      let age_seconds = (now - ts_seconds).max(0);

      let conflicts = commit.has_conflict();

      let (head_added, head_removed) = diff_stat_counts(&repo, &commit)?;

      result.insert(
        ws_name.clone(),
        CommitInfo {
          commit_short,
          age_seconds,
          message_first_line,
          conflicts,
          head_added,
          head_removed,
        },
      );
    }

    Ok(result)
  }

  fn workspace_ahead_behind_trunk(&self, _repo_root: &Path, workspace: &str) -> Result<(u32, u32)> {
    let ws_id = self.wc_commit_id(workspace)?;

    let trunk_id = match self.trunk_commit_id() {
      Some(id) => id,
      None => return Ok((0, 0)),
    };

    let ahead = self.count_between(&[trunk_id.clone()], &[ws_id.clone()])?;
    let behind = self.count_between(&[ws_id], &[trunk_id])?;

    Ok((ahead, behind))
  }

  fn workspace_ahead_behind_batch(
    &self,
    _repo_root: &Path,
    workspaces: &[String],
  ) -> Result<HashMap<String, (u32, u32)>> {
    let trunk_id = self.trunk_commit_id();
    let mut result = HashMap::with_capacity(workspaces.len());

    for ws_name in workspaces {
      let ws_id = self.wc_commit_id(ws_name)?;

      let (ahead, behind) = match &trunk_id {
        Some(tid) => {
          let a = self.count_between(&[tid.clone()], &[ws_id.clone()])?;
          let b = self.count_between(&[ws_id], &[tid.clone()])?;

          (a, b)
        }
        None => (0, 0),
      };

      result.insert(ws_name.clone(), (ahead, behind));
    }

    Ok(result)
  }

  fn bookmarks_with_remote(&self, _repo_root: &Path) -> Result<HashSet<String>> {
    let repo = self.repo();
    let set: HashSet<String> = repo
      .view()
      .all_remote_bookmarks()
      .map(|(sym, _)| sym.name.as_str().to_string())
      .collect();

    Ok(set)
  }

  fn bookmarks_local(&self, _repo_root: &Path) -> Result<Vec<String>> {
    let repo = self.repo();
    let names: Vec<String> = repo
      .view()
      .local_bookmarks()
      .filter(|(_, target)| target.is_present())
      .map(|(name, _)| name.as_str().to_string())
      .collect();

    Ok(names)
  }

  fn bookmark_sets(&self, _repo_root: &Path) -> Result<(Vec<String>, HashSet<String>)> {
    let repo = self.repo();
    let view = repo.view();

    let all_local: Vec<String> = view
      .local_bookmarks()
      .filter(|(_, target)| target.is_present())
      .map(|(name, _)| name.as_str().to_string())
      .collect();

    let with_remote: HashSet<String> = view
      .all_remote_bookmarks()
      .map(|(sym, _)| sym.name.as_str().to_string())
      .collect();

    Ok((all_local, with_remote))
  }

  fn trunk_bookmark(&self, _repo_root: &Path) -> Result<Option<String>> {
    Ok(self.resolve_trunk())
  }
}

/// Walk up from `start` to find the repo root (parent of `.jj/`).
fn find_repo_root(start: &Path) -> Result<PathBuf> {
  let mut p = start.to_path_buf();

  loop {
    let jj_dir = p.join(".jj");

    if jj_dir.is_dir() {
      let marker = jj_dir.join("repo");

      if marker.is_file() {
        let content = std::fs::read_to_string(&marker)
          .map_err(|e| anyhow::anyhow!("failed to read {marker:?}: {e}"))?;
        let target = PathBuf::from(content.trim());
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

      return Ok(p);
    }

    if !p.pop() {
      return Err(anyhow::anyhow!(
        "not inside a jj repo (no .jj/ found above {start:?})"
      ));
    }
  }
}

/// Compute workspace directory path.
fn workspace_dir(repo_root: &Path, name: &str) -> PathBuf {
  if name == "default" {
    repo_root.to_path_buf()
  } else {
    repo_root.join(".worktrees").join(name)
  }
}

/// Build minimal UserSettings for read-mostly operations.
fn minimal_settings() -> Result<UserSettings> {
  let mut config = StackedConfig::with_defaults();
  let mut layer = ConfigLayer::empty(ConfigSource::User);

  layer.set_value("user.name", "jjwt").unwrap();
  layer.set_value("user.email", "jjwt@localhost").unwrap();
  layer.set_value("operation.hostname", "localhost").unwrap();
  layer.set_value("operation.username", "jjwt").unwrap();
  layer.set_value("signing.behavior", "drop").unwrap();

  config.add_layer(layer);

  UserSettings::from_config(config).map_err(|e| anyhow::anyhow!("settings error: {e}"))
}

/// Count added/removed lines in a commit's diff vs its parent.
fn diff_stat_counts(repo: &Arc<ReadonlyRepo>, commit: &Commit) -> Result<(u32, u32)> {
  let parent_tree = pollster::block_on(commit.parent_tree(&**repo))?;
  let commit_tree = commit.tree();
  let diff_stream = parent_tree.diff_stream(&commit_tree, &EverythingMatcher);

  futures::pin_mut!(diff_stream);

  let mut added = 0u32;
  let mut removed = 0u32;

  while let Some(entry) = pollster::block_on(diff_stream.next()) {
    let Ok(diff) = entry.values else {
      continue;
    };

    let before_bytes = materialize_tree_value(repo, &diff.before);
    let after_bytes = materialize_tree_value(repo, &diff.after);

    if before_bytes.is_empty() && after_bytes.is_empty() {
      continue;
    }

    // Use jj's line-level diff to count added/removed lines.
    let hunks = jj_lib::diff::diff([&before_bytes[..], &after_bytes[..]]);

    for hunk in &hunks {
      match hunk.kind {
        jj_lib::diff::DiffHunkKind::Matching => {}
        jj_lib::diff::DiffHunkKind::Different => {
          removed += count_lines(hunk.contents[0].as_ref());
          added += count_lines(hunk.contents[1].as_ref());
        }
      }
    }
  }

  Ok((added, removed))
}

/// Read file content from a MergedTreeValue.
fn materialize_tree_value(
  repo: &Arc<ReadonlyRepo>,
  value: &jj_lib::merge::MergedTreeValue,
) -> Vec<u8> {
  let Some(tv) = value.as_resolved() else {
    return Vec::new();
  };

  let Some(jj_lib::backend::TreeValue::File { id, .. }) = tv else {
    return Vec::new();
  };

  let Ok(reader) = pollster::block_on(
    repo
      .store()
      .read_file(&jj_lib::repo_path::RepoPath::root(), id),
  ) else {
    return Vec::new();
  };

  let mut buf = Vec::new();

  pollster::block_on(async {
    use tokio::io::AsyncReadExt;
    let mut reader = reader;
    let _ = reader.read_to_end(&mut buf).await;
  });

  buf
}

/// Trigger a working-copy snapshot via `jj debug snapshot` so the repo
/// state reflects current disk changes. Silently ignores failures (e.g. if
/// `jj` is not on PATH).
fn trigger_snapshot(repo_root: &Path) {
  if let Ok(jj) = which::which("jj") {
    let _ = std::process::Command::new(jj)
      .current_dir(repo_root)
      .arg("debug")
      .arg("snapshot")
      .stdout(std::process::Stdio::null())
      .stderr(std::process::Stdio::null())
      .status();
  }
}

/// Count non-empty lines in a byte slice.
fn count_lines(data: &[u8]) -> u32 {
  if data.is_empty() {
    return 0;
  }

  let count = data.iter().filter(|&&b| b == b'\n').count() as u32;

  // If the file doesn't end with a newline, count the last line too.
  if !data.ends_with(b"\n") {
    count + 1
  } else {
    count
  }
}
