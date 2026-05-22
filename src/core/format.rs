use crate::core::types::{
  AheadBehind, CiStatus, LineDiff, ListRow, ListRowKind, StatusFlags, TrunkRel,
};
use anstyle::{AnsiColor, Color, Style};
use serde_json::{Map, Value, json};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const COL_SEP: &str = "  ";
const COL_SEP_WIDTH: usize = 2;
const GUTTER_WIDTH: usize = 2;
const EMPTY_PENALTY: u8 = 10;

const HEADERS: &[&str] = &[
  "Branch", "Status", "HEAD±", "main↕", "CI", "Path", "URL", "Commit", "Age", "Message", "Summary",
];

#[derive(Clone, Copy, PartialEq)]
enum Align {
  Left,
  Right,
}

struct ColSpec {
  priority: u8,
  shrinkable: bool,
  min_width: Option<usize>,
  max_width: Option<usize>,
  align: Align,
  truncatable: bool,
}

const COL_SPECS: [ColSpec; 11] = [
  // Branch
  ColSpec {
    priority: 1,
    shrinkable: true,
    min_width: Some(6),
    max_width: None,
    align: Align::Left,
    truncatable: false,
  },
  // Status
  ColSpec {
    priority: 2,
    shrinkable: false,
    min_width: None,
    max_width: None,
    align: Align::Left,
    truncatable: false,
  },
  // HEAD±
  ColSpec {
    priority: 3,
    shrinkable: false,
    min_width: None,
    max_width: None,
    align: Align::Right,
    truncatable: false,
  },
  // main↕
  ColSpec {
    priority: 4,
    shrinkable: false,
    min_width: None,
    max_width: None,
    align: Align::Right,
    truncatable: false,
  },
  // CI
  ColSpec {
    priority: 5,
    shrinkable: false,
    min_width: None,
    max_width: None,
    align: Align::Left,
    truncatable: false,
  },
  // Path
  ColSpec {
    priority: 7,
    shrinkable: false,
    min_width: None,
    max_width: None,
    align: Align::Left,
    truncatable: true,
  },
  // URL
  ColSpec {
    priority: 9,
    shrinkable: false,
    min_width: None,
    max_width: None,
    align: Align::Left,
    truncatable: true,
  },
  // Commit
  ColSpec {
    priority: 11,
    shrinkable: false,
    min_width: None,
    max_width: None,
    align: Align::Left,
    truncatable: false,
  },
  // Age
  ColSpec {
    priority: 12,
    shrinkable: false,
    min_width: None,
    max_width: None,
    align: Align::Left,
    truncatable: false,
  },
  // Message
  ColSpec {
    priority: 13,
    shrinkable: false,
    min_width: Some(10),
    max_width: Some(100),
    align: Align::Left,
    truncatable: true,
  },
  // Summary (LLM-generated)
  ColSpec {
    priority: 6,
    shrinkable: false,
    min_width: Some(8),
    max_width: Some(50),
    align: Align::Left,
    truncatable: true,
  },
];

const DIM: Style = Style::new().dimmed();
const BOLD: Style = Style::new().bold();
const GREEN: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green)));
const RED: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Red)));
const YELLOW: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Yellow)));
const CYAN: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));

/// One rendered cell: `plain` is used for width measurement and padding,
/// `display` is what is written out (may include ANSI escapes).
#[derive(Default, Clone)]
struct Cell {
  plain: String,
  display: String,
}

impl Cell {
  fn raw(s: String) -> Self {
    Self {
      plain: s.clone(),
      display: s,
    }
  }

  fn styled(plain: String, display: String) -> Self {
    Self { plain, display }
  }

  fn width(&self) -> usize {
    self.plain.as_str().width()
  }
}

