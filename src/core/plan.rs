use crate::core::format::{format_age, format_list_json, format_list_table, format_remove_json};
use crate::core::template::render;
use crate::core::types::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Compute the on-disk path for a workspace, using the template if configured.
fn workspace_path(root: &Path, name: &str, template: Option<&str>) -> Result<PathBuf, CoreError> {
  if name == "default" {
    return Ok(root.to_path_buf());
  }

  if let Some(tmpl) = template {
    let ctx = RenderContext {
      branch: name.into(),
      ..Default::default()
    };
    let rendered = render(tmpl, &ctx)?;

    Ok(root.join(rendered))
  } else {
    Ok(root.join(".worktrees").join(name))
  }
}

/// Build the environment variable list injected into hook subprocesses.
fn hook_env(
  workspace: &str,
  ws_path: &Path,
  hook_type: &str,
  hook_name: &str,
  source: HookSource,
) -> Vec<(String, String)> {
  vec![
    ("JJWT_WORKSPACE".into(), workspace.into()),
    ("JJWT_WORKSPACE_PATH".into(), ws_path.display().to_string()),
    ("JJWT_HOOK_TYPE".into(), hook_type.into()),
    ("JJWT_HOOK_NAME".into(), hook_name.into()),
    ("JJWT_HOOK_SOURCE".into(), source.to_string()),
  ]
}

/// Build the template render context for a hook whose template will run in
/// `ws_path`. `hook_type` is the canonical hook-event name (e.g.
/// `pre-start`); `hook_name` is the named key inside the hook group.
fn render_ctx(
  branch: &str,
  ws_path: &Path,
  repo_root: &Path,
  hook_type: &str,
  hook_name: &str,
) -> RenderContext {
  RenderContext {
    branch: branch.into(),
    worktree_path: Some(ws_path.to_path_buf()),
    worktree_name: ws_path
      .file_name()
      .map(|n| n.to_string_lossy().into_owned()),
    repo: repo_root
      .file_name()
      .map(|n| n.to_string_lossy().into_owned()),
    repo_path: Some(repo_root.to_path_buf()),
    cwd: Some(ws_path.to_path_buf()),
    hook_type: Some(hook_type.into()),
    hook_name: Some(hook_name.into()),
    args: Vec::new(),
    vars: Vec::new(),
    vars_state: HashMap::new(),
  }
}

/// Render all hooks in the given groups into `RunHook` actions.
fn render_hook_group(
  groups: &[SourcedHookGroup],
  hook_type: &str,
  branch: &str,
  ws_path: &Path,
  repo_root: &Path,
) -> Result<Vec<Action>, CoreError> {
  let mut out = Vec::new();

  for shg in groups {
    for (name, tmpl) in &shg.group {
      let ctx = render_ctx(branch, ws_path, repo_root, hook_type, name);
      let rendered = render(tmpl, &ctx)?;

      out.push(Action::RunHook {
        name: name.clone(),
        rendered_cmd: rendered,
        cwd: ws_path.to_path_buf(),
        env: hook_env(branch, ws_path, hook_type, name, shg.source),
        source: shg.source,
      });
    }
  }

  Ok(out)
}

/// Gate `render_hook_group` on a runtime flag. When the flag is false,
/// returns an empty Vec without rendering — used to honor `--no-hooks`
/// without splattering branches across every call site in `plan_switch`
/// and `plan_remove`.
fn render_hook_group_if(
  run: bool,
  groups: &[SourcedHookGroup],
  hook_type: &str,
  branch: &str,
  ws_path: &Path,
  repo_root: &Path,
) -> Result<Vec<Action>, CoreError> {
  if run {
    render_hook_group(groups, hook_type, branch, ws_path, repo_root)
  } else {
    Ok(Vec::new())
  }
}

/// Build a plan for the `switch` subcommand (create or switch to a workspace).
pub fn plan_switch(
  cfg: &MergedConfig,
  args: &SwitchArgs,
  obs: &ObservedState,
) -> Result<Plan, CoreError> {
  if !obs.is_jj_repo {
    return Err(CoreError::NotJjRepo);
  }

  if args.create {
    plan_switch_create(cfg, args, obs)
  } else {
    plan_switch_existing(cfg, args, obs)
  }
}

