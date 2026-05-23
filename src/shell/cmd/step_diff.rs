#![cfg(not(tarpaulin_include))]

use anyhow::{Result, bail};
use std::path::Path;

use crate::shell::fs::RealFs;
use crate::shell::jj_lib::JjLib;
use crate::shell::observe::observe;
use crate::shell::proc::{Proc, RealProc};

/// Run `jj diff -r trunk()..@` in the current workspace, forwarding any
/// additional arguments to `jj diff`. Inherits stdio so output is
/// displayed directly.
pub fn run(cwd: &Path, extra_args: Vec<String>) -> Result<i32> {
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

  // Build the jj diff command: `jj diff -r 'trunk()..@' <extra_args>`
  let mut cmd_parts = vec![
    "jj".to_string(),
    "diff".to_string(),
    "-r".to_string(),
    "trunk()..@".to_string(),
  ];

  cmd_parts.extend(extra_args.iter().map(|a| shell_escape(a)));

  let cmd_str = cmd_parts.join(" ");
  let proc = RealProc;
  let status = proc.run_sh_inherit(&cmd_str, &ws.path, &[])?;

  Ok(status)
}

/// Wrap a string in POSIX single quotes when it contains special characters.
fn shell_escape(s: &str) -> String {
  if s.contains(|c: char| c.is_whitespace() || "\"'\\$`!#&|;(){}[]<>?*~".contains(c)) {
    format!("'{}'", s.replace('\'', "'\\''"))
  } else {
    s.to_string()
  }
}
