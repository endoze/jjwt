use anyhow::{Result, anyhow};
use std::path::Path;

pub struct ProcOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

pub trait Proc {
    /// Run `sh -c <cmd>` with cwd and env. Returns combined output.
    fn run_sh(&self, cmd: &str, cwd: &Path, env: &[(String, String)]) -> Result<ProcOutput>;
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
}
