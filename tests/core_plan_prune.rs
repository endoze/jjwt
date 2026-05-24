use indexmap::IndexMap;
use jjwt::core::plan::plan_prune;
use jjwt::core::types::*;
use std::path::PathBuf;

fn hook(key: &str, cmd: &str) -> IndexMap<String, String> {
  let mut g = IndexMap::new();

  g.insert(key.into(), cmd.into());

  g
}

fn make_obs(
  workspaces: Vec<(&str, &str)>,
  status: Vec<(&str, bool, bool, bool)>,
  current: Option<&str>,
) -> ObservedPruneState {
  ObservedPruneState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    current_workspace: current.map(String::from),
    workspaces: workspaces
      .into_iter()
      .map(|(name, path)| Workspace {
        name: name.into(),
        path: PathBuf::from(path),
        stale: false,
      })
      .collect(),
    workspace_status: status
      .into_iter()
      .map(|(n, e, m, d)| (n.into(), e, m, d))
      .collect(),
  }
}

#[test]
fn prunes_merged_workspaces() {
  let cfg = MergedConfig::from_project(Config::default());
  let args = PruneArgs::default();
  let obs = make_obs(
    vec![
      ("default", "/repo"),
      ("feat-a", "/repo/.worktrees/feat-a"),
      ("feat-b", "/repo/.worktrees/feat-b"),
      ("feat-c", "/repo/.worktrees/feat-c"),
    ],
    vec![
      ("default", false, false, false),
      ("feat-a", true, true, false),  // merged
      ("feat-b", true, false, false), // not merged
      ("feat-c", true, true, false),  // merged
    ],
    Some("feat-b"), // current — should be skipped
  );

  let plan = plan_prune(&cfg, &args, &obs).expect("plan ok");

  // Should prune feat-a and feat-c (not default, not current feat-b, not unmerged feat-b)
  let forgotten: Vec<&str> = plan
    .actions
    .iter()
    .filter_map(|a| match a {
      Action::JjWorkspaceForget { name } => Some(name.as_str()),
      _ => None,
    })
    .collect();

  assert_eq!(forgotten, vec!["feat-a", "feat-c"]);
}

#[test]
fn dry_run_emits_only_print() {
  let cfg = MergedConfig::from_project(Config::default());
  let args = PruneArgs {
    dry_run: true,
    ..Default::default()
  };
  let obs = make_obs(
    vec![("default", "/repo"), ("feat-a", "/repo/.worktrees/feat-a")],
    vec![
      ("default", false, false, false),
      ("feat-a", true, true, false),
    ],
    None,
  );

  let plan = plan_prune(&cfg, &args, &obs).expect("plan ok");

  // Should only have PrintLine, no actual actions
  for a in &plan.actions {
    assert!(
      matches!(a, Action::PrintLine(_)),
      "dry_run should only emit PrintLine, got: {a:?}"
    );
  }

  let output = plan.actions.iter().find_map(|a| match a {
    Action::PrintLine(s) => Some(s.clone()),
    _ => None,
  });

  assert!(output.unwrap().contains("feat-a"));
}

#[test]
fn nothing_to_prune() {
  let cfg = MergedConfig::from_project(Config::default());
  let args = PruneArgs::default();
  let obs = make_obs(
    vec![("default", "/repo"), ("feat-a", "/repo/.worktrees/feat-a")],
    vec![
      ("default", false, false, false),
      ("feat-a", true, false, false), // not merged
    ],
    None,
  );

  let plan = plan_prune(&cfg, &args, &obs).expect("plan ok");

  let output = plan.actions.iter().find_map(|a| match a {
    Action::PrintLine(s) => Some(s.clone()),
    _ => None,
  });

  assert!(output.unwrap().contains("Nothing to prune"));
}

#[test]
fn skips_current_workspace() {
  let cfg = MergedConfig::from_project(Config::default());
  let args = PruneArgs::default();
  let obs = make_obs(
    vec![("default", "/repo"), ("feat-a", "/repo/.worktrees/feat-a")],
    vec![
      ("default", false, false, false),
      ("feat-a", true, true, false), // merged but current
    ],
    Some("feat-a"),
  );

  let plan = plan_prune(&cfg, &args, &obs).expect("plan ok");

  let output = plan.actions.iter().find_map(|a| match a {
    Action::PrintLine(s) => Some(s.clone()),
    _ => None,
  });

  assert!(output.unwrap().contains("Nothing to prune"));
}

#[test]
fn prune_json_format_dry_run() {
  let cfg = MergedConfig::from_project(Config::default());
  let args = PruneArgs {
    dry_run: true,
    format: OutputFormat::Json,
    ..Default::default()
  };
  let obs = make_obs(
    vec![("default", "/repo"), ("feat-a", "/repo/.worktrees/feat-a")],
    vec![
      ("default", false, false, false),
      ("feat-a", true, true, false),
    ],
    None,
  );

  let plan = plan_prune(&cfg, &args, &obs).expect("plan ok");

  let json_line = plan.actions.iter().find_map(|a| match a {
    Action::PrintLine(s) => Some(s.clone()),
    _ => None,
  });

  let parsed: serde_json::Value =
    serde_json::from_str(&json_line.expect("should have PrintLine")).expect("valid json");

  assert_eq!(parsed["dry_run"], true);

  let pruned = parsed["pruned"].as_array().expect("pruned is array");

  assert_eq!(pruned.len(), 1);
  assert_eq!(pruned[0], "feat-a");
}

