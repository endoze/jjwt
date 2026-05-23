// SPDX-License-Identifier: MIT
// Ported verbatim from https://github.com/max-sixty/worktrunk
// commit: 2b0077b

use std::hash::{Hash, Hasher};

/// Hash a string to a port in range 10000-19999.
pub fn hash_port(s: &str) -> u16 {
  let mut h = std::collections::hash_map::DefaultHasher::new();
  s.hash(&mut h);
  10000 + (h.finish() % 10000) as u16
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn result_in_range() {
    for input in ["main", "feat/login", "", "a".repeat(1000).as_str()] {
      let port = hash_port(input);

      assert!(
        (10000..=19999).contains(&port),
        "port {port} out of range for {input:?}"
      );
    }
  }

  #[test]
  fn deterministic() {
    assert_eq!(hash_port("my-branch"), hash_port("my-branch"));
  }

  #[test]
  fn different_inputs_differ() {
    assert_ne!(hash_port("alpha"), hash_port("beta"));
  }
}
