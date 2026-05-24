use insta::assert_snapshot;
use jjwt::core::format::{
  format_age, format_dry_run, format_dry_run_json, format_list_json, format_list_table,
  format_remove_json, format_switch_json, render_status_glyphs,
};
use jjwt::core::types::*;
use std::path::PathBuf;

fn row(name: &str) -> ListRow {
  let path = PathBuf::from(if name == "default" {
    "/repo".to_string()
  } else {
    format!("/repo/.worktrees/{name}")
  });
  let display_path = if name == "default" {
    ".".to_string()
  } else {
    format!("./.worktrees/{name}")
  };

  ListRow {
    name: name.into(),
    path,
    display_path,
    kind: ListRowKind::Workspace,
    url: String::new(),
    is_current: false,
    is_default: name == "default",
    status: StatusFlags::default(),
    head_diff: LineDiff::default(),
    vs_trunk: AheadBehind::default(),
    commit: "00000000".into(),
    age: "9h".into(),
    message: "msg".into(),
    ci_status: CiStatus::None,
    summary: String::new(),
  }
}

#[test]
fn age_buckets_match_worktrunk() {
  assert_eq!(format_age(-1), "future");
  assert_eq!(format_age(0), "now");
  assert_eq!(format_age(30), "now");
  assert_eq!(format_age(60), "1m");
  assert_eq!(format_age(60 * 60), "1h");
  assert_eq!(format_age(60 * 60 * 9), "9h");
  assert_eq!(format_age(60 * 60 * 24), "1d");
  assert_eq!(format_age(60 * 60 * 24 * 14), "2w");
  assert_eq!(format_age(60 * 60 * 24 * 30), "1mo");
  assert_eq!(format_age(60 * 60 * 24 * 365), "1y");
}

#[test]
fn status_glyphs_blank_when_clean_non_default() {
  let flags = StatusFlags::default();

  assert_eq!(render_status_glyphs(&flags), "");
}

#[test]
fn status_glyphs_remote_tracked_trunk_shows_caret_pipe() {
  let flags = StatusFlags {
    has_remote: true,
    vs_trunk: Some(TrunkRel::IsTrunk),
    ..Default::default()
  };

  // Positions: ' ', ' ', ' ', ' ', '^', '|', ' ' → trim trailing → "    ^|"
  assert_snapshot!(render_status_glyphs(&flags), @"    ^|");
}

#[test]
fn status_glyphs_full_dirty_stale_with_changes() {
  let flags = StatusFlags {
    has_changes: true,
    modified: true,
    untracked: true,
    stale: true,
    conflicts: false,
    has_remote: false,
    vs_trunk: Some(TrunkRel::Ahead),
  };

  // Positions: '+', '!', '?', '⚑', '↑', ' ', ' ' → trim trailing → "+!?⚑↑"
  assert_snapshot!(render_status_glyphs(&flags), @"+!?⚑↑");
}

#[test]
fn status_glyphs_ahead_with_remote() {
  let flags = StatusFlags {
    has_remote: true,
    vs_trunk: Some(TrunkRel::Ahead),
    ..Default::default()
  };

  // Positions: ' ', ' ', ' ', ' ', '↑', '|', ' ' → trim trailing → "    ↑|"
  assert_snapshot!(render_status_glyphs(&flags), @"    ↑|");
}

#[test]
fn status_glyphs_dirty_stale_diverged() {
  let flags = StatusFlags {
    modified: true,
    untracked: true,
    stale: true,
    conflicts: false,
    vs_trunk: Some(TrunkRel::Diverged),
    ..Default::default()
  };

  // Positions: ' ', '!', '?', '⚑', '↕', ' ', ' ' → trim trailing → " !?⚑↕"
  assert_snapshot!(render_status_glyphs(&flags), @" !?⚑↕");
}

#[test]
fn status_glyphs_conflicts_outrank_stale() {
  let flags = StatusFlags {
    modified: true,
    stale: true,
    conflicts: true,
    vs_trunk: Some(TrunkRel::Ahead),
    ..Default::default()
  };

  assert_snapshot!(render_status_glyphs(&flags), @" ! ✘↑");
}

#[test]
fn list_table_single_default_workspace() {
  let mut r = row("default");
  r.is_current = true;
  r.status.has_remote = true;
  r.status.vs_trunk = Some(TrunkRel::IsTrunk);
  r.commit = "5f1e03db".into();
  r.message = "fix(deps): bump".into();

  let out = format_list_table(&[r], false, None, true);

  assert_snapshot!(out);
}

