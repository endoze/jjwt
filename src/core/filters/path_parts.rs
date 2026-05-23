// SPDX-License-Identifier: MIT
// Ported from https://github.com/max-sixty/worktrunk (dirname/basename filters).

/// POSIX-style parent-of-path. `/a/b/c` → `/a/b`. No trailing slash.
pub fn dirname(value: &str) -> String {
  std::path::Path::new(value)
    .parent()
    .map(|p| p.to_string_lossy().into_owned())
    .unwrap_or_default()
}

/// POSIX-style final-component-of-path. `/a/b/c` → `c`.
pub fn basename(value: &str) -> String {
  std::path::Path::new(value)
    .file_name()
    .map(|n| n.to_string_lossy().into_owned())
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn dirname_strips_last_component() {
    assert_eq!(dirname("/a/b/c"), "/a/b");
    assert_eq!(dirname("a/b/c"), "a/b");
  }

  #[test]
  fn dirname_at_root_is_empty_or_root() {
    let out = dirname("/");

    assert!(out.is_empty() || out == "/");
  }

  #[test]
  fn basename_keeps_last_component() {
    assert_eq!(basename("/a/b/c"), "c");
    assert_eq!(basename("a/b/c.txt"), "c.txt");
  }

  #[test]
  fn basename_of_single_component_is_itself() {
    assert_eq!(basename("file"), "file");
  }

  #[test]
  fn empty_returns_empty() {
    assert_eq!(dirname(""), "");
    assert_eq!(basename(""), "");
  }
}