/// Create a new workspace and switch to it.
fn plan_switch_create(
  cfg: &MergedConfig,
  args: &SwitchArgs,
  obs: &ObservedState,
) -> Result<Plan, CoreError> {
  if obs.workspaces.iter().any(|w| w.name == args.name) {
    return Err(CoreError::WorkspaceExists(args.name.clone()));
  }

  let ws_path = workspace_path(
    &obs.repo_root,
    &args.name,
    cfg.worktree_path_template.as_deref(),
  )?;

  let mut plan = Plan::new();

  // Stale directory at the target — usually leftover from an
  // interrupted `jj workspace add`. Require `--clobber` to consent to
  // removing it, and never clobber when the path is inside an existing
  // workspace (that would torch real user data).
  if obs.target_path_exists {
    if !args.clobber {
      return Err(CoreError::TargetPathOccupied(ws_path.display().to_string()));
    }

    let inside_other = obs
      .workspaces
      .iter()
      .any(|w| ws_path != w.path && ws_path.starts_with(&w.path));

    if inside_other {
      return Err(CoreError::TargetPathInsideOtherWorkspace(
        ws_path.display().to_string(),
      ));
    }

    plan.push(Action::DeleteDir {
      path: ws_path.clone(),
    });
  }

  // `pre-switch` fires before the workspace exists; it runs in the
  // repo root so the user still has a cwd to operate from.
  for a in render_hook_group_if(
    !args.no_hooks,
    &cfg.pre_switch,
    "pre-switch",
    &args.name,
    &ws_path,
    &obs.repo_root,
  )? {
    plan.push(a);
  }

  plan.push(Action::JjWorkspaceAdd {
    name: args.name.clone(),
    path: ws_path.clone(),
  });
  plan.push(Action::JjBookmarkCreate {
    name: args.name.clone(),
    workspace: args.name.clone(),
  });

  for a in render_hook_group_if(
    !args.no_hooks,
    &cfg.pre_start,
    "pre-start",
    &args.name,
    &ws_path,
    &obs.repo_root,
  )? {
    plan.push(a);
  }

  for a in render_hook_group_if(
    !args.no_hooks,
    &cfg.post_start,
    "post-start",
    &args.name,
    &ws_path,
    &obs.repo_root,
  )? {
    plan.push(a);
  }

  emit_switch_output(
    &mut plan,
    &args.name,
    &ws_path,
    &obs.repo_root,
    args.execute.as_deref(),
    args.format,
    true,
  )?;

  for a in render_hook_group_if(
    !args.no_hooks,
    &cfg.post_switch,
    "post-switch",
    &args.name,
    &ws_path,
    &obs.repo_root,
  )? {
    plan.push(a);
  }

  Ok(plan)
}

/// Switch to an existing workspace, optionally re-running start hooks.
fn plan_switch_existing(
  cfg: &MergedConfig,
  args: &SwitchArgs,
  obs: &ObservedState,
) -> Result<Plan, CoreError> {
  // Prefer a direct workspace match; otherwise honor the trunk-bookmark
  // fallback (e.g. `switch main` -> default workspace).
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

  let mut plan = Plan::new();

  for a in render_hook_group_if(
    !args.no_hooks,
    &cfg.pre_switch,
    "pre-switch",
    &ws.name,
    &ws.path,
    &obs.repo_root,
  )? {
    plan.push(a);
  }

  if ws.stale {
    plan.push(Action::JjWorkspaceUpdateStale {
      name: ws.name.clone(),
    });
  }

  if args.rerun_hooks {
    for a in render_hook_group_if(
      !args.no_hooks,
      &cfg.pre_start,
      "pre-start",
      &ws.name,
      &ws.path,
      &obs.repo_root,
    )? {
      plan.push(a);
    }
  }

  emit_switch_output(
    &mut plan,
    &ws.name,
    &ws.path,
    &obs.repo_root,
    args.execute.as_deref(),
    args.format,
    false,
  )?;

  for a in render_hook_group_if(
    !args.no_hooks,
    &cfg.post_switch,
    "post-switch",
    &ws.name,
    &ws.path,
    &obs.repo_root,
  )? {
    plan.push(a);
  }

  Ok(plan)
}

