#![cfg(not(tarpaulin_include))]

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// How long cached summaries remain valid (7 days in seconds).
const TTL_SECONDS: i64 = 7 * 24 * 60 * 60;

/// On-disk cache mapping commit IDs to LLM-generated summaries.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LlmCache {
  /// Cached summaries keyed by commit short ID.
  #[serde(default)]
  pub summaries: HashMap<String, CachedSummary>,
}

/// A single cached summary with its creation timestamp.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CachedSummary {
  /// The cached summary text.
  pub text: String,
  /// Unix timestamp when this entry was created.
  pub created: i64,
}

/// Compute the on-disk path for the LLM cache file within the repo.
fn cache_path(repo_root: &Path) -> PathBuf {
  repo_root.join(".jj").join("jjwt-llm-cache.toml")
}

/// Return the current time as a Unix timestamp in seconds.
fn now_unix() -> i64 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_secs() as i64)
    .unwrap_or(0)
}

/// Read cache from `.jj/jjwt-llm-cache.toml`. Returns `Default` when the
/// file is missing or unreadable — cache is best-effort metadata.
pub fn load(repo_root: &Path) -> LlmCache {
  let p = cache_path(repo_root);

  let src = match std::fs::read_to_string(&p) {
    Ok(s) => s,
    Err(_) => return LlmCache::default(),
  };

  toml::from_str(&src).unwrap_or_default()
}

/// Write the cache file, evicting entries older than 7 days first.
pub fn save(repo_root: &Path, cache: &mut LlmCache) -> Result<()> {
  let cutoff = now_unix() - TTL_SECONDS;

  cache.summaries.retain(|_, v| v.created > cutoff);

  let p = cache_path(repo_root);

  if let Some(parent) = p.parent() {
    std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
  }

  let body = toml::to_string(cache).context("serialize llm cache")?;

  std::fs::write(&p, body).with_context(|| format!("write {}", p.display()))?;

  Ok(())
}

/// Look up a cached summary by commit ID. Returns `None` if missing or expired.
pub fn get<'a>(cache: &'a LlmCache, commit_id: &str) -> Option<&'a str> {
  let entry = cache.summaries.get(commit_id)?;
  let cutoff = now_unix() - TTL_SECONDS;

  if entry.created <= cutoff {
    return None;
  }

  Some(&entry.text)
}

/// Insert a summary into the cache.
pub fn put(cache: &mut LlmCache, commit_id: String, text: String) {
  cache.summaries.insert(
    commit_id,
    CachedSummary {
      text,
      created: now_unix(),
    },
  );
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn round_trip_cache() {
    let dir = tempfile::tempdir().unwrap();
    let repo_root = dir.path().join(".jj");

    std::fs::create_dir_all(&repo_root).unwrap();

    let mut cache = LlmCache::default();

    put(&mut cache, "abc123".into(), "Add login feature".into());
    save(dir.path(), &mut cache).unwrap();

    let loaded = load(dir.path());

    assert_eq!(get(&loaded, "abc123"), Some("Add login feature"));
  }

  #[test]
  fn missing_entry_returns_none() {
    let cache = LlmCache::default();

    assert_eq!(get(&cache, "nonexistent"), None);
  }

  #[test]
  fn expired_entry_returns_none() {
    let mut cache = LlmCache::default();

    cache.summaries.insert(
      "old".into(),
      CachedSummary {
        text: "old summary".into(),
        created: 0,
      },
    );

    assert_eq!(get(&cache, "old"), None);
  }

  #[test]
  fn save_evicts_old_entries() {
    let dir = tempfile::tempdir().unwrap();
    let repo_root = dir.path().join(".jj");

    std::fs::create_dir_all(&repo_root).unwrap();

    let mut cache = LlmCache::default();

    cache.summaries.insert(
      "old".into(),
      CachedSummary {
        text: "old".into(),
        created: 0,
      },
    );
    put(&mut cache, "new".into(), "new".into());
    save(dir.path(), &mut cache).unwrap();

    assert!(!cache.summaries.contains_key("old"));
    assert!(cache.summaries.contains_key("new"));
  }
}