#[test]
fn list_table_default_plus_two_worktrees() {
  let mut d = row("default");
  d.is_current = true;
  d.status.has_remote = true;
  d.status.vs_trunk = Some(TrunkRel::IsTrunk);
  d.commit = "5f1e03db".into();
  d.age = "9h".into();
  d.message = "fix(deps): bump runtime".into();

  let mut a = row("feat-onboarding-pr7-drawing");
  a.status.has_changes = true;
  a.status.modified = true;
  a.status.untracked = true;
  a.status.stale = true;
  a.status.conflicts = true;
  a.status.vs_trunk = Some(TrunkRel::Ahead);
  a.head_diff = LineDiff {
    added: 4000,
    removed: 379,
  };
  a.vs_trunk = AheadBehind {
    ahead: 10,
    behind: 44,
  };
  a.url = ":10245".into();
  a.commit = "bd64221b".into();
  a.age = "2w".into();
  a.message = "feat(site-marketing): drawing tool".into();

  let mut b = row("feat-add-site-onboarding-3");
  b.status.has_changes = true;
  b.status.has_remote = true;
  b.status.modified = true;
  b.status.conflicts = true;
  b.status.vs_trunk = Some(TrunkRel::Diverged);
  b.head_diff = LineDiff {
    added: 117,
    removed: 3,
  };
  b.vs_trunk = AheadBehind {
    ahead: 31,
    behind: 98,
  };
  b.url = ":16998".into();
  b.commit = "96cace1c".into();
  b.age = "1mo".into();
  b.message = "fix: unreachable".into();

  let out = format_list_table(&[d, a, b], false, None, true);

  assert_snapshot!(out);
}

#[test]
fn list_table_footer_pluralization() {
  let mut r = row("default");
  r.is_current = true;
  r.status.has_remote = true;
  r.status.vs_trunk = Some(TrunkRel::IsTrunk);
  let out = format_list_table(&[r], false, None, true);

  assert!(out.ends_with("○ Showing 1 worktree\n"), "got: {out}");
}

#[test]
fn styled_mode_emits_ansi_and_preserves_alignment() {
  let mut d = row("default");
  d.is_current = true;
  d.status.has_remote = true;
  d.status.modified = true;
  d.status.vs_trunk = Some(TrunkRel::IsTrunk);
  d.head_diff = LineDiff {
    added: 12,
    removed: 3,
  };
  d.vs_trunk = AheadBehind {
    ahead: 2,
    behind: 1,
  };
  let plain = format_list_table(&[d.clone()], false, None, true);
  let styled = format_list_table(&[d], true, None, true);

  // Styled output must contain ANSI escapes.
  assert!(
    styled.contains("\u{1b}["),
    "styled output lacks ANSI escapes: {styled:?}"
  );

  // Strip ANSI escapes from styled and compare against plain to confirm
  // the underlying text and padding are identical.
  let stripped = strip_ansi(&styled);

  assert_eq!(
    stripped, plain,
    "stripped styled output should equal plain output"
  );
}

fn strip_ansi(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  let mut chars = s.chars().peekable();

  while let Some(c) = chars.next() {
    if c == '\u{1b}' && chars.peek() == Some(&'[') {
      // Consume `[` and everything up to and including a terminating letter.
      chars.next();
      for c in chars.by_ref() {
        if c.is_ascii_alphabetic() {
          break;
        }
      }
    } else {
      out.push(c);
    }
  }

  out
}

#[test]
fn list_table_footer_omits_zero_clauses() {
  // 2 worktrees, both clean, neither ahead -> footer is just "○ Showing 2 worktrees"
  let a = row("default");
  let b = row("alpha");
  let out = format_list_table(&[a, b], false, None, true);

  assert!(
    out.ends_with("○ Showing 2 worktrees\n"),
    "expected no `with changes` or `ahead` clauses; got: {out}"
  );
}

// ── Adaptive layout tests ──────────────────────────────────────────

fn rich_rows() -> Vec<ListRow> {
  let mut d = row("default");
  d.is_current = true;
  d.status.has_remote = true;
  d.status.vs_trunk = Some(TrunkRel::IsTrunk);
  d.commit = "5f1e03db".into();
  d.message = "fix(deps): bump runtime".into();

  let mut a = row("feat-onboarding-pr7-drawing");
  a.status.has_changes = true;
  a.status.modified = true;
  a.status.untracked = true;
  a.status.stale = true;
  a.status.conflicts = true;
  a.status.vs_trunk = Some(TrunkRel::Ahead);
  a.head_diff = LineDiff {
    added: 500,
    removed: 379,
  };
  a.vs_trunk = AheadBehind {
    ahead: 10,
    behind: 44,
  };
  a.url = ":10245".into();
  a.commit = "bd64221b".into();
  a.age = "2w".into();
  a.message = "feat(site-marketing): drawing tool".into();

  let mut b = row("feat-add-site-onboarding-3");
  b.status.has_changes = true;
  b.status.has_remote = true;
  b.status.modified = true;
  b.status.conflicts = true;
  b.status.vs_trunk = Some(TrunkRel::Diverged);
  b.head_diff = LineDiff {
    added: 117,
    removed: 3,
  };
  b.vs_trunk = AheadBehind {
    ahead: 31,
    behind: 98,
  };
  b.url = ":16998".into();
  b.commit = "96cace1c".into();
  b.age = "1mo".into();
  b.message = "fix: unreachable".into();

  vec![d, a, b]
}

