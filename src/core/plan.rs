use crate::core::format::{format_age, format_list_json, format_list_table, format_remove_json};
use crate::core::template::render;
use crate::core::types::*;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Compute the on-disk path for a workspace by rendering the worktree-path
/// template from config.
fn workspace_path(root: &Path, name: &str, template: &str) -> Result<PathBuf, CoreError> {
  if name == "default" {
    return Ok(root.to_path_buf());
  }

  let ctx = RenderContext {
    branch: name.into(),
    repo: root.file_name().map(|n| n.to_string_lossy().into_owned()),
    repo_path: Some(root.to_path_buf()),
    ..Default::default()
  };
  let rendered = render(template, &ctx)?;

  Ok(root.join(rendered))
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
        raw_cmd: tmpl.clone(),
        rendered_cmd: rendered,
        cwd: ws_path.to_path_buf(),
        env: hook_env(branch, ws_path, hook_type, name, shg.source),
        source: shg.source,
      });
    }
  }

  Ok(out)
}

impl Plan {
  /// Render hooks from the given groups and append them to this plan.
  /// When `run` is false, does nothing — used to honor `--no-hooks`
  /// without splattering branches across every call site.
  fn extend_hooks(
    &mut self,
    run: bool,
    groups: &[SourcedHookGroup],
    hook_type: &str,
    branch: &str,
    ws_path: &Path,
    repo_root: &Path,
  ) -> Result<(), CoreError> {
    if run {
      self.actions.extend(render_hook_group(
        groups, hook_type, branch, ws_path, repo_root,
      )?);
    }

    Ok(())
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
    &cfg.worktree_path_template,
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
  plan.extend_hooks(
    !args.no_hooks,
    &cfg.hooks.pre_switch,
    "pre-switch",
    &args.name,
    &ws_path,
    &obs.repo_root,
  )?;

  let revision = args
    .base
    .as_ref()
    .or(obs.trunk_bookmark.as_ref())
    .cloned();

  plan.push(Action::JjWorkspaceAdd {
    name: args.name.clone(),
    path: ws_path.clone(),
    revision,
  });
  plan.push(Action::JjBookmarkCreate {
    name: args.name.clone(),
    workspace: args.name.clone(),
  });
  plan.push(Action::JjWorkspaceUpdateStale {
    name: args.name.clone(),
  });

  plan.extend_hooks(
    !args.no_hooks,
    &cfg.hooks.pre_start,
    "pre-start",
    &args.name,
    &ws_path,
    &obs.repo_root,
  )?;
  plan.extend_hooks(
    !args.no_hooks,
    &cfg.hooks.post_start,
    "post-start",
    &args.name,
    &ws_path,
    &obs.repo_root,
  )?;

  emit_switch_output(
    &mut plan,
    &args.name,
    &ws_path,
    &obs.repo_root,
    args.execute.as_deref(),
    args.format,
    true,
  )?;

  plan.extend_hooks(
    !args.no_hooks,
    &cfg.hooks.post_switch,
    "post-switch",
    &args.name,
    &ws_path,
    &obs.repo_root,
  )?;

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

  plan.extend_hooks(
    !args.no_hooks,
    &cfg.hooks.pre_switch,
    "pre-switch",
    &ws.name,
    &ws.path,
    &obs.repo_root,
  )?;

  if ws.stale {
    plan.push(Action::JjWorkspaceUpdateStale {
      name: ws.name.clone(),
    });
  }

  if args.rerun_hooks {
    plan.extend_hooks(
      !args.no_hooks,
      &cfg.hooks.pre_start,
      "pre-start",
      &ws.name,
      &ws.path,
      &obs.repo_root,
    )?;
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

  plan.extend_hooks(
    !args.no_hooks,
    &cfg.hooks.post_switch,
    "post-switch",
    &ws.name,
    &ws.path,
    &obs.repo_root,
  )?;

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
      let mut obj = json!({
        "name": name,
        "path": ws_path.display().to_string(),
        "created": created,
      });

      if let Some(cmd) = &rendered_exec {
        obj["execute"] = json!(cmd);
      }

      plan.push(Action::PrintLine(
        serde_json::to_string(&obj).expect("json"),
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
  name: &str,
  args: &RemoveArgs,
  obs: &ObservedState,
) -> Result<Plan, CoreError> {
  if !obs.is_jj_repo {
    return Err(CoreError::NotJjRepo);
  }

  let ws = obs
    .workspaces
    .iter()
    .find(|w| w.name == name)
    .ok_or_else(|| CoreError::WorkspaceMissing(name.to_string()))?;

  if !args.force && obs.target_workspace_dirty {
    return Err(CoreError::WorkspaceDirty(name.to_string()));
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
    return Err(CoreError::BookmarkUnmerged(name.to_string()));
  }

  let ws_path = ws.path.clone();
  let mut plan = Plan::new();

  plan.extend_hooks(
    !args.no_hooks,
    &cfg.hooks.pre_remove,
    "pre-remove",
    name,
    &ws_path,
    &obs.repo_root,
  )?;

  plan.push(Action::JjWorkspaceForget {
    name: name.to_string(),
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
      name: name.to_string(),
    });
  }

  // `post-remove` runs after the workspace directory is gone; cwd is the
  // repo root (the runtime executes it there). Template vars still reflect
  // the removed workspace's identity since users may need its name/path
  // for cleanup ("docker stop {{ branch | sanitize }}-db" etc.).
  plan.extend_hooks(
    !args.no_hooks,
    &cfg.hooks.post_remove,
    "post-remove",
    name,
    &obs.repo_root,
    &obs.repo_root,
  )?;

  if let OutputFormat::Json = args.format {
    plan.push(Action::PrintLine(format_remove_json(
      name,
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
    raw_cmd: tmpl.to_string(),
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
    &cfg.worktree_path_template,
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
      let obj = json!({
        "old_name": args.old_name,
        "new_name": args.new_name,
        "old_path": old_path.display().to_string(),
        "new_path": new_path.display().to_string(),
        "bookmark_renamed": args.rename_bookmark,
      });

      plan.push(Action::PrintLine(
        serde_json::to_string(&obj).expect("json"),
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

    let Some(ws) = obs.workspaces.iter().find(|w| &w.name == name) else {
      continue;
    };

    if args.dry_run {
      pruned.push(name.clone());

      continue;
    }

    // Emit the same actions as plan_remove for each merged workspace.
    plan.extend_hooks(
      !args.no_hooks,
      &cfg.hooks.pre_remove,
      "pre-remove",
      name,
      &ws.path,
      &obs.repo_root,
    )?;

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

    plan.extend_hooks(
      !args.no_hooks,
      &cfg.hooks.post_remove,
      "post-remove",
      name,
      &obs.repo_root,
      &obs.repo_root,
    )?;

    pruned.push(name.clone());
  }

  // Output.
  match args.format {
    OutputFormat::Json => {
      plan.push(Action::PrintLine(
        serde_json::to_string(&serde_json::json!({
          "dry_run": args.dry_run,
          "pruned": pruned,
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

  let display_path = if is_default {
    ".".to_string()
  } else {
    w.path
      .strip_prefix(repo_root)
      .map(|rel| format!("./{}", rel.display()))
      .unwrap_or_else(|_| w.path.display().to_string())
  };

  Ok(ListRow {
    name: w.name.clone(),
    path: w.path.clone(),
    display_path,
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
    display_path: String::new(),
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
      let mut obj = json!({
        "type": hook_type,
        "name": name,
        "source": source.to_string(),
        "template": tmpl,
      });

      if expanded && let Some(rendered) = render_hook_template(obs, tmpl) {
        obj["rendered"] = json!(rendered);
      }

      obj
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
    .map(|(_, _, _, s)| match s {
      HookSource::User => 4,
      HookSource::Project => 7,
    })
    .max()
    .unwrap_or(6);

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

/// Truncate a string to `max` visible characters, appending `...` if shortened.
/// Uses `char_indices()` to avoid panicking on multi-byte UTF-8 boundaries.
fn truncate_line(s: &str, max: usize) -> String {
  let first_line = s.lines().next().unwrap_or(s);

  if max <= 3 {
    // Short max: only emit dots if we actually need to truncate.
    let mut chars = first_line.chars();

    if chars.by_ref().take(max + 1).count() <= max {
      return first_line.to_string();
    }

    return ".".repeat(max);
  }

  let target = max - 3;
  let mut last_idx = 0;
  let mut count = 0;

  for (idx, ch) in first_line.char_indices() {
    count += 1;

    if count <= target {
      last_idx = idx + ch.len_utf8();
    } else if count > max {
      return format!("{}...", &first_line[..last_idx]);
    }
  }

  // Walked the whole string and the char count never exceeded `max`.
  first_line.to_string()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn truncate_line_no_op_when_short() {
    assert_eq!(truncate_line("hello", 10), "hello");
  }

  #[test]
  fn truncate_line_exact_length() {
    assert_eq!(truncate_line("hello", 5), "hello");
  }

  #[test]
  fn truncate_line_ascii_truncation() {
    assert_eq!(truncate_line("hello world", 8), "hello...");
  }

  #[test]
  fn truncate_line_multibyte_utf8() {
    // "äöü" is 6 bytes but 3 chars — must not panic
    assert_eq!(truncate_line("äöüxyz", 5), "äö...");
  }

  #[test]
  fn truncate_line_cjk() {
    assert_eq!(truncate_line("日本語テスト", 5), "日本...");
  }

  #[test]
  fn truncate_line_emoji() {
    // Each emoji is 4 bytes but 1 char — must not panic
    assert_eq!(truncate_line("🎉🎊🎈🎁🎀🎗️", 5), "🎉🎊...");
  }

  #[test]
  fn truncate_line_max_less_than_three() {
    assert_eq!(truncate_line("hello", 2), "..");
    assert_eq!(truncate_line("hello", 1), ".");
    assert_eq!(truncate_line("hello", 0), "");
  }

  #[test]
  fn truncate_line_uses_first_line() {
    assert_eq!(truncate_line("first\nsecond", 20), "first");
  }

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
