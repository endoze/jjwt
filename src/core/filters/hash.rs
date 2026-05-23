// SPDX-License-Identifier: MIT
// Ported from https://github.com/max-sixty/worktrunk (short_hash).

use std::hash::{Hash, Hasher};

/// 3-character base36 digest of the input.
///
/// Uses Rust's `DefaultHasher` (same as worktrunk's `short_hash`) for
/// byte-fidelity. 46,656 unique values — enough to disambiguate inputs
/// whose sanitized forms collide but not so many that templates grow.
pub fn short_hash(s: &str) -> String {
  let mut h = std::collections::hash_map::DefaultHasher::new();
  s.hash(&mut h);
  let hash = h.finish();

  const CHARS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
  let c0 = CHARS[(hash % 36) as usize];
  let c1 = CHARS[((hash / 36) % 36) as usize];
  let c2 = CHARS[((hash / 1296) % 36) as usize];

  String::from_utf8(vec![c0, c1, c2]).unwrap()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn produces_three_chars() {
    let h = short_hash("anything");

    assert_eq!(h.len(), 3);
    assert!(h.chars().all(|c| c.is_ascii_alphanumeric()));
  }

  #[test]
  fn deterministic_for_same_input() {
    assert_eq!(short_hash("feature/auth"), short_hash("feature/auth"));
  }

  #[test]
  fn differs_for_different_inputs() {
    assert_ne!(short_hash("a-b"), short_hash("a_b"));
  }
}