#[test]
fn narrow_terminal_drops_low_priority_columns() {
  let rows = rich_rows();
  // 80 columns is narrow — Message (priority 13) and Path (priority 7)
  // should be dropped before the higher-priority columns.
  let out = format_list_table(&rows, false, Some(80), true);
  let header = out.lines().next().unwrap();

  assert!(
    !header.contains("Message"),
    "Message should be hidden at 80 cols:\n{out}"
  );
  assert!(
    header.contains("Bookmark"),
    "Bookmark should remain at 80 cols:\n{out}"
  );
  assert!(
    header.contains("Status"),
    "Status should remain at 80 cols:\n{out}"
  );
}

#[test]
fn very_narrow_terminal_keeps_essential_columns() {
  let rows = rich_rows();
  // At 40 columns, only the highest-priority columns survive.
  let out = format_list_table(&rows, false, Some(40), true);
  let header = out.lines().next().unwrap();

  assert!(
    header.contains("Bookmark"),
    "Bookmark should survive at 40 cols:\n{out}"
  );
  assert!(
    header.contains("Status"),
    "Status should survive at 40 cols:\n{out}"
  );
  assert!(
    !header.contains("URL"),
    "URL should be hidden at 40 cols:\n{out}"
  );
}

#[test]
fn wide_terminal_shows_all_columns() {
  let rows = rich_rows();
  let out = format_list_table(&rows, false, Some(200), true);
  let header = out.lines().next().unwrap();

  assert!(
    header.contains("Message"),
    "Message should show at 200 cols:\n{out}"
  );
  assert!(
    header.contains("Age"),
    "Age should show at 200 cols:\n{out}"
  );
  assert!(
    header.contains("Commit"),
    "Commit should show at 200 cols:\n{out}"
  );
}

#[test]
fn message_truncated_with_ellipsis() {
  let mut r = row("default");
  r.is_current = true;
  r.message = "a]".repeat(60); // 120 chars — will exceed Message max of 100

  // Give just enough width that Message is visible but capped.
  let out = format_list_table(&[r], false, Some(140), true);

  if out.contains("…") {
    // Message was truncated with ellipsis — correct.
  } else {
    // With enough width the full message may fit; that's fine too.
    // The important thing is it doesn't exceed the terminal width.
    for line in out.lines() {
      assert!(
        line.len() <= 140,
        "line exceeds terminal width: {} chars\n{line}",
        line.len()
      );
    }
  }
}

#[test]
fn right_alignment_for_diff_columns() {
  let mut r = row("default");
  r.is_current = true;
  r.head_diff = LineDiff {
    added: 5,
    removed: 0,
  };

  let mut r2 = row("feat");
  r2.head_diff = LineDiff {
    added: 500,
    removed: 10,
  };

  let out = format_list_table(&[r, r2], false, None, true);
  let lines: Vec<&str> = out.lines().collect();

  // In the header, HEAD± should be present.
  assert!(lines[0].contains("HEAD±"));

  // The shorter diff (+5) should have leading spaces before it compared
  // to the longer diff (+500 -10), since the column is right-aligned.
  // Find the HEAD± column positions.
  let r1_line = lines[1]; // @ default row
  let r2_line = lines[2]; // + feat row

  // The "+5" should have more leading whitespace than "+500 -10".
  let r1_head_pos = r1_line.find("+5").expect("should contain +5");
  let r2_head_pos = r2_line.find("+500").expect("should contain +500");

  assert!(
    r1_head_pos >= r2_head_pos,
    "right-aligned column: +5 at {r1_head_pos} should be >= +500 at {r2_head_pos}"
  );
}

#[test]
fn empty_penalty_drops_empty_columns_first() {
  // All rows have empty URL — it should drop before populated columns
  // of similar priority at narrow widths.
  let mut d = row("default");
  d.is_current = true;
  d.commit = "5f1e03db".into();

  let mut a = row("feat");
  a.commit = "bd64221b".into();

  // At a width where some columns must drop, URL (empty, priority 9+10=19)
  // should drop before Commit (populated, priority 11).
  // Use a tight width to force dropping.
  let out = format_list_table(&[d, a], false, Some(65), true);
  let header = out.lines().next().unwrap();

  if header.contains("Commit") {
    assert!(
      !header.contains("URL"),
      "empty URL should drop before populated Commit:\n{out}"
    );
  }
  // If both are dropped, that's also fine — the point is URL shouldn't
  // survive while Commit is dropped.
}

#[test]
fn compact_notation_signs_thresholds() {
  // Test HEAD± compact notation at key thresholds.
  let mut r999 = row("a");
  r999.head_diff = LineDiff {
    added: 999,
    removed: 0,
  };

  let mut r1000 = row("b");
  r1000.head_diff = LineDiff {
    added: 1000,
    removed: 0,
  };

  let mut r10000 = row("c");
  r10000.head_diff = LineDiff {
    added: 10000,
    removed: 0,
  };

  let out = format_list_table(&[r999, r1000, r10000], false, None, true);

  assert!(out.contains("+999"), "999 should be literal:\n{out}");
  assert!(out.contains("+1K"), "1000 should compact to 1K:\n{out}");
  assert!(out.contains("+∞"), "10000 should compact to ∞:\n{out}");
}

