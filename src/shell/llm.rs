#![cfg(not(tarpaulin_include))]

use crate::core::types::{CommitGenerationConfig, CoreError};
use minijinja::{Environment, UndefinedBehavior};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Variables available in the LLM prompt template.
pub struct LlmPromptVars {
  /// Full diff output from `jj diff`.
  pub jj_diff: String,
  /// Summary statistics from `jj diff --stat`.
  pub jj_diff_stat: String,
  /// Current branch/workspace name.
  pub branch: String,
  /// Repository name.
  pub repo: String,
  /// Recent commit messages for style context.
  pub recent_commits: String,
}

/// Default prompt template for commit message generation.
const DEFAULT_TEMPLATE: &str = r#"Write a concise commit message for the following changes.

Branch: {{ branch }}
Repository: {{ repo }}

Diff stat:
{{ jj_diff_stat }}

Full diff:
{{ jj_diff }}
{% if recent_commits %}
Recent commits (for style reference):
{{ recent_commits }}
{% endif %}
{% if guidance %}
{{ guidance }}
{% endif %}
Rules:
- First line: imperative mood, max 72 characters, no period
- If needed, add a blank line then bullet-point details
- Focus on WHY, not WHAT (the diff shows what)
- Output ONLY the commit message, nothing else"#;

/// Render the LLM prompt template with commit-generation variables.
pub fn render_prompt(
  cfg: &CommitGenerationConfig,
  vars: &LlmPromptVars,
) -> Result<String, CoreError> {
  let template_src = match &cfg.template {
    Some(t) => t.clone(),
    None => {
      let mut base = DEFAULT_TEMPLATE.to_string();

      if let Some(append) = &cfg.template_append {
        base.push_str("\n\n");
        base.push_str(append);
      }

      base
    }
  };

  let mut env = Environment::new();

  env.set_undefined_behavior(UndefinedBehavior::Lenient);

  let tmpl = env
    .template_from_str(&template_src)
    .map_err(|e| CoreError::TemplateRender(e.to_string()))?;

  let mut data = std::collections::BTreeMap::<String, minijinja::value::Value>::new();

  data.insert("jj_diff".into(), vars.jj_diff.clone().into());
  data.insert("jj_diff_stat".into(), vars.jj_diff_stat.clone().into());
  data.insert("branch".into(), vars.branch.clone().into());
  data.insert("repo".into(), vars.repo.clone().into());
  data.insert("recent_commits".into(), vars.recent_commits.clone().into());

  tmpl
    .render(data)
    .map_err(|e| CoreError::TemplateRender(e.to_string()))
}

/// Pipe a rendered prompt to the configured command via stdin, return stdout.
/// Returns `None` on failure (command not found, non-zero exit, empty output).
pub fn run_llm_command(command: &str, prompt: &str) -> Option<String> {
  let mut child = Command::new("sh")
    .args(["-c", command])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()
    .ok()?;

  if let Some(mut stdin) = child.stdin.take() {
    let _ = stdin.write_all(prompt.as_bytes());
  }

  let output = child.wait_with_output().ok()?;

  if !output.status.success() {
    return None;
  }

  let text = String::from_utf8(output.stdout).ok()?;
  let trimmed = text.trim().to_string();

  if trimmed.is_empty() {
    return None;
  }

  Some(trimmed)
}

/// Run `jj diff -r @` in the workspace and capture output.
pub fn get_jj_diff(workspace_path: &Path) -> Option<String> {
  let output = Command::new("jj")
    .current_dir(workspace_path)
    .args(["diff", "-r", "@"])
    .output()
    .ok()?;

  if !output.status.success() {
    return None;
  }

  let text = String::from_utf8(output.stdout).ok()?;

  Some(text)
}

/// Run `jj diff -r @ --stat` in the workspace and capture output.
pub fn get_jj_diff_stat(workspace_path: &Path) -> Option<String> {
  let output = Command::new("jj")
    .current_dir(workspace_path)
    .args(["diff", "-r", "@", "--stat"])
    .output()
    .ok()?;

  if !output.status.success() {
    return None;
  }

  let text = String::from_utf8(output.stdout).ok()?;

  Some(text)
}

/// Run `jj log` for recent commits to provide style context.
pub fn get_recent_commits(workspace_path: &Path) -> Option<String> {
  let output = Command::new("jj")
    .current_dir(workspace_path)
    .args(["log", "-r", "ancestors(@, 5)", "--no-graph"])
    .output()
    .ok()?;

  if !output.status.success() {
    return None;
  }

  let text = String::from_utf8(output.stdout).ok()?;

  Some(text)
}

/// Generate a one-liner summary suitable for the list column.
/// Uses a simpler prompt than full commit-message generation.
pub fn generate_summary(command: &str, message: &str, diff_stat: &str) -> Option<String> {
  let prompt = format!(
    "Summarize this branch's work in ONE short phrase (max 50 chars, no period).\n\
     \n\
     Commit message: {message}\n\
     \n\
     Diff stat:\n\
     {diff_stat}"
  );

  run_llm_command(command, &prompt)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::types::CommitGenerationConfig;

  #[test]
  fn render_default_template() {
    let cfg = CommitGenerationConfig::default();
    let vars = LlmPromptVars {
      jj_diff: "diff content".into(),
      jj_diff_stat: "1 file changed".into(),
      branch: "feat-login".into(),
      repo: "my-app".into(),
      recent_commits: String::new(),
    };

    let result = render_prompt(&cfg, &vars).unwrap();

    assert!(result.contains("feat-login"));
    assert!(result.contains("my-app"));
    assert!(result.contains("diff content"));
    assert!(result.contains("1 file changed"));
  }

  #[test]
  fn render_custom_template() {
    let cfg = CommitGenerationConfig {
      command: None,
      template: Some("Branch: {{ branch }}\nDiff: {{ jj_diff }}".into()),
      template_append: None,
    };
    let vars = LlmPromptVars {
      jj_diff: "some diff".into(),
      jj_diff_stat: String::new(),
      branch: "fix-bug".into(),
      repo: String::new(),
      recent_commits: String::new(),
    };

    let result = render_prompt(&cfg, &vars).unwrap();

    assert_eq!(result, "Branch: fix-bug\nDiff: some diff");
  }

  #[test]
  fn render_template_append() {
    let cfg = CommitGenerationConfig {
      command: None,
      template: None,
      template_append: Some("Always mention the ticket number.".into()),
    };
    let vars = LlmPromptVars {
      jj_diff: "diff".into(),
      jj_diff_stat: "stat".into(),
      branch: "b".into(),
      repo: "r".into(),
      recent_commits: String::new(),
    };

    let result = render_prompt(&cfg, &vars).unwrap();

    assert!(result.contains("Always mention the ticket number."));
    assert!(result.contains("Write a concise commit message"));
  }

  #[test]
  fn run_echo_command() {
    let result = run_llm_command("cat", "hello world");

    assert_eq!(result, Some("hello world".into()));
  }

  #[test]
  fn run_failing_command_returns_none() {
    let result = run_llm_command("false", "input");

    assert_eq!(result, None);
  }
}
