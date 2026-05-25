use jjwt::core::config::parse;

#[test]
fn accepts_scalar_hook_shape() {
  let src = r#"
pre-start = "npm install"
"#;
  let cfg = parse(src).expect("parse ok");

  assert_eq!(cfg.pre_start.len(), 1);
  assert_eq!(cfg.pre_start[0].len(), 1);
  assert_eq!(cfg.pre_start[0].get("default").unwrap(), "npm install");
  assert_eq!(cfg.pre_remove.len(), 0);
}

#[test]
fn accepts_single_table_hook_shape() {
  let src = r#"
[pre-start]
install = "npm ci"
build = "npm run build"
"#;
  let cfg = parse(src).expect("parse ok");

  assert_eq!(cfg.pre_start.len(), 1);
  let g = &cfg.pre_start[0];
  let keys: Vec<&str> = g.keys().map(|s| s.as_str()).collect();
  assert_eq!(keys, vec!["install", "build"]);
}

#[test]
fn accepts_array_of_tables_hook_shape() {
  let src = r#"
[[pre-start]]
install = "npm ci"

[[pre-start]]
build = "npm run build"
"#;
  let cfg = parse(src).expect("parse ok");

  assert_eq!(cfg.pre_start.len(), 2);
  assert_eq!(cfg.pre_start[0].get("install").unwrap(), "npm ci");
  assert_eq!(cfg.pre_start[1].get("build").unwrap(), "npm run build");
}

#[test]
fn missing_hook_field_yields_empty_vec() {
  let cfg = parse("").expect("parse ok");

  assert!(cfg.pre_start.is_empty());
  assert!(cfg.pre_remove.is_empty());
}

#[test]
fn parses_minimal_config() {
  let src = std::fs::read_to_string("fixtures/minimal.wt.toml").unwrap();
  let cfg = parse(&src).expect("parse ok");
  let list = cfg.list.as_ref().expect("list present");
  assert!(list.url.contains("hash_port"));
  assert_eq!(cfg.pre_start.len(), 2);
  let first = &cfg.pre_start[0];
  let keys: Vec<&str> = first.keys().map(|s| s.as_str()).collect();
  assert_eq!(
    keys,
    vec!["direnv", "envrc"],
    "must preserve declaration order"
  );
  assert_eq!(cfg.pre_start[1].get("db").unwrap(), "make db-start");
  assert_eq!(cfg.pre_remove.len(), 1);
  assert_eq!(cfg.pre_remove[0].get("db_stop").unwrap(), "make db-stop");
}