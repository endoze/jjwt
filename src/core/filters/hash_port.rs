// SPDX-License-Identifier: MIT OR Apache-2.0
// Ported verbatim from https://github.com/max-sixty/worktrunk
// commit: 2b0077b

use std::hash::{Hash, Hasher};

/// Hash a string to a port in range 10000-19999.
pub fn hash_port(s: &str) -> u16 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    10000 + (h.finish() % 10000) as u16
}
