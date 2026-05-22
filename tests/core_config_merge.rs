use indexmap::IndexMap;
use jjwt::core::types::*;

fn hook(key: &str, cmd: &str) -> IndexMap<String, String> {
  let mut g = IndexMap::new();

  g.insert(key.into(), cmd.into());

  g
}

#[test]
fn merge_scalars_project_wins() {
  let user = Config {
    background_remove: Some(true),
    worktree_path_template: Some(".wt/{{ branch }}".into()),
    ..Default::default()
  };
  let project = Config {
    background_remove: Some(false),
    worktree_path_template: Some(".worktrees/{{ branch }}".into()),
    ..Default::default()
  };

  let merged = MergedConfig::from_layers(Some(&user), Some(&project));

  assert_eq!(merged.background_remove, Some(false));
  assert_eq!(
    merged.worktree_path_template.as_deref(),
    Some(".worktrees/{{ branch }}")
  );
}

#[test]
fn merge_scalars_user_default_when_project_absent() {
  let user = Config {
    background_remove: Some(true),
    worktree_path_template: Some(".wt/{{ branch }}".into()),
    list: Some(ListConfig {
      url: "http://localhost:{{ branch | hash_port }}".into(),
    }),
    ..Default::default()
  };
  let project = Config::default();

  let merged = MergedConfig::from_layers(Some(&user), Some(&project));

  assert_eq!(merged.background_remove, Some(true));
  assert_eq!(
    merged.worktree_path_template.as_deref(),
    Some(".wt/{{ branch }}")
  );
  assert!(merged.list.is_some());
}

#[test]
fn merge_aliases_project_overrides_per_key() {
  let mut user_aliases = IndexMap::new();
  user_aliases.insert("a".into(), "user-a".into());
  user_aliases.insert("b".into(), "user-b".into());

  let mut project_aliases = IndexMap::new();
  project_aliases.insert("b".into(), "project-b".into());
  project_aliases.insert("c".into(), "project-c".into());

  let user = Config {
    aliases: user_aliases,
    ..Default::default()
  };
  let project = Config {
    aliases: project_aliases,
    ..Default::default()
  };

  let merged = MergedConfig::from_layers(Some(&user), Some(&project));

  assert_eq!(merged.aliases.get("a").unwrap(), "user-a");
  assert_eq!(merged.aliases.get("b").unwrap(), "project-b");
  assert_eq!(merged.aliases.get("c").unwrap(), "project-c");
  assert_eq!(merged.aliases.len(), 3);
}

#[test]
fn merge_hooks_concatenate_user_first() {
  let user = Config {
    pre_start: vec![hook("user-setup", "npm install")],
    ..Default::default()
  };
  let project = Config {
    pre_start: vec![hook("project-db", "make db-start")],
    ..Default::default()
  };

  let merged = MergedConfig::from_layers(Some(&user), Some(&project));

  assert_eq!(merged.pre_start.len(), 2);
  assert_eq!(merged.pre_start[0].source, HookSource::User);
  assert!(merged.pre_start[0].group.contains_key("user-setup"));
  assert_eq!(merged.pre_start[1].source, HookSource::Project);
  assert!(merged.pre_start[1].group.contains_key("project-db"));
}

#[test]
fn merge_hooks_source_tracking() {
  let user = Config {
    pre_switch: vec![hook("a", "cmd-a")],
    post_remove: vec![hook("b", "cmd-b")],
    ..Default::default()
  };
  let project = Config {
    pre_switch: vec![hook("c", "cmd-c")],
    ..Default::default()
  };

  let merged = MergedConfig::from_layers(Some(&user), Some(&project));

  // pre_switch has both layers
  assert_eq!(merged.pre_switch.len(), 2);
  assert_eq!(merged.pre_switch[0].source, HookSource::User);
  assert_eq!(merged.pre_switch[1].source, HookSource::Project);

  // post_remove only has user
  assert_eq!(merged.post_remove.len(), 1);
  assert_eq!(merged.post_remove[0].source, HookSource::User);
}

#[test]
fn merge_user_only() {
  let user = Config {
    background_remove: Some(true),
    pre_start: vec![hook("setup", "npm ci")],
    ..Default::default()
  };

  let merged = MergedConfig::from_layers(Some(&user), None);

  assert_eq!(merged.background_remove, Some(true));
  assert_eq!(merged.pre_start.len(), 1);
  assert_eq!(merged.pre_start[0].source, HookSource::User);
}

#[test]
fn merge_project_only() {
  let project = Config {
    background_remove: Some(false),
    pre_start: vec![hook("db", "make db")],
    ..Default::default()
  };

  let merged = MergedConfig::from_layers(None, Some(&project));

  assert_eq!(merged.background_remove, Some(false));
  assert_eq!(merged.pre_start.len(), 1);
  assert_eq!(merged.pre_start[0].source, HookSource::Project);
}

#[test]
fn merge_both_empty() {
  let merged = MergedConfig::from_layers(Some(&Config::default()), Some(&Config::default()));

  assert!(merged.list.is_none());
  assert!(merged.background_remove.is_none());
  assert!(merged.worktree_path_template.is_none());
  assert!(merged.aliases.is_empty());
  assert!(merged.pre_switch.is_empty());
  assert!(merged.post_switch.is_empty());
  assert!(merged.pre_start.is_empty());
  assert!(merged.post_start.is_empty());
  assert!(merged.pre_remove.is_empty());
  assert!(merged.post_remove.is_empty());
}

#[test]
fn from_project_convenience_tags_all_as_project() {
  let cfg = Config {
    pre_start: vec![hook("a", "cmd-a"), hook("b", "cmd-b")],
    post_switch: vec![hook("c", "cmd-c")],
    ..Default::default()
  };

  let merged = MergedConfig::from_project(cfg);

  for shg in &merged.pre_start {
    assert_eq!(shg.source, HookSource::Project);
  }

  for shg in &merged.post_switch {
    assert_eq!(shg.source, HookSource::Project);
  }
}

#[test]
fn all_hook_groups_returns_all_types() {
  let cfg = Config {
    pre_switch: vec![hook("a", "1")],
    post_switch: vec![hook("b", "2")],
    pre_start: vec![hook("c", "3")],
    post_start: vec![hook("d", "4")],
    pre_remove: vec![hook("e", "5")],
    post_remove: vec![hook("f", "6")],
    ..Default::default()
  };

  let merged = MergedConfig::from_project(cfg);
  let groups = merged.all_hook_groups();

  assert_eq!(groups.len(), 6);

  let types: Vec<&str> = groups.iter().map(|(t, _)| *t).collect();

  assert_eq!(
    types,
    vec![
      "pre-switch",
      "post-switch",
      "pre-start",
      "post-start",
      "pre-remove",
      "post-remove"
    ]
  );
}
