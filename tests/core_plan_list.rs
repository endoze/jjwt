use jjwt::core::plan::plan_list;
use jjwt::core::types::*;
use std::path::PathBuf;

fn cfg_with_list() -> Config {
    Config {
        list: Some(ListConfig {
            url: "http://example.com/{{ branch }}".into(),
        }),
        pre_start: vec![],
        pre_remove: vec![],
    }
}

fn obs_with_workspaces() -> ObservedState {
    ObservedState {
        repo_root: PathBuf::from("/repo"),
        is_jj_repo: true,
        workspaces: vec![
            Workspace { name: "main".into(), path: PathBuf::from("/repo/.worktrees/main"), stale: false },
            Workspace { name: "feat".into(), path: PathBuf::from("/repo/.worktrees/feat"), stale: false },
        ],
        target_path_exists: false,
        target_workspace_dirty: false,
        target_bookmark_merged: false,
        target_bookmark_exists: false,
    }
}

#[test]
fn list_renders_url_per_workspace() {
    let plan = plan_list(&cfg_with_list(), &obs_with_workspaces()).expect("plan ok");
    assert_eq!(plan.actions.len(), 1);
    let Action::PrintLine(out) = &plan.actions[0] else { panic!() };
    assert!(out.contains("main"));
    assert!(out.contains("feat"));
    assert!(out.contains("http://example.com/main"));
    assert!(out.contains("http://example.com/feat"));
}

#[test]
fn list_without_list_config_still_prints_names() {
    let cfg = Config { list: None, pre_start: vec![], pre_remove: vec![] };
    let plan = plan_list(&cfg, &obs_with_workspaces()).expect("plan ok");
    let Action::PrintLine(out) = &plan.actions[0] else { panic!() };
    assert!(out.contains("main"));
    assert!(out.contains("feat"));
}