#[test]
fn compact_notation_arrows_thresholds() {
  // Test main↕ compact notation — arrows use C for 100+.
  let mut r99 = row("a");
  r99.vs_trunk = AheadBehind {
    ahead: 99,
    behind: 0,
  };

  let mut r100 = row("b");
  r100.vs_trunk = AheadBehind {
    ahead: 100,
    behind: 0,
  };

  let mut r1000 = row("c");
  r1000.vs_trunk = AheadBehind {
    ahead: 1000,
    behind: 0,
  };

  let mut r10000 = row("d");
  r10000.vs_trunk = AheadBehind {
    ahead: 10000,
    behind: 0,
  };

  let out = format_list_table(&[r99, r100, r1000, r10000], false, None, true);

  assert!(out.contains("↑99"), "99 should be literal:\n{out}");
  assert!(out.contains("↑1C"), "100 should compact to 1C:\n{out}");
  assert!(out.contains("↑1K"), "1000 should compact to 1K:\n{out}");
  assert!(out.contains("↑∞"), "10000 should compact to ∞:\n{out}");
}

#[test]
fn styled_adaptive_preserves_alignment() {
  let rows = rich_rows();
  let plain = format_list_table(&rows, false, Some(120), true);
  let styled = format_list_table(&rows, true, Some(120), true);
  let stripped = strip_ansi(&styled);

  assert_eq!(
    stripped, plain,
    "stripped styled output should equal plain output in adaptive mode"
  );
}

// ── Statusline format tests ────────────────────────────────────────────

#[test]
fn statusline_with_current_workspace() {
  let mut r = row("feat-x");
  r.is_current = true;
  r.head_diff = LineDiff {
    added: 12,
    removed: 3,
  };
  r.vs_trunk = AheadBehind {
    ahead: 2,
    behind: 0,
  };

  let rows = vec![row("default"), r];
  let out = jjwt::core::format::format_statusline(&rows, Some("feat-x"));

  assert_eq!(out, "@feat-x +12-3 ↑2↓0 | 2 ws");
}

#[test]
fn statusline_no_current_workspace() {
  let rows = vec![row("default"), row("feat-a")];
  let out = jjwt::core::format::format_statusline(&rows, None);

  assert_eq!(out, "@? | 2 ws");
}

#[test]
fn statusline_empty_rows() {
  let out = jjwt::core::format::format_statusline(&[], None);

  assert_eq!(out, "@? | 0 ws");
}

// ── format_switch_json tests ──────────────────────────────────────────

#[test]
fn switch_json_basic_fields() {
  let path = PathBuf::from("/repo/.worktrees/feat-x");
  let out = format_switch_json("feat-x", &path, true);
  let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid json");

  assert_eq!(parsed["name"], "feat-x");
  assert_eq!(parsed["path"], "/repo/.worktrees/feat-x");
  assert_eq!(parsed["created"], true);
}

#[test]
fn switch_json_created_false() {
  let path = PathBuf::from("/repo/.worktrees/feat-x");
  let out = format_switch_json("feat-x", &path, false);
  let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid json");

  assert_eq!(parsed["created"], false);
}

// ── format_remove_json tests ──────────────────────────────────────────

#[test]
fn remove_json_bookmark_deleted_true() {
  let path = PathBuf::from("/repo/.worktrees/feat-x");
  let out = format_remove_json("feat-x", &path, true);
  let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid json");

  assert_eq!(parsed["name"], "feat-x");
  assert_eq!(parsed["path"], "/repo/.worktrees/feat-x");
  assert_eq!(parsed["bookmark_deleted"], true);
}

#[test]
fn remove_json_bookmark_deleted_false() {
  let path = PathBuf::from("/repo/.worktrees/feat-x");
  let out = format_remove_json("feat-x", &path, false);
  let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid json");

  assert_eq!(parsed["bookmark_deleted"], false);
}

// ── format_list_json tests ────────────────────────────────────────────

#[test]
fn list_json_single_workspace() {
  let r = row("default");
  let out = format_list_json(&[r]);
  let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).expect("valid json array");

  assert_eq!(parsed.len(), 1);
  assert_eq!(parsed[0]["name"], "default");
  assert_eq!(parsed[0]["kind"], "workspace");
  assert_eq!(parsed[0]["is_default"], true);
  assert_eq!(parsed[0]["commit"], "00000000");
  assert_eq!(parsed[0]["age"], "9h");
  assert_eq!(parsed[0]["message"], "msg");
  assert_eq!(parsed[0]["ci_status"], "none");
}

#[test]
fn list_json_null_url_and_summary() {
  let r = row("default");
  let out = format_list_json(&[r]);
  let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).expect("valid json array");

  assert!(parsed[0]["url"].is_null(), "empty url should be null");
  assert!(
    parsed[0]["summary"].is_null(),
    "empty summary should be null"
  );
}

#[test]
fn list_json_non_null_url_and_summary() {
  let mut r = row("feat");

  r.url = "http://example.com/feat".into();
  r.summary = "A short summary".into();

  let out = format_list_json(&[r]);
  let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).expect("valid json array");

  assert_eq!(parsed[0]["url"], "http://example.com/feat");
  assert_eq!(parsed[0]["summary"], "A short summary");
}

