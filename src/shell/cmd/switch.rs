use anyhow::{Result, bail};
use std::path::Path;

use crate::core::plan::plan_switch;
use crate::core::types::{OutputFormat, SwitchArgs};
use crate::shell::config_loader::{find_config, load_config};
use crate::shell::fs::RealFs;
use crate::shell::jj::Jj;
use crate::shell::jj_lib::JjLib;
use crate::shell::observe::observe;
use crate::shell::proc::RealProc;
use crate::shell::runtime::{Runtime, execute as run_plan};
use crate::shell::state::{JjwtState, load as load_state, save as save_state};

/// Resolve the worktrunk-style shortcuts (`^`, `@`, `-`) against the
/// observed jj state plus persisted previous-workspace hint. Returns the
/// concrete workspace name or an error if a shortcut can't be resolved
/// (e.g. `-` when no previous switch has been recorded).
fn resolve_shortcut<J: Jj, F: crate::shell::fs::Fs>(
  name: &str,
  cwd: &Path,
  jj: &J,
  fs: &F,
) -> Result<(String, Option<String>)> {
  match name {
    "^" => {
      let repo_root = jj.repo_root(cwd)?;
      let trunk = jj
        .trunk_bookmark(&repo_root)?
        .ok_or_else(|| anyhow::anyhow!("`^` requires a trunk bookmark; none found"))?;
      let probe = observe(jj, fs, cwd, Some(&trunk))?;
      let cur = probe.current_workspace.clone();

      Ok((trunk, cur))
    }
    "@" => {
      let probe = observe(jj, fs, cwd, None)?;
      let cur = probe.current_workspace.clone();
      let n = cur
        .clone()
        .ok_or_else(|| anyhow::anyhow!("`@` requires being inside a known workspace"))?;

      Ok((n, cur))
    }
    "-" => {
      let repo_root = jj.repo_root(cwd)?;
      let st = load_state(&repo_root);
      let prev = st
        .previous_workspace
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no previous workspace recorded yet"))?;
      let probe = observe(jj, fs, cwd, Some(&prev))?;
      let cur = probe.current_workspace.clone();

      Ok((prev, cur))
    }
    _ => {
      let probe = observe(jj, fs, cwd, None)?;
      let cur = probe.current_workspace.clone();

      Ok((name.to_string(), cur))
    }
  }
}

#[allow(clippy::too_many_arguments)]
pub fn run(
  cwd: &Path,
  config_path: Option<&Path>,
  name: String,
  create: bool,
  rerun_hooks: bool,
  no_hooks: bool,
  execute: Option<String>,
  clobber: bool,
  format: OutputFormat,
) -> Result<()> {
  let cfg_path = find_config(cwd, config_path)?;
  let cfg = load_config(&cfg_path)?;

  let jj = JjLib::new(cwd)?;
  let fs = RealFs;
  let proc = RealProc;

  // Shortcuts (`^`, `@`, `-`) make no sense in combination with `--create`
  // — they all expand to existing workspaces. Surface a clear error rather
  // than confusing the user with a downstream "workspace already exists".
  if create && matches!(name.as_str(), "^" | "@" | "-") {
    bail!("`{name}` is a shortcut to an existing workspace and cannot be combined with --create");
  }

  let (resolved_name, current_before) = resolve_shortcut(&name, cwd, &jj, &fs)?;
  let obs = observe(&jj, &fs, cwd, Some(&resolved_name))?;
  let args = SwitchArgs {
    name: resolved_name,
    create,
    rerun_hooks,
    no_hooks,
    execute,
    clobber,
    format,
  };
  let plan = plan_switch(&cfg, &args, &obs).map_err(|e| anyhow::anyhow!("{e}"))?;
  let mut rt = Runtime::new(jj, fs, proc).with_root(obs.repo_root.clone());
  let printed = run_plan(&plan, &mut rt)?;

  for line in printed {
    println!("{line}");
  }

  // Persist the workspace we just *came from* so `jjwt switch -` knows
  // where to return on the next call. Best-effort: failure to save state
  // doesn't fail the switch (the switch already happened).
  if let Some(prev) = current_before {
    if prev != args.name {
      let state = JjwtState {
        previous_workspace: Some(prev),
      };
      let _ = save_state(&obs.repo_root, &state);
    }
  }

  Ok(())
}
