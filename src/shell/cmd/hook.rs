use anyhow::Result;
use std::path::Path;

use crate::core::plan::plan_hook;
use crate::core::types::HookArgs;
use crate::shell::config_loader::{find_config, load_config};
use crate::shell::fs::RealFs;
use crate::shell::jj::{Jj, JjCli};
use crate::shell::observe::observe;
use crate::shell::proc::RealProc;
use crate::shell::runtime::{Runtime, execute};

pub fn run(cwd: &Path, config_path: Option<&Path>, hook_name: String) -> Result<()> {
    let cfg_path = find_config(cwd, config_path)?;
    let cfg = load_config(&cfg_path)?;

    let jj = JjCli::new()?;
    let fs = RealFs;
    let proc = RealProc;

    let repo_root = jj.repo_root(cwd)?;
    let workspaces = jj.workspace_list(&repo_root)?;
    let cwd_abs = std::fs::canonicalize(cwd)?;
    let current = workspaces
        .iter()
        .find(|w| {
            std::fs::canonicalize(&w.path)
                .map(|p| cwd_abs.starts_with(p))
                .unwrap_or(false)
        })
        .ok_or_else(|| anyhow::anyhow!("not inside a known workspace (cwd: {})", cwd.display()))?
        .name
        .clone();

    let obs = observe(&jj, &fs, cwd, Some(&current))?;
    let args = HookArgs { name: hook_name, current_workspace: current };
    let plan = plan_hook(&cfg, &args, &obs).map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut rt = Runtime::new(jj, fs, proc).with_root(obs.repo_root.clone());

    execute(&plan, &mut rt)?;

    Ok(())
}