/// Truncate a cell to fit within `max_width` visible characters, appending
/// `…` as a suffix. For styled cells, the display string is rebuilt by
/// truncating the plain text and re-wrapping with the original ANSI
/// prefix/suffix that bookend the display string.
fn truncate_cell(cell: &Cell, max_width: usize) -> Cell {
  if cell.width() <= max_width || max_width == 0 {
    return cell.clone();
  }

  let target = max_width.saturating_sub(1); // reserve 1 for …
  let mut current_width = 0;
  let mut last_idx = 0;

  for (idx, ch) in cell.plain.char_indices() {
    let char_w = ch.width().unwrap_or(0);

    if current_width + char_w > target {
      break;
    }

    current_width += char_w;
    last_idx = idx + ch.len_utf8();
  }

  let truncated_plain = cell.plain[..last_idx].trim_end();
  let new_plain = format!("{truncated_plain}…");

  // If plain == display, there are no ANSI escapes.
  if cell.plain == cell.display {
    return Cell::raw(new_plain);
  }

  // Styled cell: the display may contain multiple ANSI-wrapped segments.
  // Rebuild by truncating character-by-character, keeping escape sequences
  // intact and dropping visible characters past the limit.
  let mut new_display = String::new();
  let mut visible = 0;
  let mut chars = cell.display.chars().peekable();

  while let Some(c) = chars.next() {
    if c == '\u{1b}' && chars.peek() == Some(&'[') {
      // Copy the entire escape sequence through.
      new_display.push(c);
      new_display.push(chars.next().unwrap()); // '['

      while let Some(esc_c) = chars.next() {
        new_display.push(esc_c);

        if esc_c.is_ascii_alphabetic() {
          break;
        }
      }
    } else {
      let char_w = c.width().unwrap_or(0);

      if visible + char_w > target {
        break;
      }

      new_display.push(c);
      visible += char_w;
    }
  }

  // Close any open ANSI style and append the ellipsis.
  new_display.push_str("\u{1b}[0m…");

  Cell::styled(new_plain, new_display)
}

/// Render a worktrunk-style list table for the given rows. Includes the
/// header row, data rows, blank line, and footer summary.
///
/// When `styled` is true, output is decorated with ANSI escape sequences.
/// When `term_width` is `Some`, columns are adaptively dropped, shrunk,
/// and truncated to fit the terminal.
pub fn format_list_table(rows: &[ListRow], styled: bool, term_width: Option<u16>) -> String {
  let mut cells = build_cells(rows, styled);
  let (widths, visible) = compute_widths(&cells, term_width);

  // Truncate cells in truncatable columns that exceed their allocated width.
  for row_cells in &mut cells {
    for (i, cell) in row_cells.iter_mut().enumerate() {
      if visible[i] && COL_SPECS[i].truncatable && cell.width() > widths[i] {
        *cell = truncate_cell(cell, widths[i]);
      }
    }
  }

  let mut out = String::new();

  // Header row: gutter column is empty; then the named columns.
  out.push(' ');
  out.push(' '); // gutter (1) + separator (1)

  let header_cells: [Cell; 11] = std::array::from_fn(|i| {
    let h = HEADERS[i].to_string();

    if styled {
      Cell::styled(h.clone(), wrap(&h, BOLD))
    } else {
      Cell::raw(h)
    }
  });

  push_columns(&mut out, &header_cells, &widths, &visible);
  out.push('\n');

  for (row, row_cells) in rows.iter().zip(cells.iter()) {
    out.push(gutter_char(row));
    out.push(' ');
    push_columns(&mut out, row_cells, &widths, &visible);
    out.push('\n');
  }

  out.push('\n');
  out.push_str(&format_summary(rows, styled));
  out.push('\n');

  out
}

fn push_columns(out: &mut String, cells: &[Cell; 11], widths: &[usize; 11], visible: &[bool; 11]) {
  let mut first = true;

  for (i, cell) in cells.iter().enumerate() {
    if !visible[i] {
      continue;
    }

    if !first {
      out.push_str(COL_SEP);
    }

    first = false;
    let is_last_visible = (i + 1..11).all(|j| !visible[j]);
    let pad = widths[i].saturating_sub(cell.width());

    if COL_SPECS[i].align == Align::Right {
      for _ in 0..pad {
        out.push(' ');
      }

      out.push_str(&cell.display);

      if !is_last_visible {
        // No trailing pad needed — right-aligned columns already fill width.
      }
    } else {
      out.push_str(&cell.display);

      if !is_last_visible {
        for _ in 0..pad {
          out.push(' ');
        }
      }
    }
  }
}

