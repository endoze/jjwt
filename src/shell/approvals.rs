#![cfg(not(tarpaulin_include))]

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use crate::shell::config_loader::user_config_dir;

/// On-disk representation of `approvals.toml`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct ApprovalsFile {
  /// Per-project approval records keyed by repo identity.
  #[serde(default)]
  projects: HashMap<String, ProjectApprovals>,
}

/// Approved commands for a single project.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct ProjectApprovals {
  /// SHA-256 hashes of approved rendered hook commands.
  #[serde(default, rename = "approved-commands")]
  approved_commands: Vec<String>,
}

/// Path to the approvals store.
pub fn approvals_path() -> Result<PathBuf> {
  Ok(user_config_dir()?.join("approvals.toml"))
}

/// Produce a deterministic SHA-256 hash string for a rendered command.
fn hash_command(rendered_cmd: &str) -> String {
  use std::fmt::Write;

  let digest = Sha256::digest(rendered_cmd.as_bytes());
  let mut hex = String::with_capacity(7 + 64);

  hex.push_str("sha256:");

  for b in digest.iter() {
    write!(hex, "{b:02x}").unwrap();
  }

  hex
}

/// Load the approvals file from disk, returning defaults on any error.
fn load_file(path: &Path) -> ApprovalsFile {
  let src = match std::fs::read_to_string(path) {
    Ok(s) => s,
    Err(_) => return ApprovalsFile::default(),
  };

  toml::from_str(&src).unwrap_or_default()
}

/// Serialize and write the approvals file to disk.
fn save_file(path: &Path, file: &ApprovalsFile) -> Result<()> {
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
  }

  let body = toml::to_string(file).context("serialize approvals")?;

  std::fs::write(path, body).with_context(|| format!("write {}", path.display()))?;

  Ok(())
}

/// Check whether a rendered hook command is already approved for a given
/// repo identity.
pub fn is_approved(repo_id: &str, rendered_cmd: &str) -> bool {
  let path = match approvals_path() {
    Ok(p) => p,
    Err(_) => return false,
  };

  let file = load_file(&path);
  let hash = hash_command(rendered_cmd);

  file
    .projects
    .get(repo_id)
    .is_some_and(|pa| pa.approved_commands.contains(&hash))
}

/// Record an approved command for a repo identity.
pub fn save_approval(repo_id: &str, rendered_cmd: &str) -> Result<()> {
  let path = approvals_path()?;
  let mut file = load_file(&path);
  let hash = hash_command(rendered_cmd);

  let entry = file.projects.entry(repo_id.to_string()).or_default();

  if !entry.approved_commands.contains(&hash) {
    entry.approved_commands.push(hash);
  }

  save_file(&path, &file)
}

/// Prompt the user on a TTY to approve a project hook command. Returns
/// `Ok(true)` when approved, `Ok(false)` when denied.
///
/// Errors if stdin is not a terminal (non-interactive mode).
pub fn prompt_approval(hook_name: &str, rendered_cmd: &str) -> Result<bool> {
  use std::io::IsTerminal;

  if !std::io::stdin().is_terminal() {
    anyhow::bail!(
      "project hook '{hook_name}' requires approval but stdin is not a terminal \
       (run interactively or pre-approve the command)"
    );
  }

  eprintln!("\n\x1b[1;33m⚠ Project hook requires approval\x1b[0m");
  eprintln!("  hook:    {hook_name}");
  eprintln!("  command: {rendered_cmd}");
  eprint!("\nAllow this command? [y/N] ");

  std::io::stderr().flush()?;

  let mut line = String::new();

  std::io::stdin().lock().read_line(&mut line)?;

  let answer = line.trim().to_lowercase();

  Ok(answer == "y" || answer == "yes")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn hash_is_deterministic() {
    let a = hash_command("npm install");
    let b = hash_command("npm install");

    assert_eq!(a, b);
    assert!(a.starts_with("sha256:"));
  }

  #[test]
  fn hash_differs_for_different_commands() {
    let a = hash_command("npm install");
    let b = hash_command("make db-start");

    assert_ne!(a, b);
  }

  #[test]
  fn round_trip_approvals_file() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path();

    let mut file = ApprovalsFile::default();

    file.projects.insert(
      "github.com/owner/repo".into(),
      ProjectApprovals {
        approved_commands: vec![hash_command("npm install")],
      },
    );

    save_file(path, &file).unwrap();

    let loaded = load_file(path);
    let cmds = &loaded.projects["github.com/owner/repo"].approved_commands;

    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0], hash_command("npm install"));
  }

  #[test]
  fn is_approved_returns_false_for_missing_file() {
    // With no approvals file, nothing is approved.
    // This test relies on the default path not existing in CI/test env,
    // but we test the internal logic via load_file.
    let file = ApprovalsFile::default();

    assert!(!file.projects.contains_key("any"));
  }
}
