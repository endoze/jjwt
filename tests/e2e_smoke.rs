use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn jjwt_bin() -> std::path::PathBuf {
  std::path::PathBuf::from(env!("CARGO_BIN_EXE_jjwt"))
}

/// Build a `Command` for the jjwt binary with user-config discovery pointed
/// at `fake_home` so the developer's `~/.config/jjwt/config.toml` cannot
/// leak into tests.
fn jjwt_cmd(fake_home: &Path) -> Command {
  let mut cmd = Command::new(jjwt_bin());
  cmd.env("HOME", fake_home);
  cmd.env("XDG_CONFIG_HOME", fake_home);

  cmd
}

fn jj() -> Command {
  Command::new("jj")
}

/// An upstream repo with one commit on `main`, a clone of it, and a fake home
/// holding an empty user config (jjwt refuses to run with no config at all).
fn init_with_origin() -> (TempDir, PathBuf, TempDir) {
  let tmp = TempDir::new().unwrap();
  let up = tmp.path().join("up");
  let repo = tmp.path().join("repo");

  assert!(
    jj()
      .args(["git", "init", "--colocate"])
      .arg(&up)
      .status()
      .unwrap()
      .success()
  );

  std::fs::write(up.join("a"), "a").unwrap();

  assert!(
    jj()
      .current_dir(&up)
      .args(["commit", "-m", "base"])
      .status()
      .unwrap()
      .success()
  );
  assert!(
    jj()
      .current_dir(&up)
      .args(["bookmark", "set", "main", "-r", "@-"])
      .status()
      .unwrap()
      .success()
  );
  assert!(
    jj()
      .args(["git", "clone"])
      .arg(&up)
      .arg(&repo)
      .status()
      .unwrap()
      .success()
  );

  let home = TempDir::new().unwrap();

  std::fs::create_dir_all(home.path().join("jjwt")).unwrap();
  std::fs::write(home.path().join("jjwt/config.toml"), "").unwrap();

  (tmp, repo, home)
}

#[test]
fn default_workspace_is_named_default_and_cannot_be_removed() {
  if which::which("jj").is_err() {
    return;
  }

  let (_tmp, repo, home) = init_with_origin();
  let out = jjwt_cmd(home.path())
    .current_dir(&repo)
    .args(["list", "--format", "json"])
    .output()
    .unwrap();
  let rows: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

  assert!(
    rows
      .as_array()
      .unwrap()
      .iter()
      .any(|r| r["name"] == "default"),
    "{rows}"
  );

  for args in [
    vec!["remove", "main"],
    vec!["remove", "default"],
    vec!["remove"],
  ] {
    let out = jjwt_cmd(home.path())
      .current_dir(&repo)
      .args(&args)
      .output()
      .unwrap();

    assert!(!out.status.success(), "{args:?} should refuse");
  }

  assert!(repo.join(".jj").is_dir());
}

