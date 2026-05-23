#![cfg(not(tarpaulin_include))]

use anyhow::Result;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Remove trash entries older than `max_age`. Best-effort: errors on
/// individual entries are silently ignored (the next sweep will catch them).
pub fn sweep_trash(repo_root: &Path, max_age: Duration) -> Result<()> {
  let trash_dir = repo_root.join(".jj").join(".jjwt-trash");

  if !trash_dir.is_dir() {
    return Ok(());
  }

  let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .as_millis();
  let cutoff = now.saturating_sub(max_age.as_millis());

  for entry in std::fs::read_dir(&trash_dir)? {
    let entry = match entry {
      Ok(e) => e,
      Err(_) => continue,
    };
    let name = entry.file_name();
    let name_str = name.to_string_lossy();
    let ts: u128 = match name_str.parse() {
      Ok(t) => t,
      Err(_) => continue,
    };

    if ts < cutoff {
      let _ = std::fs::remove_dir_all(entry.path());
    }
  }

  Ok(())
}
