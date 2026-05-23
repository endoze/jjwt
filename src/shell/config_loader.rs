#![cfg(not(tarpaulin_include))]

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::core::config::parse;
use crate::core::types::{Config, MergedConfig};

/// Canonical directory for user-level jjwt config. Respects
/// `$XDG_CONFIG_HOME`; falls back to `$HOME/.config`.
pub fn user_config_dir() -> Result<PathBuf> {
  if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
    && !xdg.is_empty()
  {
    return Ok(PathBuf::from(xdg).join("jjwt"));
  }

  let home = std::env::var("HOME").context("$HOME not set")?;

  Ok(PathBuf::from(home).join(".config").join("jjwt"))
}

/// Return the user config path if the file exists.
pub fn find_user_config() -> Option<PathBuf> {
  let dir = user_config_dir().ok()?;
  let path = dir.join("config.toml");

  if path.is_file() { Some(path) } else { None }
}

/// Locate the project config by walking up from `start` looking for `.config/wt.toml`.
pub fn find_config(start: &Path, override_path: Option<&Path>) -> Result<PathBuf> {
  if let Some(p) = override_path {
    return Ok(p.to_path_buf());
  }

  let mut p = start.to_path_buf();

  loop {
    let candidate = p.join(".config").join("wt.toml");

    if candidate.is_file() {
      return Ok(candidate);
    }

    if !p.pop() {
      return Err(anyhow::anyhow!(
        ".config/wt.toml not found (searched upward from {start:?})"
      ));
    }
  }
}

/// Parse a config file at `path` into a `Config` struct.
pub fn load_config(path: &Path) -> Result<Config> {
  let src = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;

  Ok(parse(&src)?)
}

/// Resolve a repository's identity string for matching against
/// `[projects."<id>"]` keys in the user config.
///
/// Strategy: read the jj-managed git remote `origin` URL, normalize it
/// to `host/owner/repo` form. Falls back to the repo root directory
/// basename when no git remote is available.
pub fn resolve_repo_identity(repo_root: &Path) -> Option<String> {
  // jj stores the git backend pointer at .jj/repo/store/git — either a
  // directory (colocated) or a file containing the path to the bare git
  // repo. In either case, the git config lives at <git_dir>/config.
  let git_pointer = repo_root.join(".jj/repo/store/git");
  let git_dir = if git_pointer.is_dir() {
    git_pointer
  } else if git_pointer.is_file() {
    let content = std::fs::read_to_string(&git_pointer).ok()?;

    PathBuf::from(content.trim())
  } else {
    return repo_root_basename(repo_root);
  };

  let git_config_path = git_dir.join("config");
  let git_config = std::fs::read_to_string(&git_config_path).ok()?;

  parse_origin_url(&git_config)
    .and_then(|url| normalize_remote_url(&url))
    .or_else(|| repo_root_basename(repo_root))
}

/// Extract the directory basename as a fallback repo identity.
fn repo_root_basename(repo_root: &Path) -> Option<String> {
  repo_root
    .file_name()
    .map(|n| n.to_string_lossy().into_owned())
}

/// Extract the URL for `[remote "origin"]` from a git config string.
fn parse_origin_url(config: &str) -> Option<String> {
  let mut in_origin = false;

  for line in config.lines() {
    let trimmed = line.trim();

    if trimmed.starts_with('[') {
      in_origin = trimmed == "[remote \"origin\"]";

      continue;
    }

    if in_origin && let Some(rest) = trimmed.strip_prefix("url") {
      let rest = rest.trim_start();

      if let Some(val) = rest.strip_prefix('=') {
        return Some(val.trim().to_string());
      }
    }
  }

  None
}

/// Normalize a git remote URL to `host/owner/repo` form.
///
/// Handles:
/// - `git@github.com:owner/repo.git`
/// - `ssh://git@github.com/owner/repo.git`
/// - `https://github.com/owner/repo.git`
/// - `https://github.com/owner/repo`
pub fn normalize_remote_url(url: &str) -> Option<String> {
  let url = url.trim();

  // SCP-style: git@host:owner/repo.git
  if let Some(rest) = url.strip_prefix("git@") {
    let (host, path) = rest.split_once(':')?;
    let path = path.strip_suffix(".git").unwrap_or(path);

    return Some(format!("{host}/{path}"));
  }

  // URL-style: https://host/owner/repo.git or ssh://git@host/owner/repo.git
  let without_scheme = url
    .strip_prefix("https://")
    .or_else(|| url.strip_prefix("ssh://git@"))
    .or_else(|| url.strip_prefix("ssh://"))?;

  let path = without_scheme
    .strip_suffix(".git")
    .unwrap_or(without_scheme);

  Some(path.to_string())
}

/// Find the jj repo root by walking upward from `start` looking for a
/// `.jj/` directory.
fn find_repo_root(start: &Path) -> Option<PathBuf> {
  crate::shell::jj::find_nearest_jj_dir(start)
}

/// Load and merge both config layers into a single `MergedConfig`.
///
/// - User config (`~/.config/jjwt/config.toml`) is optional.
/// - Project config (`.config/wt.toml`) is optional.
/// - `--config` override is treated as a project config.
/// - At least one layer must be present.
/// - When a user config contains `[projects."<id>"]`, the matching entry
///   acts as a middle layer between user defaults and project config.
pub fn load_merged_config(cwd: &Path, override_path: Option<&Path>) -> Result<MergedConfig> {
  let user_cfg = match find_user_config() {
    Some(p) => Some(load_config(&p)?),
    None => None,
  };

  let project_cfg = match find_config(cwd, override_path) {
    Ok(p) => Some(load_config(&p)?),
    Err(_) if user_cfg.is_some() => None,
    Err(e) => return Err(e),
  };

  let repo_id = find_repo_root(cwd).and_then(|root| resolve_repo_identity(&root));

  Ok(MergedConfig::from_layers_with_project_id(
    user_cfg.as_ref(),
    repo_id.as_deref(),
    project_cfg.as_ref(),
  ))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn normalize_ssh_scp_url() {
    assert_eq!(
      normalize_remote_url("git@github.com:owner/repo.git"),
      Some("github.com/owner/repo".into())
    );
  }

  #[test]
  fn normalize_https_url() {
    assert_eq!(
      normalize_remote_url("https://github.com/owner/repo.git"),
      Some("github.com/owner/repo".into())
    );
  }

  #[test]
  fn normalize_https_url_no_suffix() {
    assert_eq!(
      normalize_remote_url("https://github.com/owner/repo"),
      Some("github.com/owner/repo".into())
    );
  }

  #[test]
  fn normalize_ssh_scheme_url() {
    assert_eq!(
      normalize_remote_url("ssh://git@github.com/owner/repo.git"),
      Some("github.com/owner/repo".into())
    );
  }

  #[test]
  fn normalize_garbage_returns_none() {
    assert_eq!(normalize_remote_url("not-a-url"), None);
  }

  #[test]
  fn parse_origin_url_from_config() {
    let config = r#"
[core]
  repositoryformatversion = 0
[remote "origin"]
  url = git@github.com:owner/repo.git
  fetch = +refs/heads/*:refs/remotes/origin/*
[remote "upstream"]
  url = https://github.com/other/repo.git
"#;

    assert_eq!(
      parse_origin_url(config),
      Some("git@github.com:owner/repo.git".into())
    );
  }

  #[test]
  fn parse_origin_url_missing() {
    let config = r#"
[core]
  repositoryformatversion = 0
[remote "upstream"]
  url = https://github.com/other/repo.git
"#;

    assert_eq!(parse_origin_url(config), None);
  }
}