fn build_cells(rows: &[ListRow], styled: bool) -> Vec<[Cell; 11]> {
  rows.iter().map(|r| build_row_cells(r, styled)).collect()
}

fn build_row_cells(r: &ListRow, styled: bool) -> [Cell; 11] {
  // Worktrunk dims rows that "should dim" (typically non-current). We
  // dim non-current rows' branch/path; commit/age/url/message are always
  // dim (metadata).
  let branch_style = if styled && r.is_current {
    Some(BOLD)
  } else {
    None
  };
  let dim_if_styled = if styled { Some(DIM) } else { None };

  [
    text_cell(&r.name, branch_style),
    status_cell(&r.status, styled),
    head_diff_cell(&r.head_diff, styled),
    ahead_behind_cell(&r.vs_trunk, styled),
    ci_status_cell(r.ci_status, styled),
    text_cell(&format_path(r), None),
    text_cell(&r.url, dim_if_styled),
    text_cell(&r.commit, dim_if_styled),
    text_cell(&r.age, dim_if_styled),
    text_cell(&r.message, dim_if_styled),
    text_cell(&r.summary, dim_if_styled),
  ]
}

fn text_cell(s: &str, style: Option<Style>) -> Cell {
  match style {
    Some(st) if !s.is_empty() => Cell::styled(s.to_string(), wrap(s, st)),
    _ => Cell::raw(s.to_string()),
  }
}

/// Compute column widths and a visibility mask. When `term_width` is
/// `None`, every column gets its ideal (natural) width. When `Some`,
/// columns are dropped by priority, shrunk, and capped to fit.
fn compute_widths(cells: &[[Cell; 11]], term_width: Option<u16>) -> ([usize; 11], [bool; 11]) {
  // Phase 1: compute ideal (natural) widths — max of header and all cells.
  let mut ideal = [0usize; 11];

  for (i, h) in HEADERS.iter().enumerate() {
    ideal[i] = h.width();
  }

  for row in cells {
    for (i, cell) in row.iter().enumerate() {
      let w = cell.width();

      if w > ideal[i] {
        ideal[i] = w;
      }
    }
  }

  let term_width = match term_width {
    Some(w) => w as usize,
    None => return (ideal, [true; 11]),
  };

  // Phase 2: compute effective priorities (empty columns get a penalty).
  let mut priorities: [(u8, usize); 11] = std::array::from_fn(|i| {
    let base = COL_SPECS[i].priority;
    let all_empty = cells.iter().all(|row| row[i].width() == 0);

    let effective = if all_empty {
      base.saturating_add(EMPTY_PENALTY)
    } else {
      base
    };

    (effective, i)
  });

  // Sort ascending by effective priority (lowest = most important).
  priorities.sort_by_key(|&(p, _)| p);

  // Phase 3: allocate in priority order.
  let mut widths = [0usize; 11];
  let mut visible = [false; 11];
  let budget = term_width.saturating_sub(GUTTER_WIDTH);
  let mut remaining = budget;

  for &(_, col) in &priorities {
    let spec = &COL_SPECS[col];

    // Cap ideal at max_width if specified.
    let want = match spec.max_width {
      Some(max) => ideal[col].min(max),
      None => ideal[col],
    };

    // Account for separator (if this isn't the first visible column).
    let sep = if visible.iter().any(|&v| v) {
      COL_SEP_WIDTH
    } else {
      0
    };

    if want + sep <= remaining {
      widths[col] = want;
      visible[col] = true;
      remaining -= want + sep;
    } else if spec.shrinkable {
      let min = spec.min_width.unwrap_or(1);

      if min + sep <= remaining {
        widths[col] = remaining - sep;
        visible[col] = true;
        remaining = 0;
      }
    } else if spec.min_width.is_some() {
      let min = spec.min_width.unwrap();

      if min + sep <= remaining {
        widths[col] = min;
        visible[col] = true;
        remaining -= min + sep;
      }
    }
    // Otherwise column is hidden (width stays 0, visible stays false).
  }

  // Phase 4: distribute remaining space to Message (the most useful
  // flexible column), up to its max_width.
  let msg_idx = 9;

  if visible[msg_idx] && remaining > 0 {
    let max = COL_SPECS[msg_idx].max_width.unwrap_or(usize::MAX);
    let expansion = remaining.min(max.saturating_sub(widths[msg_idx]));

    widths[msg_idx] += expansion;
  }

  (widths, visible)
}

