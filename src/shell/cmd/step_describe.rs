use anyhow::{Result, bail};
use std::path::Path;

use crate::shell::config_loader::load_merged_config;
use crate::shell::fs::RealFs;
use crate::shell::jj_lib::JjLib;
use crate::shell::llm::{self, LlmPromptVars};
use crate::shell::observe::observe;
use crate::shell::proc::{Proc, RealProc};

/// Generate a commit message with an LLM and apply it via `jj describe`.
pub fn run(cwd: &Path, config_path: Option<&Path>, dry_run: bool) -> Result<()> {
  let cfg = load_merged_config(cwd, config_path)?;

  let gen_cfg = cfg
    .commit
    .as_ref()
    .and_then(|c| c.generation.as_ref())
    .ok_or_else(|| {
      anyhow::anyhow!(
        "no LLM command configured\n\n\
         Add to your config:\n\n\
         [commit.generation]\n\
         command = \"claude -p --no-session-persistence\""
      )
    })?;

  let command = gen_cfg.command.as_deref().ok_or_else(|| {
    anyhow::anyhow!(
      "commit.generation.command is not set\n\n\
       Example: command = \"claude -p --no-session-persistence\""
    )
  })?;

  let jj = JjLib::new(cwd)?;
  let fs = RealFs;
  let obs = observe(&jj, &fs, cwd, None, None)?;

  if !obs.is_jj_repo {
    bail!("not inside a jj repo");
  }

  let ws_name = obs
    .current_workspace
    .as_deref()
    .ok_or_else(|| anyhow::anyhow!("not inside a known workspace (cwd: {})", cwd.display()))?;
  let ws = obs
    .workspaces
    .iter()
    .find(|w| w.name == ws_name)
    .ok_or_else(|| anyhow::anyhow!("workspace '{ws_name}' missing from observation"))?;

  let diff = llm::get_jj_diff(&ws.path).unwrap_or_default();

  if diff.trim().is_empty() {
    eprintln!("No changes to describe.");

    return Ok(());
  }

  let diff_stat = llm::get_jj_diff_stat(&ws.path).unwrap_or_default();
  let recent_commits = llm::get_recent_commits(&ws.path).unwrap_or_default();

  let repo_name = obs
    .repo_root
    .file_name()
    .map(|n| n.to_string_lossy().to_string())
    .unwrap_or_default();

  let vars = LlmPromptVars {
    jj_diff: diff,
    jj_diff_stat: diff_stat,
    branch: ws_name.to_string(),
    repo: repo_name,
    recent_commits,
  };

  let prompt = llm::render_prompt(gen_cfg, &vars)?;

  if dry_run {
    eprintln!("=== PROMPT ===");
    eprintln!("{prompt}");
    eprintln!();
    eprintln!("=== COMMAND ===");
    eprintln!("{command}");
    eprintln!();

    let message = llm::run_llm_command(command, &prompt);

    eprintln!("=== MESSAGE ===");

    match message {
      Some(msg) => eprintln!("{msg}"),
      None => eprintln!("(no output from LLM command)"),
    }

    return Ok(());
  }

  let message = llm::run_llm_command(command, &prompt)
    .ok_or_else(|| anyhow::anyhow!("LLM command produced no output"))?;

  let escaped = shell_escape(&message);
  let describe_cmd = format!("jj describe -m {escaped}");
  let proc = RealProc;
  let status = proc.run_sh_inherit(&describe_cmd, &ws.path, &[])?;

  if status != 0 {
    bail!("`jj describe` exited with status {status}");
  }

  eprintln!("{message}");

  Ok(())
}

/// Simple POSIX shell escaping: wrap in single quotes, escaping any
/// embedded single quotes.
fn shell_escape(s: &str) -> String {
  format!("'{}'", s.replace('\'', "'\\''"))
}
