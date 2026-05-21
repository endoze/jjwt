use anyhow::Result;
use std::path::Path;

use crate::core::types::Workspace;

pub trait Jj {
    /// Detect the repo root (parent of `.jj/`); errors if not in a jj repo.
    fn repo_root(&self, start: &Path) -> Result<std::path::PathBuf>;
    /// Enumerate workspaces with name, path, and stale flag.
    fn workspace_list(&self, repo_root: &Path) -> Result<Vec<Workspace>>;
    /// `jj workspace add --name <name> <path>`
    fn workspace_add(&self, repo_root: &Path, name: &str, path: &Path) -> Result<()>;
    /// `jj workspace forget <name>`
    fn workspace_forget(&self, repo_root: &Path, name: &str) -> Result<()>;
    /// `jj workspace update-stale` for the named workspace
    fn workspace_update_stale(&self, repo_root: &Path, name: &str) -> Result<()>;
    /// Create a bookmark at the named workspace's `@`.
    fn bookmark_create(&self, repo_root: &Path, name: &str, workspace: &str) -> Result<()>;
    /// `jj bookmark delete <name>`
    fn bookmark_delete(&self, repo_root: &Path, name: &str) -> Result<()>;
    /// True if a bookmark with this name exists.
    fn bookmark_exists(&self, repo_root: &Path, name: &str) -> Result<bool>;
    /// True if the bookmark's target is an ancestor of trunk.
    fn bookmark_is_merged_into_trunk(&self, repo_root: &Path, name: &str) -> Result<bool>;
    /// True if `jj status` for the workspace shows any uncommitted changes.
    fn workspace_is_dirty(&self, repo_root: &Path, workspace: &str) -> Result<bool>;
}

/// Real implementation: shells out to the `jj` binary.
pub struct JjCli {
    pub jj_path: std::path::PathBuf,
}

impl JjCli {
    pub fn new() -> anyhow::Result<Self> {
        let jj_path = which::which("jj").map_err(|e| anyhow::anyhow!("jj not found on PATH: {e}"))?;
        Ok(Self { jj_path })
    }
}

fn run(cmd: &mut std::process::Command) -> Result<std::process::Output> {
    let out = cmd.output().map_err(|e| anyhow::anyhow!("spawn failed: {e}"))?;

    if !out.status.success() {
        return Err(anyhow::anyhow!(
            "jj command failed: {:?}\nstderr: {}",
            cmd,
            String::from_utf8_lossy(&out.stderr)
        ));
    }

    Ok(out)
}

impl Jj for JjCli {
    fn repo_root(&self, start: &Path) -> Result<std::path::PathBuf> {
        let mut p = start.to_path_buf();

        loop {
            if p.join(".jj").is_dir() {
                return Ok(p);
            }

            if !p.pop() {
                return Err(anyhow::anyhow!(
                    "not inside a jj repo (no .jj/ found above {start:?})"
                ));
            }
        }
    }

    fn workspace_list(&self, repo_root: &Path) -> Result<Vec<Workspace>> {
        let out = run(std::process::Command::new(&self.jj_path)
            .arg("--repository")
            .arg(repo_root)
            .arg("workspace")
            .arg("list")
            .arg("-T")
            .arg(r#"name ++ "\n""#))?;

        let text = String::from_utf8_lossy(&out.stdout);
        let mut workspaces = Vec::new();

        for line in text.lines() {
            let name = line.trim().to_string();

            if name.is_empty() {
                continue;
            }

            let path = if name == "default" {
                repo_root.to_path_buf()
            } else {
                repo_root.join(".worktrees").join(&name)
            };

            workspaces.push(Workspace { name, path, stale: false });
        }

        Ok(workspaces)
    }

    fn workspace_add(&self, repo_root: &Path, name: &str, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("failed to create workspace parent dir: {e}"))?;
        }

        run(std::process::Command::new(&self.jj_path)
            .arg("--repository")
            .arg(repo_root)
            .arg("workspace")
            .arg("add")
            .arg("--name")
            .arg(name)
            .arg(path))?;

        Ok(())
    }

    fn workspace_forget(&self, repo_root: &Path, name: &str) -> Result<()> {
        run(std::process::Command::new(&self.jj_path)
            .arg("--repository")
            .arg(repo_root)
            .arg("workspace")
            .arg("forget")
            .arg(name))?;

        Ok(())
    }

    fn workspace_update_stale(&self, repo_root: &Path, name: &str) -> Result<()> {
        let ws_path = repo_root.join(".worktrees").join(name);

        run(std::process::Command::new(&self.jj_path)
            .current_dir(&ws_path)
            .arg("workspace")
            .arg("update-stale"))?;

        Ok(())
    }

    fn bookmark_create(&self, repo_root: &Path, name: &str, workspace: &str) -> Result<()> {
        let revset = format!("{}@", workspace);

        run(std::process::Command::new(&self.jj_path)
            .arg("--repository")
            .arg(repo_root)
            .arg("bookmark")
            .arg("create")
            .arg(name)
            .arg("--revision")
            .arg(&revset))?;

        Ok(())
    }

    fn bookmark_delete(&self, repo_root: &Path, name: &str) -> Result<()> {
        run(std::process::Command::new(&self.jj_path)
            .arg("--repository")
            .arg(repo_root)
            .arg("bookmark")
            .arg("delete")
            .arg(name))?;

        Ok(())
    }

    fn bookmark_exists(&self, repo_root: &Path, name: &str) -> Result<bool> {
        let out = std::process::Command::new(&self.jj_path)
            .arg("--repository")
            .arg(repo_root)
            .arg("bookmark")
            .arg("list")
            .arg("-T")
            .arg(r#"name ++ "\n""#)
            .output()
            .map_err(|e| anyhow::anyhow!("spawn failed: {e}"))?;

        if !out.status.success() {
            return Ok(false);
        }

        let text = String::from_utf8_lossy(&out.stdout);

        Ok(text.lines().any(|l| l.trim() == name))
    }

    fn bookmark_is_merged_into_trunk(&self, repo_root: &Path, name: &str) -> Result<bool> {
        let out = run(std::process::Command::new(&self.jj_path)
            .arg("--repository")
            .arg(repo_root)
            .arg("log")
            .arg("--no-graph")
            .arg("-r")
            .arg(&format!("{} & ::trunk()", name))
            .arg("-T")
            .arg(r#""x""#)
            .arg("--limit")
            .arg("1"))?;

        Ok(!out.stdout.is_empty())
    }

    fn workspace_is_dirty(&self, repo_root: &Path, workspace: &str) -> Result<bool> {
        let ws_path = repo_root.join(".worktrees").join(workspace);

        let out = std::process::Command::new(&self.jj_path)
            .current_dir(&ws_path)
            .arg("diff")
            .arg("--summary")
            .output()
            .map_err(|e| anyhow::anyhow!("spawn failed: {e}"))?;

        if !out.status.success() {
            return Err(anyhow::anyhow!(
                "jj diff failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }

        Ok(!out.stdout.is_empty())
    }
}
