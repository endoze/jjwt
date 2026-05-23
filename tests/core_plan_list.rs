use jjwt::core::plan::plan_list;
use jjwt::core::types::*;
use std::path::PathBuf;

const DISPLAY: DisplayHints = DisplayHints {
  styled: false,
  term_width: None,
};

fn cfg_with_list() -> MergedConfig {
  MergedConfig::from_project(Config {
    list: Some(ListConfig {
      url: "http://example.com/{{ branch }}".into(),
      summary: None,
    }),
    pre_start: vec![],
    pre_remove: vec![],
    ..Default::default()
  })
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
    extra_bookmark_names: Vec::new(),
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
        ci_status: CiStatus::None,
        summary: String::new(),
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
        ci_status: CiStatus::None,
        summary: String::new(),
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
  let cfg = MergedConfig::from_project(Config {
    list: None,
    pre_start: vec![],
    pre_remove: vec![],
    ..Default::default()
  });
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

#[test]
fn list_json_format_emits_valid_json_array() {
  let plan = plan_list(
    &cfg_with_list(),
    &obs_with_workspaces(),
    &DISPLAY,
    OutputFormat::Json,
  )
  .expect("plan ok");

  assert_eq!(plan.actions.len(), 1);

  let Action::PrintLine(out) = &plan.actions[0] else {
    panic!()
  };

  let parsed: Vec<serde_json::Value> = serde_json::from_str(out).expect("valid json array");

  assert_eq!(parsed.len(), 2);
  assert_eq!(parsed[0]["name"], "default");
  assert_eq!(parsed[1]["name"], "feat");
}

#[test]
fn list_json_format_includes_all_fields() {
  let plan = plan_list(
    &cfg_with_list(),
    &obs_with_workspaces(),
    &DISPLAY,
    OutputFormat::Json,
  )
  .expect("plan ok");

  let Action::PrintLine(out) = &plan.actions[0] else {
    panic!()
  };

  let parsed: Vec<serde_json::Value> = serde_json::from_str(out).expect("valid json array");

  // Check default workspace
  let default_ws = &parsed[0];

  assert_eq!(default_ws["kind"], "workspace");
  assert_eq!(default_ws["is_current"], true);
  assert_eq!(default_ws["is_default"], true);
  assert_eq!(default_ws["commit"], "aaaaaaaa");
  assert_eq!(default_ws["message"], "init");
  assert_eq!(default_ws["ci_status"], "none");
  assert!(default_ws["url"].is_string());

  // Check status sub-object
  let status = &default_ws["status"];

  assert_eq!(status["has_changes"], false);
  assert_eq!(status["modified"], false);

  // Check head_diff sub-object
  let head_diff = &default_ws["head_diff"];

  assert_eq!(head_diff["added"], 0);
  assert_eq!(head_diff["removed"], 0);

  // Check vs_trunk sub-object
  let vs_trunk = &default_ws["vs_trunk"];

  assert_eq!(vs_trunk["ahead"], 0);
  assert_eq!(vs_trunk["behind"], 0);

  // Check feat workspace
  let feat_ws = &parsed[1];

  assert_eq!(feat_ws["is_current"], false);
  assert_eq!(feat_ws["is_default"], false);
  assert_eq!(feat_ws["vs_trunk"]["ahead"], 3);
  assert_eq!(feat_ws["vs_trunk"]["behind"], 1);
}

#[test]
fn list_json_format_null_fields_when_empty() {
  let cfg = MergedConfig::from_project(Config::default());
  let plan =
    plan_list(&cfg, &obs_with_workspaces(), &DISPLAY, OutputFormat::Json).expect("plan ok");

  let Action::PrintLine(out) = &plan.actions[0] else {
    panic!()
  };

  let parsed: Vec<serde_json::Value> = serde_json::from_str(out).expect("valid json array");

  // url should be null when no list config
  assert!(parsed[0]["url"].is_null());
  // summary should be null when empty
  assert!(parsed[0]["summary"].is_null());
}

#[test]
fn list_statusline_format_emits_compact_line() {
  let plan = plan_list(
    &cfg_with_list(),
    &obs_with_workspaces(),
    &DISPLAY,
    OutputFormat::Statusline,
  )
  .expect("plan ok");

  assert_eq!(plan.actions.len(), 1);

  let Action::PrintLine(out) = &plan.actions[0] else {
    panic!()
  };

  assert!(out.contains("@default"));
  assert!(out.contains("2 ws"));
}

#[test]
fn list_json_with_extra_bookmark_rows() {
  let mut obs = obs_with_workspaces();

  obs.extra_bookmark_names = vec!["orphan-branch".into()];

  let plan = plan_list(&cfg_with_list(), &obs, &DISPLAY, OutputFormat::Json).expect("plan ok");

  let Action::PrintLine(out) = &plan.actions[0] else {
    panic!()
  };

  let parsed: Vec<serde_json::Value> = serde_json::from_str(out).expect("valid json array");

  assert_eq!(parsed.len(), 3);
  assert_eq!(parsed[2]["name"], "orphan-branch");
  assert_eq!(parsed[2]["kind"], "bookmark");
}
