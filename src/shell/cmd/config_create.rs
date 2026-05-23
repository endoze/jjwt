#![cfg(not(tarpaulin_include))]

use anyhow::{Context, Result, bail};
use std::path::Path;

use crate::shell::config_loader::user_config_dir;

const PROJECT_TEMPLATE: &str = include_str!("../../../fixtures/wt.example.toml");

/// Write a starter config file. Exactly one of `--project` or `--user` must
/// be passed.
pub fn run(cwd: &Path, project: bool, user: bool) -> Result<()> {
  if project == user {
    bail!("specify exactly one of --project or --user");
  }

  if project {
    let dir = cwd.join(".config");
    let dest = dir.join("wt.toml");

    if dest.exists() {
      bail!(
        "refusing to overwrite existing config at {}",
        dest.display()
      );
    }

    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    std::fs::write(&dest, PROJECT_TEMPLATE).with_context(|| format!("write {}", dest.display()))?;

    println!("Wrote {}", dest.display());
  } else {
    let dir = user_config_dir()?;
    let dest = dir.join("config.toml");

    if dest.exists() {
      bail!(
        "refusing to overwrite existing config at {}",
        dest.display()
      );
    }

    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    std::fs::write(&dest, PROJECT_TEMPLATE).with_context(|| format!("write {}", dest.display()))?;

    println!("Wrote {}", dest.display());
  }

  Ok(())
}
