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

// JjCli method bodies are implemented in Task 12.
