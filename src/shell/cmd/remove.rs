use anyhow::Result;
use std::path::Path;

use crate::core::plan::plan_remove;
use crate::core::types::{OutputFormat, RemoveArgs};
use crate::shell::config_loader::{find_config, load_config};
use crate::shell::fs::RealFs;
use crate::shell::jj_lib::JjLib;
use crate::shell::observe::observe;
use crate::shell::proc::RealProc;
use crate::shell::runtime::{Runtime, execute};

/// Remove one or more workspaces. When `names` is empty, defaults to the
/// workspace whose path contains `cwd` (observation-derived).
#[allow(clippy::too_many_arguments)]
pub fn run(
  cwd: &Path,
  config_path: Option<&Path>,
  names: Vec<String>,
  force: bool,
  no_hooks: bool,
  no_delete_branch: bool,
  force_delete: bool,
  format: OutputFormat,
) -> Result<()> {
  let cfg_path = find_config(cwd, config_path)?;
  let cfg = load_config(&cfg_path)?;

  let jj = JjLib::new(cwd)?;
  let fs = RealFs;
  let proc = RealProc;

  let resolved_names: Vec<String> = if names.is_empty() {
    // Default to the current workspace. Reuse observe()'s containment
    // logic so the rule matches `list` and the alias dispatch.
    let obs0 = observe(&jj, &fs, cwd, None)?;
    let current = obs0.current_workspace.clone().ok_or_else(|| {
      anyhow::anyhow!("no workspace specified and cwd is not inside a known workspace")
    })?;

    vec![current]
  } else {
    names
  };

  let mut rt = Runtime::new(jj, fs, proc);

  for name in resolved_names {
    let obs = observe(&rt.jj, &rt.fs, cwd, Some(&name))?;

    rt.repo_root = obs.repo_root.clone();

    let args = RemoveArgs {
      name: name.clone(),
      force,
      no_hooks,
      no_delete_branch,
      force_delete,
      format,
    };
    let plan =
      plan_remove(&cfg, &args, &obs).map_err(|e| anyhow::anyhow!("remove '{name}': {e}"))?;

    let printed = execute(&plan, &mut rt)?;

    for line in printed {
      println!("{line}");
    }
  }

  Ok(())
}
