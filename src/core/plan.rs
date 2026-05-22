use crate::core::format::{format_age, format_list_table};
use crate::core::template::render;
use crate::core::types::*;
use std::path::PathBuf;

fn workspace_path(root: &std::path::Path, name: &str) -> PathBuf {
  if name == "default" {
    root.to_path_buf()
  } else {
    root.join(".worktrees").join(name)
  }
}

fn hook_env(name: &str, path: &std::path::Path) -> Vec<(String, String)> {
  vec![
    ("JJWT_WORKSPACE".into(), name.into()),
    ("JJWT_WORKSPACE_PATH".into(), path.display().to_string()),
  ]
}

fn render_pre_start_hooks(
  cfg: &Config,
  branch: &str,
  cwd: &std::path::Path,
) -> Result<Vec<Action>, CoreError> {
  let ctx = RenderContext {
    branch: branch.into(),
  };
  let mut out = Vec::new();

  for group in &cfg.pre_start {
    for (name, tmpl) in group {
      let rendered = render(tmpl, &ctx)?;

      out.push(Action::RunHook {
        name: name.clone(),
        rendered_cmd: rendered,
        cwd: cwd.to_path_buf(),
        env: hook_env(branch, cwd),
      });
    }
  }

  Ok(out)
}

pub fn plan_switch(
  cfg: &Config,
  args: &SwitchArgs,
  obs: &ObservedState,
) -> Result<Plan, CoreError> {
  if !obs.is_jj_repo {
    return Err(CoreError::NotJjRepo);
  }

  let mut plan = Plan::new();

  if args.create {
    if obs.workspaces.iter().any(|w| w.name == args.name) {
      return Err(CoreError::WorkspaceExists(args.name.clone()));
    }

    let ws_path = workspace_path(&obs.repo_root, &args.name);

    plan.push(Action::JjWorkspaceAdd {
      name: args.name.clone(),
      path: ws_path.clone(),
    });
    plan.push(Action::JjBookmarkCreate {
      name: args.name.clone(),
      workspace: args.name.clone(),
    });

    for a in render_pre_start_hooks(cfg, &args.name, &ws_path)? {
      plan.push(a);
    }

    plan.push(Action::PrintLine(ws_path.display().to_string()));

    return Ok(plan);
  }

  // Non-create: prefer a direct workspace match; otherwise honor the
  // trunk-bookmark fallback (e.g. `switch main` -> default workspace).
  let ws = obs
    .workspaces
    .iter()
    .find(|w| w.name == args.name)
    .or_else(|| {
      obs
        .target_resolved_workspace
        .as_deref()
        .and_then(|n| obs.workspaces.iter().find(|w| w.name == n))
    })
    .ok_or_else(|| CoreError::WorkspaceMissing(args.name.clone()))?;

  if ws.stale {
    plan.push(Action::JjWorkspaceUpdateStale {
      name: ws.name.clone(),
    });
  }

  if args.rerun_hooks {
    for a in render_pre_start_hooks(cfg, &ws.name, &ws.path)? {
      plan.push(a);
    }
  }

  plan.push(Action::PrintLine(ws.path.display().to_string()));

  Ok(plan)
}

fn render_pre_remove_hooks(
  cfg: &Config,
  branch: &str,
  cwd: &std::path::Path,
) -> Result<Vec<Action>, CoreError> {
  let ctx = RenderContext {
    branch: branch.into(),
  };
  let mut out = Vec::new();

  for group in &cfg.pre_remove {
    for (name, tmpl) in group {
      let rendered = render(tmpl, &ctx)?;

      out.push(Action::RunHook {
        name: name.clone(),
        rendered_cmd: rendered,
        cwd: cwd.to_path_buf(),
        env: hook_env(branch, cwd),
      });
    }
  }

  Ok(out)
}

pub fn plan_remove(
  cfg: &Config,
  args: &RemoveArgs,
  obs: &ObservedState,
) -> Result<Plan, CoreError> {
  if !obs.is_jj_repo {
    return Err(CoreError::NotJjRepo);
  }

  let ws = obs
    .workspaces
    .iter()
    .find(|w| w.name == args.name)
    .ok_or_else(|| CoreError::WorkspaceMissing(args.name.clone()))?;

  if !args.force {
    if obs.target_workspace_dirty {
      return Err(CoreError::WorkspaceDirty(args.name.clone()));
    }

    if obs.target_bookmark_exists && !obs.target_bookmark_merged {
      return Err(CoreError::BookmarkUnmerged(args.name.clone()));
    }
  }

  let ws_path = ws.path.clone();
  let mut plan = Plan::new();

  for a in render_pre_remove_hooks(cfg, &args.name, &ws_path)? {
    plan.push(a);
  }

  plan.push(Action::JjWorkspaceForget {
    name: args.name.clone(),
  });
  plan.push(Action::DeleteDir { path: ws_path });

  if obs.target_bookmark_exists && obs.target_bookmark_merged {
    plan.push(Action::JjBookmarkDelete {
      name: args.name.clone(),
    });
  }

  Ok(plan)
}

