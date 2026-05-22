// Pin tests for core filter functions.
//
// Pinned values were obtained via standalone Rust seed programs that reproduce
// the upstream logic verbatim, compiled and run with `cargo run`.
//
// hash_port seed: /tmp/seed/src/main.rs  (reproduces string_to_port from expansion.rs)
// sanitize seed:  /tmp/seed_sanitize/src/main.rs  (reproduces sanitize_branch_name from expansion.rs)
//
// To re-seed: compile and run the relevant program on the target Rust toolchain,
// update the expected values below.

use jjwt::core::filters::hash_port::hash_port;
use jjwt::core::filters::sanitize::sanitize;

#[test]
fn hash_port_pinned_values() {
  assert_eq!(hash_port("app-feat-port-webhook-to-rust"), 12200);
  assert_eq!(hash_port("app-main"), 19926);
  assert_eq!(hash_port("app-bug-foo"), 12511);
}

#[test]
fn sanitize_pinned_values() {
  assert_eq!(sanitize("feat/port-webhook"), "feat-port-webhook");
  assert_eq!(sanitize("Bug/FOO_Bar"), "Bug-FOO_Bar");
  assert_eq!(sanitize("main"), "main");
}
