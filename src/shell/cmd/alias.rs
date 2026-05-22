use anyhow::{Context, Result};
use std::path::Path;

use crate::core::plan::plan_alias;
use crate::core::types::AliasArgs;
use crate::shell::config_loader::load_merged_config;
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
  let cfg = load_merged_config(cwd, config_path)
    .with_context(|| format!("alias '{name}': locate config"))?;
  let jj = JjLib::new(cwd)?;
  let fs = RealFs;
  let proc = RealProc;
  let obs = observe(&jj, &fs, cwd, None, cfg.worktree_path_template.as_deref())?;
  let args = AliasArgs { name, forwarded };
  let plan = plan_alias(&cfg, &args, &obs).map_err(|e| anyhow::anyhow!("{e}"))?;
  let repo_id = crate::shell::config_loader::resolve_repo_identity(&obs.repo_root);
  let mut rt = Runtime::new(jj, fs, proc)
    .with_root(obs.repo_root.clone())
    .with_repo_id(repo_id);

  execute(&plan, &mut rt)?;

  Ok(())
}
