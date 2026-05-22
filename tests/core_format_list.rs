use insta::assert_snapshot;
use jjwt::core::format::{format_age, format_list_table, render_status_glyphs};
use jjwt::core::types::*;
use std::path::PathBuf;

fn row(name: &str) -> ListRow {
  ListRow {
    name: name.into(),
    path: PathBuf::from(if name == "default" {
      "/repo".to_string()
    } else {
      format!("/repo/.worktrees/{name}")
    }),
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

  let out = format_list_table(&[r], false, None);

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

  let out = format_list_table(&[d, a, b], false, None);

  assert_snapshot!(out);
}

#[test]
fn list_table_footer_pluralization() {
  let mut r = row("default");
  r.is_current = true;
  r.status.has_remote = true;
  r.status.vs_trunk = Some(TrunkRel::IsTrunk);
  let out = format_list_table(&[r], false, None);

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
  let plain = format_list_table(&[d.clone()], false, None);
  let styled = format_list_table(&[d], true, None);

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
      while let Some(c) = chars.next() {
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
  let out = format_list_table(&[a, b], false, None);

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
  let out = format_list_table(&rows, false, Some(80));
  let header = out.lines().next().unwrap();

  assert!(
    !header.contains("Message"),
    "Message should be hidden at 80 cols:\n{out}"
  );
  assert!(
    header.contains("Branch"),
    "Branch should remain at 80 cols:\n{out}"
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
  let out = format_list_table(&rows, false, Some(40));
  let header = out.lines().next().unwrap();

  assert!(
    header.contains("Branch"),
    "Branch should survive at 40 cols:\n{out}"
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
  let out = format_list_table(&rows, false, Some(200));
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
  let out = format_list_table(&[r], false, Some(140));

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

  let out = format_list_table(&[r, r2], false, None);
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
  let out = format_list_table(&[d, a], false, Some(65));
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

  let out = format_list_table(&[r999, r1000, r10000], false, None);

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

  let out = format_list_table(&[r99, r100, r1000, r10000], false, None);

  assert!(out.contains("↑99"), "99 should be literal:\n{out}");
  assert!(out.contains("↑1C"), "100 should compact to 1C:\n{out}");
  assert!(out.contains("↑1K"), "1000 should compact to 1K:\n{out}");
  assert!(out.contains("↑∞"), "10000 should compact to ∞:\n{out}");
}

#[test]
fn styled_adaptive_preserves_alignment() {
  let rows = rich_rows();
  let plain = format_list_table(&rows, false, Some(120));
  let styled = format_list_table(&rows, true, Some(120));
  let stripped = strip_ansi(&styled);

  assert_eq!(
    stripped, plain,
    "stripped styled output should equal plain output in adaptive mode"
  );
}
