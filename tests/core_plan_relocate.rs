use jjwt::core::plan::plan_relocate;
use jjwt::core::types::*;
use std::path::PathBuf;

#[test]
fn relocate_emits_rename_actions() {
  let cfg = MergedConfig::from_project(Config::default());
  let args = RelocateArgs {
    old_name: "feat-a".into(),
    new_name: "feat-b".into(),
    rename_bookmark: false,
    format: OutputFormat::Text,
  };
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    workspaces: vec![Workspace {
      name: "feat-a".into(),
      path: PathBuf::from("/repo/.worktrees/feat-a"),
      stale: false,
    }],
    ..Default::default()
  };

  let plan = plan_relocate(&cfg, &args, &obs).expect("plan ok");

  assert!(plan.actions.iter().any(|a| matches!(
    a,
    Action::JjWorkspaceRename {
      old_name,
      new_name
    } if old_name == "feat-a" && new_name == "feat-b"
  )));
  assert!(plan.actions.iter().any(|a| matches!(
    a,
    Action::RenameDir { from, to }
      if from == &PathBuf::from("/repo/.worktrees/feat-a")
        && to == &PathBuf::from("/repo/.worktrees/feat-b")
  )));
  assert!(
    !plan
      .actions
      .iter()
      .any(|a| matches!(a, Action::JjBookmarkRename { .. }))
  );
}

#[test]
fn relocate_with_bookmark_rename() {
  let cfg = MergedConfig::from_project(Config::default());
  let args = RelocateArgs {
    old_name: "feat-a".into(),
    new_name: "feat-b".into(),
    rename_bookmark: true,
    format: OutputFormat::Text,
  };
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    workspaces: vec![Workspace {
      name: "feat-a".into(),
      path: PathBuf::from("/repo/.worktrees/feat-a"),
      stale: false,
    }],
    ..Default::default()
  };

  let plan = plan_relocate(&cfg, &args, &obs).expect("plan ok");

  assert!(plan.actions.iter().any(|a| matches!(
    a,
    Action::JjBookmarkRename {
      old_name,
      new_name
    } if old_name == "feat-a" && new_name == "feat-b"
  )));
}

#[test]
fn relocate_missing_workspace_errors() {
  let cfg = MergedConfig::from_project(Config::default());
  let args = RelocateArgs {
    old_name: "feat-a".into(),
    new_name: "feat-b".into(),
    rename_bookmark: false,
    format: OutputFormat::Text,
  };
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    workspaces: vec![],
    ..Default::default()
  };

  let err = plan_relocate(&cfg, &args, &obs).unwrap_err();

  assert!(matches!(err, CoreError::WorkspaceMissing(_)));
}

#[test]
fn relocate_conflicting_name_errors() {
  let cfg = MergedConfig::from_project(Config::default());
  let args = RelocateArgs {
    old_name: "feat-a".into(),
    new_name: "feat-b".into(),
    rename_bookmark: false,
    format: OutputFormat::Text,
  };
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    workspaces: vec![
      Workspace {
        name: "feat-a".into(),
        path: PathBuf::from("/repo/.worktrees/feat-a"),
        stale: false,
      },
      Workspace {
        name: "feat-b".into(),
        path: PathBuf::from("/repo/.worktrees/feat-b"),
        stale: false,
      },
    ],
    ..Default::default()
  };

  let err = plan_relocate(&cfg, &args, &obs).unwrap_err();

  assert!(matches!(err, CoreError::WorkspaceExists(_)));
}

#[test]
fn relocate_json_format_emits_json_object() {
  let cfg = MergedConfig::from_project(Config::default());
  let args = RelocateArgs {
    old_name: "feat-a".into(),
    new_name: "feat-b".into(),
    rename_bookmark: true,
    format: OutputFormat::Json,
  };
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    workspaces: vec![Workspace {
      name: "feat-a".into(),
      path: PathBuf::from("/repo/.worktrees/feat-a"),
      stale: false,
    }],
    ..Default::default()
  };

  let plan = plan_relocate(&cfg, &args, &obs).expect("plan ok");

  let json_line = plan.actions.iter().find_map(|a| match a {
    Action::PrintLine(s) => Some(s.clone()),
    _ => None,
  });

  let parsed: serde_json::Value =
    serde_json::from_str(&json_line.expect("should have PrintLine")).expect("valid json");

  assert_eq!(parsed["old_name"], "feat-a");
  assert_eq!(parsed["new_name"], "feat-b");
  assert_eq!(parsed["old_path"], "/repo/.worktrees/feat-a");
  assert_eq!(parsed["new_path"], "/repo/.worktrees/feat-b");
  assert_eq!(parsed["bookmark_renamed"], true);
}

#[test]
fn relocate_json_format_bookmark_renamed_false() {
  let cfg = MergedConfig::from_project(Config::default());
  let args = RelocateArgs {
    old_name: "feat-a".into(),
    new_name: "feat-b".into(),
    rename_bookmark: false,
    format: OutputFormat::Json,
  };
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    workspaces: vec![Workspace {
      name: "feat-a".into(),
      path: PathBuf::from("/repo/.worktrees/feat-a"),
      stale: false,
    }],
    ..Default::default()
  };

  let plan = plan_relocate(&cfg, &args, &obs).expect("plan ok");

  let json_line = plan.actions.iter().find_map(|a| match a {
    Action::PrintLine(s) => Some(s.clone()),
    _ => None,
  });

  let parsed: serde_json::Value =
    serde_json::from_str(&json_line.expect("should have PrintLine")).expect("valid json");

  assert_eq!(parsed["bookmark_renamed"], false);
}

#[test]
fn relocate_not_jj_repo_errors() {
  let cfg = MergedConfig::from_project(Config::default());
  let args = RelocateArgs {
    old_name: "feat-a".into(),
    new_name: "feat-b".into(),
    ..Default::default()
  };
  let obs = ObservedState {
    is_jj_repo: false,
    ..Default::default()
  };

  let err = plan_relocate(&cfg, &args, &obs).unwrap_err();

  assert!(matches!(err, CoreError::NotJjRepo));
}