/// Emit the `PrintLine`s that the shell wrapper consumes after a switch.
///
/// * **Text, no `--execute`:** one line — the workspace path. The fish/bash
///   wrappers `cd` to it (and stay backward-compatible with older binaries).
/// * **Text, `--execute "<cmd>"`:** two lines, `cd:<path>` then
///   `exec:<rendered-cmd>`. Wrappers parse the prefixes; the `exec` is
///   passed to `eval` after `cd`. Users on outdated wrappers must
///   re-source `config shell init` to use `-x`.
/// * **JSON:** one structured line carrying `name`, `path`, `created`, and
///   the rendered `execute` command when present. Wrappers ignore JSON
///   output; it exists for tool integration.
fn emit_switch_output(
  plan: &mut Plan,
  name: &str,
  ws_path: &Path,
  repo_root: &Path,
  execute_tmpl: Option<&str>,
  format: OutputFormat,
  created: bool,
) -> Result<(), CoreError> {
  let rendered_exec = if let Some(tmpl) = execute_tmpl {
    let ctx = render_ctx(name, ws_path, repo_root, "switch", "execute");
    let r = render(tmpl, &ctx)?;

    Some(r)
  } else {
    None
  };

  match format {
    OutputFormat::Json => {
      let path_str = ws_path.display().to_string();
      let mut obj = serde_json::Map::new();

      obj.insert("name".into(), serde_json::Value::String(name.to_string()));
      obj.insert("path".into(), serde_json::Value::String(path_str));
      obj.insert("created".into(), serde_json::Value::Bool(created));

      if let Some(cmd) = &rendered_exec {
        obj.insert("execute".into(), serde_json::Value::String(cmd.clone()));
      }

      plan.push(Action::PrintLine(
        serde_json::to_string(&serde_json::Value::Object(obj)).expect("json"),
      ));
    }
    OutputFormat::Text | OutputFormat::Statusline => {
      if let Some(cmd) = rendered_exec {
        plan.push(Action::PrintLine(format!("cd:{}", ws_path.display())));
        plan.push(Action::PrintLine(format!("exec:{cmd}")));
      } else {
        plan.push(Action::PrintLine(ws_path.display().to_string()));
      }
    }
  }

  Ok(())
}

/// Build a plan for the `remove` subcommand (forget workspace and clean up).
pub fn plan_remove(
  cfg: &MergedConfig,
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

  if !args.force && obs.target_workspace_dirty {
    return Err(CoreError::WorkspaceDirty(args.name.clone()));
  }

  // Unmerged-bookmark guard fires when the bookmark would be deleted
  // (the default) and isn't yet merged into trunk. `--force-delete`
  // (worktrunk's `-D`) opts in to deleting it anyway; `--force` (`-f`)
  // is about the worktree, not the bookmark, and on its own only
  // lets the *worktree* be removed — the bookmark stays.
  if !args.no_delete_branch
    && obs.target_bookmark_exists
    && !obs.target_bookmark_merged
    && !args.force_delete
    && !args.force
  {
    return Err(CoreError::BookmarkUnmerged(args.name.clone()));
  }

  let ws_path = ws.path.clone();
  let mut plan = Plan::new();

  for a in render_hook_group_if(
    !args.no_hooks,
    &cfg.pre_remove,
    "pre-remove",
    &args.name,
    &ws_path,
    &obs.repo_root,
  )? {
    plan.push(a);
  }

  plan.push(Action::JjWorkspaceForget {
    name: args.name.clone(),
  });

  if cfg.background_remove == Some(true) {
    plan.push(Action::DeleteDirBackground {
      path: ws_path.clone(),
    });
  } else {
    plan.push(Action::DeleteDir {
      path: ws_path.clone(),
    });
  }

  // Delete the bookmark when:
  //   - it exists,
  //   - the user hasn't opted out via --no-delete-branch,
  //   - AND it's either already merged into trunk or the user explicitly
  //     asked for forced deletion (`-D`).
  let bookmark_deleted = obs.target_bookmark_exists
    && !args.no_delete_branch
    && (obs.target_bookmark_merged || args.force_delete);

  if bookmark_deleted {
    plan.push(Action::JjBookmarkDelete {
      name: args.name.clone(),
    });
  }

  // `post-remove` runs after the workspace directory is gone; cwd is the
  // repo root (the runtime executes it there). Template vars still reflect
  // the removed workspace's identity since users may need its name/path
  // for cleanup ("docker stop {{ branch | sanitize }}-db" etc.).
  for a in render_hook_group_if(
    !args.no_hooks,
    &cfg.post_remove,
    "post-remove",
    &args.name,
    &obs.repo_root,
    &obs.repo_root,
  )? {
    plan.push(a);
  }

  if let OutputFormat::Json = args.format {
    plan.push(Action::PrintLine(format_remove_json(
      &args.name,
      &ws_path,
      bookmark_deleted,
    )));
  }

  Ok(plan)
}

