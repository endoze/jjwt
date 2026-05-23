// SPDX-License-Identifier: MIT
// Ported verbatim from https://github.com/max-sixty/worktrunk
// commit: 2b0077b

/// Sanitize a branch name for use as a filesystem path component.
///
/// Replaces path separators (`/` and `\`) with dashes to prevent directory
/// traversal and ensure the branch name is a single path component.
pub fn sanitize(branch: &str) -> String {
  branch.replace(['/', '\\'], "-")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn replaces_forward_slashes() {
    assert_eq!(sanitize("feat/login"), "feat-login");
  }

  #[test]
  fn replaces_backslashes() {
    assert_eq!(sanitize("feat\\login"), "feat-login");
  }

  #[test]
  fn replaces_mixed_slashes() {
    assert_eq!(sanitize("feat/ui\\modal"), "feat-ui-modal");
  }

  #[test]
  fn leaves_other_characters_unchanged() {
    assert_eq!(sanitize("my-branch_name.v2"), "my-branch_name.v2");
  }

  #[test]
  fn empty_input() {
    assert_eq!(sanitize(""), "");
  }
}