fn gutter_char(row: &ListRow) -> char {
  if matches!(row.kind, ListRowKind::Branch) {
    '/'
  } else if row.is_current {
    '@'
  } else if row.is_default {
    '^'
  } else {
    '+'
  }
}

fn format_path(row: &ListRow) -> String {
  match row.kind {
    ListRowKind::Branch => String::new(),
    ListRowKind::Workspace if row.is_default => ".".to_string(),
    ListRowKind::Workspace => format!("./.worktrees/{}", row.name),
  }
}

/// Render the 7-position status column. Empty positions are filled with
/// spaces so positions align vertically across rows.
pub fn render_status_glyphs(f: &StatusFlags) -> String {
  let cell = status_cell(f, false);

  cell.plain
}

fn status_cell(f: &StatusFlags, styled: bool) -> Cell {
  // Build position-by-position. Each position is one visible char (space
  // when blank). We carry styled and plain forms in lock-step.
  let mut plain = String::new();
  let mut display = String::new();
  let push = |plain: &mut String, display: &mut String, ch: char, style: Option<Style>| {
    plain.push(ch);

    match (styled, style) {
      (true, Some(s)) => display.push_str(&wrap(&ch.to_string(), s)),
      _ => display.push(ch),
    }
  };

  // Position 0: STAGED — jj has no staging area, but we treat the `@`
  // commit's non-empty diff vs parent as the analog ("there's content
  // here").
  if f.has_changes {
    push(&mut plain, &mut display, '+', Some(GREEN));
  } else {
    push(&mut plain, &mut display, ' ', None);
  }

  // Position 1: MODIFIED
  if f.modified {
    push(&mut plain, &mut display, '!', Some(YELLOW));
  } else {
    push(&mut plain, &mut display, ' ', None);
  }

  // Position 2: UNTRACKED
  if f.untracked {
    push(&mut plain, &mut display, '?', Some(CYAN));
  } else {
    push(&mut plain, &mut display, ' ', None);
  }

  // Position 3: WORKTREE STATE — conflicts > stale > blank
  if f.conflicts {
    push(&mut plain, &mut display, '✘', Some(RED));
  } else if f.stale {
    push(&mut plain, &mut display, '⚑', Some(YELLOW));
  } else {
    push(&mut plain, &mut display, ' ', None);
  }

  // Position 4: MAIN_STATE — relationship to trunk
  match f.vs_trunk {
    Some(TrunkRel::IsTrunk) => push(&mut plain, &mut display, '^', Some(BOLD)),
    Some(TrunkRel::Ancestor) => push(&mut plain, &mut display, '⊂', Some(CYAN)),
    Some(TrunkRel::Diverged) => push(&mut plain, &mut display, '↕', Some(YELLOW)),
    Some(TrunkRel::Ahead) => push(&mut plain, &mut display, '↑', Some(GREEN)),
    Some(TrunkRel::Behind) => push(&mut plain, &mut display, '↓', Some(YELLOW)),
    Some(TrunkRel::None) | None => push(&mut plain, &mut display, ' ', None),
  }

  // Position 5: UPSTREAM_DIVERGENCE — `|` when the bookmark has a
  // remote-tracking variant (jj analog of "tracking an upstream"). Blank
  // otherwise.
  if f.has_remote {
    push(&mut plain, &mut display, '|', Some(DIM));
  } else {
    push(&mut plain, &mut display, ' ', None);
  }

  // Position 6: USER_MARKER — always blank
  push(&mut plain, &mut display, ' ', None);

  // Trim trailing spaces from both representations so column width adapts.
  while plain.ends_with(' ') {
    plain.pop();
    display.pop();
  }

  Cell::styled(plain, display)
}

