use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
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
