use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::core::config::parse;
use crate::core::types::{Config, MergedConfig};

/// Canonical directory for user-level jjwt config. Respects
/// `$XDG_CONFIG_HOME`; falls back to `$HOME/.config`.
pub fn user_config_dir() -> Result<PathBuf> {
  if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
    if !xdg.is_empty() {
      return Ok(PathBuf::from(xdg).join("jjwt"));
    }
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

pub fn load_config(path: &Path) -> Result<Config> {
  let src = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;

  parse(&src).map_err(|e| anyhow::anyhow!("{e}"))
}

/// Load and merge both config layers into a single `MergedConfig`.
///
/// - User config (`~/.config/jjwt/config.toml`) is optional.
/// - Project config (`.config/wt.toml`) is optional.
/// - `--config` override is treated as a project config.
/// - At least one layer must be present.
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

  Ok(MergedConfig::from_layers(
    user_cfg.as_ref(),
    project_cfg.as_ref(),
  ))
}
