use jjwt::core::config::parse;

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
