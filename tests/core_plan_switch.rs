use indexmap::IndexMap;
use jjwt::core::plan::plan_switch;
use jjwt::core::types::*;
use std::path::PathBuf;

fn one_hook(key: &str, cmd: &str) -> IndexMap<String, String> {
  let mut g = IndexMap::new();

  g.insert(key.into(), cmd.into());

  g
}

#[test]
fn create_emits_pre_switch_then_workspace_then_pre_post_start_then_print_then_post_switch() {
  let cfg = Config {
    pre_switch: vec![one_hook("ps", "echo pre-switch {{ branch }}")],
    pre_start: vec![one_hook("p1", "echo pre-start {{ branch }}")],
    post_start: vec![one_hook("p2", "echo post-start {{ branch }}")],
    post_switch: vec![one_hook("pe", "echo post-switch {{ branch }}")],
    ..Default::default()
  };
  let args = SwitchArgs {
    name: "feat-x".into(),
    create: true,
    ..Default::default()
  };
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    ..Default::default()
  };
  let plan = plan_switch(&cfg, &args, &obs).expect("plan ok");

  let kinds: Vec<&'static str> = plan
    .actions
    .iter()
    .map(|a| match a {
      Action::JjWorkspaceAdd { .. } => "add",
      Action::JjBookmarkCreate { .. } => "bookmark",
      Action::RunHook { env, .. } => env
        .iter()
        .find(|(k, _)| k == "JJWT_HOOK_TYPE")
        .map(|(_, v)| match v.as_str() {
          "pre-switch" => "pre-switch",
          "pre-start" => "pre-start",
          "post-start" => "post-start",
          "post-switch" => "post-switch",
          _ => "hook?",
        })
        .unwrap_or("hook?"),
      Action::PrintLine(_) => "print",
      _ => "other",
    })
    .collect();

  assert_eq!(
    kinds,
    vec![
      "pre-switch",
      "add",
      "bookmark",
      "pre-start",
      "post-start",
      "print",
      "post-switch",
    ],
  );
}

fn cfg_with_two_pre_start_groups() -> Config {
  let mut g1 = IndexMap::new();
  g1.insert("direnv".to_string(), "direnv allow .".to_string());
  g1.insert("envrc".to_string(), "echo {{ branch }}".to_string());
  let mut g2 = IndexMap::new();
  g2.insert("db".to_string(), "make db-start".to_string());
  Config {
    list: None,
    pre_start: vec![g1, g2],
    pre_remove: vec![],
    ..Default::default()
  }
}

fn observed_clean() -> ObservedState {
  ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    ..Default::default()
  }
}

#[test]
fn create_emits_workspace_then_bookmark_then_hooks_then_print() {
  let cfg = cfg_with_two_pre_start_groups();
  let args = SwitchArgs {
    name: "feat-x".into(),
    create: true,
    rerun_hooks: false,
    no_hooks: false,
    execute: None,
    clobber: false,
    format: Default::default(),
  };
  let obs = observed_clean();

  let plan = plan_switch(&cfg, &args, &obs).expect("plan ok");
  let ws_path = PathBuf::from("/repo/.worktrees/feat-x");

  assert_eq!(
    plan.actions[0],
    Action::JjWorkspaceAdd {
      name: "feat-x".into(),
      path: ws_path.clone(),
    }
  );
  assert_eq!(
    plan.actions[1],
    Action::JjBookmarkCreate {
      name: "feat-x".into(),
      workspace: "feat-x".into(),
    }
  );

  let Action::RunHook {
    name: n0,
    rendered_cmd: c0,
    cwd: cwd0,
    env: env0,
  } = &plan.actions[2]
  else {
    panic!("expected RunHook");
  };
  assert_eq!(n0, "direnv");
  assert_eq!(c0, "direnv allow .");
  assert_eq!(cwd0, &ws_path);
  assert!(
    env0
      .iter()
      .any(|(k, v)| k == "JJWT_WORKSPACE" && v == "feat-x")
  );
  assert!(env0.iter().any(|(k, _)| k == "JJWT_WORKSPACE_PATH"));

  let Action::RunHook {
    name: n1,
    rendered_cmd: c1,
    ..
  } = &plan.actions[3]
  else {
    panic!("expected RunHook");
  };
  assert_eq!(n1, "envrc");
  assert_eq!(
    c1, "echo feat-x",
    "template must be rendered with branch=feat-x"
  );

  let Action::RunHook {
    name: n2,
    rendered_cmd: c2,
    ..
  } = &plan.actions[4]
  else {
    panic!("expected RunHook");
  };
  assert_eq!(n2, "db");
  assert_eq!(c2, "make db-start");

  assert_eq!(
    plan.actions[5],
    Action::PrintLine(ws_path.display().to_string())
  );
  assert_eq!(plan.actions.len(), 6);
}