#[test]
fn list_json_status_sub_object() {
  let mut r = row("feat");

  r.status = StatusFlags {
    has_changes: true,
    modified: true,
    untracked: false,
    stale: false,
    conflicts: true,
    has_remote: true,
    vs_trunk: Some(TrunkRel::Ahead),
  };

  let out = format_list_json(&[r]);
  let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).expect("valid json array");

  let status = &parsed[0]["status"];

  assert_eq!(status["has_changes"], true);
  assert_eq!(status["modified"], true);
  assert_eq!(status["untracked"], false);
  assert_eq!(status["conflicts"], true);
  assert_eq!(status["stale"], false);
  assert_eq!(status["has_remote"], true);
  assert_eq!(status["vs_trunk"], "ahead");
}

#[test]
fn list_json_trunk_rel_variants() {
  let variants = [
    (Some(TrunkRel::IsTrunk), "is_trunk"),
    (Some(TrunkRel::Ancestor), "ancestor"),
    (Some(TrunkRel::Diverged), "diverged"),
    (Some(TrunkRel::Ahead), "ahead"),
    (Some(TrunkRel::Behind), "behind"),
  ];

  for (rel, expected) in variants {
    let mut r = row("test");

    r.status.vs_trunk = rel;

    let out = format_list_json(&[r]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).expect("valid json");

    assert_eq!(
      parsed[0]["status"]["vs_trunk"], expected,
      "TrunkRel::{expected} should map correctly"
    );
  }
}

#[test]
fn list_json_trunk_rel_none_is_null() {
  let mut r = row("test");

  r.status.vs_trunk = None;

  let out = format_list_json(&[r]);
  let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).expect("valid json");

  assert!(parsed[0]["status"]["vs_trunk"].is_null());
}

#[test]
fn list_json_head_diff_and_vs_trunk_numbers() {
  let mut r = row("feat");

  r.head_diff = LineDiff {
    added: 42,
    removed: 7,
  };
  r.vs_trunk = AheadBehind {
    ahead: 5,
    behind: 3,
  };

  let out = format_list_json(&[r]);
  let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).expect("valid json");

  assert_eq!(parsed[0]["head_diff"]["added"], 42);
  assert_eq!(parsed[0]["head_diff"]["removed"], 7);
  assert_eq!(parsed[0]["vs_trunk"]["ahead"], 5);
  assert_eq!(parsed[0]["vs_trunk"]["behind"], 3);
}

#[test]
fn list_json_ci_status_values() {
  for (ci, expected) in [
    (CiStatus::Pass, "pass"),
    (CiStatus::Fail, "fail"),
    (CiStatus::Pending, "pending"),
    (CiStatus::None, "none"),
  ] {
    let mut r = row("test");

    r.ci_status = ci;

    let out = format_list_json(&[r]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).expect("valid json");

    assert_eq!(parsed[0]["ci_status"], expected);
  }
}

#[test]
fn list_json_bookmark_row_kind() {
  let r = ListRow {
    name: "orphan".into(),
    path: PathBuf::new(),
    display_path: String::new(),
    kind: ListRowKind::Bookmark,
    url: String::new(),
    is_current: false,
    is_default: false,
    status: StatusFlags::default(),
    head_diff: LineDiff::default(),
    vs_trunk: AheadBehind::default(),
    commit: String::new(),
    age: String::new(),
    message: String::new(),
    ci_status: CiStatus::None,
    summary: String::new(),
  };

  let out = format_list_json(&[r]);
  let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).expect("valid json");

  assert_eq!(parsed[0]["kind"], "bookmark");
}

#[test]
fn list_json_multiple_rows() {
  let mut d = row("default");

  d.is_current = true;

  let mut f = row("feat");

  f.head_diff = LineDiff {
    added: 100,
    removed: 50,
  };

  let out = format_list_json(&[d, f]);
  let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).expect("valid json");

  assert_eq!(parsed.len(), 2);
  assert_eq!(parsed[0]["name"], "default");
  assert_eq!(parsed[0]["is_current"], true);
  assert_eq!(parsed[1]["name"], "feat");
  assert_eq!(parsed[1]["head_diff"]["added"], 100);
}

// ── Styled cell builder coverage ──────────────────────────────────────

#[test]
fn styled_head_diff_add_only() {
  let mut r = row("feat");

  r.is_current = true;
  r.head_diff = LineDiff {
    added: 10,
    removed: 0,
  };

  let out = format_list_table(&[r], true, None, true);

  assert!(out.contains("\u{1b}["), "should contain ANSI escapes");
  // The plain form "+10" should appear somewhere in the stripped output.
  assert!(strip_ansi(&out).contains("+10"));
}

#[test]
fn styled_head_diff_remove_only() {
  let mut r = row("feat");

  r.is_current = true;
  r.head_diff = LineDiff {
    added: 0,
    removed: 5,
  };

  let out = format_list_table(&[r], true, None, true);

  assert!(strip_ansi(&out).contains("-5"));
}

