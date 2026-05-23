#![cfg(not(tarpaulin_include))]

use anyhow::Result;
use std::path::Path;

use crate::shell::config_loader::load_merged_config;

/// Run config validation and print results.
pub fn run(cwd: &Path, config_path: Option<&Path>) -> Result<()> {
  let mut errors = 0u32;
  let mut warnings = 0u32;

  // 1. Try loading the merged config.
  let cfg = match load_merged_config(cwd, config_path) {
    Ok(c) => {
      println!("[ok]   Config loaded successfully");
      c
    }
    Err(e) => {
      println!("[err]  Config load failed: {e}");
      // Can't proceed with further checks.
      std::process::exit(1);
    }
  };

  // 2. Validate template syntax for all hook commands.
  let env = minijinja::Environment::new();

  let mut check = |tmpl: &str, label: &str| {
    if let Err(e) = env.template_from_str(tmpl) {
      println!("[err]  {label}: {e}");
      errors += 1;
    }
  };

  for (hook_type, groups) in cfg.all_hook_groups() {
    for shg in groups {
      for (name, tmpl) in &shg.group {
        check(
          tmpl,
          &format!(
            "Hook '{name}' ({hook_type}, {}): invalid template",
            shg.source
          ),
        );
      }
    }
  }

  // 3. Validate alias templates.
  for (name, tmpl) in &cfg.aliases {
    check(tmpl, &format!("Alias '{name}': invalid template"));
  }

  // 4. Validate worktree-path template.
  if let Some(ref tmpl) = cfg.worktree_path_template {
    check(tmpl, "worktree-path template: invalid");
  }

  // 5. Validate list URL template.
  if let Some(ref list) = cfg.list {
    check(&list.url, "[list].url template: invalid");
  }

  // 6. Check LLM command binary exists.
  if let Some(ref commit) = cfg.commit
    && let Some(ref generation) = commit.generation
  {
    if let Some(ref command) = generation.command {
      // Extract binary name (first word of the command).
      let binary = command.split_whitespace().next().unwrap_or(command);

      if which::which(binary).is_err() {
        println!("[warn] [commit.generation].command: '{binary}' not found on PATH");
        warnings += 1;
      }
    }

    // Validate generation template if present.
    if let Some(ref tmpl) = generation.template {
      check(tmpl, "[commit.generation].template: invalid");
    }

    if let Some(ref tmpl) = generation.template_append {
      check(tmpl, "[commit.generation].template-append: invalid");
    }
  }

  // 7. Summary.
  if errors == 0 && warnings == 0 {
    println!("[ok]   All checks passed");
  } else {
    println!("\n{} error(s), {} warning(s)", errors, warnings);
  }

  if errors > 0 {
    std::process::exit(1);
  }

  Ok(())
}