fn head_diff_cell(d: &LineDiff, styled: bool) -> Cell {
  let plain = format_head_diff(d);

  if !styled || plain.is_empty() {
    return Cell::raw(plain);
  }

  let mut display = String::new();
  let (add_str, add_compact) = compact_signs(d.added);
  let (rem_str, rem_compact) = compact_signs(d.removed);

  match (d.added, d.removed) {
    (0, 0) => {}
    (a, 0) if a > 0 => {
      let s = format!("+{add_str}");
      let style = if add_compact { BOLD } else { GREEN };

      display.push_str(&wrap(&s, style));
    }
    (0, r) if r > 0 => {
      let s = format!("-{rem_str}");
      let style = if rem_compact { BOLD } else { RED };

      display.push_str(&wrap(&s, style));
    }
    _ => {
      let add_s = format!("+{add_str}");
      let rem_s = format!("-{rem_str}");
      let add_style = if add_compact { BOLD } else { GREEN };
      let rem_style = if rem_compact { BOLD } else { RED };

      display.push_str(&wrap(&add_s, add_style));
      display.push(' ');
      display.push_str(&wrap(&rem_s, rem_style));
    }
  }

  Cell::styled(plain, display)
}

fn ahead_behind_cell(ab: &AheadBehind, styled: bool) -> Cell {
  let plain = format_ahead_behind(ab);

  if !styled || plain.is_empty() {
    return Cell::raw(plain);
  }

  let mut display = String::new();
  let (ahead_str, ahead_compact) = compact_arrows(ab.ahead);
  let (behind_str, behind_compact) = compact_arrows(ab.behind);

  match (ab.ahead, ab.behind) {
    (0, 0) => {}
    (a, 0) if a > 0 => {
      let s = format!("↑{ahead_str}");
      let style = if ahead_compact { BOLD } else { GREEN };

      display.push_str(&wrap(&s, style));
    }
    (0, b) if b > 0 => {
      let s = format!("↓{behind_str}");
      let style = if behind_compact { BOLD } else { YELLOW };

      display.push_str(&wrap(&s, style));
    }
    _ => {
      let ahead_s = format!("↑{ahead_str}");
      let behind_s = format!("↓{behind_str}");
      let a_style = if ahead_compact { BOLD } else { GREEN };
      let b_style = if behind_compact { BOLD } else { YELLOW };

      display.push_str(&wrap(&ahead_s, a_style));
      display.push(' ');
      display.push_str(&wrap(&behind_s, b_style));
    }
  }

  Cell::styled(plain, display)
}

fn ci_status_cell(ci: CiStatus, styled: bool) -> Cell {
  match ci {
    CiStatus::None => Cell::raw(String::new()),
    CiStatus::Pass => {
      let plain = "✓".to_string();

      if styled {
        Cell::styled(plain, wrap("✓", GREEN))
      } else {
        Cell::raw(plain)
      }
    }
    CiStatus::Fail => {
      let plain = "✗".to_string();

      if styled {
        Cell::styled(plain, wrap("✗", RED))
      } else {
        Cell::raw(plain)
      }
    }
    CiStatus::Pending => {
      let plain = "◌".to_string();

      if styled {
        Cell::styled(plain, wrap("◌", YELLOW))
      } else {
        Cell::raw(plain)
      }
    }
  }
}

/// Compact notation for sign-style diffs (HEAD±). Values 0–999 are
/// literal, 1000–9999 become `NK`, 10000+ become `∞`.
fn compact_signs(value: u32) -> (String, bool) {
  if value >= 10_000 {
    ("∞".to_string(), true)
  } else if value >= 1_000 {
    (format!("{}K", value / 1_000), true)
  } else {
    (value.to_string(), false)
  }
}