/// Plan a user-defined alias invocation. Looks up `args.name` in
/// `cfg.aliases`, renders the template with the current observation
/// context (workspace identity if the cwd is inside one), and emits an
/// `Exec` action.
pub fn plan_alias(
  cfg: &MergedConfig,
  args: &AliasArgs,
  obs: &ObservedState,
) -> Result<Plan, CoreError> {
  let tmpl = cfg
    .aliases
    .get(&args.name)
    .ok_or_else(|| CoreError::AliasNotFound(args.name.clone()))?;

  // Locate the active workspace so vars like `branch` and `worktree_path`
  // bind sensibly. If cwd isn't inside any workspace, fall back to the
  // repo root so `repo` / `repo_path` still resolve.
  let (branch, ws_path) = match obs.current_workspace.as_deref() {
    Some(name) => {
      let ws = obs
        .workspaces
        .iter()
        .find(|w| w.name == name)
        .ok_or_else(|| CoreError::WorkspaceMissing(name.into()))?;

      (ws.name.clone(), ws.path.clone())
    }
    None => (String::new(), obs.repo_root.clone()),
  };

  let ctx = RenderContext {
    branch,
    worktree_path: Some(ws_path.clone()),
    worktree_name: ws_path
      .file_name()
      .map(|n| n.to_string_lossy().into_owned()),
    repo: obs
      .repo_root
      .file_name()
      .map(|n| n.to_string_lossy().into_owned()),
    repo_path: Some(obs.repo_root.clone()),
    cwd: Some(ws_path.clone()),
    hook_type: None,
    hook_name: Some(args.name.clone()),
    args: args.forwarded.clone(),
    vars: Vec::new(),
    vars_state: HashMap::new(),
  };
  let rendered = render(tmpl, &ctx)?;
  let mut plan = Plan::new();

  plan.push(Action::Exec {
    rendered_cmd: rendered,
    cwd: ws_path,
    env: Vec::new(),
  });

  Ok(plan)
}

/// Build a plan for the `hook` subcommand (manual single-hook invocation).
pub fn plan_hook(
  cfg: &MergedConfig,
  args: &HookArgs,
  obs: &ObservedState,
) -> Result<Plan, CoreError> {
  let ws = obs
    .workspaces
    .iter()
    .find(|w| w.name == args.current_workspace)
    .ok_or_else(|| CoreError::WorkspaceMissing(args.current_workspace.clone()))?;

  // Search every configured hook group; remember which one matched so
  // the rendered template can advertise the correct `hook_type` to the
  // user's command.
  let mut matches: Vec<(&str, &str, HookSource)> = Vec::new();

  for (hook_type, groups) in cfg.all_hook_groups() {
    for shg in groups {
      if let Some(tmpl) = shg.group.get(&args.name) {
        matches.push((hook_type, tmpl.as_str(), shg.source));
      }
    }
  }

  let (hook_type, tmpl, source) = match matches.len() {
    0 => return Err(CoreError::HookNotFound(args.name.clone())),
    1 => matches[0],
    _ => return Err(CoreError::HookAmbiguous(args.name.clone())),
  };

  let mut ctx = render_ctx(
    &args.current_workspace,
    &ws.path,
    &obs.repo_root,
    hook_type,
    &args.name,
  );
  ctx.vars = args.vars.clone();

  let rendered = render(tmpl, &ctx)?;
  let mut plan = Plan::new();

  plan.push(Action::RunHook {
    name: args.name.clone(),
    rendered_cmd: rendered,
    cwd: ws.path.clone(),
    env: hook_env(
      &args.current_workspace,
      &ws.path,
      hook_type,
      &args.name,
      source,
    ),
    source,
  });

  Ok(plan)
}

