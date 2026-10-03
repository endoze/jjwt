use jjwt::core::types::Trunk;

#[test]
fn parses_commit_and_first_remote_bookmark_name() {
  let t = Trunk::parse("abc123\tmain main\n").unwrap();

  assert_eq!(
    t,
    Trunk {
      name: "main".into(),
      commit_id: "abc123".into(),
    }
  );
}

#[test]
fn falls_back_to_trunk_label_without_bookmarks() {
  assert_eq!(Trunk::parse("abc123\t\n").unwrap().name, "trunk");
}

#[test]
fn empty_output_means_no_trunk() {
  assert_eq!(Trunk::parse(""), None);
}

#[test]
fn prefers_conventional_trunk_name_over_alphabetical_order() {
  let t = Trunk::parse("abc123\tdependabot/x main\n").unwrap();

  assert_eq!(t.name, "main");
}

#[test]
fn keeps_first_name_when_none_is_conventional() {
  assert_eq!(Trunk::parse("abc123\tdev release\n").unwrap().name, "dev");
}
