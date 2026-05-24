#![cfg(not(tarpaulin_include))]

use anyhow::{Result, bail};
use std::path::Path;

use crate::core::format::{format_dry_run, format_dry_run_json};
use crate::core::plan::plan_switch;
use crate::core::types::{OutputFormat, SwitchArgs};
use crate::shell::config_loader::load_merged_config;
use crate::shell::fs::RealFs;
use crate::shell::jj::Jj;
use crate::shell::jj_lib::JjLib;
use crate::shell::observe::observe;
use crate::shell::proc::RealProc;
use crate::shell::runtime::{Runtime, execute as run_plan};
use crate::shell::state::{load as load_state, save as save_state};

/// Resolve `pr:N` by querying the GitHub CLI for the PR's head branch.
fn resolve_pr(n: u32, cwd: &Path) -> Result<String> {
  let out = std::process::Command::new("gh")
    .args([
      "pr",
      "view",
      &n.to_string(),
      "--json",
      "headRefName",
      "-q",
      ".headRefName",
    ])
    .current_dir(cwd)
    .output()
    .map_err(|e| {
      if e.kind() == std::io::ErrorKind::NotFound {
        anyhow::anyhow!("gh CLI not found; install from https://cli.github.com")
      } else {
        anyhow::anyhow!("failed to run gh: {e}")
      }
    })?;

  if !out.status.success() {
    return Err(anyhow::anyhow!(
      "gh pr view {n} failed: {}",
      String::from_utf8_lossy(&out.stderr).trim()
    ));
  }

  let branch = String::from_utf8_lossy(&out.stdout).trim().to_string();

  if branch.is_empty() {
    return Err(anyhow::anyhow!("gh pr view {n} returned empty branch name"));
  }

  Ok(branch)
}

/// Resolve `mr:N` by querying the GitLab CLI for the MR's source branch.
fn resolve_mr(n: u32, cwd: &Path) -> Result<String> {
  let out = std::process::Command::new("glab")
    .args(["mr", "view", &n.to_string(), "-F", "json"])
    .current_dir(cwd)
    .output()
    .map_err(|e| {
      if e.kind() == std::io::ErrorKind::NotFound {
        anyhow::anyhow!("glab CLI not found; install from https://gitlab.com/gitlab-org/cli")
      } else {
        anyhow::anyhow!("failed to run glab: {e}")
      }
    })?;

  if !out.status.success() {
    return Err(anyhow::anyhow!(
      "glab mr view {n} failed: {}",
      String::from_utf8_lossy(&out.stderr).trim()
    ));
  }

  let json: serde_json::Value = serde_json::from_slice(&out.stdout)
    .map_err(|e| anyhow::anyhow!("failed to parse glab JSON: {e}"))?;
  let branch = json["source_branch"]
    .as_str()
    .ok_or_else(|| anyhow::anyhow!("glab mr view {n}: missing source_branch in JSON"))?
    .to_string();

  Ok(branch)
}

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
  let probe = observe(jj, fs, cwd, None, crate::core::types::DEFAULT_WORKTREE_PATH_TEMPLATE)?;
  let cur = probe.current_workspace;

  let resolved = match name {
    "^" => {
      let repo_root = jj.repo_root(cwd)?;

      jj.trunk_bookmark(&repo_root)?
        .ok_or_else(|| anyhow::anyhow!("`^` requires a trunk bookmark; none found"))?
    }
    "@" => cur
      .as_ref()
      .ok_or_else(|| anyhow::anyhow!("`@` requires being inside a known workspace"))?
      .clone(),
    "-" => {
      let repo_root = jj.repo_root(cwd)?;
      let st = load_state(&repo_root);

      st.previous_workspace
        .ok_or_else(|| anyhow::anyhow!("no previous workspace recorded yet"))?
    }
    name if name.starts_with("pr:") => {
      let n: u32 = name[3..]
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid PR number: {}", &name[3..]))?;

      resolve_pr(n, cwd)?
    }
    name if name.starts_with("mr:") => {
      let n: u32 = name[3..]
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid MR number: {}", &name[3..]))?;

      resolve_mr(n, cwd)?
    }
    _ => name.to_string(),
  };

  Ok((resolved, cur))
}