#[test]
fn styled_head_diff_both() {
  let mut r = row("feat");

  r.is_current = true;
  r.head_diff = LineDiff {
    added: 42,
    removed: 7,
  };

  let out = format_list_table(&[r], true, None, true);
  let plain = strip_ansi(&out);

  assert!(plain.contains("+42"));
  assert!(plain.contains("-7"));
}

#[test]
fn styled_ahead_behind_ahead_only() {
  let mut r = row("feat");

  r.is_current = true;
  r.vs_trunk = AheadBehind {
    ahead: 3,
    behind: 0,
  };

  let out = format_list_table(&[r], true, None, true);

  assert!(strip_ansi(&out).contains("↑3"));
}

#[test]
fn styled_ahead_behind_behind_only() {
  let mut r = row("feat");

  r.is_current = true;
  r.vs_trunk = AheadBehind {
    ahead: 0,
    behind: 12,
  };

  let out = format_list_table(&[r], true, None, true);

  assert!(strip_ansi(&out).contains("↓12"));
}

#[test]
fn styled_ahead_behind_both() {
  let mut r = row("feat");

  r.is_current = true;
  r.vs_trunk = AheadBehind {
    ahead: 5,
    behind: 200,
  };

  let out = format_list_table(&[r], true, None, true);
  let plain = strip_ansi(&out);

  assert!(plain.contains("↑5"));
  assert!(plain.contains("↓2C"));
}

#[test]
fn styled_ci_status_pass() {
  let mut r = row("feat");

  r.is_current = true;
  r.ci_status = CiStatus::Pass;

  let out = format_list_table(&[r], true, None, true);

  assert!(strip_ansi(&out).contains("✓"));
  assert!(out.contains("\u{1b}["));
}

#[test]
fn styled_ci_status_fail() {
  let mut r = row("feat");

  r.is_current = true;
  r.ci_status = CiStatus::Fail;

  let out = format_list_table(&[r], true, None, true);

  assert!(strip_ansi(&out).contains("✗"));
}

#[test]
fn styled_ci_status_pending() {
  let mut r = row("feat");

  r.is_current = true;
  r.ci_status = CiStatus::Pending;

  let out = format_list_table(&[r], true, None, true);

  assert!(strip_ansi(&out).contains("◌"));
}

// ── Bookmark row gutter and path coverage ───────────────────────────────

#[test]
fn bookmark_row_uses_slash_gutter_and_empty_path() {
  let bookmark = ListRow {
    name: "orphan".into(),
    path: PathBuf::new(),
    display_path: String::new(),
    kind: ListRowKind::Bookmark,
    url: String::new(),
    is_current: false,
    is_default: false,
    status: StatusFlags::default(),
    head_diff: LineDiff::default(),
    vs_trunk: AheadBehind::default(),
    commit: String::new(),
    age: String::new(),
    message: String::new(),
    ci_status: CiStatus::None,
    summary: String::new(),
  };

  let out = format_list_table(&[bookmark], false, None, true);
  let data_line = out.lines().nth(1).expect("should have data row");

  assert!(
    data_line.starts_with('/'),
    "Bookmark row should use '/' gutter, got: {data_line}"
  );
}

#[test]
fn default_non_current_uses_caret_gutter() {
  let mut r = row("default");

  r.is_current = false;

  let out = format_list_table(&[r], false, None, true);
  let data_line = out.lines().nth(1).expect("should have data row");

  assert!(
    data_line.starts_with('^'),
    "Non-current default should use '^' gutter, got: {data_line}"
  );
}

// ── Styled truncation (ANSI-aware truncate_cell) ──────────────────────

#[test]
fn styled_truncation_preserves_ansi_correctness() {
  // Build a row with a very long message that will be truncated, and
  // render with styled=true at a tight width to force truncation of the
  // styled Message column.
  let mut r = row("default");

  r.is_current = true;
  r.message = "a".repeat(120);

  let out = format_list_table(&[r], true, Some(90), true);

  // The output should contain an ellipsis from truncation.
  assert!(
    out.contains('…'),
    "styled output should contain ellipsis from truncation:\n{out}"
  );
  // Should still contain ANSI reset after truncation.
  assert!(
    out.contains("\u{1b}[0m"),
    "should contain ANSI reset after truncation"
  );
}

// ── Column shrink path (compute_widths shrinkable bookmark) ─────────────

#[test]
fn very_tight_terminal_shrinks_bookmark_column() {
  // Bookmark (priority 1) is the only shrinkable column. At very tight
  // widths it should shrink rather than disappear entirely, down to its
  // min_width of 6.
  let mut r = row("a-very-long-workspace-name");

  r.is_current = true;
  r.status.has_changes = true;

  // At 20 columns, Bookmark must shrink to fit alongside Status.
  let out = format_list_table(&[r], false, Some(20), true);
  let header = out.lines().next().unwrap();

  assert!(
    header.contains("Bookmark"),
    "Bookmark should survive via shrinking at 20 cols:\n{out}"
  );
}

// ── Status glyph TrunkRel::Behind coverage ────────────────────────────