#[test]
fn create_errors_if_workspace_already_exists() {
  let cfg = cfg_with_two_pre_start_groups();
  let args = SwitchArgs {
    name: "feat-x".into(),
    create: true,
    rerun_hooks: false,
    no_hooks: false,
    execute: None,
    clobber: false,
    format: Default::default(),
  };
  let mut obs = observed_clean();
  obs.workspaces.push(Workspace {
    name: "feat-x".into(),
    path: PathBuf::from("/repo/.worktrees/feat-x"),
    stale: false,
  });

  let err = plan_switch(&cfg, &args, &obs).unwrap_err();
  assert!(matches!(err, CoreError::WorkspaceExists(_)));
}

#[test]
fn create_errors_if_not_a_jj_repo() {
  let cfg = cfg_with_two_pre_start_groups();
  let args = SwitchArgs {
    name: "feat-x".into(),
    create: true,
    rerun_hooks: false,
    no_hooks: false,
    execute: None,
    clobber: false,
    format: Default::default(),
  };
  let mut obs = observed_clean();
  obs.is_jj_repo = false;

  let err = plan_switch(&cfg, &args, &obs).unwrap_err();
  assert!(matches!(err, CoreError::NotJjRepo));
}

#[test]
fn switch_existing_no_create_emits_only_print() {
  let cfg = cfg_with_two_pre_start_groups();
  let args = SwitchArgs {
    name: "feat-x".into(),
    create: false,
    rerun_hooks: false,
    no_hooks: false,
    execute: None,
    clobber: false,
    format: Default::default(),
  };
  let mut obs = observed_clean();
  obs.workspaces.push(Workspace {
    name: "feat-x".into(),
    path: PathBuf::from("/repo/.worktrees/feat-x"),
    stale: false,
  });

  let plan = plan_switch(&cfg, &args, &obs).expect("plan ok");
  assert_eq!(
    plan.actions,
    vec![Action::PrintLine(
      PathBuf::from("/repo/.worktrees/feat-x")
        .display()
        .to_string()
    )]
  );
}

#[test]
fn switch_existing_stale_emits_update_stale_then_print() {
  let cfg = cfg_with_two_pre_start_groups();
  let args = SwitchArgs {
    name: "feat-x".into(),
    create: false,
    rerun_hooks: false,
    no_hooks: false,
    execute: None,
    clobber: false,
    format: Default::default(),
  };
  let mut obs = observed_clean();
  obs.workspaces.push(Workspace {
    name: "feat-x".into(),
    path: PathBuf::from("/repo/.worktrees/feat-x"),
    stale: true,
  });

  let plan = plan_switch(&cfg, &args, &obs).expect("plan ok");
  assert_eq!(
    plan.actions[0],
    Action::JjWorkspaceUpdateStale {
      name: "feat-x".into()
    }
  );
  assert!(matches!(plan.actions[1], Action::PrintLine(_)));
  assert_eq!(plan.actions.len(), 2);
}

