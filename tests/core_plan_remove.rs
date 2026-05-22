use indexmap::IndexMap;
use jjwt::core::plan::plan_remove;
use jjwt::core::types::*;
use std::path::PathBuf;

fn hook(key: &str, cmd: &str) -> IndexMap<String, String> {
  let mut g = IndexMap::new();

  g.insert(key.into(), cmd.into());

  g
}

#[test]
fn remove_emits_pre_remove_then_actions_then_post_remove() {
  let cfg = MergedConfig::from_project(Config {
    pre_remove: vec![hook("a", "echo pre-remove {{ branch }}")],
    post_remove: vec![hook("b", "echo post-remove {{ branch }}")],
    ..Default::default()
  });
  let args = RemoveArgs {
    name: "feat-x".into(),
    force: true,
    no_hooks: false,
    no_delete_branch: false,
    force_delete: false,
    ..Default::default()
  };
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    workspaces: vec![Workspace {
      name: "feat-x".into(),
      path: PathBuf::from("/repo/.worktrees/feat-x"),
      stale: false,
    }],
    target_bookmark_exists: true,
    target_bookmark_merged: true,
    ..Default::default()
  };

  let plan = plan_remove(&cfg, &args, &obs).expect("plan ok");
  let kinds: Vec<&'static str> = plan
    .actions
    .iter()
    .map(|a| match a {
      Action::JjWorkspaceForget { .. } => "forget",
      Action::DeleteDir { .. } => "del",
      Action::JjBookmarkDelete { .. } => "bookmark-del",
      Action::RunHook { env, .. } => env
        .iter()
        .find(|(k, _)| k == "JJWT_HOOK_TYPE")
        .map(|(_, v)| match v.as_str() {
          "pre-remove" => "pre-remove",
          "post-remove" => "post-remove",
          _ => "hook?",
        })
        .unwrap_or("hook?"),
      _ => "other",
    })
    .collect();

  assert_eq!(
    kinds,
    vec!["pre-remove", "forget", "del", "bookmark-del", "post-remove"],
  );
}

#[test]
fn no_delete_branch_keeps_bookmark_even_when_merged() {
  let cfg = MergedConfig::from_project(Config::default());
  let args = RemoveArgs {
    name: "feat-x".into(),
    no_delete_branch: true,
    ..Default::default()
  };
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    workspaces: vec![Workspace {
      name: "feat-x".into(),
      path: PathBuf::from("/repo/.worktrees/feat-x"),
      stale: false,
    }],
    target_bookmark_exists: true,
    target_bookmark_merged: true,
    ..Default::default()
  };
  let plan = plan_remove(&cfg, &args, &obs).expect("plan ok");

  assert!(
    !plan
      .actions
      .iter()
      .any(|a| matches!(a, Action::JjBookmarkDelete { .. }))
  );
}

#[test]
fn force_delete_removes_unmerged_bookmark() {
  let cfg = MergedConfig::from_project(Config::default());
  let args = RemoveArgs {
    name: "feat-x".into(),
    force_delete: true,
    ..Default::default()
  };
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    workspaces: vec![Workspace {
      name: "feat-x".into(),
      path: PathBuf::from("/repo/.worktrees/feat-x"),
      stale: false,
    }],
    target_bookmark_exists: true,
    target_bookmark_merged: false,
    ..Default::default()
  };
  let plan = plan_remove(&cfg, &args, &obs).expect("plan ok");

  assert!(
    plan
      .actions
      .iter()
      .any(|a| matches!(a, Action::JjBookmarkDelete { .. }))
  );
}

fn cfg() -> MergedConfig {
  let mut g = IndexMap::new();
  g.insert("db_stop".to_string(), "make db-stop".to_string());
  g.insert("teardown".to_string(), "echo bye {{ branch }}".to_string());

  MergedConfig::from_project(Config {
    list: None,
    pre_start: vec![],
    pre_remove: vec![g],
    ..Default::default()
  })
}

fn obs_existing(name: &str, dirty: bool, merged: bool, bookmark_exists: bool) -> ObservedState {
  ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    workspaces: vec![Workspace {
      name: name.into(),
      path: PathBuf::from(format!("/repo/.worktrees/{name}")),
      stale: false,
    }],
    target_path_exists: true,
    target_workspace_dirty: dirty,
    target_bookmark_merged: merged,
    target_bookmark_exists: bookmark_exists,
    ..Default::default()
  }
}

#[test]
fn remove_merged_bookmark_emits_full_sequence() {
  let cfg = cfg();
  let args = RemoveArgs {
    name: "feat-x".into(),
    force: false,
    no_hooks: false,
    no_delete_branch: false,
    force_delete: false,
    format: Default::default(),
  };
  let obs = obs_existing("feat-x", false, true, true);

  let plan = plan_remove(&cfg, &args, &obs).expect("plan ok");
  let ws_path = PathBuf::from("/repo/.worktrees/feat-x");

  let Action::RunHook {
    name: n0,
    rendered_cmd: c0,
    cwd: cwd0,
    ..
  } = &plan.actions[0]
  else {
    panic!()
  };
  assert_eq!(n0, "db_stop");
  assert_eq!(c0, "make db-stop");
  assert_eq!(cwd0, &ws_path as &PathBuf);

  let Action::RunHook {
    name: n1,
    rendered_cmd: c1,
    ..
  } = &plan.actions[1]
  else {
    panic!()
  };
  assert_eq!(n1, "teardown");
  assert_eq!(c1, "echo bye feat-x");

  assert_eq!(
    plan.actions[2],
    Action::JjWorkspaceForget {
      name: "feat-x".into()
    }
  );
  assert_eq!(plan.actions[3], Action::DeleteDir { path: ws_path });
  assert_eq!(
    plan.actions[4],
    Action::JjBookmarkDelete {
      name: "feat-x".into()
    }
  );
  assert_eq!(plan.actions.len(), 5);
}

