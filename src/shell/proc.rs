use anyhow::{Result, anyhow};
use std::path::Path;

pub struct ProcOutput {
  pub status: i32,
  pub stdout: String,
  pub stderr: String,
}

pub trait Proc {
  /// Run `sh -c <cmd>` with cwd and env. Returns captured stdio.
  fn run_sh(&self, cmd: &str, cwd: &Path, env: &[(String, String)]) -> Result<ProcOutput>;

  /// Run `sh -c <cmd>` with stdio inherited from the parent process.
  /// Returns the exit status code. Used for interactive commands
  /// (aliases, `step eval`, etc.) where users expect to see output live.
  fn run_sh_inherit(&self, cmd: &str, cwd: &Path, env: &[(String, String)]) -> Result<i32>;

  /// Spawn a command without waiting for completion. Used for background
  /// cleanup (detached `rm -rf`).
  fn spawn_detached(&self, program: &str, args: &[&str]) -> Result<()>;
}

pub struct RealProc;

impl Proc for RealProc {
  fn run_sh(&self, cmd: &str, cwd: &Path, env: &[(String, String)]) -> Result<ProcOutput> {
    let mut c = std::process::Command::new("sh");

    c.arg("-c").arg(cmd).current_dir(cwd);

    for (k, v) in env {
      c.env(k, v);
    }

    let out = c.output().map_err(|e| anyhow!("failed to spawn sh: {e}"))?;

    Ok(ProcOutput {
      status: out.status.code().unwrap_or(-1),
      stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
      stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
  }

  fn run_sh_inherit(&self, cmd: &str, cwd: &Path, env: &[(String, String)]) -> Result<i32> {
    let mut c = std::process::Command::new("sh");

    c.arg("-c").arg(cmd).current_dir(cwd);

    for (k, v) in env {
      c.env(k, v);
    }

    let status = c.status().map_err(|e| anyhow!("failed to spawn sh: {e}"))?;

    Ok(status.code().unwrap_or(-1))
  }

  fn spawn_detached(&self, program: &str, args: &[&str]) -> Result<()> {
    use std::process::{Command, Stdio};

    Command::new(program)
      .args(args)
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .map_err(|e| anyhow!("failed to spawn {program}: {e}"))?;

    Ok(())
  }
}
