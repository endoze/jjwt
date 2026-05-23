#![cfg(not(tarpaulin_include))]

use anyhow::Result;
use std::path::Path;
use std::process::Command;

/// Run environment diagnostic checks and print results.
pub fn run(cwd: &Path) -> Result<()> {
  // 1. Check jj on PATH.
  match which::which("jj") {
    Ok(path) => {
      let version = Command::new("jj")
        .arg("version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());

      println!("[ok]   jj found: {} ({version})", path.display());
    }
    Err(_) => {
      println!("[err]  jj not found on PATH (required)");
    }
  }

  // 2. Check gh (optional).
  match which::which("gh") {
    Ok(path) => println!("[ok]   gh found: {}", path.display()),
    Err(_) => println!("[warn] gh not found (PR shortcuts and CI status unavailable)"),
  }

  // 3. Check glab (optional).
  match which::which("glab") {
    Ok(path) => println!("[ok]   glab found: {}", path.display()),
    Err(_) => println!("[warn] glab not found (MR shortcuts and GitLab CI unavailable)"),
  }

  // 4. Check if inside a jj repo.
  let in_jj_repo = find_jj_root(cwd);

  match &in_jj_repo {
    Some(root) => println!("[ok]   Inside jj repo: {}", root.display()),
    None => println!("[warn] Not inside a jj repo"),
  }

  // 5. Check user config.
  let user_config_dir = crate::shell::config_loader::user_config_dir();

  match user_config_dir {
    Ok(dir) => {
      let user_cfg = dir.join("config.toml");

      if user_cfg.exists() {
        match std::fs::read_to_string(&user_cfg)
          .map_err(|e| format!("Cannot read user config: {e}"))
          .and_then(|c| {
            toml::from_str::<crate::core::types::Config>(&c)
              .map_err(|e| format!("User config parse error: {e}"))
          }) {
          Ok(_) => println!("[ok]   User config: {}", user_cfg.display()),
          Err(msg) => println!("[err]  {msg}"),
        }
      } else {
        println!("[info] No user config at {}", user_cfg.display());
      }
    }
    Err(_) => println!("[warn] Cannot determine user config directory"),
  }

  // 6. Check project config.
  if let Some(ref root) = in_jj_repo {
    let project_cfg = root.join(".config").join("wt.toml");

    if project_cfg.exists() {
      match std::fs::read_to_string(&project_cfg)
        .map_err(|e| format!("Cannot read project config: {e}"))
        .and_then(|c| {
          toml::from_str::<crate::core::types::Config>(&c)
            .map_err(|e| format!("Project config parse error: {e}"))
        }) {
        Ok(_) => println!("[ok]   Project config: {}", project_cfg.display()),
        Err(msg) => println!("[err]  {msg}"),
      }
    } else {
      println!("[info] No project config at {}", project_cfg.display());
    }
  }

  // 7. Check shell integration.
  match which::which("wt") {
    Ok(_) => println!("[ok]   Shell integration: wt command available"),
    Err(_) => println!(
      "[info] Shell integration: wt not found (run: eval \"$(jjwt config shell init <shell>)\")"
    ),
  }

  Ok(())
}

/// Walk up from `start` looking for a `.jj` directory.
fn find_jj_root(start: &Path) -> Option<std::path::PathBuf> {
  crate::shell::jj::find_nearest_jj_dir(start)
}
