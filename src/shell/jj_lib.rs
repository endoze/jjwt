#![cfg(not(tarpaulin_include))]

use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use futures::StreamExt as _;
use jj_lib::backend::CommitId;
use jj_lib::commit::Commit;
use jj_lib::config::{ConfigLayer, ConfigSource, StackedConfig};
use jj_lib::matchers::EverythingMatcher;
use jj_lib::ref_name::{RefName, WorkspaceName, WorkspaceNameBuf};
use jj_lib::repo::{ReadonlyRepo, Repo as _, StoreFactories};
use jj_lib::settings::UserSettings;
use jj_lib::workspace::{Workspace, default_working_copy_factories};

use crate::core::types::{self, CommitInfo};
use crate::shell::jj::{Jj, find_repo_root, workspace_dir};

/// In-process jj backend using jj-lib. Loads the repo once and answers all
/// queries from memory — no subprocess spawning.
pub struct JjLib {
  /// Thread-safe handle to the loaded repo snapshot.
  repo: RwLock<Arc<ReadonlyRepo>>,
  /// Absolute path to the repo root directory.
  repo_root: PathBuf,
  /// Worktree-path template for resolving workspace directories.
  worktree_path_template: String,
  /// What `trunk()` resolved to at load time.
  trunk: Option<(types::Trunk, CommitId)>,
}

impl JjLib {
  /// Load the jj repo at or above `start`, snapshotting all workspaces.
  /// Uses the default worktree-path template.
  pub fn new(start: &Path) -> Result<Self> {
    Self::with_template(start, crate::core::types::DEFAULT_WORKTREE_PATH_TEMPLATE)
  }

  /// Load the jj repo with a custom worktree-path template.
  pub fn with_template(start: &Path, template: &str) -> Result<Self> {
    let repo_root = find_repo_root(start)?;
    let worktree_path_template = template.to_string();
    let settings = minimal_settings()?;
    let store_factories = StoreFactories::default();
    let wc_factories = default_working_copy_factories();

    let workspace = Workspace::load(&settings, &repo_root, &store_factories, &wc_factories)
      .context("failed to load workspace")?;

    // Trigger a working-copy snapshot for every workspace so the repo
    // reflects current disk state. Every jj command does this for the
    // current workspace; we do all of them since `list` reads from all.
    {
      let pre_repo = pollster::block_on(workspace.repo_loader().load_at_head())
        .context("failed to load repo")?;
      let ws_dirs: Vec<PathBuf> = pre_repo
        .view()
        .wc_commit_ids()
        .keys()
        .map(|ws| workspace_dir(&repo_root, ws.as_str(), &worktree_path_template))
        .collect();

      for dir in &ws_dirs {
        if dir.is_dir() {
          trigger_snapshot(dir);
        }
      }
    }

    // Load repo after snapshot — the op head may have changed.
    let repo =
      pollster::block_on(workspace.repo_loader().load_at_head()).context("failed to load repo")?;
    let trunk = query_trunk(&repo_root);

    Ok(Self {
      repo: RwLock::new(repo),
      repo_root,
      worktree_path_template,
      trunk,
    })
  }

  /// Borrow the current repo snapshot.
  fn repo(&self) -> Arc<ReadonlyRepo> {
    self.repo.read().unwrap_or_else(|e| e.into_inner()).clone()
  }

  /// Resolve a workspace name to its working-copy CommitId.
  fn wc_commit_id(&self, workspace: &str) -> Result<CommitId> {
    let repo = self.repo();

    repo
      .view()
      .get_wc_commit_id(WorkspaceName::new(workspace))
      .cloned()
      .ok_or_else(|| anyhow::anyhow!("workspace '{workspace}' not found"))
  }