/// Build a plan for the `relocate` subcommand (rename workspace and directory).
pub fn plan_relocate(
  cfg: &MergedConfig,
  args: &RelocateArgs,
  obs: &ObservedState,
) -> Result<Plan, CoreError> {
  if !obs.is_jj_repo {
    return Err(CoreError::NotJjRepo);
  }

  let ws = obs
    .workspaces
    .iter()
    .find(|w| w.name == args.old_name)
    .ok_or_else(|| CoreError::WorkspaceMissing(args.old_name.clone()))?;

  if obs.workspaces.iter().any(|w| w.name == args.new_name) {
    return Err(CoreError::WorkspaceExists(args.new_name.clone()));
  }

  let old_path = ws.path.clone();
  let new_path = workspace_path(
    &obs.repo_root,
    &args.new_name,
    cfg.worktree_path_template.as_deref(),
  )?;

  let mut plan = Plan::new();

  plan.push(Action::JjWorkspaceRename {
    old_name: args.old_name.clone(),
    new_name: args.new_name.clone(),
  });
  plan.push(Action::RenameDir {
    from: old_path.clone(),
    to: new_path.clone(),
  });

  if args.rename_bookmark {
    plan.push(Action::JjBookmarkRename {
      old_name: args.old_name.clone(),
      new_name: args.new_name.clone(),
    });
  }

  match args.format {
    OutputFormat::Json => {
      let mut obj = serde_json::Map::new();

      obj.insert(
        "old_name".into(),
        serde_json::Value::String(args.old_name.clone()),
      );
      obj.insert(
        "new_name".into(),
        serde_json::Value::String(args.new_name.clone()),
      );
      obj.insert(
        "old_path".into(),
        serde_json::Value::String(old_path.display().to_string()),
      );
      obj.insert(
        "new_path".into(),
        serde_json::Value::String(new_path.display().to_string()),
      );
      obj.insert(
        "bookmark_renamed".into(),
        serde_json::Value::Bool(args.rename_bookmark),
      );

      plan.push(Action::PrintLine(
        serde_json::to_string(&serde_json::Value::Object(obj)).expect("json"),
      ));
    }
    OutputFormat::Text | OutputFormat::Statusline => {
      plan.push(Action::PrintLine(format!(
        "Relocated '{}' → '{}'",
        args.old_name, args.new_name
      )));
    }
  }

  Ok(plan)
}

