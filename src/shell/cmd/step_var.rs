#![cfg(not(tarpaulin_include))]

use anyhow::Result;
use std::path::Path;

use crate::shell::fs::RealFs;
use crate::shell::jj::Jj;
use crate::shell::jj_lib::JjLib;
use crate::shell::observe::observe;
use crate::shell::state;

/// Set a per-workspace variable in persistent state.
pub fn run_set(cwd: &Path, key: &str, value: &str) -> Result<()> {
  let (workspace, repo_root) = resolve_workspace(cwd)?;
  let mut st = state::load(&repo_root);

  st.set_var(&workspace, key, value);
  state::save(&repo_root, &st)?;

  Ok(())
}

/// Print the value of a per-workspace variable.
pub fn run_get(cwd: &Path, key: &str) -> Result<()> {
  let (workspace, repo_root) = resolve_workspace(cwd)?;
  let st = state::load(&repo_root);

  match st.get_vars(&workspace).get(key) {
    Some(val) => {
      println!("{val}");

      Ok(())
    }
    None => anyhow::bail!("variable '{key}' not set for workspace '{workspace}'"),
  }
}

/// List all variables for the current workspace.
pub fn run_list(cwd: &Path) -> Result<()> {
  let (workspace, repo_root) = resolve_workspace(cwd)?;
  let st = state::load(&repo_root);
  let ws_vars = st.get_vars(&workspace);

  if ws_vars.is_empty() {
    println!("(no variables set for workspace '{workspace}')");
  } else {
    let mut keys: Vec<_> = ws_vars.keys().collect();

    keys.sort();

    for k in keys {
      println!("{k}={}", ws_vars[k]);
    }
  }

  Ok(())
}

/// Delete a per-workspace variable from persistent state.
pub fn run_delete(cwd: &Path, key: &str) -> Result<()> {
  let (workspace, repo_root) = resolve_workspace(cwd)?;
  let mut st = state::load(&repo_root);

  match st.remove_var(&workspace, key) {
    Some(_) => {
      state::save(&repo_root, &st)?;

      Ok(())
    }
    None => anyhow::bail!("variable '{key}' not set for workspace '{workspace}'"),
  }
}

/// Determine the current workspace name and repo root from `cwd`.
fn resolve_workspace(cwd: &Path) -> Result<(String, std::path::PathBuf)> {
  let jj = JjLib::new(cwd)?;
  let fs = RealFs;
  let obs = observe(&jj, &fs, cwd, None, None)?;

  let workspace = obs
    .current_workspace
    .ok_or_else(|| anyhow::anyhow!("not inside a workspace"))?;

  let repo_root = jj.repo_root(cwd)?;

  Ok((workspace, repo_root))
}
