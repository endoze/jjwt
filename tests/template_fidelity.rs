// The fixture uses only `branch` as a template variable (via the `hash_port` and `sanitize`
// filters), so it exercises the full real-world template surface without requiring any
// RenderContext extension.
//
// Worktrunk availability: `worktrunk` binary was NOT found; `wt` resolved to an unrelated
// tool. Cross-validation against real worktrunk output was therefore not performed.
// See tests/snapshots/README.md for re-seeding instructions.
use jjwt::core::config::parse;
use jjwt::core::template::render;
use jjwt::core::types::RenderContext;

const BRANCHES: &[&str] = &["feat-port-webhook-to-rust", "main", "bug-foo"];

fn render_all_for_branch(cfg: &jjwt::core::types::Config, branch: &str) -> String {
  let ctx = RenderContext {
    branch: branch.into(),
    ..Default::default()
  };
  let mut out = String::new();

  if let Some(list) = &cfg.list {
    out.push_str("[list.url]\n");
    out.push_str(&render(&list.url, &ctx).expect("render list.url"));
    out.push('\n');
  }

  for (i, group) in cfg.pre_start.iter().enumerate() {
    for (name, tmpl) in group {
      out.push_str(&format!("[pre-start.{i}.{name}]\n"));
      out.push_str(&render(tmpl, &ctx).expect("render"));
      out.push('\n');
    }
  }

  for (i, group) in cfg.pre_remove.iter().enumerate() {
    for (name, tmpl) in group {
      out.push_str(&format!("[pre-remove.{i}.{name}]\n"));
      out.push_str(&render(tmpl, &ctx).expect("render"));
      out.push('\n');
    }
  }

  out
}

#[test]
fn fidelity_myapp() {
  let src = std::fs::read_to_string("fixtures/myapp.wt.toml").unwrap();
  let cfg = parse(&src).expect("parse ok");

  for branch in BRANCHES {
    let actual = render_all_for_branch(&cfg, branch);
    insta::assert_snapshot!(format!("myapp_{branch}"), actual);
  }
}
