use anyhow::Result;
use std::path::Path;

use crate::shell::config_loader::find_config;

/// Print the discovered config path and the file's raw contents. When the
/// config isn't found, prints the search root and a hint instead of
/// failing — informational by design.
pub fn run(cwd: &Path, config_override: Option<&Path>) -> Result<()> {
  match find_config(cwd, config_override) {
    Ok(path) => {
      println!("config: {}", path.display());

      let src = std::fs::read_to_string(&path)?;

      println!("---");
      print!("{src}");

      if !src.ends_with('\n') {
        println!();
      }

      Ok(())
    }
    Err(_) => {
      println!("config: (none found)");
      println!("searched upward from: {}", cwd.display());
      println!("hint: run `jjwt config create --project` to create one");

      Ok(())
    }
  }
}
