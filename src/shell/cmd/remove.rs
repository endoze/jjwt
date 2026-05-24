#![cfg(not(tarpaulin_include))]

use anyhow::{Context, Result};
use std::path::Path;

use crate::core::format::{format_dry_run, format_dry_run_json};
use crate::core::plan::plan_remove;
use crate::core::types::{OutputFormat, RemoveArgs};
use crate::shell::config_loader::load_merged_config;
use crate::shell::fs::RealFs;
use crate::shell::jj::Jj;
use crate::shell::jj_lib::JjLib;
use crate::shell::observe::observe;
use crate::shell::proc::RealProc;
use crate::shell::runtime::{Runtime, execute};

/// Remove one or more workspaces. When `names` is empty, defaults to the
/// workspace whose path contains `cwd` (observation-derived).
pub fn run(
  cwd: &Path,
  config_path: Option<&Path>,
  names: Vec<String>,
  args: RemoveArgs,
) -> Result<()> {
  let cfg = load_merged_config(cwd, config_path)?;

  let jj = JjLib::with_template(cwd, &cfg.worktree_path_template)?;

  // Best-effort: clean up stale background-remove trash.
  if cfg.background_remove == Some(true)
    && let Ok(root) = jj.repo_root(cwd)
  {
    let _ = crate::shell::trash::sweep_trash(&root, std::time::Duration::from_secs(86400));
  }

  let fs = RealFs;
  let proc = RealProc;

  let resolved_names: Vec<String> = if names.is_empty() {
    // Default to the current workspace. Reuse observe()'s containment
    // logic so the rule matches `list` and the alias dispatch.
    let obs0 = observe(&jj, &fs, cwd, None, &cfg.worktree_path_template)?;
    let current = obs0.current_workspace.clone().ok_or_else(|| {
      anyhow::anyhow!("no workspace specified and cwd is not inside a known workspace")
    })?;

    vec![current]
  } else {
    names
  };

  let mut rt = Runtime::new(jj, fs, proc);

  for name in resolved_names {
    let obs = observe(
      &rt.jj,
      &rt.fs,
      cwd,
      Some(&name),
      &cfg.worktree_path_template,
    )?;

    rt.repo_root = obs.repo_root.clone();
    rt.repo_id = crate::shell::config_loader::resolve_repo_identity(&obs.repo_root);

    let per_name_args = RemoveArgs {
      name: name.clone(),
      ..args.clone()
    };
    let plan =
      plan_remove(&cfg, &per_name_args, &obs).with_context(|| format!("remove '{name}'"))?;

    if args.dry_run {
      let output = match args.format {
        OutputFormat::Json => format_dry_run_json(&plan.actions),
        _ => format_dry_run(&plan.actions),
      };

      println!("{output}");

      continue;
    }

    let printed = execute(&plan, &mut rt)?;

    for line in &printed {
      println!("{line}");
    }

    if args.format != OutputFormat::Json && obs.current_workspace.as_deref() == Some(name.as_str())
    {
      println!("cd:{}", obs.repo_root.display());
    }
  }

  Ok(())
}
