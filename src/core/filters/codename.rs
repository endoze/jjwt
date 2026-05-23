// SPDX-License-Identifier: MIT
// Ported from https://github.com/max-sixty/worktrunk (codename filter).

use sha2::{Digest, Sha256};

/// Max words supported by `codename(n)`. Matches worktrunk.
pub const CODENAME_MAX_WORDS: usize = 5;

/// Deterministic friendly name from an input string.
///
/// `codename(1)` returns a noun, `codename(2)` returns `adjective-noun`,
/// and higher counts add more adjectives. Pool: `petname::Petnames::medium`
/// (1198 adjectives, 1052 nouns — about 1.26M `codename(2)` combinations).
///
/// Hash is computed via SHA-256 with fixed-width little-endian byte
/// representations for position/salt so output is identical on 32-bit
/// and 64-bit hosts. Matches worktrunk's algorithm byte-for-byte.
pub fn codename(input: &str, words: usize) -> String {
  let lists = petname::Petnames::medium();
  let adjectives: &[&str] = lists.adjectives.as_ref();
  let nouns: &[&str] = lists.nouns.as_ref();

  let mut parts: Vec<&str> = Vec::with_capacity(words);
  let adjective_count = words.saturating_sub(1);

  for position in 0..adjective_count {
    let mut salt: usize = 0;

    loop {
      let index = codename_index(input, position, salt, "adjective", adjectives.len());
      let word = adjectives[index];

      if !parts.contains(&word) || salt >= adjectives.len() {
        parts.push(word);

        break;
      }

      salt += 1;
    }
  }

  let index = codename_index(input, adjective_count, 0, "noun", nouns.len());

  parts.push(nouns[index]);

  parts.join("-")
}

/// Compute a deterministic index into a word pool using SHA-256.
fn codename_index(input: &str, position: usize, salt: usize, pool: &str, len: usize) -> usize {
  let mut hasher = Sha256::new();

  hasher.update(input.as_bytes());
  hasher.update([0]);
  hasher.update((position as u64).to_le_bytes());
  hasher.update((salt as u64).to_le_bytes());
  hasher.update(pool.as_bytes());

  let digest = hasher.finalize();
  let mut bytes = [0u8; 8];

  bytes.copy_from_slice(&digest[..8]);

  (u64::from_le_bytes(bytes) % len as u64) as usize
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn deterministic_for_same_input() {
    assert_eq!(codename("feature/auth", 2), codename("feature/auth", 2));
  }

  #[test]
  fn one_word_is_noun_only() {
    let out = codename("anything", 1);

    assert!(!out.contains('-'));
  }

  #[test]
  fn two_words_is_adjective_noun() {
    let out = codename("anything", 2);

    assert_eq!(out.matches('-').count(), 1);
  }

  #[test]
  fn three_words_has_two_separators() {
    let out = codename("anything", 3);

    assert_eq!(out.matches('-').count(), 2);
  }
}