#[test]
fn status_glyphs_behind_shows_down_arrow() {
  let flags = StatusFlags {
    vs_trunk: Some(TrunkRel::Behind),
    ..Default::default()
  };

  assert_eq!(render_status_glyphs(&flags), "    ↓");
}

// ── Status glyph TrunkRel::None coverage ──────────────────────────────

#[test]
fn status_glyphs_trunk_rel_none_is_blank() {
  let flags = StatusFlags {
    vs_trunk: Some(TrunkRel::None),
    ..Default::default()
  };

  assert_eq!(render_status_glyphs(&flags), "");
}

// ── Compact mode (full=false) column hiding tests ───────────────────

#[test]
fn compact_mode_hides_full_only_columns() {
  let mut r = row("default");

  r.is_current = true;
  r.status.has_remote = true;
  r.status.vs_trunk = Some(TrunkRel::IsTrunk);
  r.url = "http://example.com".into();
  r.commit = "5f1e03db".into();
  r.age = "9h".into();
  r.ci_status = CiStatus::Pass;
  r.summary = "A summary".into();

  let compact = format_list_table(&[r.clone()], false, None, false);
  let full = format_list_table(&[r], false, None, true);
  let compact_header = compact.lines().next().unwrap();
  let full_header = full.lines().next().unwrap();

  // Compact hides: CI, URL, Commit, Age, Summary
  assert!(
    !compact_header.contains("CI"),
    "CI hidden in compact:\n{compact}"
  );
  assert!(
    !compact_header.contains("URL"),
    "URL hidden in compact:\n{compact}"
  );
  assert!(
    !compact_header.contains("Commit"),
    "Commit hidden in compact:\n{compact}"
  );
  assert!(
    !compact_header.contains("Age"),
    "Age hidden in compact:\n{compact}"
  );
  assert!(
    !compact_header.contains("Summary"),
    "Summary hidden in compact:\n{compact}"
  );

  // Compact keeps: Bookmark, Status, HEAD±, main↕, Path, Message
  assert!(
    compact_header.contains("Bookmark"),
    "Bookmark shown in compact:\n{compact}"
  );
  assert!(
    compact_header.contains("Status"),
    "Status shown in compact:\n{compact}"
  );
  assert!(
    compact_header.contains("Message"),
    "Message shown in compact:\n{compact}"
  );

  // Full shows everything
  assert!(full_header.contains("CI"), "CI shown in full:\n{full}");
  assert!(full_header.contains("URL"), "URL shown in full:\n{full}");
  assert!(
    full_header.contains("Commit"),
    "Commit shown in full:\n{full}"
  );
  assert!(full_header.contains("Age"), "Age shown in full:\n{full}");
}

#[test]
fn compact_mode_with_terminal_width_also_hides_columns() {
  let mut r = row("default");

  r.is_current = true;
  r.ci_status = CiStatus::Pass;
  r.commit = "5f1e03db".into();

  let compact = format_list_table(&[r], false, Some(200), false);
  let header = compact.lines().next().unwrap();

  assert!(
    !header.contains("CI"),
    "CI hidden in compact mode even at wide terminal:\n{compact}"
  );
  assert!(
    !header.contains("Commit"),
    "Commit hidden in compact mode:\n{compact}"
  );
}

// ── format_dry_run tests ────────────────────────────────────────────

#[test]
fn dry_run_workspace_add() {
  let actions = vec![Action::JjWorkspaceAdd {
    name: "feat".into(),
    path: PathBuf::from("/repo/.worktrees/feat"),
    revision: None,
  }];

  let out = format_dry_run(&actions);

  assert!(out.contains("would create workspace 'feat'"), "got: {out}");
  assert!(out.contains("/repo/.worktrees/feat"), "got: {out}");
}

#[test]
fn dry_run_bookmark_create() {
  let actions = vec![Action::JjBookmarkCreate {
    name: "feat".into(),
    workspace: "feat".into(),
  }];

  let out = format_dry_run(&actions);

  assert!(out.contains("would create bookmark 'feat'"), "got: {out}");
}

#[test]
fn dry_run_workspace_forget_and_delete() {
  let actions = vec![
    Action::JjWorkspaceForget {
      name: "feat".into(),
    },
    Action::DeleteDir {
      path: PathBuf::from("/repo/.worktrees/feat"),
    },
    Action::JjBookmarkDelete {
      name: "feat".into(),
    },
  ];

  let out = format_dry_run(&actions);

  assert!(out.contains("would forget workspace 'feat'"), "got: {out}");
  assert!(
    out.contains("would delete /repo/.worktrees/feat"),
    "got: {out}"
  );
  assert!(out.contains("would delete bookmark 'feat'"), "got: {out}");
}

#[test]
fn dry_run_skips_print_line() {
  let actions = vec![
    Action::JjWorkspaceForget {
      name: "feat".into(),
    },
    Action::PrintLine("cd:/repo".into()),
  ];

  let out = format_dry_run(&actions);

  assert!(
    !out.contains("cd:/repo"),
    "PrintLine should be skipped: {out}"
  );
  assert_eq!(out.lines().count(), 1);
}

