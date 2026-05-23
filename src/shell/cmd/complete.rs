#![cfg(not(tarpaulin_include))]

use anyhow::Result;
use std::path::Path;

use crate::shell::jj::Jj;
use crate::shell::jj_lib::JjLib;

/// Print workspace names, one per line, for shell completion.
/// Optionally filter by a prefix.
pub fn run(cwd: &Path, prefix: Option<&str>) -> Result<()> {
  let jj = match JjLib::new(cwd) {
    Ok(j) => j,
    Err(_) => return Ok(()), // silently return nothing if not in a jj repo
  };

  let repo_root = match jj.repo_root(cwd) {
    Ok(r) => r,
    Err(_) => return Ok(()),
  };

  let workspaces = jj.workspace_list(&repo_root)?;

  for ws in workspaces {
    if prefix.is_none_or(|p| ws.name.starts_with(p)) {
      println!("{}", ws.name);
    }
  }

  Ok(())
}
