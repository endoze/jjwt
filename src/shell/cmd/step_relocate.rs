use anyhow::Result;
use std::path::Path;

use crate::core::plan::plan_relocate;
use crate::core::types::{OutputFormat, RelocateArgs};
use crate::shell::config_loader::load_merged_config;
use crate::shell::fs::RealFs;
use crate::shell::jj_lib::JjLib;
use crate::shell::observe::observe;
use crate::shell::proc::RealProc;
use crate::shell::runtime::{Runtime, execute};

pub fn run(
  cwd: &Path,
  config_path: Option<&Path>,
  old_name: String,
  new_name: String,
  rename_bookmark: bool,
  format: OutputFormat,
) -> Result<()> {
  let cfg = load_merged_config(cwd, config_path)?;

  let jj = JjLib::new(cwd)?;
  let fs = RealFs;
  let proc = RealProc;

  let obs = observe(
    &jj,
    &fs,
    cwd,
    Some(&old_name),
    cfg.worktree_path_template.as_deref(),
  )?;
  let args = RelocateArgs {
    old_name,
    new_name,
    rename_bookmark,
    format,
  };
  let plan = plan_relocate(&cfg, &args, &obs).map_err(|e| anyhow::anyhow!("{e}"))?;
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
