use anyhow::Result;
use std::path::Path;

use crate::core::plan::plan_hook_show;
use crate::core::types::OutputFormat;
use crate::shell::config_loader::{find_config, load_config};
use crate::shell::fs::RealFs;
use crate::shell::jj_lib::JjLib;
use crate::shell::observe::observe;
use crate::shell::proc::RealProc;
use crate::shell::runtime::{Runtime, execute};

pub fn run(
  cwd: &Path,
  config_path: Option<&Path>,
  expanded: bool,
  format: OutputFormat,
) -> Result<()> {
  let cfg_path = find_config(cwd, config_path)?;
  let cfg = load_config(&cfg_path)?;

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

  let plan =
    plan_hook_show(&cfg, expanded, obs.as_ref(), format).map_err(|e| anyhow::anyhow!("{e}"))?;

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
