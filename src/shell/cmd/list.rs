use anyhow::Result;
use std::path::Path;

use crate::core::plan::plan_list;
use crate::shell::config_loader::{find_config, load_config};
use crate::shell::fs::RealFs;
use crate::shell::jj::JjCli;
use crate::shell::observe::observe;
use crate::shell::proc::RealProc;
use crate::shell::runtime::{Runtime, execute};

pub fn run(cwd: &Path, config_path: Option<&Path>) -> Result<()> {
    let cfg_path = find_config(cwd, config_path)?;
    let cfg = load_config(&cfg_path)?;

    let jj = JjCli::new()?;
    let fs = RealFs;
    let proc = RealProc;

    let obs = observe(&jj, &fs, cwd, None)?;
    let plan = plan_list(&cfg, &obs).map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut rt = Runtime::new(jj, fs, proc).with_root(obs.repo_root.clone());
    let printed = execute(&plan, &mut rt)?;

    for line in printed {
        print!("{line}");
    }

    Ok(())
}
