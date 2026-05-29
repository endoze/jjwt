use std::path::Path;
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

#[test]
fn switch_create_produces_workspace_bookmark_and_hook_output() {
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
    bm_text.lines().any(|l| l.trim() == "test-branch"),
    "bookmark missing; got:\n{bm_text}"
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
    .args(["remove", "-f", "gamma"])
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