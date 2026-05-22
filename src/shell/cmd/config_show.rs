use anyhow::Result;
use std::path::Path;

use crate::shell::config_loader::{find_config, find_user_config};

/// Print both config layers (user + project) and their raw contents.
pub fn run(cwd: &Path, config_override: Option<&Path>) -> Result<()> {
  // User config layer.
  match find_user_config() {
    Some(path) => {
      println!("user config: {}", path.display());
      println!("---");

      let src = std::fs::read_to_string(&path)?;

      print!("{src}");

      if !src.ends_with('\n') {
        println!();
      }
    }
    None => {
      println!("user config: (none)");
      println!("hint: run `jjwt config create --user` to create one");
    }
  }

  println!();

  // Project config layer.
  match find_config(cwd, config_override) {
    Ok(path) => {
      println!("project config: {}", path.display());
      println!("---");

      let src = std::fs::read_to_string(&path)?;

      print!("{src}");

      if !src.ends_with('\n') {
        println!();
      }
    }
    Err(_) => {
      println!("project config: (none)");
      println!("searched upward from: {}", cwd.display());
      println!("hint: run `jjwt config create --project` to create one");
    }
  }

  Ok(())
}
