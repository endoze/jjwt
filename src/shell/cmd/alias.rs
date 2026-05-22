use anyhow::{Context, Result};
use std::path::Path;

use crate::core::plan::plan_alias;
use crate::core::types::AliasArgs;
use crate::shell::config_loader::{find_config, load_config};
use crate::shell::fs::RealFs;
use crate::shell::jj_lib::JjLib;
use crate::shell::observe::observe;
use crate::shell::proc::RealProc;
use crate::shell::runtime::{Runtime, execute};

/// Run a user-defined alias (`jjwt <name> ...`). Tokens after the alias
/// name are forwarded as `{{ args }}` to the template.
pub fn run(
  cwd: &Path,
  config_path: Option<&Path>,
  name: String,
  forwarded: Vec<String>,
) -> Result<()> {
  let cfg_path =
    find_config(cwd, config_path).with_context(|| format!("alias '{name}': locate config"))?;
  let cfg = load_config(&cfg_path)?;
  let jj = JjLib::new(cwd)?;
  let fs = RealFs;
  let proc = RealProc;
  let obs = observe(&jj, &fs, cwd, None)?;
  let args = AliasArgs { name, forwarded };
  let plan = plan_alias(&cfg, &args, &obs).map_err(|e| anyhow::anyhow!("{e}"))?;
  let mut rt = Runtime::new(jj, fs, proc).with_root(obs.repo_root.clone());

  execute(&plan, &mut rt)?;

  Ok(())
}
