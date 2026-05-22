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

  let out = format_list_table(&[r], false);

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

  let out = format_list_table(&[d, a, b], false);

  assert_snapshot!(out);
}

#[test]
fn list_table_footer_pluralization() {
  let mut r = row("default");
  r.is_current = true;
  r.status.has_remote = true;
  r.status.vs_trunk = Some(TrunkRel::IsTrunk);
  let out = format_list_table(&[r], false);

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
  let plain = format_list_table(&[d.clone()], false);
  let styled = format_list_table(&[d], true);

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
  let out = format_list_table(&[a, b], false);

  assert!(
    out.ends_with("○ Showing 2 worktrees\n"),
    "expected no `with changes` or `ahead` clauses; got: {out}"
  );
}