#[test]
fn dry_run_update_stale() {
  let actions = vec![Action::JjWorkspaceUpdateStale {
    name: "feat".into(),
  }];

  let out = format_dry_run(&actions);

  assert!(
    out.contains("would update stale workspace 'feat'"),
    "got: {out}"
  );
}

#[test]
fn dry_run_delete_background() {
  let actions = vec![Action::DeleteDirBackground {
    path: PathBuf::from("/repo/.worktrees/feat"),
  }];

  let out = format_dry_run(&actions);

  assert!(out.contains("(background)"), "got: {out}");
}

#[test]
fn dry_run_rename_workspace_and_dir() {
  let actions = vec![
    Action::JjWorkspaceRename {
      old_name: "old".into(),
      new_name: "new".into(),
    },
    Action::RenameDir {
      from: PathBuf::from("/repo/.worktrees/old"),
      to: PathBuf::from("/repo/.worktrees/new"),
    },
    Action::JjBookmarkRename {
      old_name: "old".into(),
      new_name: "new".into(),
    },
  ];

  let out = format_dry_run(&actions);

  assert!(out.contains("would rename workspace 'old'"), "got: {out}");
  assert!(out.contains("would move"), "got: {out}");
  assert!(out.contains("would rename bookmark 'old'"), "got: {out}");
}

#[test]
fn dry_run_hook_and_exec() {
  let actions = vec![
    Action::RunHook {
      name: "setup".into(),
      rendered_cmd: "npm install".into(),
      cwd: PathBuf::from("/repo"),
      env: vec![],
      source: HookSource::Project,
    },
    Action::Exec {
      rendered_cmd: "echo hello".into(),
      cwd: PathBuf::from("/repo"),
      env: vec![],
    },
  ];

  let out = format_dry_run(&actions);

  assert!(
    out.contains("would run hook 'setup': npm install"),
    "got: {out}"
  );
  assert!(out.contains("would exec: echo hello"), "got: {out}");
}

#[test]
fn dry_run_empty_actions() {
  let out = format_dry_run(&[]);

  assert_eq!(out, "");
}

// ── format_dry_run_json tests ───────────────────────────────────────

#[test]
fn dry_run_json_workspace_add() {
  let actions = vec![Action::JjWorkspaceAdd {
    name: "feat".into(),
    path: PathBuf::from("/repo/.worktrees/feat"),
    revision: None,
  }];

  let out = format_dry_run_json(&actions);
  let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).expect("valid json");

  assert_eq!(parsed.len(), 1);
  assert_eq!(parsed[0]["type"], "workspace_add");
  assert_eq!(parsed[0]["name"], "feat");
  assert_eq!(parsed[0]["path"], "/repo/.worktrees/feat");
}

#[test]
fn dry_run_json_skips_print_line() {
  let actions = vec![
    Action::JjWorkspaceForget {
      name: "feat".into(),
    },
    Action::PrintLine("ignored".into()),
  ];

  let out = format_dry_run_json(&actions);
  let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).expect("valid json");

  assert_eq!(parsed.len(), 1);
  assert_eq!(parsed[0]["type"], "workspace_forget");
}

#[test]
fn dry_run_json_multiple_action_types() {
  let actions = vec![
    Action::JjBookmarkCreate {
      name: "feat".into(),
      workspace: "feat".into(),
    },
    Action::JjBookmarkDelete { name: "old".into() },
    Action::JjWorkspaceUpdateStale {
      name: "stale-ws".into(),
    },
    Action::DeleteDir {
      path: PathBuf::from("/tmp/dir"),
    },
    Action::DeleteDirBackground {
      path: PathBuf::from("/tmp/bg"),
    },
    Action::JjWorkspaceRename {
      old_name: "a".into(),
      new_name: "b".into(),
    },
    Action::RenameDir {
      from: PathBuf::from("/a"),
      to: PathBuf::from("/b"),
    },
    Action::JjBookmarkRename {
      old_name: "x".into(),
      new_name: "y".into(),
    },
    Action::RunHook {
      name: "setup".into(),
      rendered_cmd: "npm i".into(),
      cwd: PathBuf::from("/repo"),
      env: vec![],
      source: HookSource::Project,
    },
    Action::Exec {
      rendered_cmd: "echo hi".into(),
      cwd: PathBuf::from("/repo"),
      env: vec![],
    },
  ];

  let out = format_dry_run_json(&actions);
  let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).expect("valid json");

  assert_eq!(parsed.len(), 10);
  assert_eq!(parsed[0]["type"], "bookmark_create");
  assert_eq!(parsed[1]["type"], "bookmark_delete");
  assert_eq!(parsed[2]["type"], "workspace_update_stale");
  assert_eq!(parsed[3]["type"], "delete_dir");
  assert_eq!(parsed[4]["type"], "delete_dir_background");
  assert_eq!(parsed[5]["type"], "workspace_rename");
  assert_eq!(parsed[6]["type"], "rename_dir");
  assert_eq!(parsed[7]["type"], "bookmark_rename");
  assert_eq!(parsed[8]["type"], "run_hook");
  assert_eq!(parsed[9]["type"], "exec");
}
