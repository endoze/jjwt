use indexmap::IndexMap;
use jjwt::core::plan::plan_alias;
use jjwt::core::types::*;
use std::path::PathBuf;

fn cfg_with(aliases: &[(&str, &str)]) -> MergedConfig {
  let mut a = IndexMap::new();

  for (k, v) in aliases {
    a.insert((*k).into(), (*v).into());
  }

  MergedConfig::from_project(Config {
    aliases: a,
    ..Default::default()
  })
}

fn obs_inside(name: &str) -> ObservedState {
  ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    workspaces: vec![Workspace {
      name: name.into(),
      path: PathBuf::from(format!("/repo/.worktrees/{name}")),
      stale: false,
    }],
    current_workspace: Some(name.into()),
    ..Default::default()
  }
}

#[test]
fn renders_alias_with_branch_and_args() {
  let cfg = cfg_with(&[(
    "greet",
    "echo Hello from {{ branch }} :: {{ args | length }}",
  )]);
  let plan = plan_alias(
    &cfg,
    &AliasArgs {
      name: "greet".into(),
      forwarded: vec!["a".into(), "b".into(), "c".into()],
    },
    &obs_inside("feat"),
  )
  .expect("plan ok");

  assert_eq!(plan.actions.len(), 1);
  let Action::Exec { rendered_cmd, .. } = &plan.actions[0] else {
    panic!("expected Exec action, got {:?}", plan.actions[0])
  };
  assert_eq!(rendered_cmd, "echo Hello from feat :: 3");
}

#[test]
fn alias_not_found_errors() {
  let cfg = cfg_with(&[]);
  let err = plan_alias(
    &cfg,
    &AliasArgs {
      name: "nope".into(),
      forwarded: vec![],
    },
    &obs_inside("feat"),
  );

  assert!(matches!(err, Err(CoreError::AliasNotFound(_))));
}

#[test]
fn alias_renders_outside_workspace_with_empty_branch() {
  let cfg = cfg_with(&[("here", "pwd at {{ repo }}")]);
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    workspaces: vec![],
    current_workspace: None,
    ..Default::default()
  };
  let plan = plan_alias(
    &cfg,
    &AliasArgs {
      name: "here".into(),
      forwarded: vec![],
    },
    &obs,
  )
  .expect("plan ok");

  let Action::Exec {
    rendered_cmd, cwd, ..
  } = &plan.actions[0]
  else {
    panic!()
  };
  assert_eq!(rendered_cmd, "pwd at repo");
  assert_eq!(cwd, &PathBuf::from("/repo"));
}
