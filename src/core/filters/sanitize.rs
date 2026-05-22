// SPDX-License-Identifier: MIT OR Apache-2.0
// Ported verbatim from https://github.com/max-sixty/worktrunk
// commit: 2b0077b

/// Sanitize a branch name for use as a filesystem path component.
///
/// Replaces path separators (`/` and `\`) with dashes to prevent directory
/// traversal and ensure the branch name is a single path component.
pub fn sanitize(branch: &str) -> String {
  branch.replace(['/', '\\'], "-")
}