/// Build a plan for the `prune` subcommand (remove all merged workspaces).
pub fn plan_prune(
  cfg: &MergedConfig,
  args: &PruneArgs,
  obs: &ObservedPruneState,
) -> Result<Plan, CoreError> {
  if !obs.is_jj_repo {
    return Err(CoreError::NotJjRepo);
  }

  let mut plan = Plan::new();
  let mut pruned: Vec<String> = Vec::new();

  for (name, bm_exists, bm_merged, _dirty) in &obs.workspace_status {
    // Skip default workspace and current workspace.
    if name == "default" {
      continue;
    }

    if obs.current_workspace.as_deref() == Some(name.as_str()) {
      continue;
    }

    // Only prune if the bookmark is merged into trunk.
    if !bm_exists || !bm_merged {
      continue;
    }

    let ws = match obs.workspaces.iter().find(|w| &w.name == name) {
      Some(w) => w,
      None => continue,
    };

    if args.dry_run {
      pruned.push(name.clone());

      continue;
    }

    // Emit the same actions as plan_remove for each merged workspace.
    for a in render_hook_group_if(
      !args.no_hooks,
      &cfg.pre_remove,
      "pre-remove",
      name,
      &ws.path,
      &obs.repo_root,
    )? {
      plan.push(a);
    }

    plan.push(Action::JjWorkspaceForget { name: name.clone() });

    if cfg.background_remove == Some(true) {
      plan.push(Action::DeleteDirBackground {
        path: ws.path.clone(),
      });
    } else {
      plan.push(Action::DeleteDir {
        path: ws.path.clone(),
      });
    }

    plan.push(Action::JjBookmarkDelete { name: name.clone() });

    for a in render_hook_group_if(
      !args.no_hooks,
      &cfg.post_remove,
      "post-remove",
      name,
      &obs.repo_root,
      &obs.repo_root,
    )? {
      plan.push(a);
    }

    pruned.push(name.clone());
  }

  // Output.
  match args.format {
    OutputFormat::Json => {
      let items: Vec<serde_json::Value> = pruned
        .iter()
        .map(|n| serde_json::Value::String(n.clone()))
        .collect();

      plan.push(Action::PrintLine(
        serde_json::to_string(&serde_json::json!({
          "dry_run": args.dry_run,
          "pruned": items,
        }))
        .expect("json"),
      ));
    }
    OutputFormat::Text | OutputFormat::Statusline => {
      if pruned.is_empty() {
        plan.push(Action::PrintLine("Nothing to prune.".into()));
      } else if args.dry_run {
        plan.push(Action::PrintLine(format!(
          "Would prune {} workspace(s): {}",
          pruned.len(),
          pruned.join(", ")
        )));
      } else {
        plan.push(Action::PrintLine(format!(
          "Pruned {} workspace(s): {}",
          pruned.len(),
          pruned.join(", ")
        )));
      }
    }
  }

  Ok(plan)
}

/// Derive the trunk relationship from ahead/behind commit counts.
fn trunk_rel(ahead: u32, behind: u32) -> Option<TrunkRel> {
  match (ahead, behind) {
    (0, 0) => Some(TrunkRel::IsTrunk),
    (0, _) => Some(TrunkRel::Ancestor),
    (_, 0) => Some(TrunkRel::Ahead),
    (_, _) => Some(TrunkRel::Diverged),
  }
}

/// Transform an observed workspace row into a renderable `ListRow`.
fn build_list_row(
  cfg: &MergedConfig,
  obs_row: &ObservedListRow,
  repo_root: &Path,
  is_current: bool,
) -> Result<ListRow, CoreError> {
  let w = &obs_row.workspace;
  let d = &obs_row.details;
  let is_default = w.path == repo_root;
  let url = if let Some(list) = &cfg.list {
    let ctx = RenderContext {
      branch: w.name.clone(),
      worktree_path: Some(w.path.clone()),
      worktree_name: w.path.file_name().map(|n| n.to_string_lossy().into_owned()),
      repo: repo_root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned()),
      repo_path: Some(repo_root.to_path_buf()),
      cwd: Some(w.path.clone()),
      ..Default::default()
    };

    render(&list.url, &ctx)?
  } else {
    String::new()
  };

  // A workspace shows `|` when its bookmark has a remote variant. As a
  // small convenience: workspaces sitting exactly on `trunk()` also show
  // `|` since trunk in practice tracks an upstream — this catches the
  // `default` workspace whose name doesn't itself match a bookmark.
  let is_on_trunk = obs_row.ahead == 0 && obs_row.behind == 0;
  let has_remote = obs_row.has_remote_bookmark || is_on_trunk;
  let status = StatusFlags {
    has_changes: d.head_added > 0 || d.head_removed > 0,
    modified: d.modified,
    untracked: d.untracked,
    stale: w.stale,
    conflicts: d.conflicts,
    has_remote,
    vs_trunk: trunk_rel(obs_row.ahead, obs_row.behind),
  };

  Ok(ListRow {
    name: w.name.clone(),
    path: w.path.clone(),
    kind: ListRowKind::Workspace,
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
    ci_status: obs_row.ci_status,
    summary: obs_row.summary.clone(),
  })
}