#[test]
fn switch_existing_with_rerun_hooks_reruns_them() {
  let cfg = cfg_with_two_pre_start_groups();
  let args = SwitchArgs {
    name: "feat-x".into(),
    create: false,
    rerun_hooks: true,
    no_hooks: false,
    execute: None,
    clobber: false,
    format: Default::default(),
  };
  let mut obs = observed_clean();
  obs.workspaces.push(Workspace {
    name: "feat-x".into(),
    path: PathBuf::from("/repo/.worktrees/feat-x"),
    stale: false,
  });

  let plan = plan_switch(&cfg, &args, &obs).expect("plan ok");
  assert_eq!(plan.actions.len(), 4);
  assert!(matches!(plan.actions[0], Action::RunHook { .. }));
  assert!(matches!(plan.actions[1], Action::RunHook { .. }));
  assert!(matches!(plan.actions[2], Action::RunHook { .. }));
  assert!(matches!(plan.actions[3], Action::PrintLine(_)));
}

#[test]
fn switch_missing_without_create_errors() {
  let cfg = cfg_with_two_pre_start_groups();
  let args = SwitchArgs {
    name: "feat-x".into(),
    create: false,
    rerun_hooks: false,
    no_hooks: false,
    execute: None,
    clobber: false,
    format: Default::default(),
  };
  let obs = observed_clean();

  let err = plan_switch(&cfg, &args, &obs).unwrap_err();
  assert!(matches!(err, CoreError::WorkspaceMissing(_)));
}

#[test]
fn switch_trunk_bookmark_name_routes_to_default_workspace() {
  let cfg = cfg_with_two_pre_start_groups();
  let args = SwitchArgs {
    name: "main".into(),
    create: false,
    rerun_hooks: false,
    no_hooks: false,
    execute: None,
    clobber: false,
    format: Default::default(),
  };
  let mut obs = observed_clean();

  // No workspace called "main" exists. "default" exists at the repo root.
  // observe() has detected that "main" is the trunk bookmark and resolved
  // it to "default".
  obs.workspaces.push(Workspace {
    name: "default".into(),
    path: PathBuf::from("/repo"),
    stale: false,
  });
  obs.target_resolved_workspace = Some("default".into());

  let plan = plan_switch(&cfg, &args, &obs).expect("plan ok");

  assert_eq!(
    plan.actions,
    vec![Action::PrintLine(
      PathBuf::from("/repo").display().to_string()
    )]
  );
}

#[test]
fn create_with_custom_worktree_path_template() {
  let cfg = Config {
    worktree_path_template: Some(".wt/{{ branch }}".into()),
    ..Default::default()
  };
  let args = SwitchArgs {
    name: "feat-x".into(),
    create: true,
    ..Default::default()
  };
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    ..Default::default()
  };

  let plan = plan_switch(&cfg, &args, &obs).expect("plan ok");

  let add_path = plan.actions.iter().find_map(|a| match a {
    Action::JjWorkspaceAdd { path, .. } => Some(path.clone()),
    _ => None,
  });

  assert_eq!(add_path, Some(PathBuf::from("/repo/.wt/feat-x")));
}

#[test]
fn create_without_template_uses_default_worktrees_dir() {
  let cfg = Config::default();
  let args = SwitchArgs {
    name: "feat-x".into(),
    create: true,
    ..Default::default()
  };
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    ..Default::default()
  };

  let plan = plan_switch(&cfg, &args, &obs).expect("plan ok");

  let add_path = plan.actions.iter().find_map(|a| match a {
    Action::JjWorkspaceAdd { path, .. } => Some(path.clone()),
    _ => None,
  });

  assert_eq!(add_path, Some(PathBuf::from("/repo/.worktrees/feat-x")));
}

#[test]
fn create_with_template_using_filters() {
  let cfg = Config {
    worktree_path_template: Some(".wt/{{ branch | sanitize }}".into()),
    ..Default::default()
  };
  let args = SwitchArgs {
    name: "feat/x".into(),
    create: true,
    ..Default::default()
  };
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    ..Default::default()
  };

  let plan = plan_switch(&cfg, &args, &obs).expect("plan ok");

  let add_path = plan.actions.iter().find_map(|a| match a {
    Action::JjWorkspaceAdd { path, .. } => Some(path.clone()),
    _ => None,
  });

  // sanitize replaces `/` with `-`
  assert_eq!(add_path, Some(PathBuf::from("/repo/.wt/feat-x")));
}
