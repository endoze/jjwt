use anyhow::{Result, anyhow};

use crate::core::types::{Action, Plan};
use crate::shell::fs::Fs;
use crate::shell::jj::Jj;
use crate::shell::proc::Proc;

pub struct Runtime<J: Jj, F: Fs, P: Proc> {
    pub jj: J,
    pub fs: F,
    pub proc: P,
    pub repo_root: std::path::PathBuf,
}

impl<J: Jj, F: Fs, P: Proc> Runtime<J, F, P> {
    pub fn new(jj: J, fs: F, proc: P) -> Self {
        Self { jj, fs, proc, repo_root: std::path::PathBuf::from(".") }
    }

    pub fn with_root(mut self, root: std::path::PathBuf) -> Self {
        self.repo_root = root;

        self
    }
}

/// Execute the plan in order; collect any PrintLine payloads and return them.
pub fn execute<J: Jj, F: Fs, P: Proc>(
    plan: &Plan,
    rt: &mut Runtime<J, F, P>,
) -> Result<Vec<String>> {
    let mut printed = Vec::new();

    for action in &plan.actions {
        match action {
            Action::JjWorkspaceAdd { name, path } => {
                rt.jj.workspace_add(&rt.repo_root, name, path)?;
            }
            Action::JjBookmarkCreate { name, workspace } => {
                rt.jj.bookmark_create(&rt.repo_root, name, workspace)?;
            }
            Action::JjWorkspaceForget { name } => {
                rt.jj.workspace_forget(&rt.repo_root, name)?;
            }
            Action::JjBookmarkDelete { name } => {
                rt.jj.bookmark_delete(&rt.repo_root, name)?;
            }
            Action::JjWorkspaceUpdateStale { name } => {
                rt.jj.workspace_update_stale(&rt.repo_root, name)?;
            }
            Action::DeleteDir { path } => {
                rt.fs.remove_dir_all(path)?;
            }
            Action::RunHook { name, rendered_cmd, cwd, env } => {
                let out = rt.proc.run_sh(rendered_cmd, cwd, env)?;

                if out.status != 0 {
                    return Err(anyhow!(
                        "hook '{name}' failed (status {}): {}\nstderr: {}",
                        out.status,
                        rendered_cmd,
                        out.stderr
                    ));
                }
            }
            Action::PrintLine(s) => {
                printed.push(s.clone());
            }
        }
    }

    Ok(printed)
}
