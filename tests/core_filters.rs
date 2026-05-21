// Pin tests for core filter functions.
//
// Pinned values were obtained via approach 3 from the task spec:
// a standalone Rust seed program at /tmp/seed that reproduces the upstream
// string_to_port logic verbatim, compiled and run with `cargo run`.
// Seed source: /tmp/seed/src/main.rs
// To re-seed: compile and run that program on the target Rust toolchain,
// update the expected values below.

use jjwt::core::filters::hash_port::hash_port;

#[test]
fn hash_port_pinned_values() {
    assert_eq!(hash_port("app-feat-port-webhook-to-rust"), 12200);
    assert_eq!(hash_port("app-main"), 19926);
    assert_eq!(hash_port("app-bug-foo"), 12511);
}

#[test]
fn sanitize_pinned_values() {
    // Placeholder — will be filled in by Task 5.
    // assert_eq!(sanitize("app-feat/port-webhook-to-rust"), "app-feat-port-webhook-to-rust");
}
