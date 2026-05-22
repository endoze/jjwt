use anyhow::Result;
use std::io::IsTerminal;
use std::path::Path;

use crate::core::plan::plan_list;
use crate::core::types::{DisplayHints, ListOptions, OutputFormat};
use crate::shell::config_loader::load_merged_config;
use crate::shell::fs::RealFs;
use crate::shell::jj_lib::JjLib;
use crate::shell::observe::observe_list;
use crate::shell::proc::RealProc;
use crate::shell::runtime::{Runtime, execute};

pub fn run(
  cwd: &Path,
  config_path: Option<&Path>,
  opts: ListOptions,
  format: OutputFormat,
) -> Result<()> {
  let cfg = load_merged_config(cwd, config_path)?;

  let jj = JjLib::new(cwd)?;
  let fs = RealFs;
  let proc = RealProc;

  let mut obs = observe_list(&jj, &fs, cwd, opts)?;

  if opts.full {
    let bookmarks: Vec<String> = obs.rows.iter().map(|r| r.workspace.name.clone()).collect();
    let ci_map = crate::shell::ci::query_ci_statuses(&obs.repo_root, &bookmarks);

    for row in &mut obs.rows {
      if let Some(&status) = ci_map.get(&row.workspace.name) {
        row.ci_status = status;
      }
    }

    let summary_enabled = cfg.list.as_ref().and_then(|l| l.summary).unwrap_or(false);
    let llm_command = cfg
      .commit
      .as_ref()
      .and_then(|c| c.generation.as_ref())
      .and_then(|g| g.command.as_deref());

    if summary_enabled && let Some(command) = llm_command {
      let mut cache = crate::shell::llm_cache::load(&obs.repo_root);
      let mut cache_dirty = false;

      for row in &mut obs.rows {
        let commit_id = &row.details.commit_short;

        if commit_id.is_empty() {
          continue;
        }

        if let Some(cached) = crate::shell::llm_cache::get(&cache, commit_id) {
          row.summary = cached.to_string();
          continue;
        }

        let message = &row.details.message_first_line;
        let diff_stat =
          crate::shell::llm::get_jj_diff_stat(&row.workspace.path).unwrap_or_default();

        if let Some(summary) = crate::shell::llm::generate_summary(command, message, &diff_stat) {
          row.summary = summary.clone();
          crate::shell::llm_cache::put(&mut cache, commit_id.clone(), summary);
          cache_dirty = true;
        }
      }

      if cache_dirty {
        let _ = crate::shell::llm_cache::save(&obs.repo_root, &mut cache);
      }
    }
  }

  // JSON output is machine-readable; never style it. Text output styles
  // only when stdout is a real terminal and the user hasn't opted out.
  let display = DisplayHints {
    styled: matches!(format, OutputFormat::Text) && use_color(),
    term_width: if matches!(format, OutputFormat::Text) {
      terminal_size::terminal_size().map(|(w, _)| w.0)
    } else {
      None
    },
  };
  let plan = plan_list(&cfg, &obs, &display, format).map_err(|e| anyhow::anyhow!("{e}"))?;
  let mut rt = Runtime::new(jj, fs, proc).with_root(obs.repo_root.clone());
  let printed = execute(&plan, &mut rt)?;

  match format {
    OutputFormat::Text => {
      for line in printed {
        print!("{line}");
      }
    }
    OutputFormat::Json | OutputFormat::Statusline => {
      for line in printed {
        println!("{line}");
      }
    }
  }

  Ok(())
}

/// Enable ANSI styling iff stdout is a terminal and `NO_COLOR` is not set.
fn use_color() -> bool {
  if std::env::var_os("NO_COLOR").is_some() {
    return false;
  }

  std::io::stdout().is_terminal()
}