/// Determine whether a workspace should be auto-created.
///
/// Returns `Ok(true)` when the workspace does not yet exist **and** either
/// the name matches a known bookmark or originated from a `pr:`/`mr:`
/// shortcut. Returns `Ok(false)` when the workspace already exists.
fn should_auto_create<J: Jj, F: crate::shell::fs::Fs>(
  jj: &J,
  fs: &F,
  cwd: &Path,
  resolved_name: &str,
  original_name: &str,
) -> Result<bool> {
  let probe = observe(jj, fs, cwd, Some(resolved_name), crate::core::types::DEFAULT_WORKTREE_PATH_TEMPLATE)?;
  let ws_exists = probe.workspaces.iter().any(|w| w.name == resolved_name);

  if ws_exists {
    return Ok(false);
  }

  let repo_root = jj.repo_root(cwd)?;

  // pr:/mr: always need a fetch to pull the remote branch.
  if original_name.starts_with("pr:") || original_name.starts_with("mr:") {
    let _ = jj.git_fetch(&repo_root);
  }

  // Auto-create if a bookmark with this name already exists (local
  // or remote). This covers checking out an existing branch without
  // requiring --create. For truly new branches (no bookmark), the
  // user must pass --create explicitly.
  let bookmark_known = jj.bookmark_exists(&repo_root, resolved_name)?;

  Ok(bookmark_known || original_name.starts_with("pr:") || original_name.starts_with("mr:"))
}

/// Execute the `switch` command: resolve the target, plan, and run.
pub fn run(cwd: &Path, config_path: Option<&Path>, mut args: SwitchArgs) -> Result<()> {
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

  // Shortcuts (`^`, `@`, `-`) make no sense in combination with `--create`
  // — they all expand to existing workspaces. Surface a clear error rather
  // than confusing the user with a downstream "workspace already exists".
  if args.create && matches!(args.name.as_str(), "^" | "@" | "-") {
    bail!(
      "`{}` is a shortcut to an existing workspace and cannot be combined with --create",
      args.name
    );
  }

  let (resolved_name, current_before) = resolve_shortcut(&args.name, cwd, &jj, &fs)?;

  if let Some(ref base) = args.base {
    let (resolved_base, _) = resolve_shortcut(base, cwd, &jj, &fs)?;

    args.base = Some(resolved_base);
  }

  args.create = args.create || should_auto_create(&jj, &fs, cwd, &resolved_name, &args.name)?;
  args.name = resolved_name;

  let obs = observe(
    &jj,
    &fs,
    cwd,
    Some(&args.name),
    &cfg.worktree_path_template,
  )?;
  let plan = plan_switch(&cfg, &args, &obs)?;

  if args.dry_run {
    let output = match args.format {
      OutputFormat::Json => format_dry_run_json(&plan.actions),
      _ => format_dry_run(&plan.actions),
    };

    println!("{output}");

    return Ok(());
  }

  let repo_id = crate::shell::config_loader::resolve_repo_identity(&obs.repo_root);
  let mut rt = Runtime::new(jj, fs, proc)
    .with_root(obs.repo_root.clone())
    .with_repo_id(repo_id);
  let printed = run_plan(&plan, &mut rt)?;

  for line in printed {
    println!("{line}");
  }

  // Persist the workspace we just *came from* so `jjwt switch -` knows
  // where to return on the next call. Best-effort: failure to save state
  // doesn't fail the switch (the switch already happened).
  if let Some(prev) = current_before
    && prev != args.name
  {
    let mut state = load_state(&obs.repo_root);

    state.previous_workspace = Some(prev);

    let _ = save_state(&obs.repo_root, &state);
  }

  Ok(())
}
