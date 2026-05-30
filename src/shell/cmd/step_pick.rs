#![cfg(not(tarpaulin_include))]

use anyhow::Result;
use std::path::Path;

use crate::shell::config_loader::load_merged_config;
use crate::shell::fs::RealFs;
use crate::shell::jj_lib::JjLib;
use crate::shell::observe::observe;

/// Interactive workspace picker using skim. Prints the selected workspace
/// path to stdout so the shell wrapper can `cd` into it.
pub fn run(cwd: &Path, config_path: Option<&Path>) -> Result<()> {
  use std::io::IsTerminal;

  if !std::io::stdin().is_terminal() {
    anyhow::bail!("step pick requires an interactive terminal");
  }

  let cfg = load_merged_config(cwd, config_path)?;

  let jj = JjLib::with_template(cwd, &cfg.worktree_path_template)?;
  let fs = RealFs;
  let obs = observe(&jj, &fs, cwd, None, &cfg.worktree_path_template)?;

  if obs.workspaces.is_empty() {
    anyhow::bail!("no workspaces found");
  }

  let items: Vec<String> = obs.workspaces.iter().map(|w| w.name.clone()).collect();

  let selected = pick_interactive(&items)?;

  let ws = obs
    .workspaces
    .iter()
    .find(|w| w.name == selected)
    .ok_or_else(|| anyhow::anyhow!("selected workspace '{selected}' not found"))?;

  println!("{}", ws.path.display());

  Ok(())
}

/// Launch the skim fuzzy picker and return the selected workspace name.
#[cfg(feature = "picker")]
fn pick_interactive(items: &[String]) -> Result<String> {
  use skim::prelude::*;
  use std::io::Cursor;

  let input = items.join("\n");

  let options = SkimOptionsBuilder::default()
    .height("40%".to_string())
    .prompt("workspace> ".to_string())
    .preview(Some(
      "jj log --no-graph -r {}@ -T 'description' 2>/dev/null || echo '(no log)'".to_string(),
    ))
    .build()
    .map_err(|e| anyhow::anyhow!("skim options: {e}"))?;

  let item_reader = SkimItemReader::default();
  let items = item_reader.of_bufread(Cursor::new(input));

  let result =
    Skim::run_with(&options, Some(items)).ok_or_else(|| anyhow::anyhow!("picker was cancelled"))?;

  if result.is_abort {
    anyhow::bail!("picker was cancelled");
  }

  let selected = result
    .selected_items
    .first()
    .ok_or_else(|| anyhow::anyhow!("no selection made"))?;

  Ok(selected.output().to_string())
}

/// Stub when the `picker` feature is not enabled.
#[cfg(not(feature = "picker"))]
fn pick_interactive(_items: &[String]) -> Result<String> {
  anyhow::bail!("interactive picker not available (compile with --features picker)")
}
