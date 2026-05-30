#![cfg(not(tarpaulin_include))]

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::shell::config_loader::load_merged_config;
use crate::shell::fs::RealFs;
use crate::shell::jj_lib::JjLib;
use crate::shell::observe::observe;

/// Copy jj-ignored files from one workspace to another using CoW reflinks
/// when available, falling back to regular copy.
pub fn run(cwd: &Path, config_path: Option<&Path>, source: &str, dest: Option<&str>) -> Result<()> {
  let cfg = load_merged_config(cwd, config_path)?;

  let jj = JjLib::with_template(cwd, &cfg.worktree_path_template)?;
  let fs = RealFs;
  let obs = observe(&jj, &fs, cwd, None, &cfg.worktree_path_template)?;

  let source_ws = obs
    .workspaces
    .iter()
    .find(|w| w.name == source)
    .ok_or_else(|| anyhow::anyhow!("workspace '{source}' not found"))?;

  let dest_name = match dest {
    Some(d) => d.to_string(),
    None => obs
      .current_workspace
      .ok_or_else(|| anyhow::anyhow!("not inside a workspace (specify destination explicitly)"))?,
  };

  let dest_ws = obs
    .workspaces
    .iter()
    .find(|w| w.name == dest_name)
    .ok_or_else(|| anyhow::anyhow!("workspace '{dest_name}' not found"))?;

  if source_ws.name == dest_ws.name {
    anyhow::bail!("source and destination are the same workspace");
  }

  let source_path = &source_ws.path;
  let dest_path = &dest_ws.path;

  // Get tracked files in the source workspace to identify ignored files.
  let tracked = tracked_files(&obs.repo_root, source, &cfg.worktree_path_template)?;

  // Walk source directory, skip .jj/ and tracked files, copy the rest.
  let mut copied = 0u32;

  for entry in walkdir::WalkDir::new(source_path)
    .into_iter()
    .filter_entry(|e| !is_excluded(e.path(), source_path))
  {
    let entry = entry.with_context(|| format!("walk {}", source_path.display()))?;

    if !entry.file_type().is_file() {
      continue;
    }

    let rel = entry
      .path()
      .strip_prefix(source_path)
      .expect("walk yields children of root");

    if tracked.contains(rel) {
      continue;
    }

    let target = dest_path.join(rel);

    if let Some(parent) = target.parent() {
      std::fs::create_dir_all(parent)
        .with_context(|| format!("create dir {}", parent.display()))?;
    }

    copy_cow(entry.path(), &target)
      .with_context(|| format!("copy {} → {}", entry.path().display(), target.display()))?;

    copied += 1;
  }

  if copied == 0 {
    println!("No ignored files to copy.");
  } else {
    println!("Copied {copied} ignored file(s) from '{source}' to '{dest_name}'.");
  }

  Ok(())
}

/// List all tracked files in a workspace via `jj file list`.
fn tracked_files(repo_root: &Path, workspace: &str, template: &str) -> Result<HashSet<PathBuf>> {
  let ws_path = crate::shell::jj::workspace_dir(repo_root, workspace, template);

  let output = std::process::Command::new("jj")
    .current_dir(&ws_path)
    .args(["file", "list"])
    .output()
    .context("jj file list")?;

  if !output.status.success() {
    anyhow::bail!(
      "jj file list failed: {}",
      String::from_utf8_lossy(&output.stderr)
    );
  }

  let text = String::from_utf8_lossy(&output.stdout);
  let set: HashSet<PathBuf> = text
    .lines()
    .filter(|l| !l.trim().is_empty())
    .map(|l| PathBuf::from(l.trim()))
    .collect();

  Ok(set)
}

/// Check if a path should be excluded from the walk.
fn is_excluded(path: &Path, root: &Path) -> bool {
  let rel = match path.strip_prefix(root) {
    Ok(r) => r,
    Err(_) => return false,
  };

  rel
    .components()
    .any(|c| matches!(c, std::path::Component::Normal(s) if s == ".jj" || s == ".git"))
}

/// Copy a file using CoW reflink if available, falling back to regular copy.
fn copy_cow(from: &Path, to: &Path) -> Result<()> {
  #[cfg(target_os = "macos")]
  {
    use std::ffi::CString;
    let src = CString::new(from.to_str().unwrap_or_default())?;
    let dst = CString::new(to.to_str().unwrap_or_default())?;

    // SAFETY: clonefile is a standard macOS syscall. Both paths are valid
    // C strings. The call either succeeds or we fall back to regular copy.
    let ret = unsafe { libc::clonefile(src.as_ptr(), dst.as_ptr(), 0) };

    if ret == 0 {
      return Ok(());
    }
    // Fall through to regular copy on failure.
  }

  std::fs::copy(from, to)?;

  Ok(())
}
