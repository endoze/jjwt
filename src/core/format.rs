use crate::core::types::{AheadBehind, LineDiff, ListRow, ListRowKind, StatusFlags, TrunkRel};
use anstyle::{AnsiColor, Color, Style};
use serde_json::{Map, Value, json};
use unicode_width::UnicodeWidthStr;

const COL_SEP: &str = "  ";

const HEADERS: &[&str] = &[
  "Branch", "Status", "HEAD±", "main↕", "Path", "URL", "Commit", "Age", "Message",
];

const DIM: Style = Style::new().dimmed();
const BOLD: Style = Style::new().bold();
const GREEN: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green)));
const RED: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Red)));
const YELLOW: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Yellow)));
const CYAN: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));

/// One rendered cell: `plain` is used for width measurement and padding,
/// `display` is what is written out (may include ANSI escapes).
#[derive(Default)]
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

/// Render a worktrunk-style list table for the given rows. Includes the
/// header row, data rows, blank line, and footer summary.
///
/// When `styled` is true, output is decorated with ANSI escape sequences.
pub fn format_list_table(rows: &[ListRow], styled: bool) -> String {
  let cells = build_cells(rows, styled);
  let widths = compute_widths(&cells);
  let mut out = String::new();

  // Header row: gutter column is empty; then the named columns.
  out.push(' ');
  out.push(' '); // gutter (1) + separator (1)

  let header_cells: [Cell; 9] = std::array::from_fn(|i| {
    let h = HEADERS[i].to_string();

    if styled {
      Cell::styled(h.clone(), wrap(&h, BOLD))
    } else {
      Cell::raw(h)
    }
  });

  push_columns(&mut out, &header_cells, &widths);
  out.push('\n');

  for (row, row_cells) in rows.iter().zip(cells.iter()) {
    out.push(gutter_char(row));
    out.push(' ');
    push_columns(&mut out, row_cells, &widths);
    out.push('\n');
  }

  out.push('\n');
  out.push_str(&format_summary(rows, styled));
  out.push('\n');

  out
}

fn push_columns(out: &mut String, cells: &[Cell; 9], widths: &[usize; 9]) {
  let last = cells.len().saturating_sub(1);

  for (i, cell) in cells.iter().enumerate() {
    out.push_str(&cell.display);

    if i < last {
      let pad = widths[i].saturating_sub(cell.width());

      for _ in 0..pad {
        out.push(' ');
      }

      out.push_str(COL_SEP);
    }
  }
}

fn build_cells(rows: &[ListRow], styled: bool) -> Vec<[Cell; 9]> {
  rows.iter().map(|r| build_row_cells(r, styled)).collect()
}

fn build_row_cells(r: &ListRow, styled: bool) -> [Cell; 9] {
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
    text_cell(&format_path(r), None),
    text_cell(&r.url, dim_if_styled),
    text_cell(&r.commit, dim_if_styled),
    text_cell(&r.age, dim_if_styled),
    text_cell(&r.message, dim_if_styled),
  ]
}

fn text_cell(s: &str, style: Option<Style>) -> Cell {
  match style {
    Some(st) if !s.is_empty() => Cell::styled(s.to_string(), wrap(s, st)),
    _ => Cell::raw(s.to_string()),
  }
}

fn compute_widths(cells: &[[Cell; 9]]) -> [usize; 9] {
  let mut widths = [0usize; 9];

  for (i, h) in HEADERS.iter().enumerate() {
    widths[i] = h.width();
  }

  for row in cells {
    for (i, cell) in row.iter().enumerate() {
      let w = cell.width();

      if w > widths[i] {
        widths[i] = w;
      }
    }
  }

  widths
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

  match (d.added, d.removed) {
    (0, 0) => {}
    (a, 0) => display.push_str(&wrap(&format!("+{a}"), GREEN)),
    (0, r) => display.push_str(&wrap(&format!("-{r}"), RED)),
    (a, r) => {
      display.push_str(&wrap(&format!("+{a}"), GREEN));
      display.push(' ');
      display.push_str(&wrap(&format!("-{r}"), RED));
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

  match (ab.ahead, ab.behind) {
    (0, 0) => {}
    (a, 0) => display.push_str(&wrap(&format!("↑{a}"), GREEN)),
    (0, b) => display.push_str(&wrap(&format!("↓{b}"), YELLOW)),
    (a, b) => {
      display.push_str(&wrap(&format!("↑{a}"), GREEN));
      display.push(' ');
      display.push_str(&wrap(&format!("↓{b}"), YELLOW));
    }
  }

  Cell::styled(plain, display)
}

fn format_head_diff(d: &LineDiff) -> String {
  match (d.added, d.removed) {
    (0, 0) => String::new(),
    (a, 0) => format!("+{a}"),
    (0, r) => format!("-{r}"),
    (a, r) => format!("+{a} -{r}"),
  }
}

fn format_ahead_behind(ab: &AheadBehind) -> String {
  match (ab.ahead, ab.behind) {
    (0, 0) => String::new(),
    (a, 0) => format!("↑{a}"),
    (0, b) => format!("↓{b}"),
    (a, b) => format!("↑{a} ↓{b}"),
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