pub fn plan_hook(cfg: &Config, args: &HookArgs, obs: &ObservedState) -> Result<Plan, CoreError> {
  let ws = obs
    .workspaces
    .iter()
    .find(|w| w.name == args.current_workspace)
    .ok_or_else(|| CoreError::WorkspaceMissing(args.current_workspace.clone()))?;

  let mut matches: Vec<&str> = Vec::new();

  for group in cfg.pre_start.iter().chain(cfg.pre_remove.iter()) {
    if let Some(tmpl) = group.get(&args.name) {
      matches.push(tmpl.as_str());
    }
  }

  let tmpl = match matches.len() {
    0 => return Err(CoreError::HookNotFound(args.name.clone())),
    1 => matches[0],
    _ => return Err(CoreError::HookAmbiguous(args.name.clone())),
  };

  let ctx = RenderContext {
    branch: args.current_workspace.clone(),
  };
  let rendered = render(tmpl, &ctx)?;
  let mut plan = Plan::new();

  plan.push(Action::RunHook {
    name: args.name.clone(),
    rendered_cmd: rendered,
    cwd: ws.path.clone(),
    env: hook_env(&args.current_workspace, &ws.path),
  });

  Ok(plan)
}

fn trunk_rel(d: &WorkspaceDetails, ahead: u32, behind: u32) -> Option<TrunkRel> {
  if d.is_trunk {
    return Some(TrunkRel::IsTrunk);
  }

  if d.is_ancestor_of_trunk {
    return Some(TrunkRel::Ancestor);
  }

  match (ahead, behind) {
    (0, 0) => Some(TrunkRel::None),
    (_, 0) => Some(TrunkRel::Ahead),
    (0, _) => Some(TrunkRel::Behind),
    (_, _) => Some(TrunkRel::Diverged),
  }
}

fn build_list_row(
  cfg: &Config,
  obs_row: &ObservedListRow,
  is_current: bool,
) -> Result<ListRow, CoreError> {
  let w = &obs_row.workspace;
  let d = &obs_row.details;
  let is_default = w.name == "default";
  let url = if let Some(list) = &cfg.list {
    let ctx = RenderContext {
      branch: w.name.clone(),
    };

    render(&list.url, &ctx)?
  } else {
    String::new()
  };

  // A workspace shows `|` when its bookmark has a remote variant. As a
  // small convenience: workspaces sitting exactly on `trunk()` also show
  // `|` since trunk in practice tracks an upstream — this catches the
  // `default` workspace whose name doesn't itself match a bookmark.
  let has_remote = obs_row.has_remote_bookmark || d.is_trunk;
  let status = StatusFlags {
    has_changes: d.head_added > 0 || d.head_removed > 0,
    modified: d.modified,
    untracked: d.untracked,
    stale: w.stale,
    conflicts: d.conflicts,
    has_remote,
    vs_trunk: trunk_rel(d, obs_row.ahead, obs_row.behind),
  };

  Ok(ListRow {
    name: w.name.clone(),
    path: w.path.clone(),
    url,
    is_current,
    is_default,
    status,
    head_diff: LineDiff {
      added: d.head_added,
      removed: d.head_removed,
    },
    vs_trunk: AheadBehind {
      ahead: obs_row.ahead,
      behind: obs_row.behind,
    },
    commit: d.commit_short.clone(),
    age: format_age(d.age_seconds),
    message: d.message_first_line.clone(),
  })
}

pub fn plan_list(cfg: &Config, obs: &ObservedListState, styled: bool) -> Result<Plan, CoreError> {
  if !obs.is_jj_repo {
    return Err(CoreError::NotJjRepo);
  }

  let mut rows = Vec::with_capacity(obs.rows.len());

  for r in &obs.rows {
    let is_current = obs.current_workspace.as_deref() == Some(r.workspace.name.as_str());

    rows.push(build_list_row(cfg, r, is_current)?);
  }

  let mut plan = Plan::new();

  plan.push(Action::PrintLine(format_list_table(&rows, styled)));

  Ok(plan)
}
