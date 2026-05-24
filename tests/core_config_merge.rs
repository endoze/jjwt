use indexmap::IndexMap;
use jjwt::core::types::*;
use std::collections::HashMap;

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
    merged.worktree_path_template.as_str(),
    ".worktrees/{{ branch }}"
  );
}

#[test]
fn merge_scalars_user_default_when_project_absent() {
  let user = Config {
    background_remove: Some(true),
    worktree_path_template: Some(".wt/{{ branch }}".into()),
    list: Some(ListConfig {
      url: "http://localhost:{{ branch | hash_port }}".into(),
      summary: None,
    }),
    ..Default::default()
  };
  let project = Config::default();

  let merged = MergedConfig::from_layers(Some(&user), Some(&project));

  assert_eq!(merged.background_remove, Some(true));
  assert_eq!(
    merged.worktree_path_template.as_str(),
    ".wt/{{ branch }}"
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

  assert_eq!(merged.hooks.pre_start.len(), 2);
  assert_eq!(merged.hooks.pre_start[0].source, HookSource::User);
  assert!(merged.hooks.pre_start[0].group.contains_key("user-setup"));
  assert_eq!(merged.hooks.pre_start[1].source, HookSource::Project);
  assert!(merged.hooks.pre_start[1].group.contains_key("project-db"));
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
  assert_eq!(merged.hooks.pre_switch.len(), 2);
  assert_eq!(merged.hooks.pre_switch[0].source, HookSource::User);
  assert_eq!(merged.hooks.pre_switch[1].source, HookSource::Project);

  // post_remove only has user
  assert_eq!(merged.hooks.post_remove.len(), 1);
  assert_eq!(merged.hooks.post_remove[0].source, HookSource::User);
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
  assert_eq!(merged.hooks.pre_start.len(), 1);
  assert_eq!(merged.hooks.pre_start[0].source, HookSource::User);
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
  assert_eq!(merged.hooks.pre_start.len(), 1);
  assert_eq!(merged.hooks.pre_start[0].source, HookSource::Project);
}

#[test]
fn merge_both_empty() {
  let merged = MergedConfig::from_layers(Some(&Config::default()), Some(&Config::default()));

  assert!(merged.list.is_none());
  assert!(merged.background_remove.is_none());
  assert_eq!(
    merged.worktree_path_template.as_str(),
    jjwt::core::types::DEFAULT_WORKTREE_PATH_TEMPLATE
  );
  assert!(merged.aliases.is_empty());
  assert!(merged.hooks.pre_switch.is_empty());
  assert!(merged.hooks.post_switch.is_empty());
  assert!(merged.hooks.pre_start.is_empty());
  assert!(merged.hooks.post_start.is_empty());
  assert!(merged.hooks.pre_remove.is_empty());
  assert!(merged.hooks.post_remove.is_empty());
}

#[test]
fn from_project_convenience_tags_all_as_project() {
  let cfg = Config {
    pre_start: vec![hook("a", "cmd-a"), hook("b", "cmd-b")],
    post_switch: vec![hook("c", "cmd-c")],
    ..Default::default()
  };

  let merged = MergedConfig::from_project(cfg);

  for shg in &merged.hooks.pre_start {
    assert_eq!(shg.source, HookSource::Project);
  }

  for shg in &merged.hooks.post_switch {
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

// ── Three-layer merge (per-project overrides) ──────────────────────────

fn user_with_project_override(user_bg: Option<bool>, override_bg: Option<bool>) -> Config {
  let mut projects = HashMap::new();

  projects.insert(
    "github.com/owner/repo".into(),
    Config {
      background_remove: override_bg,
      ..Default::default()
    },
  );

  Config {
    background_remove: user_bg,
    projects,
    ..Default::default()
  }
}

#[test]
fn three_layer_scalars_project_wins_over_override() {
  let user = user_with_project_override(Some(false), Some(true));
  let project = Config {
    background_remove: Some(false),
    ..Default::default()
  };

  let merged = MergedConfig::from_layers_with_project_id(
    Some(&user),
    Some("github.com/owner/repo"),
    Some(&project),
  );

  // project-local wins over the per-project override
  assert_eq!(merged.background_remove, Some(false));
}

#[test]
fn three_layer_override_wins_over_user_default() {
  let user = user_with_project_override(Some(false), Some(true));

  let merged =
    MergedConfig::from_layers_with_project_id(Some(&user), Some("github.com/owner/repo"), None);

  // per-project override wins over user default
  assert_eq!(merged.background_remove, Some(true));
}

#[test]
fn three_layer_unmatched_id_ignores_override() {
  let user = user_with_project_override(Some(false), Some(true));

  let merged =
    MergedConfig::from_layers_with_project_id(Some(&user), Some("github.com/other/repo"), None);

  // no matching project override → user default
  assert_eq!(merged.background_remove, Some(false));
}

#[test]
fn three_layer_no_project_id_ignores_override() {
  let user = user_with_project_override(Some(false), Some(true));

  let merged = MergedConfig::from_layers_with_project_id(Some(&user), None, None);

  assert_eq!(merged.background_remove, Some(false));
}

#[test]
fn three_layer_hooks_all_layers_contribute() {
  let mut projects = HashMap::new();

  projects.insert(
    "github.com/owner/repo".into(),
    Config {
      pre_start: vec![hook("override-hook", "make db-start")],
      ..Default::default()
    },
  );

  let user = Config {
    pre_start: vec![hook("user-hook", "npm ci")],
    projects,
    ..Default::default()
  };

  let project = Config {
    pre_start: vec![hook("project-hook", "make test")],
    ..Default::default()
  };

  let merged = MergedConfig::from_layers_with_project_id(
    Some(&user),
    Some("github.com/owner/repo"),
    Some(&project),
  );

  // All three layers contribute to hooks: user + override + project
  assert_eq!(merged.hooks.pre_start.len(), 3);
  assert!(merged.hooks.pre_start[0].group.contains_key("user-hook"));
  assert!(
    merged.hooks.pre_start[1]
      .group
      .contains_key("override-hook")
  );
  assert!(merged.hooks.pre_start[2].group.contains_key("project-hook"));
}

#[test]
fn three_layer_aliases_project_overrides_override_overrides_user() {
  let mut user_aliases = IndexMap::new();
  user_aliases.insert("a".into(), "user-a".into());
  user_aliases.insert("b".into(), "user-b".into());

  let mut override_aliases = IndexMap::new();
  override_aliases.insert("b".into(), "override-b".into());
  override_aliases.insert("c".into(), "override-c".into());

  let mut project_aliases = IndexMap::new();
  project_aliases.insert("c".into(), "project-c".into());
  project_aliases.insert("d".into(), "project-d".into());

  let mut projects = HashMap::new();

  projects.insert(
    "github.com/owner/repo".into(),
    Config {
      aliases: override_aliases,
      ..Default::default()
    },
  );

  let user = Config {
    aliases: user_aliases,
    projects,
    ..Default::default()
  };

  let project = Config {
    aliases: project_aliases,
    ..Default::default()
  };

  let merged = MergedConfig::from_layers_with_project_id(
    Some(&user),
    Some("github.com/owner/repo"),
    Some(&project),
  );

  assert_eq!(merged.aliases.get("a").unwrap(), "user-a");
  assert_eq!(merged.aliases.get("b").unwrap(), "override-b");
  assert_eq!(merged.aliases.get("c").unwrap(), "project-c");
  assert_eq!(merged.aliases.get("d").unwrap(), "project-d");
  assert_eq!(merged.aliases.len(), 4);
}

#[test]
fn three_layer_worktree_path_override_as_middle_layer() {
  let mut projects = HashMap::new();

  projects.insert(
    "github.com/owner/repo".into(),
    Config {
      worktree_path_template: Some(".wt/{{ branch }}".into()),
      ..Default::default()
    },
  );

  let user = Config {
    worktree_path_template: Some(".worktrees/{{ branch }}".into()),
    projects,
    ..Default::default()
  };

  // No project config → override wins over user default
  let merged =
    MergedConfig::from_layers_with_project_id(Some(&user), Some("github.com/owner/repo"), None);

  assert_eq!(
    merged.worktree_path_template.as_str(),
    ".wt/{{ branch }}"
  );

  // With project config → project wins
  let project = Config {
    worktree_path_template: Some(".trees/{{ branch }}".into()),
    ..Default::default()
  };

  let merged2 = MergedConfig::from_layers_with_project_id(
    Some(&user),
    Some("github.com/owner/repo"),
    Some(&project),
  );

  assert_eq!(
    merged2.worktree_path_template.as_str(),
    ".trees/{{ branch }}"
  );
}
