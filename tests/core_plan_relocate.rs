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