/// Compact notation for arrow-style diffs (main↕). Values 0–99 are
/// literal, 100–999 become `NC`, 1000–9999 become `NK`, 10000+ become `∞`.
fn compact_arrows(value: u32) -> (String, bool) {
  if value >= 10_000 {
    ("∞".to_string(), true)
  } else if value >= 1_000 {
    (format!("{}K", value / 1_000), true)
  } else if value >= 100 {
    (format!("{}C", value / 100), true)
  } else {
    (value.to_string(), false)
  }
}

fn format_head_diff(d: &LineDiff) -> String {
  let (add_str, _) = compact_signs(d.added);
  let (rem_str, _) = compact_signs(d.removed);

  match (d.added, d.removed) {
    (0, 0) => String::new(),
    (a, 0) if a > 0 => format!("+{add_str}"),
    (0, r) if r > 0 => format!("-{rem_str}"),
    _ => format!("+{add_str} -{rem_str}"),
  }
}

fn format_ahead_behind(ab: &AheadBehind) -> String {
  let (ahead_str, _) = compact_arrows(ab.ahead);
  let (behind_str, _) = compact_arrows(ab.behind);

  match (ab.ahead, ab.behind) {
    (0, 0) => String::new(),
    (a, 0) if a > 0 => format!("↑{ahead_str}"),
    (0, b) if b > 0 => format!("↓{behind_str}"),
    _ => format!("↑{ahead_str} ↓{behind_str}"),
  }
}

/// Format an age in seconds as a short relative string. Matches
/// worktrunk's `format_relative_time_short` bucket boundaries.
pub fn format_age(seconds_ago: i64) -> String {
  const MINUTE: i64 = 60;
  const HOUR: i64 = MINUTE * 60;
  const DAY: i64 = HOUR * 24;
  const WEEK: i64 = DAY * 7;
  const MONTH: i64 = DAY * 30;
  const YEAR: i64 = DAY * 365;

  if seconds_ago < 0 {
    return "future".to_string();
  }

  if seconds_ago < MINUTE {
    return "now".to_string();
  }

  const UNITS: &[(i64, &str)] = &[
    (YEAR, "y"),
    (MONTH, "mo"),
    (WEEK, "w"),
    (DAY, "d"),
    (HOUR, "h"),
    (MINUTE, "m"),
  ];

  for &(unit, abbrev) in UNITS {
    let v = seconds_ago / unit;

    if v > 0 {
      return format!("{v}{abbrev}");
    }
  }

  "now".to_string()
}

fn format_summary(rows: &[ListRow], styled: bool) -> String {
  let n = rows.len();
  let dirty = rows
    .iter()
    .filter(|r| r.status.modified || r.status.untracked || r.status.conflicts)
    .count();
  let ahead = rows.iter().filter(|r| r.vs_trunk.ahead > 0).count();
  let plural = if n == 1 { "" } else { "s" };
  let mut parts = vec![format!("{n} worktree{plural}")];

  if dirty > 0 {
    parts.push(format!("{dirty} with changes"));
  }

  if ahead > 0 {
    parts.push(format!("{ahead} ahead"));
  }

  let body = format!("○ Showing {}", parts.join(", "));

  if styled { wrap(&body, DIM) } else { body }
}

/// Wrap a string in `style`'s ANSI escapes.
fn wrap(s: &str, style: Style) -> String {
  format!("{style}{s}{style:#}")
}

/// Render the list as a JSON array (one object per workspace). Designed
/// for tool integration — the keys mirror the table columns but expose
/// raw values (numbers stay numbers, the trunk relation is a string).
pub fn format_list_json(rows: &[ListRow]) -> String {
  let arr: Vec<Value> = rows.iter().map(list_row_json).collect();

  serde_json::to_string_pretty(&Value::Array(arr)).expect("json serialize")
}

