#![cfg(not(tarpaulin_include))]

use anyhow::Result;
use std::path::Path;

use crate::core::plan::plan_prune;
use crate::core::types::{OutputFormat, PruneArgs};
use crate::shell::config_loader::load_merged_config;
use crate::shell::fs::RealFs;
use crate::shell::jj_lib::JjLib;
use crate::shell::observe::observe_prune;
use crate::shell::proc::RealProc;
use crate::shell::runtime::{Runtime, execute};

/// Execute the `prune` command: remove workspaces whose bookmarks are merged.
pub fn run(
  cwd: &Path,
  config_path: Option<&Path>,
  dry_run: bool,
  no_hooks: bool,
  format: OutputFormat,
) -> Result<()> {
  let cfg = load_merged_config(cwd, config_path)?;

  let jj = JjLib::with_template(cwd, &cfg.worktree_path_template)?;
  let fs = RealFs;
  let proc = RealProc;

  let obs = observe_prune(&jj, &fs, cwd)?;
  let args = PruneArgs {
    dry_run,
    no_hooks,
    format,
  };
  let plan = plan_prune(&cfg, &args, &obs)?;
  let repo_id = crate::shell::config_loader::resolve_repo_identity(&obs.repo_root);
  let mut rt = Runtime::new(jj, fs, proc)
    .with_root(obs.repo_root.clone())
    .with_repo_id(repo_id);
  let printed = execute(&plan, &mut rt)?;

  for line in printed {
    println!("{line}");
  }

  Ok(())
}
