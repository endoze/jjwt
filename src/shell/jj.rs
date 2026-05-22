use anyhow::Result;
use std::path::Path;

use crate::core::types::{CommitInfo, Workspace};

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
  /// [`workspace_commit_info_batch`] (which handles commit metadata,
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
  /// suffix). Used by `list --branches` and `--remotes` to discover
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

/// Real implementation: shells out to the `jj` binary.
pub struct JjCli {
  pub jj_path: std::path::PathBuf,
}

impl JjCli {
  pub fn new() -> anyhow::Result<Self> {
    let jj_path = which::which("jj").map_err(|e| anyhow::anyhow!("jj not found on PATH: {e}"))?;
    Ok(Self { jj_path })
  }
}

fn run(cmd: &mut std::process::Command) -> Result<std::process::Output> {
  let out = cmd
    .output()
    .map_err(|e| anyhow::anyhow!("spawn failed: {e}"))?;

  if !out.status.success() {
    return Err(anyhow::anyhow!(
      "jj command failed: {:?}\nstderr: {}",
      cmd,
      String::from_utf8_lossy(&out.stderr)
    ));
  }

  Ok(out)
}

/// Compute the path to a workspace directory. For "default", this is repo_root;
/// for other workspaces, it is repo_root/.worktrees/<name>.
fn workspace_dir(repo_root: &Path, name: &str) -> std::path::PathBuf {
  if name == "default" {
    repo_root.to_path_buf()
  } else {
    repo_root.join(".worktrees").join(name)
  }
}

impl Jj for JjCli {
  fn repo_root(&self, start: &Path) -> Result<std::path::PathBuf> {
    let mut p = start.to_path_buf();

    loop {
      let jj_dir = p.join(".jj");

      if jj_dir.is_dir() {
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

        return Ok(p);
      }

      if !p.pop() {
        return Err(anyhow::anyhow!(
          "not inside a jj repo (no .jj/ found above {start:?})"
        ));
      }
    }
  }

  fn workspace_list(&self, repo_root: &Path) -> Result<Vec<Workspace>> {
    let out = run(
      std::process::Command::new(&self.jj_path)
        .arg("--repository")
        .arg(repo_root)
        .arg("workspace")
        .arg("list"),
    )?;

    let text = String::from_utf8_lossy(&out.stdout);
    let mut workspaces = Vec::new();

    for line in text.lines() {
      let trimmed = line.trim();

      if trimmed.is_empty() {
        continue;
      }

      // `jj workspace list` rows look like "<name>: <change_id> <hash> ..."
      // Strip the trailing colon so callers get a bare workspace name.
      let name = trimmed
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_end_matches(':')
        .to_string();

      if name.is_empty() {
        continue;
      }

      let stale = trimmed.contains("(stale)");
      let path = workspace_dir(repo_root, &name);

      workspaces.push(Workspace { name, path, stale });
    }

    Ok(workspaces)
  }

