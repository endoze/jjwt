use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::core::config::parse;
use crate::core::types::Config;

/// Locate the config file by walking up from `start` looking for `.config/wt.toml`.
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
