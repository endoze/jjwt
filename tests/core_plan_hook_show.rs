use indexmap::IndexMap;
use jjwt::core::plan::plan_hook_show;
use jjwt::core::types::*;
use std::path::PathBuf;

fn hook(key: &str, cmd: &str) -> IndexMap<String, String> {
  let mut g = IndexMap::new();

  g.insert(key.into(), cmd.into());

  g
}

#[test]
fn show_lists_all_hooks_text() {
  let cfg = MergedConfig::from_project(Config {
    pre_start: vec![hook("setup", "npm install")],
    pre_remove: vec![hook("cleanup", "docker stop {{ branch }}")],
    ..Default::default()
  });

  let plan = plan_hook_show(&cfg, false, None, OutputFormat::Text, None).expect("plan ok");

  assert_eq!(plan.actions.len(), 1);

  let output = match &plan.actions[0] {
    Action::PrintLine(s) => s.clone(),
    _ => panic!("expected PrintLine"),
  };

  assert!(output.contains("pre-start"));
  assert!(output.contains("setup"));
  assert!(output.contains("npm install"));
  assert!(output.contains("pre-remove"));
  assert!(output.contains("cleanup"));
  assert!(output.contains("docker stop {{ branch }}"));
}

#[test]
fn show_expanded_renders_templates() {
  let cfg = MergedConfig::from_project(Config {
    pre_start: vec![hook("setup", "echo {{ branch }}")],
    ..Default::default()
  });
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    current_workspace: Some("feat-x".into()),
    workspaces: vec![Workspace {
      name: "feat-x".into(),
      path: PathBuf::from("/repo/.worktrees/feat-x"),
      stale: false,
    }],
    ..Default::default()
  };

  let plan = plan_hook_show(&cfg, true, Some(&obs), OutputFormat::Text, None).expect("plan ok");

  let output = match &plan.actions[0] {
    Action::PrintLine(s) => s.clone(),
    _ => panic!("expected PrintLine"),
  };

  assert!(output.contains("echo feat-x"));
}

#[test]
fn show_json_format() {
  let cfg = MergedConfig::from_project(Config {
    pre_start: vec![hook("setup", "npm install")],
    ..Default::default()
  });

  let plan = plan_hook_show(&cfg, false, None, OutputFormat::Json, None).expect("plan ok");

  let output = match &plan.actions[0] {
    Action::PrintLine(s) => s.clone(),
    _ => panic!("expected PrintLine"),
  };

  let parsed: Vec<serde_json::Value> = serde_json::from_str(&output).expect("valid json");

  assert_eq!(parsed.len(), 1);
  assert_eq!(parsed[0]["type"], "pre-start");
  assert_eq!(parsed[0]["name"], "setup");
}

#[test]
fn show_empty_config() {
  let cfg = MergedConfig::from_project(Config::default());

  let plan = plan_hook_show(&cfg, false, None, OutputFormat::Text, None).expect("plan ok");

  let output = match &plan.actions[0] {
    Action::PrintLine(s) => s.clone(),
    _ => panic!("expected PrintLine"),
  };

  assert!(output.contains("No hooks configured"));
}
