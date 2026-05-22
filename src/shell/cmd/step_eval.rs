use anyhow::Result;
use std::path::Path;

use crate::core::template::render;
use crate::core::types::RenderContext;
use crate::shell::fs::RealFs;
use crate::shell::jj_lib::JjLib;
use crate::shell::observe::observe;

/// Render a template expression with the current observation context and
/// print the result to stdout (no trailing newline beyond what the
/// template produces). Mirrors `wt step eval`.
pub fn run(cwd: &Path, template: &str) -> Result<()> {
  let jj = JjLib::new(cwd)?;
  let fs = RealFs;
  let obs = observe(&jj, &fs, cwd, None)?;
  let (branch, ws_path) = match obs.current_workspace.as_deref() {
    Some(name) => {
      let ws = obs
        .workspaces
        .iter()
        .find(|w| w.name == name)
        .map(|w| (w.name.clone(), w.path.clone()))
        .unwrap_or_else(|| (String::new(), obs.repo_root.clone()));

      ws
    }
    None => (String::new(), obs.repo_root.clone()),
  };
  let ctx = RenderContext {
    branch,
    worktree_path: Some(ws_path.clone()),
    worktree_name: ws_path
      .file_name()
      .map(|n| n.to_string_lossy().into_owned()),
    repo: obs
      .repo_root
      .file_name()
      .map(|n| n.to_string_lossy().into_owned()),
    repo_path: Some(obs.repo_root.clone()),
    cwd: Some(ws_path),
    ..Default::default()
  };
  let out = render(template, &ctx).map_err(|e| anyhow::anyhow!("{e}"))?;

  println!("{out}");

  Ok(())
}
