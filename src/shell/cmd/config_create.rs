#![cfg(not(tarpaulin_include))]

use anyhow::{Context, Result, bail};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::shell::config_loader::user_config_dir;

const PROJECT_TEMPLATE: &str = include_str!("../../../fixtures/wt.example.toml");

/// Write a starter config file. Exactly one of `--project` or `--user` must
/// be passed.  When neither flag is given and stdin is a TTY, prompt
/// interactively.
pub fn run(cwd: &Path, project: bool, user: bool) -> Result<()> {
  let (project, _user) = if project == user {
    if !std::io::stdin().is_terminal() {
      bail!("specify exactly one of --project or --user");
    }

    prompt_config_type()?
  } else {
    (project, user)
  };

  if project {
    let dir = cwd.join(".config");
    let dest = dir.join("wt.toml");

    if dest.exists() {
      bail!(
        "refusing to overwrite existing config at {}",
        dest.display()
      );
    }

    write_config(&dir, &dest, PROJECT_TEMPLATE)?;
  } else {
    let dir = user_config_dir()?;
    let dest = dir.join("config.toml");

    if dest.exists() {
      bail!(
        "refusing to overwrite existing config at {}",
        dest.display()
      );
    }

    write_config(&dir, &dest, PROJECT_TEMPLATE)?;
  }

  Ok(())
}

fn write_config(dir: &Path, dest: &PathBuf, template: &str) -> Result<()> {
  std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
  std::fs::write(dest, template).with_context(|| format!("write {}", dest.display()))?;

  println!("Wrote {}", dest.display());

  Ok(())
}

fn prompt_config_type() -> Result<(bool, bool)> {
  eprintln!("Create config for:");
  eprintln!("  (p)roject  .config/wt.toml");
  eprintln!("  (u)ser     ~/.config/jjwt/config.toml");
  eprint!("Choice [p/u]: ");

  let mut input = String::new();

  std::io::stdin().read_line(&mut input)?;

  let choice = input.trim().to_lowercase();

  match choice.as_str() {
    "p" | "project" => Ok((true, false)),
    "u" | "user" => Ok((false, true)),
    _ => bail!("invalid choice '{choice}'; expected 'p' or 'u'"),
  }
}
