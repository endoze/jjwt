#![cfg(not(tarpaulin_include))]

use anyhow::{Result, bail};
use std::path::Path;

use crate::core::template::render;
use crate::core::types::RenderContext;
use crate::shell::fs::RealFs;
use crate::shell::jj_lib::JjLib;
use crate::shell::observe::observe;
use crate::shell::proc::{Proc, RealProc};

/// Run `argv` in every observed workspace, continuing on failure.
///
/// Each argv token is rendered as a minijinja template with that
/// workspace's context (`branch`, `worktree_path`, etc.), then the tokens
/// are POSIX-quoted and concatenated into a single command line passed
/// to `sh -c`. Use `sh -c '<pipeline>'` in the argv when shell features
/// (pipes, redirects, variables) are needed.
pub fn run(cwd: &Path, argv: Vec<String>) -> Result<()> {
  if argv.is_empty() {
    bail!("step for-each: missing command (after `--`)");
  }

  let jj = JjLib::new(cwd)?;
  let fs = RealFs;
  let proc = RealProc;
  let obs = observe(&jj, &fs, cwd, None, None)?;

  if !obs.is_jj_repo {
    bail!("not inside a jj repo");
  }

  let mut ok = 0usize;
  let mut failed = 0usize;

  for ws in &obs.workspaces {
    let ctx = RenderContext {
      branch: ws.name.clone(),
      worktree_path: Some(ws.path.clone()),
      worktree_name: ws
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned()),
      repo: obs
        .repo_root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned()),
      repo_path: Some(obs.repo_root.clone()),
      cwd: Some(ws.path.clone()),
      ..Default::default()
    };
    let mut rendered_tokens: Vec<String> = Vec::with_capacity(argv.len());

    for tok in &argv {
      let r = render(tok, &ctx).map_err(|e| anyhow::anyhow!("render '{tok}': {e}"))?;

      rendered_tokens.push(r);
    }

    let cmd_line = rendered_tokens
      .iter()
      .map(|t| posix_quote(t))
      .collect::<Vec<_>>()
      .join(" ");

    println!("==> [{}] {}", ws.name, cmd_line);

    let status = proc.run_sh_inherit(&cmd_line, &ws.path, &[])?;

    if status == 0 {
      ok += 1;
    } else {
      failed += 1;
      eprintln!("    (status {status})");
    }
  }

  println!("--- summary: {ok} ok, {failed} failed");

  if failed > 0 {
    bail!("{failed} workspace(s) returned non-zero exit");
  }

  Ok(())
}

/// Minimal POSIX shell quoting: wrap in single quotes, escaping embedded
/// single quotes. Conservative — every token gets quoted even when it
/// wouldn't strictly need to be. That matches what users expect when
/// reading the echoed command line.
fn posix_quote(s: &str) -> String {
  if s.is_empty() {
    return "''".into();
  }

  let mut out = String::with_capacity(s.len() + 2);

  out.push('\'');

  for c in s.chars() {
    if c == '\'' {
      out.push_str("'\\''");
    } else {
      out.push(c);
    }
  }

  out.push('\'');

  out
}

#[cfg(test)]
mod tests {
  use super::posix_quote;

  #[test]
  fn quotes_simple_tokens() {
    assert_eq!(posix_quote("hello"), "'hello'");
  }

  #[test]
  fn escapes_embedded_single_quotes() {
    assert_eq!(posix_quote("a'b"), "'a'\\''b'");
  }

  #[test]
  fn empty_becomes_pair_of_quotes() {
    assert_eq!(posix_quote(""), "''");
  }
}