  fn workspace_add(&self, repo_root: &Path, name: &str, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent)
        .map_err(|e| anyhow::anyhow!("failed to create workspace parent dir: {e}"))?;
    }

    run(
      std::process::Command::new(&self.jj_path)
        .arg("--repository")
        .arg(repo_root)
        .arg("workspace")
        .arg("add")
        .arg("--name")
        .arg(name)
        .arg(path),
    )?;

    Ok(())
  }

  fn workspace_forget(&self, repo_root: &Path, name: &str) -> Result<()> {
    run(
      std::process::Command::new(&self.jj_path)
        .arg("--repository")
        .arg(repo_root)
        .arg("workspace")
        .arg("forget")
        .arg(name),
    )?;

    Ok(())
  }

  fn workspace_update_stale(&self, repo_root: &Path, name: &str) -> Result<()> {
    let ws_path = workspace_dir(repo_root, name);

    run(
      std::process::Command::new(&self.jj_path)
        .current_dir(&ws_path)
        .arg("workspace")
        .arg("update-stale"),
    )?;

    Ok(())
  }

  fn bookmark_create(&self, repo_root: &Path, name: &str, workspace: &str) -> Result<()> {
    let revset = format!("{}@", workspace);

    run(
      std::process::Command::new(&self.jj_path)
        .arg("--repository")
        .arg(repo_root)
        .arg("bookmark")
        .arg("create")
        .arg(name)
        .arg("--revision")
        .arg(&revset),
    )?;

    Ok(())
  }

  fn bookmark_delete(&self, repo_root: &Path, name: &str) -> Result<()> {
    run(
      std::process::Command::new(&self.jj_path)
        .arg("--repository")
        .arg(repo_root)
        .arg("bookmark")
        .arg("delete")
        .arg(name),
    )?;

    Ok(())
  }

  fn bookmark_exists(&self, repo_root: &Path, name: &str) -> Result<bool> {
    let out = std::process::Command::new(&self.jj_path)
      .arg("--repository")
      .arg(repo_root)
      .arg("bookmark")
      .arg("list")
      .arg("-T")
      .arg(r#"name ++ "\n""#)
      .output()
      .map_err(|e| anyhow::anyhow!("spawn failed: {e}"))?;

    if !out.status.success() {
      return Ok(false);
    }

    let text = String::from_utf8_lossy(&out.stdout);

    Ok(text.lines().any(|l| l.trim() == name))
  }

  fn bookmark_is_merged_into_trunk(&self, repo_root: &Path, name: &str) -> Result<bool> {
    let out = run(
      std::process::Command::new(&self.jj_path)
        .arg("--repository")
        .arg(repo_root)
        .arg("log")
        .arg("--no-graph")
        .arg("-r")
        .arg(format!("{} & ::trunk()", name))
        .arg("-T")
        .arg(r#""x""#)
        .arg("--limit")
        .arg("1"),
    )?;

    Ok(!out.stdout.is_empty())
  }

  fn workspace_is_dirty(&self, repo_root: &Path, workspace: &str) -> Result<bool> {
    let ws_path = workspace_dir(repo_root, workspace);

    let out = std::process::Command::new(&self.jj_path)
      .current_dir(&ws_path)
      .arg("diff")
      .arg("--summary")
      .output()
      .map_err(|e| anyhow::anyhow!("spawn failed: {e}"))?;

    if !out.status.success() {
      return Err(anyhow::anyhow!(
        "jj diff failed: {}",
        String::from_utf8_lossy(&out.stderr)
      ));
    }

    Ok(!out.stdout.is_empty())
  }

  fn workspace_status(&self, repo_root: &Path, workspace: &str) -> Result<(bool, bool)> {
    let ws_path = workspace_dir(repo_root, workspace);

    let status_out = std::process::Command::new(&self.jj_path)
      .current_dir(&ws_path)
      .arg("status")
      .output()
      .map_err(|e| anyhow::anyhow!("spawn failed: {e}"))?;

    if !status_out.status.success() {
      return Err(anyhow::anyhow!(
        "jj status failed: {}",
        String::from_utf8_lossy(&status_out.stderr)
      ));
    }

    let status_text = String::from_utf8_lossy(&status_out.stdout);
    let (modified, untracked, _conflicts) = parse_status(&status_text);

    Ok((modified, untracked))
  }

  fn workspace_commit_info_batch(
    &self,
    repo_root: &Path,
    workspaces: &[String],
  ) -> Result<std::collections::HashMap<String, CommitInfo>> {
    let mut result = std::collections::HashMap::new();

    if workspaces.is_empty() {
      return Ok(result);
    }

    // Build a revset union: `all:(default@|feat@|…)`
    let rev_parts: Vec<String> = workspaces.iter().map(|w| format!("{w}@")).collect();
    let revset = format!("all:({})", rev_parts.join("|"));

    // Template emitting tab-delimited fields per workspace, with
    // `diff.stat()` (multi-line) terminated by \x1e record separator.
    let tmpl = r#"working_copies ++ "\t" ++ change_id.shortest(8) ++ "\t" ++ description.first_line() ++ "\t" ++ committer.timestamp().format("%s") ++ "\t" ++ if(conflict, "1", "0") ++ "\t" ++ diff.stat() ++ "\x1e""#;

    let out = run(
      std::process::Command::new(&self.jj_path)
        .arg("--repository")
        .arg(repo_root)
        .arg("log")
        .arg("--no-graph")
        .arg("-r")
        .arg(&revset)
        .arg("-T")
        .arg(tmpl),
    )?;

    let now = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .map(|d| d.as_secs() as i64)
      .unwrap_or(0);

    let text = String::from_utf8_lossy(&out.stdout);

    // Records are separated by \x1e. Within each record the first line
    // holds the tab-delimited metadata; subsequent lines are diff.stat()
    // file rows; the last non-empty line is the diff summary.
    for record in text.split('\x1e') {
      let record = record.trim();

      if record.is_empty() {
        continue;
      }

      let first_line = record.lines().next().unwrap_or("");
      let mut parts = first_line.splitn(6, '\t');
      let ws_tag = parts.next().unwrap_or("").trim();
      let commit_short = parts.next().unwrap_or("").to_string();
      let message_first_line = parts.next().unwrap_or("").to_string();
      let ts_str = parts.next().unwrap_or("0").trim();
      let conflict_str = parts.next().unwrap_or("0").trim();
      // Remaining part (index 5) is the first line of diff.stat() output;
      // we need the summary line which is the LAST line of the record.

      let timestamp: i64 = ts_str.parse().unwrap_or(0);
      let age_seconds = (now - timestamp).max(0);
      let conflicts = conflict_str == "1";

      // Parse diff stat summary from the last non-empty line of the record.
      let (head_added, head_removed) = parse_diff_stat_summary(record);

      if let Some(ws_name) = ws_tag.strip_suffix('@') {
        result.insert(
          ws_name.to_string(),
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
    }

    Ok(result)
  }

  fn workspace_ahead_behind_trunk(&self, repo_root: &Path, workspace: &str) -> Result<(u32, u32)> {
    let at_rev = format!("{workspace}@");
    let ahead = revset_count(&self.jj_path, repo_root, &format!("trunk()..{at_rev}"))?;
    let behind = revset_count(&self.jj_path, repo_root, &format!("{at_rev}..trunk()"))?;

    Ok((ahead, behind))
  }

  fn workspace_ahead_behind_batch(
    &self,
    repo_root: &Path,
    workspaces: &[String],
  ) -> Result<std::collections::HashMap<String, (u32, u32)>> {
    let mut result: std::collections::HashMap<String, (u32, u32)> =
      workspaces.iter().map(|w| (w.clone(), (0, 0))).collect();

    if workspaces.is_empty() {
      return Ok(result);
    }

    let rev_parts: Vec<String> = workspaces.iter().map(|w| format!("{w}@")).collect();
    let union = rev_parts.join("|");

    // Ahead: commits in `trunk()..(w1@|w2@|…)`, tagged by workspace
    // using `self.contained_in("trunk()..wi@")`.
    let ahead_revset = format!("trunk()..({union})");
    let ahead_contained: Vec<String> = workspaces
      .iter()
      .map(|w| format!(r#"if(self.contained_in("trunk()..{w}@"), "{w}\n", "")"#))
      .collect();
    let ahead_tmpl = ahead_contained.join(" ++ ");

    let ahead_out = run(
      std::process::Command::new(&self.jj_path)
        .arg("--repository")
        .arg(repo_root)
        .arg("log")
        .arg("--no-graph")
        .arg("-r")
        .arg(&ahead_revset)
        .arg("-T")
        .arg(&ahead_tmpl),
    )?;

    let ahead_text = String::from_utf8_lossy(&ahead_out.stdout);

    for line in ahead_text.lines() {
      let ws = line.trim();

      if let Some(entry) = result.get_mut(ws) {
        entry.0 += 1;
      }
    }

    // Behind: commits in `(w1@|w2@|…)..trunk()`, tagged similarly.
    let behind_revset = format!("({union})..trunk()");
    let behind_contained: Vec<String> = workspaces
      .iter()
      .map(|w| format!(r#"if(self.contained_in("{w}@..trunk()"), "{w}\n", "")"#))
      .collect();
    let behind_tmpl = behind_contained.join(" ++ ");

    let behind_out = run(
      std::process::Command::new(&self.jj_path)
        .arg("--repository")
        .arg(repo_root)
        .arg("log")
        .arg("--no-graph")
        .arg("-r")
        .arg(&behind_revset)
        .arg("-T")
        .arg(&behind_tmpl),
    )?;

    let behind_text = String::from_utf8_lossy(&behind_out.stdout);

    for line in behind_text.lines() {
      let ws = line.trim();

      if let Some(entry) = result.get_mut(ws) {
        entry.1 += 1;
      }
    }

    Ok(result)
  }

  fn bookmarks_local(&self, repo_root: &Path) -> Result<Vec<String>> {
    let out = std::process::Command::new(&self.jj_path)
      .arg("--repository")
      .arg(repo_root)
      .arg("bookmark")
      .arg("list")
      .arg("-T")
      .arg(r#"name ++ "\n""#)
      .output()
      .map_err(|e| anyhow::anyhow!("spawn failed: {e}"))?;

    if !out.status.success() {
      return Ok(Vec::new());
    }

    let text = String::from_utf8_lossy(&out.stdout);
    let names: Vec<String> = text
      .lines()
      .map(|s| s.trim().to_string())
      .filter(|s| !s.is_empty())
      .collect();

    Ok(names)
  }

  fn bookmarks_with_remote(&self, repo_root: &Path) -> Result<std::collections::HashSet<String>> {
    // `jj bookmark list --all-remotes -T 'name ++ "\n"'` emits one line
    // per (bookmark, remote) pair: `<name>@<remote>` for remote-tracking
    // refs and `<name>` for local-only. Extract just the local name from
    // entries that include `@<remote>`.
    let out = std::process::Command::new(&self.jj_path)
      .arg("--repository")
      .arg(repo_root)
      .arg("bookmark")
      .arg("list")
      .arg("--all-remotes")
      .arg("-T")
      .arg(r#"name ++ "@" ++ remote ++ "\n""#)
      .output()
      .map_err(|e| anyhow::anyhow!("spawn failed: {e}"))?;
    let mut set = std::collections::HashSet::new();

    if !out.status.success() {
      return Ok(set);
    }

    let text = String::from_utf8_lossy(&out.stdout);

    for line in text.lines() {
      let trimmed = line.trim();

      if let Some((name, remote)) = trimmed.rsplit_once('@')
        && !name.is_empty()
        && !remote.is_empty()
      {
        set.insert(name.to_string());
      }
    }

    Ok(set)
  }

  fn bookmark_sets(
    &self,
    repo_root: &Path,
  ) -> Result<(Vec<String>, std::collections::HashSet<String>)> {
    // Single `jj bookmark list --all-remotes` call extracts both:
    // - `all_local`: names where the `remote` field is empty (local-only)
    //   or any name that appears at least once with empty remote
    // - `with_remote`: names that have at least one non-empty remote
    let out = std::process::Command::new(&self.jj_path)
      .arg("--repository")
      .arg(repo_root)
      .arg("bookmark")
      .arg("list")
      .arg("--all-remotes")
      .arg("-T")
      .arg(r#"name ++ "\t" ++ remote ++ "\n""#)
      .output()
      .map_err(|e| anyhow::anyhow!("spawn failed: {e}"))?;

    let mut all_local = std::collections::HashSet::new();
    let mut with_remote = std::collections::HashSet::new();

    if !out.status.success() {
      return Ok((Vec::new(), std::collections::HashSet::new()));
    }

    let text = String::from_utf8_lossy(&out.stdout);

    for line in text.lines() {
      let trimmed = line.trim();

      if trimmed.is_empty() {
        continue;
      }

      if let Some((name, remote)) = trimmed.split_once('\t') {
        if name.is_empty() {
          continue;
        }

        // Every name we see is a local bookmark (the --all-remotes
        // output includes local entries with empty remote).
        all_local.insert(name.to_string());

        if !remote.is_empty() {
          with_remote.insert(name.to_string());
        }
      }
    }

    let local_vec: Vec<String> = all_local.into_iter().collect();

    Ok((local_vec, with_remote))
  }

  fn trunk_bookmark(&self, repo_root: &Path) -> Result<Option<String>> {
    let out = std::process::Command::new(&self.jj_path)
      .arg("--repository")
      .arg(repo_root)
      .arg("bookmark")
      .arg("list")
      .arg("-r")
      .arg("trunk()")
      .arg("-T")
      .arg(r#"name ++ "\n""#)
      .output()
      .map_err(|e| anyhow::anyhow!("spawn failed: {e}"))?;

    if !out.status.success() {
      return Ok(None);
    }

    let text = String::from_utf8_lossy(&out.stdout);
    let name = text
      .lines()
      .next()
      .map(|s| s.trim().to_string())
      .filter(|s| !s.is_empty());

    Ok(name)
  }

  fn git_fetch(&self, repo_root: &Path) -> Result<()> {
    let mut cmd = std::process::Command::new(&self.jj_path);

    cmd.arg("git").arg("fetch").arg("-R").arg(repo_root);

    run(&mut cmd)?;

    Ok(())
  }

  fn workspace_rename(&self, repo_root: &Path, old: &str, new: &str) -> Result<()> {
    let mut cmd = std::process::Command::new(&self.jj_path);

    cmd
      .arg("workspace")
      .arg("rename")
      .arg("--from")
      .arg(old)
      .arg("--to")
      .arg(new)
      .arg("-R")
      .arg(repo_root);

    run(&mut cmd)?;

    Ok(())
  }

  fn bookmark_rename(&self, repo_root: &Path, old: &str, new: &str) -> Result<()> {
    let mut create = std::process::Command::new(&self.jj_path);

    create
      .arg("bookmark")
      .arg("create")
      .arg(new)
      .arg("-r")
      .arg(old)
      .arg("-R")
      .arg(repo_root);

    run(&mut create)?;

    let mut del = std::process::Command::new(&self.jj_path);

    del
      .arg("bookmark")
      .arg("delete")
      .arg(old)
      .arg("-R")
      .arg(repo_root);

    run(&mut del)?;

    Ok(())
  }
}

fn revset_count(jj: &Path, repo_root: &Path, revset: &str) -> Result<u32> {
  let out = run(
    std::process::Command::new(jj)
      .arg("--repository")
      .arg(repo_root)
      .arg("log")
      .arg("--no-graph")
      .arg("-r")
      .arg(revset)
      .arg("-T")
      .arg(r#""x\n""#),
  )?;
  let n = String::from_utf8_lossy(&out.stdout).lines().count();

  Ok(n as u32)
}

/// Parse `jj status` output into `(modified, untracked, conflicts)`.
///
/// jj 0.36 status format example:
/// ```text
/// Working copy changes:
/// M file.rs
/// A new.rs
/// D old.rs
/// Untracked paths:
/// ? untracked.rs
/// There are unresolved conflicts.
/// Working copy  (@) : ...
/// Parent commit (@-): ...
/// ```
pub(crate) fn parse_status(text: &str) -> (bool, bool, bool) {
  let mut modified = false;
  let mut untracked = false;
  let mut conflicts = false;
  let mut in_changes = false;
  let mut in_untracked = false;

  for line in text.lines() {
    let trimmed = line.trim_end();

    if trimmed.starts_with("Working copy changes:") {
      in_changes = true;
      in_untracked = false;

      continue;
    }

    if trimmed.starts_with("Untracked paths:") {
      in_changes = false;
      in_untracked = true;

      continue;
    }

    if trimmed.starts_with("Working copy") || trimmed.starts_with("Parent commit") {
      in_changes = false;
      in_untracked = false;

      continue;
    }

    if trimmed.contains("unresolved conflicts") {
      conflicts = true;

      continue;
    }

    if in_changes && is_change_line(trimmed) {
      modified = true;
    }

    if in_untracked && !trimmed.is_empty() && !trimmed.starts_with("(") {
      untracked = true;
    }
  }

  (modified, untracked, conflicts)
}

fn is_change_line(s: &str) -> bool {
  let mut chars = s.chars();
  let first = chars.next();

  matches!(first, Some('M' | 'A' | 'D' | 'R' | 'C')) && chars.next() == Some(' ')
}

/// Parse `jj diff --stat` summary line into `(added, removed)`.
///
/// Summary line format:
/// `N files changed, A insertions(+), D deletions(-)`
/// (singular forms also possible: "1 file changed", "1 insertion(+)", "1 deletion(-)")
pub(crate) fn parse_diff_stat_summary(text: &str) -> (u32, u32) {
  let mut added = 0u32;
  let mut removed = 0u32;

  for line in text.lines().rev() {
    let l = line.trim();

    if l.is_empty() {
      continue;
    }

    if let Some(rest) = l.split_once("insertion") {
      // Tokens preceding "insertion" end with "<N> "
      if let Some(n) = rest.0.trim_end_matches(", ").split_whitespace().last() {
        added = n.parse().unwrap_or(0);
      }
    }

    if let Some(rest) = l.split_once("deletion")
      && let Some(n) = rest.0.trim_end_matches(", ").split_whitespace().last()
    {
      removed = n.parse().unwrap_or(0);
    }

    // Stop after the first non-empty line scanned from the bottom; it's
    // the summary line by construction.
    break;
  }

  (added, removed)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse_status_clean() {
    let text = "Working copy  (@) : abc 123\nParent commit (@-): def 456\n";

    assert_eq!(parse_status(text), (false, false, false));
  }

  #[test]
  fn parse_status_modified_and_untracked() {
    let text = "\
Working copy changes:
M src/main.rs
A new.rs
Untracked paths:
? scratch.txt
Working copy  (@) : abc 123
Parent commit (@-): def 456
";
    assert_eq!(parse_status(text), (true, true, false));
  }

  #[test]
  fn parse_status_with_conflicts() {
    let text = "\
Working copy changes:
M conflicted.rs
There are unresolved conflicts.
Working copy  (@) : abc 123
";
    assert_eq!(parse_status(text), (true, false, true));
  }

  #[test]
  fn parse_diff_stat_basic() {
    let text = "\
file.rs | 5 +++--
2 files changed, 3 insertions(+), 2 deletions(-)
";
    assert_eq!(parse_diff_stat_summary(text), (3, 2));
  }

  #[test]
  fn parse_diff_stat_singular() {
    let text = "file.rs | 1 +\n1 file changed, 1 insertion(+)\n";

    assert_eq!(parse_diff_stat_summary(text), (1, 0));
  }

  #[test]
  fn parse_diff_stat_empty() {
    assert_eq!(parse_diff_stat_summary(""), (0, 0));
  }
}
