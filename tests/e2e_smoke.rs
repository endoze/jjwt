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

    assert!(jj().arg("git").arg("init").arg(repo).status().unwrap().success());

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
    assert!(jj().current_dir(repo).arg("new").status().unwrap().success());

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