#[test]
fn switch_create_produces_workspace_and_hook_output() {
  if which::which("jj").is_err() {
    eprintln!("skipping e2e: jj not on PATH");

    return;
  }

  let tmp = TempDir::new().unwrap();
  let repo = tmp.path();
  let fake_home = TempDir::new().unwrap();

  assert!(
    jj()
      .arg("git")
      .arg("init")
      .arg(repo)
      .status()
      .unwrap()
      .success()
  );

  std::fs::write(repo.join("README.md"), "init").unwrap();

  assert!(
    jj()
      .current_dir(repo)
      .arg("describe")
      .arg("-m")
      .arg("init")
      .status()
      .unwrap()
      .success()
  );
  assert!(
    jj()
      .current_dir(repo)
      .arg("new")
      .status()
      .unwrap()
      .success()
  );

  std::fs::create_dir_all(repo.join(".config")).unwrap();
  std::fs::write(
    repo.join(".config/wt.toml"),
    r#"
[[pre-start]]
sentinel = "echo {{ branch }} > sentinel.txt"
"#,
  )
  .unwrap();

  let out = jjwt_cmd(fake_home.path())
    .arg("-C")
    .arg(repo)
    .args(["switch", "test-branch", "--create"])
    .env("JJWT_TRUST_PROJECT_HOOKS", "1")
    .output()
    .unwrap();

  assert!(
    out.status.success(),
    "jjwt failed:\nstdout: {}\nstderr: {}",
    String::from_utf8_lossy(&out.stdout),
    String::from_utf8_lossy(&out.stderr)
  );

  let stdout = String::from_utf8(out.stdout).unwrap();
  let printed_path = stdout.lines().last().unwrap().trim();

  let repo_name = repo.file_name().unwrap().to_string_lossy();
  let expected_suffix = format!("{repo_name}.test-branch");

  assert!(
    printed_path.ends_with(&expected_suffix),
    "expected workspace path ending with '{expected_suffix}', got: {stdout:?}"
  );

  let ws_path = repo.parent().unwrap().join(&expected_suffix);

  assert!(ws_path.is_dir(), "workspace dir missing: {ws_path:?}");

  let sentinel = ws_path.join("sentinel.txt");

  assert!(sentinel.is_file(), "sentinel.txt missing");

  let body = std::fs::read_to_string(&sentinel).unwrap();

  assert_eq!(body.trim(), "test-branch");

  let bm = jj()
    .current_dir(repo)
    .args(["bookmark", "list", "-T", r#"name ++ "\n""#])
    .output()
    .unwrap();
  let bm_text = String::from_utf8_lossy(&bm.stdout);

  assert!(
    !bm_text.lines().any(|l| l.trim() == "test-branch"),
    "switch --create must not create a bookmark; got:\n{bm_text}"
  );
}

#[test]
fn list_renders_table_with_default_and_added_workspaces() {
  if which::which("jj").is_err() {
    eprintln!("skipping e2e: jj not on PATH");

    return;
  }

  let tmp = TempDir::new().unwrap();
  let repo = tmp.path();
  let fake_home = TempDir::new().unwrap();

  assert!(
    jj()
      .arg("git")
      .arg("init")
      .arg(repo)
      .status()
      .unwrap()
      .success()
  );

  std::fs::write(repo.join("README.md"), "init").unwrap();

  assert!(
    jj()
      .current_dir(repo)
      .args(["describe", "-m", "init"])
      .status()
      .unwrap()
      .success()
  );
  assert!(
    jj()
      .current_dir(repo)
      .arg("new")
      .status()
      .unwrap()
      .success()
  );

  std::fs::create_dir_all(repo.join(".config")).unwrap();
  std::fs::write(
    repo.join(".config/wt.toml"),
    r#"
[list]
url = "http://example.com/{{ branch }}"
"#,
  )
  .unwrap();

  // Commit the wt.toml so the default workspace is clean.
  assert!(
    jj()
      .current_dir(repo)
      .args(["describe", "-m", "add wt config"])
      .status()
      .unwrap()
      .success()
  );
  assert!(
    jj()
      .current_dir(repo)
      .arg("new")
      .status()
      .unwrap()
      .success()
  );

  let out = jjwt_cmd(fake_home.path())
    .arg("-C")
    .arg(repo)
    .args(["switch", "alpha", "--create"])
    .output()
    .unwrap();

  assert!(
    out.status.success(),
    "switch --create failed:\nstdout: {}\nstderr: {}",
    String::from_utf8_lossy(&out.stdout),
    String::from_utf8_lossy(&out.stderr)
  );

  // Dirty the alpha workspace.
  let alpha_name = format!(
    "{}.alpha",
    repo.file_name().unwrap().to_string_lossy()
  );
  let alpha_path = repo.parent().unwrap().join(&alpha_name);

  std::fs::write(alpha_path.join("scratch.txt"), "scratch").unwrap();

  // Compact list (without --full): hides CI, URL, Commit, Age, Summary.
  let list_out = jjwt_cmd(fake_home.path())
    .arg("-C")
    .arg(repo)
    .arg("list")
    .output()
    .unwrap();

  assert!(
    list_out.status.success(),
    "list failed:\nstdout: {}\nstderr: {}",
    String::from_utf8_lossy(&list_out.stdout),
    String::from_utf8_lossy(&list_out.stderr)
  );

  let text = String::from_utf8(list_out.stdout).unwrap();

  eprintln!("---list output---\n{text}\n---");

  // Header present
  assert!(
    text.contains("Bookmark"),
    "missing Bookmark header:\n{text}"
  );
  assert!(text.contains("Status"), "missing Status header:\n{text}");
  assert!(text.contains("HEAD±"), "missing HEAD± header:\n{text}");
  assert!(text.contains("main↕"), "missing main↕ header:\n{text}");

  // Compact mode hides URL column.
  assert!(
    !text.contains("URL"),
    "URL header should be hidden in compact mode:\n{text}"
  );

  // Both workspaces present
  assert!(
    text.contains("default"),
    "default workspace row missing:\n{text}"
  );
  assert!(
    text.contains("alpha"),
    "alpha workspace row missing:\n{text}"
  );

  // Footer: 2 worktrees, 1 with changes (alpha is dirty)
  assert!(
    text.contains("○ Showing 2 worktrees"),
    "footer should report 2 worktrees:\n{text}"
  );
  assert!(
    text.contains("1 with changes"),
    "footer should report 1 with changes:\n{text}"
  );

  // Full list (with --full): shows all columns including URL.
  let full_out = jjwt_cmd(fake_home.path())
    .arg("-C")
    .arg(repo)
    .arg("list")
    .arg("--full")
    .output()
    .unwrap();

  assert!(full_out.status.success());

  let full_text = String::from_utf8(full_out.stdout).unwrap();

  // URL templated per branch
  assert!(
    full_text.contains("http://example.com/alpha"),
    "URL should render with branch substitution in --full:\n{full_text}"
  );
}

#[test]
fn dynamic_completion_does_not_error_outside_repo() {
  let tmp = TempDir::new().unwrap();
  let fake_home = TempDir::new().unwrap();

  let out = jjwt_cmd(fake_home.path())
    .env("COMPLETE", "fish")
    .current_dir(tmp.path())
    .arg("--")
    .arg("jjwt")
    .arg("switch")
    .arg("")
    .output()
    .unwrap();

  assert!(
    out.status.success(),
    "completion exited non-zero:\nstdout: {}\nstderr: {}",
    String::from_utf8_lossy(&out.stdout),
    String::from_utf8_lossy(&out.stderr)
  );
}

#[test]
fn list_works_from_inside_a_workspace_subdir() {
  if which::which("jj").is_err() {
    eprintln!("skipping e2e: jj not on PATH");

    return;
  }

  let tmp = TempDir::new().unwrap();
  let repo = tmp.path();
  let fake_home = TempDir::new().unwrap();

  assert!(
    jj()
      .arg("git")
      .arg("init")
      .arg(repo)
      .status()
      .unwrap()
      .success()
  );

  std::fs::write(repo.join("README.md"), "init").unwrap();

  assert!(
    jj()
      .current_dir(repo)
      .args(["describe", "-m", "init"])
      .status()
      .unwrap()
      .success()
  );
  assert!(
    jj()
      .current_dir(repo)
      .arg("new")
      .status()
      .unwrap()
      .success()
  );

  std::fs::create_dir_all(repo.join(".config")).unwrap();
  std::fs::write(
    repo.join(".config/wt.toml"),
    "worktree-path = \".worktrees/{{ branch | sanitize }}\"\n",
  )
  .unwrap();

  let out = jjwt_cmd(fake_home.path())
    .arg("-C")
    .arg(repo)
    .args(["switch", "beta", "--create"])
    .output()
    .unwrap();

  assert!(
    out.status.success(),
    "switch --create failed:\nstdout: {}\nstderr: {}",
    String::from_utf8_lossy(&out.stdout),
    String::from_utf8_lossy(&out.stderr)
  );

  // Run `list` with cwd inside the newly created workspace, which has its
  // own `.jj/` whose `repo` is a file pointing back to the main repo.
  let ws_dir = repo.join(".worktrees/beta");
  let list_out = jjwt_cmd(fake_home.path())
    .arg("-C")
    .arg(&ws_dir)
    .arg("list")
    .output()
    .unwrap();

  assert!(
    list_out.status.success(),
    "list from inside workspace failed:\nstdout: {}\nstderr: {}",
    String::from_utf8_lossy(&list_out.stdout),
    String::from_utf8_lossy(&list_out.stderr)
  );

  let text = String::from_utf8(list_out.stdout).unwrap();

  assert!(
    text.contains("default"),
    "default workspace row missing:\n{text}"
  );
  assert!(text.contains("beta"), "beta workspace row missing:\n{text}");
}

#[test]
fn remove_forgets_workspace_without_panicking() {
  if which::which("jj").is_err() {
    eprintln!("skipping e2e: jj not on PATH");

    return;
  }

  let tmp = TempDir::new().unwrap();
  let repo = tmp.path();
  let fake_home = TempDir::new().unwrap();

  assert!(
    jj()
      .arg("git")
      .arg("init")
      .arg(repo)
      .status()
      .unwrap()
      .success()
  );

  std::fs::write(repo.join("README.md"), "init").unwrap();

  assert!(
    jj()
      .current_dir(repo)
      .args(["describe", "-m", "init"])
      .status()
      .unwrap()
      .success()
  );
  assert!(
    jj()
      .current_dir(repo)
      .arg("new")
      .status()
      .unwrap()
      .success()
  );

  std::fs::create_dir_all(repo.join(".config")).unwrap();
  std::fs::write(
    repo.join(".config/wt.toml"),
    "worktree-path = \".worktrees/{{ branch | sanitize }}\"\n",
  )
  .unwrap();

  let create = jjwt_cmd(fake_home.path())
    .arg("-C")
    .arg(repo)
    .args(["switch", "gamma", "--create"])
    .output()
    .unwrap();

  assert!(
    create.status.success(),
    "switch --create failed:\nstdout: {}\nstderr: {}",
    String::from_utf8_lossy(&create.stdout),
    String::from_utf8_lossy(&create.stderr)
  );

  let ws_dir = repo.join(".worktrees/gamma");

  assert!(ws_dir.is_dir(), "workspace dir missing: {ws_dir:?}");

  // The fresh workspace's working-copy commit is an empty head, so forgetting
  // it abandons that commit. This previously panicked in jj-lib because the
  // transaction was committed without rebasing descendants.
  let remove = jjwt_cmd(fake_home.path())
    .arg("-C")
    .arg(repo)
    .args(["remove", "gamma"])
    .output()
    .unwrap();

  assert!(
    remove.status.success(),
    "remove failed:\nstdout: {}\nstderr: {}",
    String::from_utf8_lossy(&remove.stdout),
    String::from_utf8_lossy(&remove.stderr)
  );
  assert!(
    !ws_dir.exists(),
    "workspace dir should be removed: {ws_dir:?}"
  );
}

/// Path of the named workspace, as jj records it.
fn workspace_root(repo: &Path, name: &str) -> PathBuf {
  let out = jj()
    .current_dir(repo)
    .args(["workspace", "root", "--name", name])
    .output()
    .unwrap();

  assert!(
    out.status.success(),
    "{}",
    String::from_utf8_lossy(&out.stderr)
  );

  PathBuf::from(String::from_utf8(out.stdout).unwrap().trim())
}

#[test]
fn remove_keeps_unsnapshotted_edits_as_a_commit() {
  if which::which("jj").is_err() {
    return;
  }

  let (_tmp, repo, home) = init_with_origin();

  assert!(
    jjwt_cmd(home.path())
      .current_dir(&repo)
      .args(["switch", "--create", "feat", "--no-hooks"])
      .status()
      .unwrap()
      .success()
  );

  let ws = workspace_root(&repo, "feat");

  std::fs::write(ws.join("wip.txt"), "wip").unwrap();

  let rm = jjwt_cmd(home.path())
    .current_dir(&repo)
    .args(["remove", "feat"])
    .output()
    .unwrap();

  assert!(
    rm.status.success(),
    "{}",
    String::from_utf8_lossy(&rm.stderr)
  );

  let out = jj()
    .current_dir(&repo)
    .args([
      "log",
      "--no-graph",
      "-r",
      "files(wip.txt)",
      "-T",
      "commit_id",
    ])
    .output()
    .unwrap();

  assert!(!out.stdout.is_empty());
}

#[test]
fn remove_works_when_directory_already_deleted() {
  if which::which("jj").is_err() {
    return;
  }

  let (_tmp, repo, home) = init_with_origin();

  assert!(
    jjwt_cmd(home.path())
      .current_dir(&repo)
      .args(["switch", "--create", "feat", "--no-hooks"])
      .status()
      .unwrap()
      .success()
  );

  std::fs::remove_dir_all(workspace_root(&repo, "feat")).unwrap();

  let rm = jjwt_cmd(home.path())
    .current_dir(&repo)
    .args(["remove", "feat"])
    .output()
    .unwrap();

  assert!(
    rm.status.success(),
    "{}",
    String::from_utf8_lossy(&rm.stderr)
  );
}

#[test]
fn ahead_behind_is_measured_against_trunk_revset_not_local_bookmark() {
  if which::which("jj").is_err() {
    return;
  }

  let (_tmp, repo, home) = init_with_origin();

  assert!(
    jj()
      .current_dir(&repo)
      .args(["new", "--no-edit", "main", "-m", "local"])
      .status()
      .unwrap()
      .success()
  );
  assert!(
    jj()
      .current_dir(&repo)
      .args(["bookmark", "set", "main", "-r", "main+ ~ @"])
      .status()
      .unwrap()
      .success()
  );

  let out = jjwt_cmd(home.path())
    .current_dir(&repo)
    .args(["list", "--format", "json"])
    .output()
    .unwrap();
  let rows: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
  let default = rows
    .as_array()
    .unwrap()
    .iter()
    .find(|r| r["name"] == "default")
    .expect("default row");

  assert_eq!(default["vs_trunk"]["behind"], 0, "{default}");
}

#[test]
fn switch_create_works_without_a_remote() {
  if which::which("jj").is_err() {
    return;
  }

  let tmp = TempDir::new().unwrap();
  let repo = tmp.path().join("repo");
  let home = TempDir::new().unwrap();

  std::fs::create_dir_all(home.path().join("jjwt")).unwrap();
  std::fs::write(home.path().join("jjwt/config.toml"), "").unwrap();

  assert!(
    jj()
      .args(["git", "init"])
      .arg(&repo)
      .status()
      .unwrap()
      .success()
  );

  let out = jjwt_cmd(home.path())
    .current_dir(&repo)
    .args(["switch", "--create", "x", "--no-hooks"])
    .output()
    .unwrap();

  assert!(
    out.status.success(),
    "{}",
    String::from_utf8_lossy(&out.stderr)
  );
}

#[test]
fn fresh_workspace_reads_as_on_trunk() {
  if which::which("jj").is_err() {
    return;
  }

  let (_tmp, repo, home) = init_with_origin();

  assert!(
    jjwt_cmd(home.path())
      .current_dir(&repo)
      .args(["switch", "--create", "feat", "--no-hooks"])
      .status()
      .unwrap()
      .success()
  );

  let out = jjwt_cmd(home.path())
    .current_dir(&repo)
    .args(["list", "--format", "json"])
    .output()
    .unwrap();
  let rows: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

  for r in rows.as_array().unwrap() {
    assert_eq!(r["status"]["vs_trunk"], "is_trunk", "{}", r["name"]);
  }
}

/// The `status.has_remote` flag of the named row in `jjwt list --format json`.
fn list_has_remote(repo: &Path, home: &Path, name: &str) -> serde_json::Value {
  let out = jjwt_cmd(home)
    .current_dir(repo)
    .args(["list", "--format", "json"])
    .output()
    .unwrap();
  let rows: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

  rows
    .as_array()
    .unwrap()
    .iter()
    .find(|r| r["name"] == name)
    .unwrap_or_else(|| panic!("no {name} row in {rows}"))["status"]["has_remote"]
    .clone()
}

#[test]
fn remote_marker_follows_pushed_bookmarks_in_workspace_range() {
  if which::which("jj").is_err() {
    return;
  }

  let (_tmp, repo, home) = init_with_origin();

  assert!(
    jjwt_cmd(home.path())
      .current_dir(&repo)
      .args(["switch", "--create", "feat", "--no-hooks"])
      .status()
      .unwrap()
      .success()
  );

  let ws = workspace_root(&repo, "feat");

  std::fs::write(ws.join("f.txt"), "f").unwrap();

  assert!(
    jj()
      .current_dir(&ws)
      .args(["commit", "-m", "feat work"])
      .status()
      .unwrap()
      .success()
  );
  assert!(
    jj()
      .current_dir(&repo)
      .args(["bookmark", "set", "cs/pr", "-r", "feat@-"])
      .status()
      .unwrap()
      .success()
  );
  assert_eq!(list_has_remote(&repo, home.path(), "feat"), false);
  assert!(
    jj()
      .current_dir(&repo)
      .args(["git", "push", "-b", "cs/pr"])
      .status()
      .unwrap()
      .success()
  );
  assert_eq!(list_has_remote(&repo, home.path(), "feat"), true);
}

#[test]
fn switch_to_trunk_name_goes_to_default_workspace() {
  if which::which("jj").is_err() {
    return;
  }

  let (_tmp, repo, home) = init_with_origin();
  let out = jjwt_cmd(home.path())
    .current_dir(&repo)
    .args(["switch", "main", "--no-hooks"])
    .output()
    .unwrap();

  assert!(
    out.status.success(),
    "{}",
    String::from_utf8_lossy(&out.stderr)
  );

  let ws = jj()
    .current_dir(&repo)
    .args(["workspace", "list", "-T", r#"name ++ "\n""#])
    .output()
    .unwrap();

  assert_eq!(String::from_utf8_lossy(&ws.stdout).trim(), "default");
}

#[test]
fn switch_create_with_caret_base_starts_on_trunk() {
  if which::which("jj").is_err() {
    return;
  }

  let (_tmp, repo, home) = init_with_origin();
  let out = jjwt_cmd(home.path())
    .current_dir(&repo)
    .args(["switch", "--create", "x", "--base", "^", "--no-hooks"])
    .output()
    .unwrap();

  assert!(
    out.status.success(),
    "{}",
    String::from_utf8_lossy(&out.stderr)
  );

  let parent = jj()
    .current_dir(&repo)
    .args([
      "log",
      "--no-graph",
      "-r",
      "x@- & main@origin",
      "-T",
      "commit_id",
    ])
    .output()
    .unwrap();

  assert!(!parent.stdout.is_empty());
}

#[test]
fn switch_create_with_unknown_base_leaves_no_workspace() {
  if which::which("jj").is_err() {
    return;
  }

  let (_tmp, repo, home) = init_with_origin();
  let out = jjwt_cmd(home.path())
    .current_dir(&repo)
    .args(["switch", "--create", "y", "--base", "nope", "--no-hooks"])
    .output()
    .unwrap();

  assert!(!out.status.success());

  let ws = jj()
    .current_dir(&repo)
    .args(["workspace", "list", "-T", r#"name ++ "\n""#])
    .output()
    .unwrap();

  assert_eq!(String::from_utf8_lossy(&ws.stdout).trim(), "default");
}
