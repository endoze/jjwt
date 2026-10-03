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
  let args = RemoveArgs::default();
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    workspaces: vec![Workspace {
      name: "feat-x".into(),
      path: PathBuf::from("/repo/.worktrees/feat-x"),
      stale: false,
    }],
    target_bookmark_exists: true,
    ..Default::default()
  };

  let plan = plan_remove(&cfg, "feat-x", &args, &obs).expect("plan ok");
  let kinds: Vec<&'static str> = plan
    .actions
    .iter()
    .map(|a| match a {
      Action::JjSnapshot { .. } => "snapshot",
      Action::JjWorkspaceForget { .. } => "forget",
      Action::DeleteDir { .. } => "del",
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
    vec!["pre-remove", "snapshot", "forget", "del", "post-remove"],
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

fn obs_existing(name: &str) -> ObservedState {
  ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    workspaces: vec![Workspace {
      name: name.into(),
      path: PathBuf::from(format!("/repo/.worktrees/{name}")),
      stale: false,
    }],
    target_path_exists: true,
    target_bookmark_exists: true,
    ..Default::default()
  }
}

#[test]
fn remove_emits_full_sequence() {
  let cfg = cfg();
  let args = RemoveArgs::default();
  let obs = obs_existing("feat-x");

  let plan = plan_remove(&cfg, "feat-x", &args, &obs).expect("plan ok");
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
    Action::JjSnapshot {
      name: "feat-x".into(),
      path: ws_path.clone(),
      stale: false,
    }
  );
  assert_eq!(
    plan.actions[3],
    Action::JjWorkspaceForget {
      name: "feat-x".into()
    }
  );
  assert_eq!(plan.actions[4], Action::DeleteDir { path: ws_path });
  assert_eq!(plan.actions.len(), 5);
}

#[test]
fn remove_missing_workspace_errors() {
  let cfg = cfg();
  let args = RemoveArgs::default();
  let mut obs = obs_existing("feat-x");
  obs.workspaces.clear();

  let err = plan_remove(&cfg, "feat-x", &args, &obs).unwrap_err();
  assert!(matches!(err, CoreError::WorkspaceMissing(_)));
}

#[test]
fn background_remove_emits_delete_dir_background() {
  let cfg = MergedConfig::from_project(Config {
    background_remove: Some(true),
    ..Default::default()
  });
  let args = RemoveArgs::default();
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    workspaces: vec![Workspace {
      name: "feat-x".into(),
      path: PathBuf::from("/repo/.worktrees/feat-x"),
      stale: false,
    }],
    ..Default::default()
  };

  let plan = plan_remove(&cfg, "feat-x", &args, &obs).expect("plan ok");

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
  let args = RemoveArgs::default();
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    workspaces: vec![Workspace {
      name: "feat-x".into(),
      path: PathBuf::from("/repo/.worktrees/feat-x"),
      stale: false,
    }],
    ..Default::default()
  };

  let plan = plan_remove(&cfg, "feat-x", &args, &obs).expect("plan ok");

  let has_sync_delete = plan
    .actions
    .iter()
    .any(|a| matches!(a, Action::DeleteDir { .. }));

  assert!(has_sync_delete, "should have DeleteDir");
}

#[test]
fn remove_json_format_emits_name_and_path() {
  let cfg = MergedConfig::from_project(Config::default());
  let args = RemoveArgs {
    format: OutputFormat::Json,
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
    ..Default::default()
  };

  let plan = plan_remove(&cfg, "feat-x", &args, &obs).expect("plan ok");

  let json_line = plan
    .actions
    .iter()
    .filter_map(|a| match a {
      Action::PrintLine(s) => Some(s.clone()),
      _ => None,
    })
    .next_back();

  let parsed: serde_json::Value =
    serde_json::from_str(&json_line.expect("should have PrintLine")).expect("valid json");

  assert_eq!(
    parsed,
    serde_json::json!({ "name": "feat-x", "path": "/repo/.worktrees/feat-x" })
  );
}

#[test]
fn remove_text_format_does_not_emit_json() {
  let cfg = MergedConfig::from_project(Config::default());
  let args = RemoveArgs {
    format: OutputFormat::Text,
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
    ..Default::default()
  };

  let plan = plan_remove(&cfg, "feat-x", &args, &obs).expect("plan ok");

  let has_json = plan.actions.iter().any(|a| match a {
    Action::PrintLine(s) => s.starts_with('{'),
    _ => false,
  });

  assert!(!has_json, "Text format should not emit JSON");
}

#[test]
fn remove_not_jj_repo_errors() {
  let cfg = MergedConfig::from_project(Config::default());
  let args = RemoveArgs {
    ..Default::default()
  };
  let obs = ObservedState {
    is_jj_repo: false,
    ..Default::default()
  };

  let err = plan_remove(&cfg, "feat-x", &args, &obs).unwrap_err();

  assert!(matches!(err, CoreError::NotJjRepo));
}

fn obs_root_ws(name: &str) -> ObservedState {
  ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    workspaces: vec![Workspace {
      name: name.into(),
      path: PathBuf::from("/repo"),
      stale: false,
    }],
    ..Default::default()
  }
}

#[test]
fn remove_refuses_default_workspace() {
  let cfg = MergedConfig::from_project(Config::default());
  let err = plan_remove(&cfg, "default", &RemoveArgs::default(), &obs_root_ws("default")).unwrap_err();

  assert!(matches!(err, CoreError::DefaultWorkspace(_)));
}

#[test]
fn remove_refuses_workspace_rooted_at_repo_root_under_any_name() {
  let cfg = MergedConfig::from_project(Config::default());
  let err = plan_remove(&cfg, "main", &RemoveArgs::default(), &obs_root_ws("main")).unwrap_err();

  assert!(matches!(err, CoreError::DefaultWorkspace(_)));
}

#[test]
fn remove_ignores_non_empty_wc_and_never_touches_bookmarks() {
  let cfg = MergedConfig::from_project(Config::default());
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    workspaces: vec![Workspace {
      name: "feat-x".into(),
      path: PathBuf::from("/repo/.worktrees/feat-x"),
      stale: false,
    }],
    target_bookmark_exists: true,
    ..Default::default()
  };
  let plan = plan_remove(&cfg, "feat-x", &RemoveArgs::default(), &obs).expect("plan ok");
  let kinds: Vec<&str> = plan
    .actions
    .iter()
    .map(|a| match a {
      Action::JjSnapshot { .. } => "snapshot",
      Action::JjWorkspaceForget { .. } => "forget",
      Action::DeleteDir { .. } => "del",
      _ => "other",
    })
    .collect();

  assert_eq!(kinds, vec!["snapshot", "forget", "del"]);
}