#[test]
fn prune_json_format_actual_run() {
  let cfg = MergedConfig::from_project(Config::default());
  let args = PruneArgs {
    dry_run: false,
    format: OutputFormat::Json,
    ..Default::default()
  };
  let obs = make_obs(
    vec![
      ("default", "/repo"),
      ("feat-a", "/repo/.worktrees/feat-a"),
      ("feat-b", "/repo/.worktrees/feat-b"),
    ],
    vec![
      ("default", false, false, false),
      ("feat-a", true, true, false),
      ("feat-b", true, true, false),
    ],
    None,
  );

  let plan = plan_prune(&cfg, &args, &obs).expect("plan ok");

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

  assert_eq!(parsed["dry_run"], false);

  let pruned = parsed["pruned"].as_array().expect("pruned is array");

  assert_eq!(pruned.len(), 2);
  assert_eq!(pruned[0], "feat-a");
  assert_eq!(pruned[1], "feat-b");
}

#[test]
fn prune_json_format_nothing_to_prune() {
  let cfg = MergedConfig::from_project(Config::default());
  let args = PruneArgs {
    format: OutputFormat::Json,
    ..Default::default()
  };
  let obs = make_obs(
    vec![("default", "/repo"), ("feat-a", "/repo/.worktrees/feat-a")],
    vec![
      ("default", false, false, false),
      ("feat-a", true, false, false), // not merged
    ],
    None,
  );

  let plan = plan_prune(&cfg, &args, &obs).expect("plan ok");

  let json_line = plan.actions.iter().find_map(|a| match a {
    Action::PrintLine(s) => Some(s.clone()),
    _ => None,
  });

  let parsed: serde_json::Value =
    serde_json::from_str(&json_line.expect("should have PrintLine")).expect("valid json");

  assert_eq!(parsed["dry_run"], false);

  let pruned = parsed["pruned"].as_array().expect("pruned is array");

  assert!(pruned.is_empty());
}

#[test]
fn prune_actual_run_with_hooks_emits_hook_actions() {
  let cfg = MergedConfig::from_project(Config {
    pre_remove: vec![hook("cleanup", "echo removing {{ branch }}")],
    post_remove: vec![hook("notify", "echo removed {{ branch }}")],
    ..Default::default()
  });
  let args = PruneArgs {
    dry_run: false,
    no_hooks: false,
    ..Default::default()
  };
  let obs = make_obs(
    vec![("default", "/repo"), ("feat-a", "/repo/.worktrees/feat-a")],
    vec![
      ("default", false, false, false),
      ("feat-a", true, true, false),
    ],
    None,
  );

  let plan = plan_prune(&cfg, &args, &obs).expect("plan ok");

  let kinds: Vec<&str> = plan
    .actions
    .iter()
    .map(|a| match a {
      Action::RunHook { env, .. } => env
        .iter()
        .find(|(k, _)| k == "JJWT_HOOK_TYPE")
        .map(|(_, v)| match v.as_str() {
          "pre-remove" => "pre-remove",
          "post-remove" => "post-remove",
          _ => "hook?",
        })
        .unwrap_or("hook?"),
      Action::JjWorkspaceForget { .. } => "forget",
      Action::DeleteDir { .. } => "del",
      Action::JjBookmarkDelete { .. } => "bookmark-del",
      Action::PrintLine(_) => "print",
      _ => "other",
    })
    .collect();

  assert_eq!(
    kinds,
    vec![
      "pre-remove",
      "forget",
      "del",
      "bookmark-del",
      "post-remove",
      "print"
    ],
  );
}

#[test]
fn prune_actual_run_with_no_hooks_skips_hooks() {
  let cfg = MergedConfig::from_project(Config {
    pre_remove: vec![hook("cleanup", "echo removing")],
    ..Default::default()
  });
  let args = PruneArgs {
    dry_run: false,
    no_hooks: true,
    ..Default::default()
  };
  let obs = make_obs(
    vec![("default", "/repo"), ("feat-a", "/repo/.worktrees/feat-a")],
    vec![
      ("default", false, false, false),
      ("feat-a", true, true, false),
    ],
    None,
  );

  let plan = plan_prune(&cfg, &args, &obs).expect("plan ok");

  assert!(
    !plan
      .actions
      .iter()
      .any(|a| matches!(a, Action::RunHook { .. })),
    "no_hooks should suppress hooks in prune"
  );
  assert!(
    plan
      .actions
      .iter()
      .any(|a| matches!(a, Action::JjWorkspaceForget { .. })),
    "should still have workspace forget"
  );
}

#[test]
fn prune_actual_run_with_background_remove() {
  let cfg = MergedConfig::from_project(Config {
    background_remove: Some(true),
    ..Default::default()
  });
  let args = PruneArgs::default();
  let obs = make_obs(
    vec![("default", "/repo"), ("feat-a", "/repo/.worktrees/feat-a")],
    vec![
      ("default", false, false, false),
      ("feat-a", true, true, false),
    ],
    None,
  );

  let plan = plan_prune(&cfg, &args, &obs).expect("plan ok");

  assert!(
    plan
      .actions
      .iter()
      .any(|a| matches!(a, Action::DeleteDirBackground { .. })),
    "should use DeleteDirBackground when background_remove=true"
  );
  assert!(
    !plan
      .actions
      .iter()
      .any(|a| matches!(a, Action::DeleteDir { .. })),
    "should not have sync DeleteDir"
  );
}

#[test]
fn prune_not_jj_repo_errors() {
  let cfg = MergedConfig::from_project(Config::default());
  let args = PruneArgs::default();
  let obs = ObservedPruneState {
    is_jj_repo: false,
    ..Default::default()
  };

  let err = plan_prune(&cfg, &args, &obs).unwrap_err();

  assert!(matches!(err, CoreError::NotJjRepo));
}
