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

  let obs = observe_list(&jj, &fs, cwd, opts)?;

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
    OutputFormat::Json => {
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
