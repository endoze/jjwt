#![cfg(not(tarpaulin_include))]

use anyhow::Result;
use std::path::Path;

use crate::core::plan::plan_hook_show;
use crate::core::types::OutputFormat;
use crate::shell::config_loader::load_merged_config;
use crate::shell::fs::RealFs;
use crate::shell::jj_lib::JjLib;
use crate::shell::observe::observe;
use crate::shell::proc::RealProc;
use crate::shell::runtime::{Runtime, execute};

/// Display configured hooks, optionally expanding template variables.
pub fn run(
  cwd: &Path,
  config_path: Option<&Path>,
  expanded: bool,
  format: OutputFormat,
  source_filter: Option<crate::core::types::HookSource>,
) -> Result<()> {
  let cfg = load_merged_config(cwd, config_path)?;

  let jj = JjLib::new(cwd)?;
  let fs = RealFs;
  let proc = RealProc;

  let obs = if expanded {
    Some(observe(
      &jj,
      &fs,
      cwd,
      None,
      cfg.worktree_path_template.as_deref(),
    )?)
  } else {
    None
  };

  let plan = plan_hook_show(&cfg, expanded, obs.as_ref(), format, source_filter)
    .map_err(|e| anyhow::anyhow!("{e}"))?;

  let mut rt = Runtime::new(jj, fs, proc);

  if let Some(ref o) = obs {
    rt = rt.with_root(o.repo_root.clone());
  }

  let printed = execute(&plan, &mut rt)?;

  for line in printed {
    println!("{line}");
  }

  Ok(())
}
