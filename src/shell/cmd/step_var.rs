#![cfg(not(tarpaulin_include))]

use anyhow::Result;
use std::path::Path;

use crate::shell::config_loader::load_merged_config;
use crate::shell::fs::RealFs;
use crate::shell::jj_lib::JjLib;
use crate::shell::observe::observe;
use crate::shell::state;

/// Set a per-workspace variable in persistent state.
pub fn run_set(cwd: &Path, config_path: Option<&Path>, key: &str, value: &str) -> Result<()> {
  let (workspace, repo_root) = resolve_workspace(cwd, config_path)?;
  let mut st = state::load(&repo_root);

  st.set_var(&workspace, key, value);
  state::save(&repo_root, &st)?;

  Ok(())
}

/// Print the value of a per-workspace variable.
pub fn run_get(cwd: &Path, config_path: Option<&Path>, key: &str) -> Result<()> {
  let (workspace, repo_root) = resolve_workspace(cwd, config_path)?;
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
pub fn run_list(cwd: &Path, config_path: Option<&Path>) -> Result<()> {
  let (workspace, repo_root) = resolve_workspace(cwd, config_path)?;
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
pub fn run_delete(cwd: &Path, config_path: Option<&Path>, key: &str) -> Result<()> {
  let (workspace, repo_root) = resolve_workspace(cwd, config_path)?;
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
fn resolve_workspace(
  cwd: &Path,
  config_path: Option<&Path>,
) -> Result<(String, std::path::PathBuf)> {
  let cfg = load_merged_config(cwd, config_path)?;

  let jj = JjLib::with_template(cwd, &cfg.worktree_path_template)?;
  let fs = RealFs;
  let obs = observe(&jj, &fs, cwd, None, &cfg.worktree_path_template)?;

  let workspace = obs
    .current_workspace
    .ok_or_else(|| anyhow::anyhow!("not inside a workspace"))?;

  Ok((workspace, obs.repo_root))
}