#[test]
fn remove_unmerged_bookmark_errors_without_force() {
  let cfg = cfg();
  let args = RemoveArgs {
    name: "feat-x".into(),
    force: false,
    no_hooks: false,
    no_delete_branch: false,
    force_delete: false,
    format: Default::default(),
  };
  let obs = obs_existing("feat-x", false, false, true);

  let err = plan_remove(&cfg, &args, &obs).unwrap_err();
  assert!(matches!(err, CoreError::BookmarkUnmerged(_)));
}

#[test]
fn remove_unmerged_bookmark_with_force_skips_bookmark_delete() {
  let cfg = cfg();
  let args = RemoveArgs {
    name: "feat-x".into(),
    force: true,
    no_hooks: false,
    no_delete_branch: false,
    force_delete: false,
    format: Default::default(),
  };
  let obs = obs_existing("feat-x", false, false, true);

  let plan = plan_remove(&cfg, &args, &obs).expect("plan ok");
  assert!(
    !plan
      .actions
      .iter()
      .any(|a| matches!(a, Action::JjBookmarkDelete { .. }))
  );
  assert!(
    plan
      .actions
      .iter()
      .any(|a| matches!(a, Action::JjWorkspaceForget { .. }))
  );
}

#[test]
fn remove_dirty_without_force_errors() {
  let cfg = cfg();
  let args = RemoveArgs {
    name: "feat-x".into(),
    force: false,
    no_hooks: false,
    no_delete_branch: false,
    force_delete: false,
    format: Default::default(),
  };
  let obs = obs_existing("feat-x", true, true, true);

  let err = plan_remove(&cfg, &args, &obs).unwrap_err();
  assert!(matches!(err, CoreError::WorkspaceDirty(_)));
}

#[test]
fn remove_dirty_with_force_proceeds() {
  let cfg = cfg();
  let args = RemoveArgs {
    name: "feat-x".into(),
    force: true,
    no_hooks: false,
    no_delete_branch: false,
    force_delete: false,
    format: Default::default(),
  };
  let obs = obs_existing("feat-x", true, true, true);

  let plan = plan_remove(&cfg, &args, &obs).expect("plan ok");
  assert!(
    plan
      .actions
      .iter()
      .any(|a| matches!(a, Action::JjWorkspaceForget { .. }))
  );
  assert!(
    plan
      .actions
      .iter()
      .any(|a| matches!(a, Action::DeleteDir { .. }))
  );
  assert!(
    plan
      .actions
      .iter()
      .any(|a| matches!(a, Action::JjBookmarkDelete { .. }))
  );
}

#[test]
fn remove_missing_workspace_errors() {
  let cfg = cfg();
  let args = RemoveArgs {
    name: "feat-x".into(),
    force: false,
    no_hooks: false,
    no_delete_branch: false,
    force_delete: false,
    format: Default::default(),
  };
  let mut obs = obs_existing("feat-x", false, true, true);
  obs.workspaces.clear();

  let err = plan_remove(&cfg, &args, &obs).unwrap_err();
  assert!(matches!(err, CoreError::WorkspaceMissing(_)));
}

#[test]
fn remove_no_bookmark_skips_delete() {
  let cfg = cfg();
  let args = RemoveArgs {
    name: "feat-x".into(),
    force: false,
    no_hooks: false,
    no_delete_branch: false,
    force_delete: false,
    format: Default::default(),
  };
  let obs = obs_existing("feat-x", false, true, false);

  let plan = plan_remove(&cfg, &args, &obs).expect("plan ok");
  assert!(
    !plan
      .actions
      .iter()
      .any(|a| matches!(a, Action::JjBookmarkDelete { .. }))
  );
}

#[test]
fn background_remove_emits_delete_dir_background() {
  let cfg = MergedConfig::from_project(Config {
    background_remove: Some(true),
    ..Default::default()
  });
  let args = RemoveArgs {
    name: "feat-x".into(),
    force: true,
    ..Default::default()
  };
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    workspaces: vec![Workspace {
      name: "feat-x".into(),
      path: PathBuf::from("/repo/.worktrees/feat-x"),
      stale: false,
    }],
    target_bookmark_exists: true,
    target_bookmark_merged: true,
    ..Default::default()
  };

  let plan = plan_remove(&cfg, &args, &obs).expect("plan ok");

  let has_bg_delete = plan
    .actions
    .iter()
    .any(|a| matches!(a, Action::DeleteDirBackground { .. }));
  let has_sync_delete = plan
    .actions
    .iter()
    .any(|a| matches!(a, Action::DeleteDir { .. }));

  assert!(has_bg_delete, "should have DeleteDirBackground");
  assert!(!has_sync_delete, "should NOT have DeleteDir");
}

#[test]
fn sync_remove_when_background_not_configured() {
  let cfg = MergedConfig::from_project(Config::default());
  let args = RemoveArgs {
    name: "feat-x".into(),
    force: true,
    ..Default::default()
  };
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    workspaces: vec![Workspace {
      name: "feat-x".into(),
      path: PathBuf::from("/repo/.worktrees/feat-x"),
      stale: false,
    }],
    target_bookmark_exists: true,
    target_bookmark_merged: true,
    ..Default::default()
  };

  let plan = plan_remove(&cfg, &args, &obs).expect("plan ok");

  let has_sync_delete = plan
    .actions
    .iter()
    .any(|a| matches!(a, Action::DeleteDir { .. }));

  assert!(has_sync_delete, "should have DeleteDir");
}
