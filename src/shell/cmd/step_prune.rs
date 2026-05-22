use anyhow::Result;
use std::path::Path;

use crate::core::plan::plan_prune;
use crate::core::types::{OutputFormat, PruneArgs};
use crate::shell::config_loader::{find_config, load_config};
use crate::shell::fs::RealFs;
use crate::shell::jj_lib::JjLib;
use crate::shell::observe::observe_prune;
use crate::shell::proc::RealProc;
use crate::shell::runtime::{Runtime, execute};

pub fn run(
  cwd: &Path,
  config_path: Option<&Path>,
  dry_run: bool,
  no_hooks: bool,
  format: OutputFormat,
) -> Result<()> {
  let cfg_path = find_config(cwd, config_path)?;
  let cfg = load_config(&cfg_path)?;

  let jj = JjLib::new(cwd)?;
  let fs = RealFs;
  let proc = RealProc;

  let obs = observe_prune(&jj, &fs, cwd)?;
  let args = PruneArgs {
    dry_run,
    no_hooks,
    format,
  };
  let plan = plan_prune(&cfg, &args, &obs).map_err(|e| anyhow::anyhow!("{e}"))?;
  let mut rt = Runtime::new(jj, fs, proc).with_root(obs.repo_root.clone());
  let printed = execute(&plan, &mut rt)?;

  for line in printed {
    println!("{line}");
  }

  Ok(())
}
