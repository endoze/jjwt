use indexmap::IndexMap;
use jjwt::core::plan::plan_hook;
use jjwt::core::types::*;
use std::path::PathBuf;

fn cfg_with_hooks() -> MergedConfig {
  let mut g1 = IndexMap::new();
  g1.insert("direnv".to_string(), "direnv allow .".to_string());
  let mut g2 = IndexMap::new();
  g2.insert(
    "db".to_string(),
    "make db-start in {{ branch }}".to_string(),
  );
  let mut g3 = IndexMap::new();
  g3.insert("db_stop".to_string(), "make db-stop".to_string());

  MergedConfig::from_project(Config {
    list: None,
    pre_start: vec![g1, g2],
    pre_remove: vec![g3],
    ..Default::default()
  })
}

fn obs() -> ObservedState {
  ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    workspaces: vec![Workspace {
      name: "feat-x".into(),
      path: PathBuf::from("/repo/.worktrees/feat-x"),
      stale: false,
    }],
    target_path_exists: true,
    target_workspace_dirty: false,
    target_bookmark_merged: true,
    target_bookmark_exists: true,
    ..Default::default()
  }
}

#[test]
fn resolves_hook_from_pre_start() {
  let cfg = cfg_with_hooks();
  let args = HookArgs {
    vars: vec![],
    name: "db".into(),
    current_workspace: "feat-x".into(),
  };
  let plan = plan_hook(&cfg, &args, &obs()).expect("plan ok");
  assert_eq!(plan.actions.len(), 1);
  let Action::RunHook {
    name,
    rendered_cmd,
    cwd,
    env,
    ..
  } = &plan.actions[0]
  else {
    panic!()
  };
  assert_eq!(name, "db");
  assert_eq!(rendered_cmd, "make db-start in feat-x");
  assert_eq!(*cwd, PathBuf::from("/repo/.worktrees/feat-x"));
  assert!(
    env
      .iter()
      .any(|(k, v)| k == "JJWT_WORKSPACE" && v == "feat-x")
  );
}

#[test]
fn resolves_hook_from_pre_remove() {
  let cfg = cfg_with_hooks();
  let args = HookArgs {
    vars: vec![],
    name: "db_stop".into(),
    current_workspace: "feat-x".into(),
  };
  let plan = plan_hook(&cfg, &args, &obs()).expect("plan ok");
  let Action::RunHook { rendered_cmd, .. } = &plan.actions[0] else {
    panic!()
  };
  assert_eq!(rendered_cmd, "make db-stop");
}

#[test]
fn missing_hook_errors() {
  let cfg = cfg_with_hooks();
  let args = HookArgs {
    vars: vec![],
    name: "nope".into(),
    current_workspace: "feat-x".into(),
  };
  let err = plan_hook(&cfg, &args, &obs()).unwrap_err();
  assert!(matches!(err, CoreError::HookNotFound(_)));
}

#[test]
fn ambiguous_hook_errors() {
  let mut cfg = cfg_with_hooks();
  let mut dup = IndexMap::new();
  dup.insert("db".to_string(), "different".to_string());

  cfg.hooks.pre_remove.push(SourcedHookGroup {
    source: HookSource::Project,
    group: dup,
  });

  let args = HookArgs {
    vars: vec![],
    name: "db".into(),
    current_workspace: "feat-x".into(),
  };
  let err = plan_hook(&cfg, &args, &obs()).unwrap_err();
  assert!(matches!(err, CoreError::HookAmbiguous(_)));
}

#[test]
fn missing_current_workspace_errors() {
  let cfg = cfg_with_hooks();
  let args = HookArgs {
    vars: vec![],
    name: "db".into(),
    current_workspace: "nope".into(),
  };
  let err = plan_hook(&cfg, &args, &obs()).unwrap_err();
  assert!(matches!(err, CoreError::WorkspaceMissing(_)));
}

#[test]
fn resolves_all_hooks_by_type() {
  let cfg = cfg_with_hooks();
  let args = HookArgs {
    vars: vec![],
    name: "pre-start".into(),
    current_workspace: "feat-x".into(),
  };
  let plan = plan_hook(&cfg, &args, &obs()).expect("plan ok");

  assert_eq!(plan.actions.len(), 2, "pre-start has 2 steps: direnv + db");

  let Action::RunHook {
    name: n0,
    rendered_cmd: c0,
    ..
  } = &plan.actions[0]
  else {
    panic!("expected RunHook");
  };
  assert_eq!(n0, "direnv");
  assert_eq!(c0, "direnv allow .");

  let Action::RunHook {
    name: n1,
    rendered_cmd: c1,
    ..
  } = &plan.actions[1]
  else {
    panic!("expected RunHook");
  };
  assert_eq!(n1, "db");
  assert_eq!(c1, "make db-start in feat-x");
}

#[test]
fn type_name_takes_precedence_over_step_name() {
  // Even if a step is named "pre-remove", the type match should win.
  let mut g = IndexMap::new();
  g.insert("pre-remove".to_string(), "should not match".to_string());

  let mut cfg = cfg_with_hooks();

  cfg.hooks.pre_start.push(SourcedHookGroup {
    source: HookSource::Project,
    group: g,
  });

  let args = HookArgs {
    vars: vec![],
    name: "pre-remove".into(),
    current_workspace: "feat-x".into(),
  };
  let plan = plan_hook(&cfg, &args, &obs()).expect("plan ok");

  // pre-remove slot has one step: db_stop
  assert_eq!(plan.actions.len(), 1);

  let Action::RunHook { name, .. } = &plan.actions[0] else {
    panic!("expected RunHook");
  };
  assert_eq!(name, "db_stop");
}

#[test]
fn empty_hook_type_errors() {
  let cfg = cfg_with_hooks();
  let args = HookArgs {
    vars: vec![],
    name: "post-switch".into(),
    current_workspace: "feat-x".into(),
  };
  let err = plan_hook(&cfg, &args, &obs()).unwrap_err();
  assert!(matches!(err, CoreError::HookNotFound(_)));
}