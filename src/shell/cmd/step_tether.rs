use anyhow::{Context, Result, bail};
use std::path::Path;
use std::time::Duration;

use crate::shell::fs::RealFs;
use crate::shell::jj_lib::JjLib;
use crate::shell::observe::observe;

/// Tie the lifetime of `argv` to the current workspace. Spawn the command
/// in its own process group; poll the workspace path; once it disappears,
/// send SIGTERM, wait briefly, then SIGKILL the whole group. Unix only.
///
/// When the child exits on its own before the workspace disappears, return
/// its exit code without sending any signal.
pub fn run(cwd: &Path, argv: Vec<String>) -> Result<i32> {
  if argv.is_empty() {
    bail!("step tether: missing command (after `--`)");
  }

  let jj = JjLib::new(cwd)?;
  let fs = RealFs;
  let obs = observe(&jj, &fs, cwd, None)?;

  if !obs.is_jj_repo {
    bail!("not inside a jj repo");
  }

  let ws_name = obs
    .current_workspace
    .clone()
    .ok_or_else(|| anyhow::anyhow!("not inside a known workspace (cwd: {})", cwd.display()))?;
  let ws = obs
    .workspaces
    .iter()
    .find(|w| w.name == ws_name)
    .ok_or_else(|| anyhow::anyhow!("workspace '{ws_name}' missing from observation"))?;
  let ws_path = ws.path.clone();

  let exit = run_tethered(&argv, &ws_path)?;

  Ok(exit)
}

#[cfg(unix)]
fn run_tethered(argv: &[String], ws_path: &Path) -> Result<i32> {
  use std::os::unix::process::CommandExt;
  use std::process::Command;

  let mut cmd = Command::new(&argv[0]);

  cmd.args(&argv[1..]).current_dir(ws_path).process_group(0);

  let mut child = cmd
    .spawn()
    .with_context(|| format!("failed to spawn {:?}", argv[0]))?;
  let pid = child.id() as libc::pid_t;

  loop {
    // Has the child finished on its own?
    if let Some(status) = child.try_wait().context("wait on child")? {
      return Ok(status.code().unwrap_or(-1));
    }

    // Is the workspace still on disk?
    if !ws_path.exists() {
      // Workspace gone. Politely ask the whole group to leave, give them
      // a moment, then fire SIGKILL at anyone still around.
      unsafe {
        libc::kill(-pid, libc::SIGTERM);
      }

      let deadline = std::time::Instant::now() + Duration::from_secs(5);

      while std::time::Instant::now() < deadline {
        if let Some(status) = child.try_wait().context("wait on child")? {
          return Ok(status.code().unwrap_or(-1));
        }

        std::thread::sleep(Duration::from_millis(200));
      }

      unsafe {
        libc::kill(-pid, libc::SIGKILL);
      }

      let status = child.wait().context("wait on killed child")?;

      return Ok(status.code().unwrap_or(-1));
    }

    std::thread::sleep(Duration::from_secs(1));
  }
}

#[cfg(not(unix))]
fn run_tethered(_argv: &[String], _ws_path: &Path) -> Result<i32> {
  bail!("step tether is supported on Unix only");
}