  /// Resolve a local bookmark name or a full hex commit id to a commit.
  fn resolve_revision(&self, rev: &str) -> Result<Commit> {
    let repo = self.repo();
    let id = repo
      .view()
      .get_local_bookmark(RefName::new(rev))
      .as_normal()
      .cloned()
      .or_else(|| CommitId::try_from_hex(rev));

    id.and_then(|id| repo.store().get_commit(&id).ok())
      .ok_or_else(|| anyhow::anyhow!("revision '{rev}' not found"))
  }

  /// Load a Commit by its CommitId.
  fn get_commit(&self, id: &CommitId) -> Result<Commit> {
    let repo = self.repo();

    repo.store().get_commit(id).context("failed to load commit")
  }

  /// The commit `trunk()` resolved to, or `None` when it is `root()`.
  fn trunk_commit_id(&self) -> Option<CommitId> {
    self.trunk.as_ref().map(|(_, id)| id.clone())
  }

  /// The commit that holds a workspace's work: `@`, or `@-` when `@` is an
  /// empty, undescribed, single-parent working-copy commit.
  fn work_head(&self, wc_id: &CommitId) -> Result<CommitId> {
    let commit = self.get_commit(wc_id)?;
    let repo = self.repo();
    let empty = pollster::block_on(commit.is_empty(&*repo))?;

    if empty && commit.description().is_empty() && commit.parent_ids().len() == 1 {
      return Ok(commit.parent_ids()[0].clone());
    }

    Ok(wc_id.clone())
  }

  /// Count commits in `roots..heads` using revset walk_revs.
  fn count_between(&self, roots: &[CommitId], heads: &[CommitId]) -> Result<u32> {
    let repo = self.repo();

    let revset = jj_lib::revset::walk_revs(&*repo, heads, roots).context("walk_revs failed")?;

    let stream = revset.stream();

    futures::pin_mut!(stream);

    let mut count = 0u32;

    while let Some(item) = pollster::block_on(stream.next()) {
      let _ = item.context("revset stream error")?;
      count += 1;
    }

    Ok(count)
  }

  /// Reload the repo at the current op head after an out-of-process write.
  fn reload(&self) -> Result<()> {
    let settings = minimal_settings()?;
    let ws = Workspace::load(
      &settings,
      &self.repo_root,
      &StoreFactories::default(),
      &default_working_copy_factories(),
    )
    .context("failed to load workspace")?;
    let repo =
      pollster::block_on(ws.repo_loader().load_at_head()).context("failed to load repo")?;

    self.swap_repo(repo);

    Ok(())
  }

  /// Replace the stored repo after a write transaction.
  fn swap_repo(&self, new_repo: Arc<ReadonlyRepo>) {
    let mut guard = self.repo.write().unwrap_or_else(|e| e.into_inner());

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
    let current_op_id = repo.op_id();
    let settings = minimal_settings()?;
    let store_factories = StoreFactories::default();
    let wc_factories = default_working_copy_factories();
    let mut workspaces = Vec::with_capacity(wc_ids.len());

    for ws_name in wc_ids.keys() {
      let name = ws_name.as_str().to_string();
      let path = workspace_dir(&self.repo_root, &name, &self.worktree_path_template);
      let stale = path.is_dir()
        && Workspace::load(&settings, &path, &store_factories, &wc_factories)
          .map(|ws| ws.working_copy().operation_id() != current_op_id)
          .unwrap_or(false);

      workspaces.push(types::Workspace { name, path, stale });
    }

    Ok(workspaces)
  }

