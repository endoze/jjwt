use jjwt::core::plan::plan_list;
use jjwt::core::types::*;
use std::path::PathBuf;

const DISPLAY: DisplayHints = DisplayHints {
  styled: false,
  term_width: None,
};

fn cfg_with_list() -> Config {
  Config {
    list: Some(ListConfig {
      url: "http://example.com/{{ branch }}".into(),
    }),
    pre_start: vec![],
    pre_remove: vec![],
    ..Default::default()
  }
}

fn details(commit: &str, msg: &str) -> WorkspaceDetails {
  WorkspaceDetails {
    modified: false,
    untracked: false,
    conflicts: false,
    commit_short: commit.into(),
    age_seconds: 0,
    message_first_line: msg.into(),
    head_added: 0,
    head_removed: 0,
  }
}

fn obs_with_workspaces() -> ObservedListState {
  ObservedListState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    current_workspace: Some("default".into()),
    extra_branch_names: Vec::new(),
    extra_remote_only_names: Vec::new(),
    rows: vec![
      ObservedListRow {
        workspace: Workspace {
          name: "default".into(),
          path: PathBuf::from("/repo"),
          stale: false,
        },
        details: details("aaaaaaaa", "init"),
        ahead: 0,
        behind: 0,
        has_remote_bookmark: false,
      },
      ObservedListRow {
        workspace: Workspace {
          name: "feat".into(),
          path: PathBuf::from("/repo/.worktrees/feat"),
          stale: false,
        },
        details: details("bbbbbbbb", "feat: thing"),
        ahead: 3,
        behind: 1,
        has_remote_bookmark: true,
      },
    ],
  }
}

#[test]
fn list_renders_url_per_workspace() {
  let plan = plan_list(
    &cfg_with_list(),
    &obs_with_workspaces(),
    &DISPLAY,
    OutputFormat::Text,
  )
  .expect("plan ok");

  assert_eq!(plan.actions.len(), 1);

  let Action::PrintLine(out) = &plan.actions[0] else {
    panic!()
  };

  assert!(out.contains("default"));
  assert!(out.contains("feat"));
  assert!(out.contains("http://example.com/default"));
  assert!(out.contains("http://example.com/feat"));
}

#[test]
fn list_without_list_config_still_prints_names() {
  let cfg = Config {
    list: None,
    pre_start: vec![],
    pre_remove: vec![],
    ..Default::default()
  };
  let plan =
    plan_list(&cfg, &obs_with_workspaces(), &DISPLAY, OutputFormat::Text).expect("plan ok");
  let Action::PrintLine(out) = &plan.actions[0] else {
    panic!()
  };

  assert!(out.contains("default"));
  assert!(out.contains("feat"));
}

#[test]
fn list_default_workspace_has_dot_path_not_worktrees() {
  let plan = plan_list(
    &cfg_with_list(),
    &obs_with_workspaces(),
    &DISPLAY,
    OutputFormat::Text,
  )
  .expect("plan ok");
  let Action::PrintLine(out) = &plan.actions[0] else {
    panic!()
  };

  // Default workspace renders Path as "." and non-defaults as "./.worktrees/<name>"
  assert!(
    out
      .lines()
      .any(|l| l.starts_with("@ default") && l.contains(" . ")),
    "expected default workspace row with '.' path, got:\n{out}"
  );
  assert!(
    out.contains("./.worktrees/feat"),
    "expected feat workspace path, got:\n{out}"
  );
}

#[test]
fn list_footer_counts_ahead_and_dirty() {
  let mut obs = obs_with_workspaces();

  obs.rows[1].details.modified = true;
  let plan = plan_list(&cfg_with_list(), &obs, &DISPLAY, OutputFormat::Text).expect("plan ok");
  let Action::PrintLine(out) = &plan.actions[0] else {
    panic!()
  };

  assert!(
    out.contains("○ Showing 2 worktrees, 1 with changes, 1 ahead"),
    "footer missing or wrong; got:\n{out}"
  );
}

#[test]
fn list_errors_when_not_jj_repo() {
  let obs = ObservedListState {
    repo_root: PathBuf::from("/tmp"),
    is_jj_repo: false,
    ..Default::default()
  };
  let err = plan_list(&cfg_with_list(), &obs, &DISPLAY, OutputFormat::Text).unwrap_err();

  assert!(matches!(err, CoreError::NotJjRepo), "got: {err:?}");
}
