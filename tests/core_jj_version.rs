use jjwt::core::types::{MIN_JJ_VERSION, parse_jj_version};

#[test]
fn parses_release_version() {
  assert_eq!(parse_jj_version("jj 0.45.1\n"), Some((0, 45)));
}

#[test]
fn parses_dev_build_version() {
  assert_eq!(parse_jj_version("jj 0.39.0-abc1234"), Some((0, 39)));
}

#[test]
fn rejects_garbage() {
  assert_eq!(parse_jj_version("not jj"), None);
}

#[test]
fn minimum_is_first_release_with_util_snapshot() {
  assert_eq!(MIN_JJ_VERSION, (0, 39));
  assert!((0, 38) < MIN_JJ_VERSION);
}