  fn workspace_add(
    &self,
    _repo_root: &Path,
    name: &str,
    path: &Path,
    revision: Option<&str>,
    edit_in_place: bool,
  ) -> Result<()> {
    let base = revision.map(|rev| self.resolve_revision(rev)).transpose()?;

    std::fs::create_dir_all(path).context("failed to create workspace dir")?;

    let repo = self.repo();
    let repo_path = self.repo_root.join(".jj").join("repo");

    let (_new_ws, new_repo) = pollster::block_on(Workspace::init_workspace_with_existing_repo(
      path,
      &repo_path,
      &repo,
      &jj_lib::local_working_copy::LocalWorkingCopyFactory {},
      WorkspaceNameBuf::from(name),
    ))
    .context("workspace add failed")?;

    self.swap_repo(new_repo);

    if let (Some(rev), Some(commit)) = (revision, base) {
      let repo = self.repo();
      let mut tx = repo.start_transaction();
      let ws_name = WorkspaceNameBuf::from(name);

      if edit_in_place {
        pollster::block_on(tx.repo_mut().edit(ws_name, &commit))
          .context("failed to edit revision")?;
      } else {
        pollster::block_on(tx.repo_mut().check_out(ws_name, &commit))
          .context("failed to check out revision")?;
      }

      pollster::block_on(tx.repo_mut().rebase_descendants())
        .context("failed to rebase descendants")?;

      let new_repo =
        pollster::block_on(tx.commit(format!("check out {rev} in workspace {name}")))
          .context("transaction commit failed")?;

      self.swap_repo(new_repo);
    }

    Ok(())
  }

  fn workspace_snapshot(&self, path: &Path, stale: bool) -> Result<()> {
    if !path.is_dir() {
      return Ok(());
    }

    if stale {
      run_jj(path, &["workspace", "update-stale"])?;
    }

    run_jj(path, &["util", "snapshot"])?;

    self.reload()
  }

  fn workspace_forget(&self, _repo_root: &Path, name: &str) -> Result<()> {
    let repo = self.repo();
    let mut tx = repo.start_transaction();
    let ws_name = WorkspaceNameBuf::from(name);

    pollster::block_on(tx.repo_mut().remove_wc_commit(&ws_name))
      .context("workspace forget failed")?;

    pollster::block_on(tx.repo_mut().rebase_descendants())
      .context("failed to rebase descendants")?;

    let new_repo = pollster::block_on(tx.commit(format!("forget workspace {name}")))
      .context("transaction commit failed")?;

    self.swap_repo(new_repo);

    Ok(())
  }

  fn workspace_update_stale(&self, repo_root: &Path, name: &str) -> Result<()> {
    // Complex jj-internal logic. Fall back to subprocess.
    let jj_path = which::which("jj").context("jj not found")?;
    let ws_path = workspace_dir(repo_root, name, &self.worktree_path_template);

    let out = std::process::Command::new(&jj_path)
      .current_dir(&ws_path)
      .arg("workspace")
      .arg("update-stale")
      .output()
      .context("failed to spawn jj workspace update-stale")?;

    if !out.status.success() {
      return Err(anyhow::anyhow!(
        "jj workspace update-stale failed: {}",
        String::from_utf8_lossy(&out.stderr)
      ));
    }

    Ok(())
  }

  fn bookmark_exists(&self, _repo_root: &Path, name: &str) -> Result<bool> {
    let repo = self.repo();
    let ref_name = RefName::new(name);

    Ok(repo.view().get_local_bookmark(ref_name).is_present())
  }

