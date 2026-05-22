use anyhow::Result;
use std::path::Path;

use crate::core::types::{Workspace, WorkspaceDetails};

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
  /// Per-workspace details for list rendering (see [`WorkspaceDetails`]).
  fn workspace_details(&self, repo_root: &Path, workspace: &str) -> Result<WorkspaceDetails>;
  /// Commits ahead/behind trunk for the given workspace's `@`.
  /// Returns `(ahead, behind)`.
  fn workspace_ahead_behind_trunk(&self, repo_root: &Path, workspace: &str) -> Result<(u32, u32)>;
  /// Set of local bookmark names that have at least one remote-tracking
  /// variant (e.g. `name@origin`). Used to decide whether to render the
  /// "tracks remote" glyph in the list view.
  fn bookmarks_with_remote(&self, repo_root: &Path) -> Result<std::collections::HashSet<String>>;
  /// Name of the bookmark at `trunk()`, if any (typically "main" or "master").
  /// Used so `switch <default-branch>` routes to the default workspace.
  fn trunk_bookmark(&self, repo_root: &Path) -> Result<Option<String>>;
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
        .arg(&format!("{} & ::trunk()", name))
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

  fn workspace_details(&self, repo_root: &Path, workspace: &str) -> Result<WorkspaceDetails> {
    let ws_path = workspace_dir(repo_root, workspace);

    // Compose the workspace's `@` revset. We can't unconditionally use
    // `<ws>@` because the syntax depends on whether jj knows the workspace
    // by name; using `at_operation(@, <ws>@)` would over-complicate.
    // Empirically `<ws>@` works for all known workspaces, including
    // "default".
    let at_rev = format!("{workspace}@");

    // Single log call: change_id(8) \t description.first_line() \t committer_unix
    let tmpl = r#"change_id.shortest(8) ++ "\t" ++ description.first_line() ++ "\t" ++ committer.timestamp().format("%s") ++ "\n""#;
    let log_out = run(
      std::process::Command::new(&self.jj_path)
        .arg("--repository")
        .arg(repo_root)
        .arg("log")
        .arg("--no-graph")
        .arg("-r")
        .arg(&at_rev)
        .arg("-T")
        .arg(tmpl)
        .arg("--limit")
        .arg("1"),
    )?;
    let log_text = String::from_utf8_lossy(&log_out.stdout);
    let log_line = log_text.lines().next().unwrap_or("");
    let mut log_parts = log_line.splitn(3, '\t');
    let commit_short = log_parts.next().unwrap_or("").to_string();
    let message_first_line = log_parts.next().unwrap_or("").to_string();
    let ts_str = log_parts.next().unwrap_or("0").trim();
    let timestamp: i64 = ts_str.parse().unwrap_or(0);
    let now = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .map(|d| d.as_secs() as i64)
      .unwrap_or(0);
    let age_seconds = (now - timestamp).max(0);

    // is_trunk: `<ws>@ & latest(trunk())` non-empty
    let is_trunk = revset_nonempty(
      &self.jj_path,
      repo_root,
      &format!("{at_rev} & latest(trunk())"),
    )?;
    // is_ancestor_of_trunk: `<ws>@ & ::trunk()` non-empty
    let is_ancestor_of_trunk =
      revset_nonempty(&self.jj_path, repo_root, &format!("{at_rev} & ::trunk()"))?;

    // Status flags from `jj status` in the workspace.
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
    let (modified, untracked, conflicts) = parse_status(&status_text);

    // Head diff: `jj diff -r @ --stat`. Parse the trailing summary line.
    let diff_out = std::process::Command::new(&self.jj_path)
      .current_dir(&ws_path)
      .arg("diff")
      .arg("-r")
      .arg("@")
      .arg("--stat")
      .output()
      .map_err(|e| anyhow::anyhow!("spawn failed: {e}"))?;

    if !diff_out.status.success() {
      return Err(anyhow::anyhow!(
        "jj diff --stat failed: {}",
        String::from_utf8_lossy(&diff_out.stderr)
      ));
    }

    let diff_text = String::from_utf8_lossy(&diff_out.stdout);
    let (head_added, head_removed) = parse_diff_stat_summary(&diff_text);

    Ok(WorkspaceDetails {
      modified,
      untracked,
      conflicts,
      is_trunk,
      is_ancestor_of_trunk,
      commit_short,
      age_seconds,
      message_first_line,
      head_added,
      head_removed,
    })
  }

  fn workspace_ahead_behind_trunk(&self, repo_root: &Path, workspace: &str) -> Result<(u32, u32)> {
    let at_rev = format!("{workspace}@");
    let ahead = revset_count(&self.jj_path, repo_root, &format!("trunk()..{at_rev}"))?;
    let behind = revset_count(&self.jj_path, repo_root, &format!("{at_rev}..trunk()"))?;

    Ok((ahead, behind))
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

      if let Some((name, remote)) = trimmed.rsplit_once('@') {
        if !name.is_empty() && !remote.is_empty() {
          set.insert(name.to_string());
        }
      }
    }

    Ok(set)
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
}

fn revset_nonempty(jj: &Path, repo_root: &Path, revset: &str) -> Result<bool> {
  let out = run(
    std::process::Command::new(jj)
      .arg("--repository")
      .arg(repo_root)
      .arg("log")
      .arg("--no-graph")
      .arg("-r")
      .arg(revset)
      .arg("-T")
      .arg(r#""x""#)
      .arg("--limit")
      .arg("1"),
  )?;

  Ok(!out.stdout.is_empty())
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

    if let Some(rest) = l.split_once("deletion") {
      if let Some(n) = rest.0.trim_end_matches(", ").split_whitespace().last() {
        removed = n.parse().unwrap_or(0);
      }
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
