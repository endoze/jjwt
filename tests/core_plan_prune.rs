use jjwt::core::plan::plan_prune;
use jjwt::core::types::*;
use std::path::PathBuf;

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
  let cfg = Config::default();
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
  let cfg = Config::default();
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
  let cfg = Config::default();
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
  let cfg = Config::default();
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