  fn bookmark_commit_state(&self, _repo_root: &Path, name: &str) -> Result<(bool, bool)> {
    let repo = self.repo();
    let ref_name = RefName::new(name);

    let Some(commit_id) = repo.view().get_local_bookmark(ref_name).as_normal().cloned() else {
      return Ok((false, false));
    };

    let occupied = repo
      .view()
      .wc_commit_ids()
      .values()
      .any(|id| *id == commit_id);
    let commit = self.get_commit(&commit_id)?;
    let empty = pollster::block_on(commit.is_empty(&*repo))?;

    Ok((empty, occupied))
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
      let ws = WorkspaceName::new(ws_name);

      let Some(commit_id) = repo.view().get_wc_commit_id(ws) else {
        continue;
      };

      let commit = repo
        .store()
        .get_commit(commit_id)
        .with_context(|| format!("failed to load commit for {ws_name}"))?;

      let change_id = commit.change_id();
      let mut commit_short = change_id.reverse_hex();

      commit_short.truncate(8);

      let message_first_line = commit
        .description()
        .lines()
        .next()
        .map(str::to_string)
        .unwrap_or_default();

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

    let Some(trunk_id) = self.trunk_commit_id() else {
      return Ok((0, 0));
    };

    let ahead = self.count_between(
      std::slice::from_ref(&trunk_id),
      std::slice::from_ref(&ws_id),
    )?;
    let behind = self.count_between(
      std::slice::from_ref(&ws_id),
      std::slice::from_ref(&trunk_id),
    )?;

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
      let head = self.work_head(&self.wc_commit_id(ws_name)?)?;

      let (ahead, behind) = match &trunk_id {
        Some(tid) => {
          let a = self.count_between(std::slice::from_ref(tid), std::slice::from_ref(&head))?;
          let b = self.count_between(std::slice::from_ref(&head), std::slice::from_ref(tid))?;

          (a, b)
        }
        None => (0, 0),
      };

      result.insert(ws_name.clone(), (ahead, behind));
    }

    Ok(result)
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

  fn workspace_bookmarks_batch(
    &self,
    _repo_root: &Path,
    workspaces: &[String],
  ) -> Result<HashMap<String, Vec<types::WorkspaceBookmark>>> {
    let mut out = HashMap::with_capacity(workspaces.len());
    let Some(trunk_id) = self.trunk_commit_id() else {
      return Ok(out);
    };
    let repo = self.repo();
    let view = repo.view();
    let mut by_commit: HashMap<CommitId, Vec<String>> = HashMap::new();

    for (name, target) in view.local_bookmarks() {
      if let Some(id) = target.as_normal() {
        by_commit
          .entry(id.clone())
          .or_default()
          .push(name.as_str().to_string());
      }
    }

    let pushed: HashSet<String> = view
      .all_remote_bookmarks()
      .filter(|(sym, r)| {
        sym.remote != jj_lib::git::REMOTE_NAME_FOR_LOCAL_GIT_REPO
          && r.is_tracked()
          && r.is_present()
      })
      .map(|(sym, _)| sym.name.as_str().to_string())
      .collect();

    for ws in workspaces {
      let wc = self.wc_commit_id(ws)?;
      let revset = jj_lib::revset::walk_revs(
        &*repo,
        std::slice::from_ref(&wc),
        std::slice::from_ref(&trunk_id),
      )
      .context("walk_revs failed")?;
      let stream = revset.stream();
      let mut found = Vec::new();

      futures::pin_mut!(stream);

      while let Some(item) = pollster::block_on(stream.next()) {
        let id = item.context("revset stream error")?;

        for name in by_commit.get(&id).into_iter().flatten() {
          found.push(types::WorkspaceBookmark {
            name: name.clone(),
            has_remote: pushed.contains(name),
          });
        }
      }

      out.insert(ws.clone(), found);
    }

    Ok(out)
  }

  fn trunk(&self, _repo_root: &Path) -> Result<Option<types::Trunk>> {
    Ok(self.trunk.as_ref().map(|(t, _)| t.clone()))
  }

  fn git_fetch(&self, repo_root: &Path) -> Result<()> {
    let jj_path = which::which("jj").context("jj not found")?;
    let mut cmd = std::process::Command::new(jj_path);

    cmd.arg("git").arg("fetch").arg("-R").arg(repo_root);

    let out = cmd.output().context("failed to spawn jj git fetch")?;

    if !out.status.success() {
      return Err(anyhow::anyhow!(
        "jj git fetch failed: {}",
        String::from_utf8_lossy(&out.stderr)
      ));
    }

    Ok(())
  }

