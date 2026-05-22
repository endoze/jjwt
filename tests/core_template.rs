use jjwt::core::template::render;
use jjwt::core::types::RenderContext;

fn ctx_with_branch(branch: &str) -> RenderContext {
  RenderContext {
    branch: branch.into(),
    ..Default::default()
  }
}

#[test]
fn renders_branch_variable() {
  let out = render("hello {{ branch }}", &ctx_with_branch("feat-foo")).expect("render ok");

  assert_eq!(out, "hello feat-foo");
}

#[test]
fn renders_with_hash_port_filter() {
  let out = render(
    "{{ ('app-' ~ branch) | hash_port }}",
    &ctx_with_branch("feat-foo"),
  )
  .expect("render ok");
  let _: u16 = out.parse().expect("u16");
}

#[test]
fn renders_with_sanitize_filter() {
  let out = render("{{ branch | sanitize }}", &ctx_with_branch("Bug/FOO")).expect("render ok");

  assert!(!out.is_empty());
  assert!(!out.contains('/'), "sanitize must strip slashes");
}

#[test]
fn missing_variable_errors() {
  let err = render("{{ undefined_var }}", &ctx_with_branch("x"));

  assert!(err.is_err());
}

#[test]
fn renders_with_sanitize_db_filter() {
  let out = render(
    "{{ branch | sanitize_db }}",
    &ctx_with_branch("Feature/Auth"),
  )
  .expect("render ok");

  assert!(out.starts_with("feature_auth_"));
  assert!(out.len() <= 48);
}

#[test]
fn renders_with_sanitize_hash_filter() {
  let out = render(
    "{{ branch | sanitize_hash }}",
    &ctx_with_branch("feature/auth"),
  )
  .expect("render ok");

  assert!(out.starts_with("feature-auth-"));
}

#[test]
fn renders_with_hash_filter() {
  let out = render("{{ branch | hash }}", &ctx_with_branch("anything")).expect("render ok");

  assert_eq!(out.len(), 3);
}

#[test]
fn renders_with_dirname_basename_filters() {
  let ctx = ctx_with_branch("/a/b/c");

  assert_eq!(render("{{ branch | dirname }}", &ctx).expect("ok"), "/a/b");
  assert_eq!(render("{{ branch | basename }}", &ctx).expect("ok"), "c");
}

#[test]
fn renders_with_codename_filter_default_two_words() {
  let out = render("{{ branch | codename }}", &ctx_with_branch("anything")).expect("render ok");

  assert_eq!(out.matches('-').count(), 1);
}

#[test]
fn renders_with_codename_filter_explicit_words() {
  let out = render("{{ branch | codename(3) }}", &ctx_with_branch("anything")).expect("render ok");

  assert_eq!(out.matches('-').count(), 2);
}

#[test]
fn codename_rejects_zero_words() {
  let err = render("{{ branch | codename(0) }}", &ctx_with_branch("anything"));

  assert!(err.is_err());
}

#[test]
fn codename_rejects_too_many_words() {
  let err = render("{{ branch | codename(99) }}", &ctx_with_branch("anything"));

  assert!(err.is_err());
}

#[test]
fn renders_new_template_vars_when_set() {
  let ctx = RenderContext {
    branch: "feat".into(),
    worktree_path: Some("/repo/.worktrees/feat".into()),
    worktree_name: Some("feat".into()),
    repo: Some("repo".into()),
    repo_path: Some("/repo".into()),
    cwd: Some("/repo/.worktrees/feat".into()),
    hook_type: Some("pre-start".into()),
    hook_name: Some("setup".into()),
    args: vec!["a".into(), "b".into()],
  };
  let tmpl = "{{ branch }}|{{ worktree_path }}|{{ worktree_name }}|{{ repo }}|{{ repo_path }}|{{ cwd }}|{{ hook_type }}|{{ hook_name }}|{{ args | length }}";
  let out = render(tmpl, &ctx).expect("render ok");

  assert_eq!(
    out,
    "feat|/repo/.worktrees/feat|feat|repo|/repo|/repo/.worktrees/feat|pre-start|setup|2"
  );
}
