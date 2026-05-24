#![cfg(not(tarpaulin_include))]

use anyhow::{Result, anyhow};

use crate::core::types::{Action, HookSource, Plan};
use crate::shell::fs::Fs;
use crate::shell::jj::Jj;
use crate::shell::proc::Proc;

/// Holds the backend implementations used during plan execution.
pub struct Runtime<J: Jj, F: Fs, P: Proc> {
  /// Jj backend for repository operations.
  pub jj: J,
  /// Filesystem backend for file operations.
  pub fs: F,
  /// Process backend for shell command execution.
  pub proc: P,
  /// Root path of the jj repository.
  pub repo_root: std::path::PathBuf,
  /// Repo identity for approval lookups (e.g. `github.com/owner/repo`).
  pub repo_id: Option<String>,
  /// Whether the session is interactive (TTY attached). When false,
  /// unapproved project hooks error instead of prompting.
  pub interactive: bool,
}

impl<J: Jj, F: Fs, P: Proc> Runtime<J, F, P> {
  /// Create a new runtime with default repo root and auto-detected interactivity.
  pub fn new(jj: J, fs: F, proc: P) -> Self {
    Self {
      jj,
      fs,
      proc,
      repo_root: std::path::PathBuf::from("."),
      repo_id: None,
      interactive: std::io::IsTerminal::is_terminal(&std::io::stdin()),
    }
  }

  /// Set the repo root path, returning the modified runtime.
  pub fn with_root(mut self, root: std::path::PathBuf) -> Self {
    self.repo_root = root;

    self
  }

  /// Set the repo identity for approval lookups, returning the modified runtime.
  pub fn with_repo_id(mut self, id: Option<String>) -> Self {
    self.repo_id = id;

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
      Action::JjWorkspaceAdd {
        name,
        path,
        revision,
      } => {
        rt.jj
          .workspace_add(&rt.repo_root, name, path, revision.as_deref())?;
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
      Action::JjWorkspaceRename { old_name, new_name } => {
        rt.jj.workspace_rename(&rt.repo_root, old_name, new_name)?;
      }
      Action::RenameDir { from, to } => {
        rt.fs.rename(from, to)?;
      }
      Action::JjBookmarkRename { old_name, new_name } => {
        rt.jj.bookmark_rename(&rt.repo_root, old_name, new_name)?;
      }
      Action::DeleteDirBackground { path } => {
        let ts = std::time::SystemTime::now()
          .duration_since(std::time::UNIX_EPOCH)
          .unwrap_or_default()
          .as_millis();
        let trash_dir = rt.repo_root.join(".jj").join(".jjwt-trash");
        let trash_path = trash_dir.join(ts.to_string());

        rt.fs.create_dir_all(&trash_dir)?;
        rt.fs.rename(path, &trash_path)?;
        rt.proc
          .spawn_detached("rm", &["-rf", &trash_path.display().to_string()])?;
      }
      Action::RunHook {
        name,
        rendered_cmd,
        cwd,
        env,
        source,
      } => {
        if *source == HookSource::Project {
          check_approval(rt, name, rendered_cmd)?;
        }

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
      Action::Exec {
        rendered_cmd,
        cwd,
        env,
      } => {
        let status = rt.proc.run_sh_inherit(rendered_cmd, cwd, env)?;

        if status != 0 {
          return Err(anyhow!("command failed (status {status}): {rendered_cmd}"));
        }
      }
      Action::PrintLine(s) => {
        printed.push(s.clone());
      }
    }
  }

  Ok(printed)
}

/// Check whether a project-sourced hook command is approved. If not,
/// prompt the user (interactive) or error (non-interactive). Approved
/// commands are persisted for future runs.
///
/// Set `JJWT_TRUST_PROJECT_HOOKS=1` to skip approval checks entirely
/// (useful in CI, testing, and scripted environments).
fn check_approval<J: Jj, F: Fs, P: Proc>(
  rt: &Runtime<J, F, P>,
  hook_name: &str,
  rendered_cmd: &str,
) -> Result<()> {
  use crate::shell::approvals;

  if std::env::var("JJWT_TRUST_PROJECT_HOOKS").as_deref() == Ok("1") {
    return Ok(());
  }

  let repo_id = match rt.repo_id.as_deref() {
    Some(id) => id,
    None => return Ok(()),
  };

  if approvals::is_approved(repo_id, rendered_cmd) {
    return Ok(());
  }

  if !rt.interactive {
    anyhow::bail!(
      "project hook '{hook_name}' requires approval but session is not interactive \
       (run interactively to approve, or pre-approve the command)"
    );
  }

  if approvals::prompt_approval(hook_name, rendered_cmd)? {
    approvals::save_approval(repo_id, rendered_cmd)?;

    Ok(())
  } else {
    anyhow::bail!("hook '{hook_name}' denied by user")
  }
}
