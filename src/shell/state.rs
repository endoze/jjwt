use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Tiny persistent state for jjwt, stored at `.jj/jjwt-state.toml`. Lives
/// inside `.jj/` so it's automatically scoped per repository and doesn't
/// pollute project source. Schema is intentionally minimal — every field
/// is optional so missing/empty files round-trip cleanly.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct JjwtState {
  /// Workspace the user was in immediately before the most recent
  /// successful `jjwt switch`. Powers the `-` shortcut.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub previous_workspace: Option<String>,
  /// Per-workspace key-value variables. Outer key is workspace name,
  /// inner map is key-value pairs. Accessible in hook templates as
  /// `{{ vars.KEY }}`.
  #[serde(default, skip_serializing_if = "HashMap::is_empty")]
  pub vars: HashMap<String, HashMap<String, String>>,
}

impl JjwtState {
  /// Get the variables for a workspace (empty map if none set).
  pub fn get_vars(&self, workspace: &str) -> &HashMap<String, String> {
    static EMPTY: std::sync::LazyLock<HashMap<String, String>> =
      std::sync::LazyLock::new(HashMap::new);

    self.vars.get(workspace).unwrap_or(&EMPTY)
  }

  /// Set a variable for a workspace.
  pub fn set_var(&mut self, workspace: &str, key: &str, val: &str) {
    self
      .vars
      .entry(workspace.to_string())
      .or_default()
      .insert(key.to_string(), val.to_string());
  }

  /// Remove a variable for a workspace. Returns the removed value.
  pub fn remove_var(&mut self, workspace: &str, key: &str) -> Option<String> {
    let ws_vars = self.vars.get_mut(workspace)?;
    let removed = ws_vars.remove(key);

    if ws_vars.is_empty() {
      self.vars.remove(workspace);
    }

    removed
  }
}

fn state_path(repo_root: &Path) -> PathBuf {
  repo_root.join(".jj").join("jjwt-state.toml")
}

/// Read state from `.jj/jjwt-state.toml`. Returns `Default` when the file
/// is missing or unreadable — state is best-effort metadata.
pub fn load(repo_root: &Path) -> JjwtState {
  let p = state_path(repo_root);

  let src = match std::fs::read_to_string(&p) {
    Ok(s) => s,
    Err(_) => return JjwtState::default(),
  };

  toml::from_str(&src).unwrap_or_default()
}

/// Atomically write the state file. Errors propagate so callers can decide
/// whether to surface them (typically: log and continue — losing the
/// `previous_workspace` hint is a minor inconvenience, not a failure).
pub fn save(repo_root: &Path, state: &JjwtState) -> Result<()> {
  let p = state_path(repo_root);

  if let Some(parent) = p.parent() {
    std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
  }

  let body = toml::to_string(state).context("serialize state")?;

  std::fs::write(&p, body).with_context(|| format!("write {}", p.display()))?;

  Ok(())
}
