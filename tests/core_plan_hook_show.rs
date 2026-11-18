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

#[test]
fn show_json_expanded_includes_rendered_field() {
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

  let plan = plan_hook_show(&cfg, true, Some(&obs), OutputFormat::Json, None).expect("plan ok");

  let output = match &plan.actions[0] {
    Action::PrintLine(s) => s.clone(),
    _ => panic!("expected PrintLine"),
  };

  let parsed: Vec<serde_json::Value> = serde_json::from_str(&output).expect("valid json");

  assert_eq!(parsed.len(), 1);
  assert_eq!(parsed[0]["rendered"], "echo feat-x");
  assert_eq!(parsed[0]["template"], "echo {{ branch }}");
  assert_eq!(parsed[0]["source"], "project");
}

#[test]
fn show_json_not_expanded_omits_rendered_field() {
  let cfg = MergedConfig::from_project(Config {
    pre_start: vec![hook("setup", "echo {{ branch }}")],
    ..Default::default()
  });

  let plan = plan_hook_show(&cfg, false, None, OutputFormat::Json, None).expect("plan ok");

  let output = match &plan.actions[0] {
    Action::PrintLine(s) => s.clone(),
    _ => panic!("expected PrintLine"),
  };

  let parsed: Vec<serde_json::Value> = serde_json::from_str(&output).expect("valid json");

  assert!(
    parsed[0].get("rendered").is_none(),
    "rendered should not be present when not expanded"
  );
}

#[test]
fn show_statusline_format_emits_count() {
  let cfg = MergedConfig::from_project(Config {
    pre_start: vec![hook("a", "cmd1")],
    pre_remove: vec![hook("b", "cmd2")],
    ..Default::default()
  });

  let plan = plan_hook_show(&cfg, false, None, OutputFormat::Statusline, None).expect("plan ok");

  let output = match &plan.actions[0] {
    Action::PrintLine(s) => s.clone(),
    _ => panic!("expected PrintLine"),
  };

  assert_eq!(output, "2 hook(s) configured");
}

#[test]
fn show_source_filter_limits_results() {
  let cfg = MergedConfig::from_layers(
    Some(&Config {
      pre_start: vec![hook("user-hook", "cmd1")],
      ..Default::default()
    }),
    Some(&Config {
      pre_start: vec![hook("project-hook", "cmd2")],
      ..Default::default()
    }),
  );

  let plan = plan_hook_show(
    &cfg,
    false,
    None,
    OutputFormat::Json,
    Some(HookSource::Project),
  )
  .expect("plan ok");

  let output = match &plan.actions[0] {
    Action::PrintLine(s) => s.clone(),
    _ => panic!("expected PrintLine"),
  };

  let parsed: Vec<serde_json::Value> = serde_json::from_str(&output).expect("valid json");

  assert_eq!(parsed.len(), 1);
  assert_eq!(parsed[0]["name"], "project-hook");
  assert_eq!(parsed[0]["source"], "project");
}

#[test]
fn show_json_without_obs_when_expanded() {
  let cfg = MergedConfig::from_project(Config {
    pre_start: vec![hook("setup", "echo {{ branch }}")],
    ..Default::default()
  });

  // expanded=true but obs=None — should not crash, just skip rendered field
  let plan = plan_hook_show(&cfg, true, None, OutputFormat::Json, None).expect("plan ok");

  let output = match &plan.actions[0] {
    Action::PrintLine(s) => s.clone(),
    _ => panic!("expected PrintLine"),
  };

  let parsed: Vec<serde_json::Value> = serde_json::from_str(&output).expect("valid json");

  assert_eq!(parsed.len(), 1);
  // When obs is None and expanded is true, rendered should not be present
  assert!(
    parsed[0].get("rendered").is_none(),
    "rendered should not be present without obs"
  );
}

#[test]
fn show_text_expanded_shows_rendered_header() {
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

  assert!(
    output.contains("Rendered"),
    "expanded text output should have 'Rendered' header"
  );
  assert!(output.contains("echo feat-x"));
}

#[test]
fn show_text_truncates_long_templates() {
  let long_template = "a".repeat(80);
  let cfg = MergedConfig::from_project(Config {
    pre_start: vec![hook("setup", &long_template)],
    ..Default::default()
  });

  let plan = plan_hook_show(&cfg, false, None, OutputFormat::Text, None).expect("plan ok");

  let output = match &plan.actions[0] {
    Action::PrintLine(s) => s.clone(),
    _ => panic!("expected PrintLine"),
  };

  assert!(
    output.contains("..."),
    "long template should be truncated with '...'"
  );
  assert!(
    !output.contains(&long_template),
    "full template should not appear"
  );
}

#[test]
fn show_text_expanded_without_obs_falls_back_to_template() {
  let cfg = MergedConfig::from_project(Config {
    pre_start: vec![hook("setup", "echo {{ branch }}")],
    ..Default::default()
  });

  let plan = plan_hook_show(&cfg, true, None, OutputFormat::Text, None).expect("plan ok");

  let output = match &plan.actions[0] {
    Action::PrintLine(s) => s.clone(),
    _ => panic!("expected PrintLine"),
  };

  assert!(
    output.contains("echo {{ branch }}"),
    "without obs, expanded should fall back to template"
  );
}

#[test]
fn show_json_expanded_with_workspace_not_found_falls_back() {
  let cfg = MergedConfig::from_project(Config {
    pre_start: vec![hook("setup", "echo {{ branch }}")],
    ..Default::default()
  });
  let obs = ObservedState {
    repo_root: PathBuf::from("/repo"),
    is_jj_repo: true,
    current_workspace: Some("missing".into()),
    workspaces: vec![],
    ..Default::default()
  };

  let plan = plan_hook_show(&cfg, true, Some(&obs), OutputFormat::Json, None).expect("plan ok");

  let output = match &plan.actions[0] {
    Action::PrintLine(s) => s.clone(),
    _ => panic!("expected PrintLine"),
  };

  let parsed: Vec<serde_json::Value> = serde_json::from_str(&output).expect("valid json");

  assert!(parsed[0].get("rendered").is_some());
}
