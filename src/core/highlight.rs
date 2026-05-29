//! Bash syntax highlighting for the hook command echoed before each hook
//! runs. Pure string-to-string transformation: takes a fully template-expanded
//! shell command and returns it wrapped in a gutter, optionally with
//! tree-sitter bash syntax colors. No IO — the caller decides whether color is
//! wanted (TTY / `NO_COLOR`) and handles writing the result.

use anstyle::{AnsiColor, Color, Style};

/// Gutter block: a bright-white background applied to a single space, forming
/// the subtle left margin that separates an echoed command from its output.
/// BrightWhite reads as a faint bar on both dark and light terminals.
const GUTTER: Style = Style::new().bg_color(Some(Color::Ansi(AnsiColor::BrightWhite)));

/// Highlight names recognized in the bash query, in the order passed to
/// `configure`. `HighlightEvent::HighlightStart(idx)` indexes into this slice.
/// These mirror the `@`-capture names in tree-sitter-bash's `highlights.scm`.
#[cfg(feature = "syntax-highlighting")]
const HIGHLIGHT_NAMES: [&str; 9] = [
  "comment", "constant", "embedded", "function", "keyword", "number", "operator", "property",
  "string",
];

/// Render a bash command inside a gutter, with syntax colors when `color` is
/// true. Multi-line commands (e.g. heredoc hooks) get a gutter per line.
///
/// When `color` is false — or the `syntax-highlighting` feature is disabled —
/// falls back to a plain two-space gutter with no ANSI, keeping piped output
/// clean.
pub fn highlight_bash_with_gutter(cmd: &str, color: bool) -> String {
  let content = cmd.replace("\r\n", "\n");
  let content = content.trim_end_matches('\n');

  if !color {
    return plain_gutter(content);
  }

  #[cfg(feature = "syntax-highlighting")]
  {
    highlighted(content).unwrap_or_else(|| dim_gutter(content))
  }

  #[cfg(not(feature = "syntax-highlighting"))]
  {
    dim_gutter(content)
  }
}

/// Two-space left margin per line, no ANSI. Used when color is off.
fn plain_gutter(content: &str) -> String {
  content
    .split('\n')
    .map(|line| format!("  {line}"))
    .collect::<Vec<_>>()
    .join("\n")
}

/// Colored gutter with dim (uncolored) command text. Used when color is on but
/// syntax highlighting is unavailable or fails.
fn dim_gutter(content: &str) -> String {
  let dim = Style::new().dimmed();
  let reset = anstyle::Reset;

  content
    .split('\n')
    .map(|line| format!("{GUTTER} {GUTTER:#} {dim}{line}{reset}"))
    .collect::<Vec<_>>()
    .join("\n")
}

/// Map a bash token capture name to its display style. Mirrors worktrunk's
/// palette so output matches: commands blue, keywords magenta, strings green,
/// operators/flags cyan, variables/numbers yellow. Every style is dimmed so
/// colored tokens sit at the same brightness as the dim base text (bold is
/// avoided — it cancels dim on some terminals).
#[cfg(feature = "syntax-highlighting")]
fn bash_token_style(kind: &str) -> Option<Style> {
  let style = match kind {
    "function" => Style::new().fg_color(Some(Color::Ansi(AnsiColor::Blue))),
    "keyword" => Style::new().fg_color(Some(Color::Ansi(AnsiColor::Magenta))),
    "string" => Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green))),
    "operator" => Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan))),
    "property" => Style::new().fg_color(Some(Color::Ansi(AnsiColor::Yellow))),
    "number" => Style::new().fg_color(Some(Color::Ansi(AnsiColor::Yellow))),
    "constant" => Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan))),
    _ => return None,
  };

  Some(style.dimmed())
}

/// Tree-sitter highlight path. Returns `None` on any setup/parse failure so
/// the caller can fall back to an uncolored gutter.
#[cfg(feature = "syntax-highlighting")]
fn highlighted(content: &str) -> Option<String> {
  use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

  let dim = Style::new().dimmed();
  let reset = anstyle::Reset;

  let bash_language = tree_sitter_bash::LANGUAGE.into();
  let mut config = HighlightConfiguration::new(
    bash_language,
    "bash",
    tree_sitter_bash::HIGHLIGHT_QUERY,
    "",
    "",
  )
  .ok()?;

  config.configure(&HIGHLIGHT_NAMES);

  let mut highlighter = Highlighter::new();
  let events = highlighter
    .highlight(&config, content.as_bytes(), None, |_| None)
    .ok()?;
  let bytes = content.as_bytes();

  // Build the styled command string, restoring the active style after each
  // newline so every physical line is self-contained once split for gutters.
  let mut styled = format!("{dim}");
  let mut pending_highlight: Option<usize> = None;
  let mut active_style: Option<Style> = None;

  for event in events {
    match event.ok()? {
      HighlightEvent::Source { start, end } => {
        let text = std::str::from_utf8(&bytes[start..end]).ok()?;

        if let Some(idx) = pending_highlight.take()
          && let Some(name) = HIGHLIGHT_NAMES.get(idx)
          && let Some(style) = bash_token_style(name)
        {
          styled.push_str(&format!("{reset}{style}"));
          active_style = Some(style);
        }

        let style_restore = match active_style {
          Some(style) => format!("{dim}{reset}{style}"),
          None => format!("{dim}"),
        };

        styled.push_str(&text.replace('\n', &format!("\n{style_restore}")));
      }
      HighlightEvent::HighlightStart(idx) => {
        pending_highlight = Some(idx.0);
      }
      HighlightEvent::HighlightEnd => {
        pending_highlight = None;
        active_style = None;
        styled.push_str(&format!("{reset}{dim}"));
      }
    }
  }

  let out = styled
    .split('\n')
    .map(|line| format!("{GUTTER} {GUTTER:#} {line}{reset}"))
    .collect::<Vec<_>>()
    .join("\n");

  Some(out)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn plain_has_no_ansi() {
    let out = highlight_bash_with_gutter("echo hi", false);

    assert_eq!(out, "  echo hi");
    assert!(!out.contains('\x1b'));
  }

  #[test]
  fn plain_gutters_every_line() {
    let out = highlight_bash_with_gutter("line1\nline2", false);

    assert_eq!(out, "  line1\n  line2");
  }

  #[test]
  fn trailing_newlines_trimmed() {
    let out = highlight_bash_with_gutter("echo hi\n\n", false);

    assert_eq!(out, "  echo hi");
  }

  #[cfg(feature = "syntax-highlighting")]
  #[test]
  fn colored_contains_ansi_and_all_lines_guttered() {
    let out = highlight_bash_with_gutter("echo hi\nls -la", true);

    assert!(out.contains('\x1b'));
    assert_eq!(out.lines().count(), 2);
  }
}
