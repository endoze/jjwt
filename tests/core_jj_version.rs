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

/// Major and minor of the `jj-lib` version requirement in Cargo.toml.
fn bundled_jj_lib_version() -> (u32, u32) {
  let manifest: toml::Table = toml::from_str(include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/Cargo.toml"
  )))
  .unwrap();
  let requirement = manifest["dependencies"]["jj-lib"]["version"]
    .as_str()
    .expect("jj-lib version is a string");
  let mut parts = requirement
    .trim_start_matches(|c: char| !c.is_ascii_digit())
    .split('.')
    .map(|part| part.parse::<u32>().expect("numeric jj-lib version"));

  (parts.next().unwrap(), parts.next().unwrap())
}

#[test]
fn minimum_matches_bundled_jj_lib() {
  assert_eq!(
    MIN_JJ_VERSION,
    bundled_jj_lib_version(),
    "jj-lib in Cargo.toml changed: update MIN_JJ_VERSION and the jj version in README.md to match"
  );
  assert!((0, 46) < MIN_JJ_VERSION);
}