/// Build a placeholder row for a bookmark that doesn't have a workspace.
/// Phase 1 leaves working-copy details empty; richer details can be added
/// in Phase 2 alongside `worktree-path` template support.
fn build_bookmark_row(name: &str) -> ListRow {
  ListRow {
    name: name.into(),
    path: PathBuf::new(),
    kind: ListRowKind::Bookmark,
    url: String::new(),
    is_current: false,
    is_default: false,
    status: StatusFlags::default(),
    head_diff: LineDiff::default(),
    vs_trunk: AheadBehind::default(),
    commit: String::new(),
    age: String::new(),
    message: String::new(),
    ci_status: CiStatus::None,
    summary: String::new(),
  }
}

/// Build a plan for the `list` subcommand (render workspace table).
pub fn plan_list(
  cfg: &MergedConfig,
  obs: &ObservedListState,
  display: &DisplayHints,
  format: OutputFormat,
) -> Result<Plan, CoreError> {
  if !obs.is_jj_repo {
    return Err(CoreError::NotJjRepo);
  }

  let mut rows = Vec::with_capacity(
    obs.rows.len() + obs.extra_bookmark_names.len() + obs.extra_remote_only_names.len(),
  );

  for r in &obs.rows {
    let is_current = obs.current_workspace.as_deref() == Some(r.workspace.name.as_str());

    rows.push(build_list_row(cfg, r, &obs.repo_root, is_current)?);
  }

  for n in &obs.extra_bookmark_names {
    rows.push(build_bookmark_row(n));
  }

  for n in &obs.extra_remote_only_names {
    rows.push(build_bookmark_row(n));
  }

  let mut plan = Plan::new();

  let current = obs.current_workspace.as_deref();

  let body = match format {
    OutputFormat::Text => format_list_table(&rows, display.styled, display.term_width, obs.full),
    OutputFormat::Json => format_list_json(&rows),
    OutputFormat::Statusline => crate::core::format::format_statusline(&rows, current),
  };

  plan.push(Action::PrintLine(body));

  Ok(plan)
}

/// Render a hook template using the current workspace context, returning
/// `None` when no observed state is available.
fn render_hook_template(obs: Option<&ObservedState>, tmpl: &str) -> Option<String> {
  obs.map(|obs| {
    let (branch, ws_path) = current_workspace_or_root(obs);
    let ctx = render_ctx(&branch, &ws_path, &obs.repo_root, "", "");

    match render(tmpl, &ctx) {
      Ok(rendered) => rendered,
      Err(e) => format!("<error: {e}>"),
    }
  })
}

/// Render the hook entries as a JSON array and push the result onto `plan`.
fn plan_hook_show_json(
  entries: &[(&str, &str, &str, HookSource)],
  expanded: bool,
  obs: Option<&ObservedState>,
) -> Result<Plan, CoreError> {
  let mut plan = Plan::new();

  let items: Vec<serde_json::Value> = entries
    .iter()
    .map(|(hook_type, name, tmpl, source)| {
      let mut obj = serde_json::Map::new();

      obj.insert(
        "type".into(),
        serde_json::Value::String(hook_type.to_string()),
      );
      obj.insert("name".into(), serde_json::Value::String(name.to_string()));
      obj.insert(
        "source".into(),
        serde_json::Value::String(source.to_string()),
      );
      obj.insert(
        "template".into(),
        serde_json::Value::String(tmpl.to_string()),
      );

      if expanded && let Some(rendered) = render_hook_template(obs, tmpl) {
        obj.insert("rendered".into(), serde_json::Value::String(rendered));
      }

      serde_json::Value::Object(obj)
    })
    .collect();

  plan.push(Action::PrintLine(
    serde_json::to_string(&items).expect("json"),
  ));

  Ok(plan)
}

