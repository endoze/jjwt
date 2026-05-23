// SPDX-License-Identifier: MIT
// Ported from https://github.com/max-sixty/worktrunk (sanitize_for_filename).

use crate::core::filters::hash::short_hash;
use sanitize_filename::{Options as SanitizeOptions, sanitize_with_options};

/// Sanitize a string for use as a filename on all platforms.
///
/// If the input is already a safe filename, it is returned unchanged.
/// Otherwise the disallowed characters are replaced with `-`, and a
/// 3-character base36 hash suffix is appended so that distinct inputs
/// whose sanitized forms collide produce distinct outputs.
///
/// Handles invalid characters, control characters, Windows reserved
/// names (CON, PRN, etc.), and trailing dots/spaces — via the
/// `sanitize-filename` crate, matching worktrunk's behavior byte-for-byte.
pub fn sanitize_hash(value: &str) -> String {
  let sanitized = sanitize_with_options(
    value,
    SanitizeOptions {
      windows: true,
      truncate: false,
      replacement: "-",
    },
  );

  if sanitized == value && !value.is_empty() {
    return sanitized;
  }

  let mut result = if sanitized.is_empty() {
    "_empty".to_string()
  } else {
    sanitized
  };

  if !result.ends_with('-') {
    result.push('-');
  }

  result.push_str(&short_hash(value));

  result
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn already_safe_passes_through_unchanged() {
    assert_eq!(sanitize_hash("simple-branch"), "simple-branch");
  }

  #[test]
  fn unsafe_input_gets_hash_suffix() {
    let out = sanitize_hash("feature/auth");

    assert!(out.starts_with("feature-auth-"));
    assert_eq!(out.len(), "feature-auth-".len() + 3);
  }

  #[test]
  fn empty_input_uses_empty_placeholder() {
    let out = sanitize_hash("");

    assert!(out.starts_with("_empty-"));
  }

  #[test]
  fn distinguishes_colliding_inputs() {
    assert_ne!(sanitize_hash("a/b"), sanitize_hash("a-b"));
  }
}
