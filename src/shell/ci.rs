#![cfg(not(tarpaulin_include))]

use crate::core::types::CiStatus;
use std::collections::HashMap;
use std::path::Path;

/// Query CI check statuses for a set of bookmarks by calling the GitHub
/// or GitLab CLI. Returns a map of bookmark name to CI status.
///
/// Only runs when `gh` or `glab` is found on PATH. Falls back to all
/// `CiStatus::None` when neither is available or the query fails.
pub fn query_ci_statuses(repo_root: &Path, bookmarks: &[String]) -> HashMap<String, CiStatus> {
  let mut result: HashMap<String, CiStatus> = HashMap::new();

  if bookmarks.is_empty() {
    return result;
  }

  if let Some(statuses) = try_github(repo_root) {
    fill_statuses(&mut result, bookmarks, &statuses);

    return result;
  }

  if let Some(statuses) = try_gitlab(repo_root) {
    fill_statuses(&mut result, bookmarks, &statuses);

    return result;
  }

  for name in bookmarks {
    result.insert(name.clone(), CiStatus::None);
  }

  result
}

/// Populate `result` with one entry per bookmark, looking up the CI status
/// in `statuses` and defaulting to `CiStatus::None` for misses.
fn fill_statuses(
  result: &mut HashMap<String, CiStatus>,
  bookmarks: &[String],
  statuses: &HashMap<String, CiStatus>,
) {
  for name in bookmarks {
    let status = statuses
      .get(name.as_str())
      .copied()
      .unwrap_or(CiStatus::None);

    result.insert(name.clone(), status);
  }
}

/// Attempt to query CI statuses from GitHub via `gh pr list`.
fn try_github(repo_root: &Path) -> Option<HashMap<String, CiStatus>> {
  if which::which("gh").is_err() {
    return None;
  }

  let output = std::process::Command::new("gh")
    .current_dir(repo_root)
    .args([
      "pr",
      "list",
      "--state=open",
      "--json",
      "headRefName,statusCheckRollup",
      "--limit=100",
    ])
    .output()
    .ok()?;

  if !output.status.success() {
    return None;
  }

  let text = String::from_utf8(output.stdout).ok()?;
  let arr: Vec<serde_json::Value> = serde_json::from_str(&text).ok()?;

  let mut statuses = HashMap::new();

  for item in arr {
    let branch = item.get("headRefName")?.as_str()?;
    let checks = item.get("statusCheckRollup").and_then(|v| v.as_array());
    let ci = match checks.map(|v| v.as_slice()) {
      None | Some(&[]) => CiStatus::None,
      Some(checks) => aggregate_github_checks(checks),
    };

    statuses.insert(branch.to_string(), ci);
  }

  Some(statuses)
}

/// Reduce a list of GitHub status checks into a single CI status.
fn aggregate_github_checks(checks: &[serde_json::Value]) -> CiStatus {
  let mut has_pending = false;

  for check in checks {
    let conclusion = check
      .get("conclusion")
      .and_then(|v| v.as_str())
      .unwrap_or("");
    let status = check.get("status").and_then(|v| v.as_str()).unwrap_or("");

    if conclusion == "FAILURE" || conclusion == "ERROR" {
      return CiStatus::Fail;
    }

    if status == "IN_PROGRESS" || status == "QUEUED" || status == "PENDING" || conclusion.is_empty()
    {
      has_pending = true;
    }
  }

  if has_pending {
    CiStatus::Pending
  } else {
    CiStatus::Pass
  }
}

/// Attempt to query CI statuses from GitLab via `glab mr list`.
fn try_gitlab(repo_root: &Path) -> Option<HashMap<String, CiStatus>> {
  if which::which("glab").is_err() {
    return None;
  }

  let output = std::process::Command::new("glab")
    .current_dir(repo_root)
    .args(["mr", "list", "--state=opened", "-F", "json"])
    .output()
    .ok()?;

  if !output.status.success() {
    return None;
  }

  let text = String::from_utf8(output.stdout).ok()?;
  let arr: Vec<serde_json::Value> = serde_json::from_str(&text).ok()?;

  let mut statuses = HashMap::new();

  for item in arr {
    let branch = item.get("source_branch")?.as_str()?;
    let pipeline = item.get("head_pipeline");
    let ci = match pipeline {
      None => CiStatus::None,
      Some(p) => {
        let status = p.get("status").and_then(|v| v.as_str()).unwrap_or("");

        match status {
          "success" => CiStatus::Pass,
          "failed" => CiStatus::Fail,
          "running" | "pending" | "created" => CiStatus::Pending,
          _ => CiStatus::None,
        }
      }
    };

    statuses.insert(branch.to_string(), ci);
  }

  Some(statuses)
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn aggregate_github_all_success() {
    let checks = vec![
      json!({"conclusion": "SUCCESS", "status": "COMPLETED"}),
      json!({"conclusion": "SUCCESS", "status": "COMPLETED"}),
    ];

    assert_eq!(aggregate_github_checks(&checks), CiStatus::Pass);
  }

  #[test]
  fn aggregate_github_any_failure() {
    let checks = vec![
      json!({"conclusion": "SUCCESS", "status": "COMPLETED"}),
      json!({"conclusion": "FAILURE", "status": "COMPLETED"}),
    ];

    assert_eq!(aggregate_github_checks(&checks), CiStatus::Fail);
  }

  #[test]
  fn aggregate_github_error_is_fail() {
    let checks = vec![json!({"conclusion": "ERROR", "status": "COMPLETED"})];

    assert_eq!(aggregate_github_checks(&checks), CiStatus::Fail);
  }

  #[test]
  fn aggregate_github_in_progress() {
    let checks = vec![
      json!({"conclusion": "SUCCESS", "status": "COMPLETED"}),
      json!({"conclusion": "", "status": "IN_PROGRESS"}),
    ];

    assert_eq!(aggregate_github_checks(&checks), CiStatus::Pending);
  }

  #[test]
  fn aggregate_github_queued() {
    let checks = vec![json!({"conclusion": "", "status": "QUEUED"})];

    assert_eq!(aggregate_github_checks(&checks), CiStatus::Pending);
  }

  #[test]
  fn aggregate_github_empty_conclusion_is_pending() {
    let checks = vec![json!({"status": "COMPLETED"})];

    assert_eq!(aggregate_github_checks(&checks), CiStatus::Pending);
  }
}