fn list_row_json(r: &ListRow) -> Value {
  let mut m = Map::new();

  m.insert("name".into(), Value::String(r.name.clone()));
  m.insert(
    "kind".into(),
    Value::String(
      match r.kind {
        ListRowKind::Workspace => "workspace",
        ListRowKind::Branch => "branch",
      }
      .into(),
    ),
  );
  m.insert("path".into(), Value::String(r.path.display().to_string()));
  m.insert(
    "url".into(),
    if r.url.is_empty() {
      Value::Null
    } else {
      Value::String(r.url.clone())
    },
  );
  m.insert("is_current".into(), Value::Bool(r.is_current));
  m.insert("is_default".into(), Value::Bool(r.is_default));
  m.insert("commit".into(), Value::String(r.commit.clone()));
  m.insert("age".into(), Value::String(r.age.clone()));
  m.insert("message".into(), Value::String(r.message.clone()));
  m.insert(
    "status".into(),
    json!({
      "has_changes": r.status.has_changes,
      "modified": r.status.modified,
      "untracked": r.status.untracked,
      "conflicts": r.status.conflicts,
      "stale": r.status.stale,
      "has_remote": r.status.has_remote,
      "vs_trunk": trunk_rel_str(r.status.vs_trunk),
    }),
  );
  m.insert(
    "head_diff".into(),
    json!({"added": r.head_diff.added, "removed": r.head_diff.removed}),
  );
  m.insert(
    "vs_trunk".into(),
    json!({"ahead": r.vs_trunk.ahead, "behind": r.vs_trunk.behind}),
  );
  m.insert("ci_status".into(), Value::String(r.ci_status.to_string()));
  m.insert(
    "summary".into(),
    if r.summary.is_empty() {
      Value::Null
    } else {
      Value::String(r.summary.clone())
    },
  );

  Value::Object(m)
}

fn trunk_rel_str(rel: Option<TrunkRel>) -> Value {
  match rel {
    Some(TrunkRel::IsTrunk) => json!("is_trunk"),
    Some(TrunkRel::Ancestor) => json!("ancestor"),
    Some(TrunkRel::Diverged) => json!("diverged"),
    Some(TrunkRel::Ahead) => json!("ahead"),
    Some(TrunkRel::Behind) => json!("behind"),
    Some(TrunkRel::None) | None => Value::Null,
  }
}

/// JSON envelope for `jjwt switch`. Fields:
/// `name` — workspace, `path` — absolute workspace path,
/// `created` — true when the plan added a new workspace.
pub fn format_switch_json(name: &str, path: &std::path::Path, created: bool) -> String {
  serde_json::to_string(&json!({
    "name": name,
    "path": path.display().to_string(),
    "created": created,
  }))
  .expect("json serialize")
}

/// JSON envelope for `jjwt remove`. Fields: `name`, `path`,
/// `bookmark_deleted` (true when the bookmark was merged and removed).
pub fn format_remove_json(name: &str, path: &std::path::Path, bookmark_deleted: bool) -> String {
  serde_json::to_string(&json!({
    "name": name,
    "path": path.display().to_string(),
    "bookmark_deleted": bookmark_deleted,
  }))
  .expect("json serialize")
}

/// Compact one-line summary of workspaces for status display integrations.
///
/// Format: `@<current> +A-R ↑H↓B | N ws` where:
/// - `<current>` is the current workspace name (or `?` if none)
/// - `+A-R` is the HEAD diff (lines added/removed)
/// - `↑H↓B` is ahead/behind trunk
/// - `N ws` is the total workspace count
pub fn format_statusline(rows: &[ListRow], current: Option<&str>) -> String {
  let total = rows.len();

  let current_row = current.and_then(|name| rows.iter().find(|r| r.name == name));

  match current_row {
    Some(row) => {
      let name = &row.name;
      let added = row.head_diff.added;
      let removed = row.head_diff.removed;
      let ahead = row.vs_trunk.ahead;
      let behind = row.vs_trunk.behind;

      format!("@{name} +{added}-{removed} ↑{ahead}↓{behind} | {total} ws")
    }
    None => {
      format!("@? | {total} ws")
    }
  }
}