/// Render the hook entries as a human-readable text table and push the result
/// onto `plan`.
fn plan_hook_show_text(
  entries: &[(&str, &str, &str, HookSource)],
  expanded: bool,
  obs: Option<&ObservedState>,
) -> Result<Plan, CoreError> {
  let mut plan = Plan::new();
  let mut lines = Vec::new();

  // Compute column widths.
  let type_w = entries
    .iter()
    .map(|(t, _, _, _)| t.len())
    .max()
    .unwrap_or(4)
    .max(4);
  let name_w = entries
    .iter()
    .map(|(_, n, _, _)| n.len())
    .max()
    .unwrap_or(4)
    .max(4);
  let source_w = entries
    .iter()
    .map(|(_, _, _, s)| s.to_string().len())
    .max()
    .unwrap_or(6)
    .max(6);

  // Header.
  let last_col = if expanded { "Rendered" } else { "Template" };

  lines.push(format!(
    "{:<type_w$}  {:<name_w$}  {:<source_w$}  {}",
    "Type", "Name", "Source", last_col,
  ));

  // Separator.
  lines.push(format!(
    "{:<type_w$}  {:<name_w$}  {:<source_w$}  {}",
    "-".repeat(type_w),
    "-".repeat(name_w),
    "-".repeat(source_w),
    "-".repeat(8),
  ));

  for (hook_type, name, tmpl, source) in entries {
    let display_val = if expanded {
      render_hook_template(obs, tmpl)
        .map(|r| truncate_line(&r, 60))
        .unwrap_or_else(|| tmpl.to_string())
    } else {
      truncate_line(tmpl, 60)
    };

    lines.push(format!(
      "{:<type_w$}  {:<name_w$}  {:<source_w$}  {}",
      hook_type, name, source, display_val,
    ));
  }

  plan.push(Action::PrintLine(lines.join("\n")));

  Ok(plan)
}

/// Build a plan for `hook show` (list all configured hooks).
pub fn plan_hook_show(
  cfg: &MergedConfig,
  expanded: bool,
  obs: Option<&ObservedState>,
  format: OutputFormat,
  source_filter: Option<HookSource>,
) -> Result<Plan, CoreError> {
  // Gather all hooks across all types, applying source filter if set.
  let mut entries: Vec<(&str, &str, &str, HookSource)> = Vec::new();

  for (hook_type, groups) in cfg.all_hook_groups() {
    for shg in groups {
      if let Some(filter) = source_filter
        && shg.source != filter
      {
        continue;
      }

      for (name, tmpl) in &shg.group {
        entries.push((hook_type, name.as_str(), tmpl.as_str(), shg.source));
      }
    }
  }

  if entries.is_empty() {
    let mut plan = Plan::new();

    plan.push(Action::PrintLine("No hooks configured.".into()));

    return Ok(plan);
  }

  match format {
    OutputFormat::Json => plan_hook_show_json(&entries, expanded, obs),
    OutputFormat::Text => plan_hook_show_text(&entries, expanded, obs),
    OutputFormat::Statusline => {
      let mut plan = Plan::new();

      // Statusline not meaningful for hook show; fall back to text count.
      plan.push(Action::PrintLine(format!(
        "{} hook(s) configured",
        entries.len()
      )));

      Ok(plan)
    }
  }
}

/// Extract current workspace branch + path from observation, falling back to
/// repo root when not inside a workspace.
fn current_workspace_or_root(obs: &ObservedState) -> (String, PathBuf) {
  obs
    .current_workspace
    .as_deref()
    .and_then(|name| obs.workspaces.iter().find(|w| w.name == name))
    .map(|w| (w.name.clone(), w.path.clone()))
    .unwrap_or_else(|| (String::new(), obs.repo_root.clone()))
}

/// Truncate a string to `max` characters, appending `...` if shortened.
fn truncate_line(s: &str, max: usize) -> String {
  let first_line = s.lines().next().unwrap_or(s);

  if first_line.len() > max {
    format!("{}...", &first_line[..max - 3])
  } else {
    first_line.to_string()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn trunk_rel_is_trunk() {
    assert_eq!(trunk_rel(0, 0), Some(TrunkRel::IsTrunk));
  }

  #[test]
  fn trunk_rel_ancestor() {
    assert_eq!(trunk_rel(0, 5), Some(TrunkRel::Ancestor));
  }

  #[test]
  fn trunk_rel_ahead() {
    assert_eq!(trunk_rel(3, 0), Some(TrunkRel::Ahead));
  }

  #[test]
  fn trunk_rel_diverged() {
    assert_eq!(trunk_rel(2, 4), Some(TrunkRel::Diverged));
  }
}