  fn workspace_rename(&self, _repo_root: &Path, old: &str, new: &str) -> Result<()> {
    let repo = self.repo();
    let old_ws = WorkspaceName::new(old);

    let commit_id = repo
      .view()
      .get_wc_commit_id(old_ws)
      .cloned()
      .ok_or_else(|| anyhow::anyhow!("workspace '{old}' not found"))?;

    let mut tx = repo.start_transaction();
    let new_ws = WorkspaceNameBuf::from(new);

    let _ = tx.repo_mut().set_wc_commit(new_ws, commit_id);

    let old_ws_buf = WorkspaceNameBuf::from(old);

    pollster::block_on(tx.repo_mut().remove_wc_commit(&old_ws_buf))
      .context("workspace rename failed")?;

    pollster::block_on(tx.repo_mut().rebase_descendants())
      .context("failed to rebase descendants")?;

    let new_repo = pollster::block_on(tx.commit(format!("rename workspace {old} → {new}")))
      .context("transaction commit failed")?;

    self.swap_repo(new_repo);

    Ok(())
  }
}

/// Build minimal UserSettings for read-mostly operations.
fn minimal_settings() -> Result<UserSettings> {
  let mut config = StackedConfig::with_defaults();
  let mut layer = ConfigLayer::empty(ConfigSource::User);

  layer
    .set_value("user.name", "jjwt")
    .context("set user.name")?;
  layer
    .set_value("user.email", "jjwt@localhost")
    .context("set user.email")?;
  layer
    .set_value("operation.hostname", "localhost")
    .context("set operation.hostname")?;
  layer
    .set_value("operation.username", "jjwt")
    .context("set operation.username")?;
  layer
    .set_value("signing.behavior", "drop")
    .context("set signing.behavior")?;

  config.add_layer(layer);

  UserSettings::from_config(config).context("settings error")
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
      .read_file(jj_lib::repo_path::RepoPath::root(), id),
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

/// Run `jj <args>` in `dir`, failing with jj's stderr on a non-zero exit.
fn run_jj(dir: &Path, args: &[&str]) -> Result<()> {
  let jj = which::which("jj").context("jj not found on PATH")?;
  let out = std::process::Command::new(jj)
    .current_dir(dir)
    .args(args)
    .output()
    .context("failed to spawn jj")?;

  if !out.status.success() {
    anyhow::bail!(
      "`jj {}` failed in {}: {}",
      args.join(" "),
      dir.display(),
      String::from_utf8_lossy(&out.stderr).trim()
    );
  }

  Ok(())
}

/// Trigger a working-copy snapshot via `jj util snapshot` so the repo
/// state reflects current disk changes. Silently ignores failures (e.g. if
/// `jj` is not on PATH).
fn trigger_snapshot(repo_root: &Path) {
  if let Ok(jj) = which::which("jj") {
    let _ = std::process::Command::new(jj)
      .current_dir(repo_root)
      .arg("util")
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

/// Template printing `commit_id\tremote bookmark names` for a non-root commit.
const TRUNK_TEMPLATE: &str =
  r#"if(!root, commit_id ++ "\t" ++ remote_bookmarks.map(|b| b.name()).join(" ") ++ "\n")"#;

/// Resolve `trunk()` through the jj CLI so user and default revset aliases
/// apply. The default alias lives in jj-cli's config, not in jj-lib.
fn query_trunk(repo_root: &Path) -> Option<(types::Trunk, CommitId)> {
  let jj = which::which("jj").ok()?;
  let out = std::process::Command::new(jj)
    .arg("-R")
    .arg(repo_root)
    .args([
      "--ignore-working-copy",
      "--color",
      "never",
      "log",
      "--no-graph",
      "-r",
      "trunk()",
      "-T",
      TRUNK_TEMPLATE,
    ])
    .output()
    .ok()?;

  if !out.status.success() {
    return None;
  }

  let trunk = types::Trunk::parse(&String::from_utf8_lossy(&out.stdout))?;
  let id = CommitId::try_from_hex(&trunk.commit_id)?;

  Some((trunk, id))
}
