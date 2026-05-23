// SPDX-License-Identifier: MIT
// Ported from https://github.com/max-sixty/worktrunk (sanitize_db).

use crate::core::filters::hash::short_hash;

/// Sanitize a string for use as a database identifier.
///
/// Transformation (applied in order):
/// 1. Lowercase.
/// 2. Replace non-alphanumeric runs with a single `_`.
/// 3. Prefix `_` if the result starts with a digit.
/// 4. Truncate base to 44 chars (leaves room for the 4-char `_xxx` suffix).
/// 5. Append `_` (if not already present) and a 3-char base36 hash of the
///    original input.
///
/// Final length is at most 48 chars — well within PostgreSQL's 63-char
/// identifier limit. Empty input maps to empty string (not a valid
/// identifier; callers should guard).
pub fn sanitize_db(s: &str) -> String {
  if s.is_empty() {
    return String::new();
  }

  let mut result = String::with_capacity(s.len() + 4);
  let mut prev_underscore = false;

  for c in s.chars() {
    if c.is_ascii_alphanumeric() {
      result.push(c.to_ascii_lowercase());
      prev_underscore = false;
    } else if !prev_underscore {
      result.push('_');
      prev_underscore = true;
    }
  }

  if result.starts_with(|c: char| c.is_ascii_digit()) {
    result.insert(0, '_');
  }

  if result.len() > 44 {
    result.truncate(44);
  }

  if !result.ends_with('_') {
    result.push('_');
  }

  result.push_str(&short_hash(s));

  result
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn empty_in_empty_out() {
    assert_eq!(sanitize_db(""), "");
  }

  #[test]
  fn lowercases_and_replaces_non_alphanumeric() {
    let out = sanitize_db("Feature/Auth-OAuth2");

    assert!(out.starts_with("feature_auth_oauth2_"));
    assert_eq!(out.len(), 23); // "feature_auth_oauth2_" + 3-char hash
  }

  #[test]
  fn prefixes_underscore_for_leading_digit() {
    let out = sanitize_db("123-bug-fix");

    assert!(out.starts_with("_123_bug_fix_"));
  }

  #[test]
  fn collapses_consecutive_separators() {
    let out = sanitize_db("a---b");

    assert!(out.starts_with("a_b_"));
  }

  #[test]
  fn truncates_long_input_within_48_chars() {
    let long = "a".repeat(100);
    let out = sanitize_db(&long);

    assert!(out.len() <= 48);
  }

  #[test]
  fn distinguishes_colliding_inputs_via_hash() {
    assert_ne!(sanitize_db("a-b"), sanitize_db("a_b"));
  }
}
