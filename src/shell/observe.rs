use anyhow::Result;
use std::path::Path;

use crate::core::types::ObservedState;
use crate::shell::fs::Fs;
use crate::shell::jj::Jj;

pub fn observe<J: Jj, F: Fs>(
    jj: &J,
    fs: &F,
    start_dir: &Path,
    target_name: Option<&str>,
) -> Result<ObservedState> {
    let repo_root = match jj.repo_root(start_dir) {
        Ok(r) => r,
        Err(_) => {
            return Ok(ObservedState {
                repo_root: start_dir.to_path_buf(),
                is_jj_repo: false,
                workspaces: vec![],
                target_path_exists: false,
                target_workspace_dirty: false,
                target_bookmark_merged: false,
                target_bookmark_exists: false,
            });
        }
    };

    let workspaces = jj.workspace_list(&repo_root)?;

    let mut target_path_exists = false;
    let mut target_workspace_dirty = false;
    let mut target_bookmark_merged = false;
    let mut target_bookmark_exists = false;

    if let Some(name) = target_name {
        let target_path = repo_root.join(".worktrees").join(name);
        target_path_exists = fs.exists(&target_path);

        if workspaces.iter().any(|w| w.name == name) {
            target_workspace_dirty = jj.workspace_is_dirty(&repo_root, name)?;
        }

        target_bookmark_exists = jj.bookmark_exists(&repo_root, name)?;
        if target_bookmark_exists {
            target_bookmark_merged = jj.bookmark_is_merged_into_trunk(&repo_root, name)?;
        }
    }

    Ok(ObservedState {
        repo_root,
        is_jj_repo: true,
        workspaces,
        target_path_exists,
        target_workspace_dirty,
        target_bookmark_merged,
        target_bookmark_exists,
    })
}
