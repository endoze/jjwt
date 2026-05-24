#![cfg(not(tarpaulin_include))]

use anyhow::Result;
use std::path::Path;

use crate::core::plan::plan_hook;
use crate::core::types::HookArgs;
use crate::shell::config_loader::load_merged_config;
use crate::shell::fs::RealFs;
use crate::shell::jj_lib::JjLib;
use crate::shell::observe::observe;
use crate::shell::proc::RealProc;
use crate::shell::runtime::{Runtime, execute};

/// Execute a named hook in the current workspace with optional variables.
pub fn run(
  cwd: &Path,
  config_path: Option<&Path>,
  hook_name: String,
  vars: Vec<String>,
) -> Result<()> {
  let cfg = load_merged_config(cwd, config_path)?;

  let jj = JjLib::with_template(cwd, &cfg.worktree_path_template)?;
  let fs = RealFs;
  let proc = RealProc;

  let obs = observe(&jj, &fs, cwd, None, &cfg.worktree_path_template)?;

  let current = obs
    .current_workspace
    .clone()
    .ok_or_else(|| anyhow::anyhow!("not inside a known workspace (cwd: {})", cwd.display()))?;
  let parsed_vars: Vec<(String, String)> = vars
    .iter()
    .filter_map(|s| {
      let (k, v) = s.split_once('=')?;

      Some((k.to_string(), v.to_string()))
    })
    .collect();

  let args = HookArgs {
    name: hook_name,
    current_workspace: current,
    vars: parsed_vars,
  };
  let plan = plan_hook(&cfg, &args, &obs)?;

  let repo_id = crate::shell::config_loader::resolve_repo_identity(&obs.repo_root);
  let mut rt = Runtime::new(jj, fs, proc)
    .with_root(obs.repo_root.clone())
    .with_repo_id(repo_id);

  execute(&plan, &mut rt)?;

  Ok(())
}
