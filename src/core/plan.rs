use crate::core::template::render;
use crate::core::types::*;
use std::path::PathBuf;

fn workspace_path(root: &std::path::Path, name: &str) -> PathBuf {
    root.join(".worktrees").join(name)
}

fn hook_env(name: &str, path: &std::path::Path) -> Vec<(String, String)> {
    vec![
        ("JJWT_WORKSPACE".into(), name.into()),
        ("JJWT_WORKSPACE_PATH".into(), path.display().to_string()),
    ]
}

fn render_pre_start_hooks(
    cfg: &Config,
    branch: &str,
    cwd: &std::path::Path,
) -> Result<Vec<Action>, CoreError> {
    let ctx = RenderContext { branch: branch.into() };
    let mut out = Vec::new();

    for group in &cfg.pre_start {
        for (name, tmpl) in group {
            let rendered = render(tmpl, &ctx)?;

            out.push(Action::RunHook {
                name: name.clone(),
                rendered_cmd: rendered,
                cwd: cwd.to_path_buf(),
                env: hook_env(branch, cwd),
            });
        }
    }

    Ok(out)
}

pub fn plan_switch(
    cfg: &Config,
    args: &SwitchArgs,
    obs: &ObservedState,
) -> Result<Plan, CoreError> {
    if !obs.is_jj_repo {
        return Err(CoreError::NotJjRepo);
    }

    let ws_path = workspace_path(&obs.repo_root, &args.name);
    let exists = obs.workspaces.iter().any(|w| w.name == args.name);
    let mut plan = Plan::new();

    if args.create {
        if exists {
            return Err(CoreError::WorkspaceExists(args.name.clone()));
        }

        plan.push(Action::JjWorkspaceAdd {
            name: args.name.clone(),
            path: ws_path.clone(),
        });
        plan.push(Action::JjBookmarkCreate {
            name: args.name.clone(),
            workspace: args.name.clone(),
        });

        for a in render_pre_start_hooks(cfg, &args.name, &ws_path)? {
            plan.push(a);
        }
    } else {
        let ws = obs
            .workspaces
            .iter()
            .find(|w| w.name == args.name)
            .ok_or_else(|| CoreError::WorkspaceMissing(args.name.clone()))?;

        if ws.stale {
            plan.push(Action::JjWorkspaceUpdateStale { name: args.name.clone() });
        }

        if args.rerun_hooks {
            for a in render_pre_start_hooks(cfg, &args.name, &ws_path)? {
                plan.push(a);
            }
        }
    }

    plan.push(Action::PrintLine(ws_path.display().to_string()));

    Ok(plan)
}
