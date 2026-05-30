#![cfg(not(tarpaulin_include))]

use anyhow::{Context, Result, bail};
use std::path::Path;
use std::time::Duration;

use crate::shell::config_loader::load_merged_config;
use crate::shell::fs::RealFs;
use crate::shell::jj_lib::JjLib;
use crate::shell::observe::observe;

/// Tie the lifetime of `argv` to the current workspace. Spawn the command
/// in its own process group; poll the workspace path; once it disappears,
/// send SIGTERM, wait briefly, then SIGKILL the whole group. Unix only.
///
/// When the child exits on its own before the workspace disappears, return
/// its exit code without sending any signal.
pub fn run(cwd: &Path, config_path: Option<&Path>, argv: Vec<String>) -> Result<i32> {
  if argv.is_empty() {
    bail!("step tether: missing command (after `--`)");
  }

  let cfg = load_merged_config(cwd, config_path)?;

  let jj = JjLib::with_template(cwd, &cfg.worktree_path_template)?;
  let fs = RealFs;
  let obs = observe(&jj, &fs, cwd, None, &cfg.worktree_path_template)?;

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
  let exit = run_tethered(&argv, &ws.path)?;

  Ok(exit)
}

/// A child process spawned as its own process group leader. The only way
/// to obtain one is through [`ProcessGroup::spawn`], which enforces the
/// `.process_group(0)` invariant at construction time. Signaling methods
/// are safe because the pgid is guaranteed valid by construction.
#[cfg(unix)]
struct ProcessGroup {
  /// The spawned child process handle.
  child: std::process::Child,
  /// Process group id (equal to the child's pid since the child is its own group leader).
  pgid: libc::pid_t,
}

#[cfg(unix)]
impl ProcessGroup {
  /// Spawn a command as a new process group leader. The child becomes
  /// its own group leader (`.process_group(0)`), so its pgid equals its
  /// pid — the value stored in `self.pgid`.
  fn spawn(argv: &[String], cwd: &Path) -> Result<Self> {
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    let mut cmd = Command::new(&argv[0]);

    cmd.args(&argv[1..]).current_dir(cwd).process_group(0);

    let child = cmd
      .spawn()
      .with_context(|| format!("failed to spawn {:?}", argv[0]))?;
    let pgid = child.id() as libc::pid_t;

    Ok(Self { child, pgid })
  }

  /// Send a signal to the entire process group. Signaling an
  /// already-exited group is harmless (kernel returns ESRCH).
  fn signal(&self, signal: libc::c_int) {
    // SAFETY: `self.pgid` was obtained from `child.id()` immediately
    // after spawning with `.process_group(0)`, so it is always a valid
    // process group leader. `kill(2)` with a negative pid targets the
    // group and has no memory-safety implications.
    unsafe {
      libc::kill(-self.pgid, signal);
    }
  }

  /// Check whether the child has exited without blocking.
  fn try_wait(&mut self) -> Result<Option<std::process::ExitStatus>> {
    self.child.try_wait().context("wait on child")
  }

  /// Block until the child exits.
  fn wait(&mut self) -> Result<std::process::ExitStatus> {
    self.child.wait().context("wait on child")
  }
}

/// Unix implementation: spawn in a process group, poll workspace, signal on removal.
#[cfg(unix)]
fn run_tethered(argv: &[String], ws_path: &Path) -> Result<i32> {
  let mut pg = ProcessGroup::spawn(argv, ws_path)?;

  loop {
    if let Some(status) = pg.try_wait()? {
      return Ok(status.code().unwrap_or(-1));
    }

    if !ws_path.exists() {
      // Workspace gone — ask the group to terminate, then escalate.
      pg.signal(libc::SIGTERM);

      let deadline = std::time::Instant::now() + Duration::from_secs(5);

      while std::time::Instant::now() < deadline {
        if let Some(status) = pg.try_wait()? {
          return Ok(status.code().unwrap_or(-1));
        }

        std::thread::sleep(Duration::from_millis(200));
      }

      pg.signal(libc::SIGKILL);

      let status = pg.wait()?;

      return Ok(status.code().unwrap_or(-1));
    }

    std::thread::sleep(Duration::from_secs(1));
  }
}

/// Non-Unix stub: tethering is not supported on this platform.
#[cfg(not(unix))]
fn run_tethered(_argv: &[String], _ws_path: &Path) -> Result<i32> {
  bail!("step tether is supported on Unix only");
}
