use anyhow::{Context, Result, bail};
use std::path::Path;

const PROJECT_TEMPLATE: &str = include_str!("../../../fixtures/wt.example.toml");

/// Write a starter `.config/wt.toml` under `cwd`. Refuses to overwrite an
/// existing file.
pub fn run(cwd: &Path, project: bool) -> Result<()> {
  if !project {
    bail!(
      "user config is not yet supported (planned for a later phase); pass --project to write the project config under .config/wt.toml"
    );
  }

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

  Ok(())
}
