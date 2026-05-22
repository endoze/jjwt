use jjwt::core::template::render;
use jjwt::core::types::RenderContext;

#[test]
fn renders_branch_variable() {
  let ctx = RenderContext {
    branch: "feat-foo".into(),
  };
  let out = render("hello {{ branch }}", &ctx).expect("render ok");
  assert_eq!(out, "hello feat-foo");
}

#[test]
fn renders_with_hash_port_filter() {
  let ctx = RenderContext {
    branch: "feat-foo".into(),
  };
  let out = render("{{ ('app-' ~ branch) | hash_port }}", &ctx).expect("render ok");
  let n: u16 = out.parse().expect("u16");
  let _ = n;
}

#[test]
fn renders_with_sanitize_filter() {
  let ctx = RenderContext {
    branch: "Bug/FOO".into(),
  };
  let out = render("{{ branch | sanitize }}", &ctx).expect("render ok");
  assert!(!out.is_empty());
  assert!(!out.contains('/'), "sanitize must strip slashes");
}

#[test]
fn missing_variable_errors() {
  let ctx = RenderContext { branch: "x".into() };
  let err = render("{{ undefined_var }}", &ctx);
  assert!(err.is_err());
}
