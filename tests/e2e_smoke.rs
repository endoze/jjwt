use std::process::Command;
use tempfile::TempDir;

fn jjwt_bin() -> std::path::PathBuf {
  std::path::PathBuf::from(env!("CARGO_BIN_EXE_jjwt"))
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

  let out = Command::new(jjwt_bin())
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

  assert!(
    printed_path.ends_with(".worktrees/test-branch"),
    "expected workspace path on last line, got: {stdout:?}"
  );

  let ws_path = repo.join(".worktrees/test-branch");

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

  let out = Command::new(jjwt_bin())
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
  std::fs::write(repo.join(".worktrees/alpha/scratch.txt"), "scratch").unwrap();

  let list_out = Command::new(jjwt_bin())
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

  // Both workspaces present
  assert!(
    text.contains("default"),
    "default workspace row missing:\n{text}"
  );
  assert!(
    text.contains("alpha"),
    "alpha workspace row missing:\n{text}"
  );

  // Default workspace path is `.` somewhere on its row
  assert!(
    text
      .lines()
      .any(|l| l.contains("default") && l.contains(" . ")),
    "default row should show '.' as path:\n{text}"
  );

  // Alpha workspace path is `./.worktrees/alpha`
  assert!(
    text.contains("./.worktrees/alpha"),
    "alpha row should show './.worktrees/alpha':\n{text}"
  );

  // URL templated per branch
  assert!(
    text.contains("http://example.com/alpha"),
    "URL should render with branch substitution:\n{text}"
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
}

#[test]
fn list_works_from_inside_a_workspace_subdir() {
  if which::which("jj").is_err() {
    eprintln!("skipping e2e: jj not on PATH");

    return;
  }

  let tmp = TempDir::new().unwrap();
  let repo = tmp.path();

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
  std::fs::write(repo.join(".config/wt.toml"), "").unwrap();

  let out = Command::new(jjwt_bin())
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
  let list_out = Command::new(jjwt_bin())
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
