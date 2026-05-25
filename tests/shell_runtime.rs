use anyhow::Result;
use jjwt::core::types::*;
use jjwt::shell::fs::Fs;
use jjwt::shell::jj::Jj;
use jjwt::shell::proc::{Proc, ProcOutput};
use jjwt::shell::runtime::{Runtime, execute};
use std::cell::RefCell;
use std::path::{Path, PathBuf};

#[derive(Default)]
struct FakeJj {
  calls: RefCell<Vec<String>>,
}

#[derive(Default)]
struct FakeFs {
  calls: RefCell<Vec<String>>,
  deleted: RefCell<Vec<PathBuf>>,
}

#[derive(Default)]
struct FakeProc {
  calls: RefCell<Vec<String>>,
  fail_on: Option<String>,
}

impl Jj for FakeJj {
  fn repo_root(&self, _start: &Path) -> Result<PathBuf> {
    Ok(PathBuf::from("/repo"))
  }

  fn workspace_list(&self, _r: &Path) -> Result<Vec<Workspace>> {
    Ok(vec![])
  }

  fn workspace_add(&self, _r: &Path, n: &str, p: &Path, rev: Option<&str>) -> Result<()> {
    let rev_str = rev.map(|r| format!(" @{r}")).unwrap_or_default();

    self
      .calls
      .borrow_mut()
      .push(format!("workspace_add {n} {}{rev_str}", p.display()));

    Ok(())
  }

  fn workspace_forget(&self, _r: &Path, n: &str) -> Result<()> {
    self
      .calls
      .borrow_mut()
      .push(format!("workspace_forget {n}"));

    Ok(())
  }

  fn workspace_update_stale(&self, _r: &Path, n: &str) -> Result<()> {
    self
      .calls
      .borrow_mut()
      .push(format!("workspace_update_stale {n}"));

    Ok(())
  }

  fn bookmark_create(&self, _r: &Path, n: &str, w: &str) -> Result<()> {
    self
      .calls
      .borrow_mut()
      .push(format!("bookmark_create {n} {w}"));

    Ok(())
  }

  fn bookmark_delete(&self, _r: &Path, n: &str) -> Result<()> {
    self.calls.borrow_mut().push(format!("bookmark_delete {n}"));

    Ok(())
  }

  fn bookmark_exists(&self, _r: &Path, _n: &str) -> Result<bool> {
    Ok(false)
  }

  fn bookmark_is_merged_into_trunk(&self, _r: &Path, _n: &str) -> Result<bool> {
    Ok(false)
  }

  fn workspace_is_dirty(&self, _r: &Path, _w: &str) -> Result<bool> {
    Ok(false)
  }

  fn workspace_status(&self, _r: &Path, _w: &str) -> Result<(bool, bool)> {
    Ok((false, false))
  }

  fn workspace_commit_info_batch(
    &self,
    _r: &Path,
    _w: &[String],
  ) -> Result<std::collections::HashMap<String, jjwt::core::types::CommitInfo>> {
    Ok(std::collections::HashMap::new())
  }

  fn workspace_ahead_behind_trunk(&self, _r: &Path, _w: &str) -> Result<(u32, u32)> {
    Ok((0, 0))
  }

  fn workspace_ahead_behind_batch(
    &self,
    _r: &Path,
    _w: &[String],
  ) -> Result<std::collections::HashMap<String, (u32, u32)>> {
    Ok(std::collections::HashMap::new())
  }

  fn bookmarks_with_remote(&self, _r: &Path) -> Result<std::collections::HashSet<String>> {
    Ok(std::collections::HashSet::new())
  }

  fn bookmarks_local(&self, _r: &Path) -> Result<Vec<String>> {
    Ok(Vec::new())
  }

  fn bookmark_sets(&self, _r: &Path) -> Result<(Vec<String>, std::collections::HashSet<String>)> {
    Ok((Vec::new(), std::collections::HashSet::new()))
  }

  fn trunk_bookmark(&self, _r: &Path) -> Result<Option<String>> {
    Ok(None)
  }

  fn git_fetch(&self, _r: &Path) -> Result<()> {
    self.calls.borrow_mut().push("git_fetch".into());

    Ok(())
  }

  fn workspace_rename(&self, _r: &Path, old: &str, new: &str) -> Result<()> {
    self
      .calls
      .borrow_mut()
      .push(format!("workspace_rename {old} {new}"));

    Ok(())
  }

  fn bookmark_rename(&self, _r: &Path, old: &str, new: &str) -> Result<()> {
    self
      .calls
      .borrow_mut()
      .push(format!("bookmark_rename {old} {new}"));

    Ok(())
  }
}

impl Fs for FakeFs {
  fn exists(&self, _p: &Path) -> bool {
    false
  }

  fn remove_dir_all(&self, p: &Path) -> Result<()> {
    self.deleted.borrow_mut().push(p.to_path_buf());
    self
      .calls
      .borrow_mut()
      .push(format!("remove_dir_all {}", p.display()));

    Ok(())
  }

  fn current_dir(&self) -> Result<PathBuf> {
    Ok(PathBuf::from("/repo"))
  }

  fn rename(&self, from: &Path, to: &Path) -> Result<()> {
    self
      .calls
      .borrow_mut()
      .push(format!("rename {} {}", from.display(), to.display()));

    Ok(())
  }

  fn create_dir_all(&self, p: &Path) -> Result<()> {
    self
      .calls
      .borrow_mut()
      .push(format!("create_dir_all {}", p.display()));

    Ok(())
  }
}

impl Proc for FakeProc {
  fn run_sh(&self, cmd: &str, cwd: &Path, _env: &[(String, String)]) -> Result<ProcOutput> {
    self
      .calls
      .borrow_mut()
      .push(format!("sh -c {cmd} (cwd={})", cwd.display()));

    if self.fail_on.as_deref() == Some(cmd) {
      return Ok(ProcOutput {
        status: 1,
        stdout: String::new(),
        stderr: "boom".into(),
      });
    }

    Ok(ProcOutput {
      status: 0,
      stdout: String::new(),
      stderr: String::new(),
    })
  }

  fn run_sh_inherit(&self, cmd: &str, cwd: &Path, _env: &[(String, String)]) -> Result<i32> {
    self
      .calls
      .borrow_mut()
      .push(format!("exec {cmd} (cwd={})", cwd.display()));

    if self.fail_on.as_deref() == Some(cmd) {
      return Ok(1);
    }

    Ok(0)
  }

  fn spawn_detached(&self, program: &str, args: &[&str]) -> Result<()> {
    self
      .calls
      .borrow_mut()
      .push(format!("spawn_detached {} {}", program, args.join(" ")));

    Ok(())
  }
}

#[test]
fn execute_runs_actions_in_order() {
  let jj = FakeJj::default();
  let fs = FakeFs::default();
  let proc = FakeProc::default();
  let mut rt = Runtime::new(jj, fs, proc);

  let plan = Plan {
    actions: vec![
      Action::JjWorkspaceAdd {
        name: "x".into(),
        path: PathBuf::from("/repo/.worktrees/x"),
        revision: None,
      },
      Action::JjBookmarkCreate {
        name: "x".into(),
        workspace: "x".into(),
      },
      Action::RunHook {
        name: "h".into(),
        rendered_cmd: "echo hi".into(),
        cwd: PathBuf::from("/repo/.worktrees/x"),
        env: vec![("JJWT_WORKSPACE".into(), "x".into())],
        source: HookSource::Project,
      },
      Action::PrintLine("/repo/.worktrees/x".into()),
    ],
  };

  let printed = execute(&plan, &mut rt).expect("ok");
  let calls_jj = rt.jj.calls.borrow().clone();
  let calls_proc = rt.proc.calls.borrow().clone();

  assert_eq!(
    calls_jj,
    vec![
      "workspace_add x /repo/.worktrees/x".to_string(),
      "bookmark_create x x".to_string(),
    ]
  );
  assert_eq!(calls_proc.len(), 1);
  assert!(calls_proc[0].starts_with("sh -c echo hi"));
  assert_eq!(printed, vec!["/repo/.worktrees/x".to_string()]);
}

#[test]
fn execute_halts_on_hook_failure() {
  let mut rt = Runtime::new(
    FakeJj::default(),
    FakeFs::default(),
    FakeProc {
      fail_on: Some("bad".into()),
      ..Default::default()
    },
  );
  let plan = Plan {
    actions: vec![
      Action::RunHook {
        name: "bad-hook".into(),
        rendered_cmd: "bad".into(),
        cwd: PathBuf::from("/repo"),
        env: vec![],
        source: HookSource::Project,
      },
      Action::PrintLine("unreached".into()),
    ],
  };

  let err = execute(&plan, &mut rt).unwrap_err();
  let msg = format!("{err:#}");

  assert!(
    msg.contains("bad-hook"),
    "error should name the failing hook: {msg}"
  );
}

#[test]
fn execute_delete_dir_background_renames_and_spawns() {
  let mut rt = Runtime::new(FakeJj::default(), FakeFs::default(), FakeProc::default())
    .with_root(PathBuf::from("/repo"));

  let plan = Plan {
    actions: vec![Action::DeleteDirBackground {
      path: PathBuf::from("/repo/.worktrees/feat-x"),
    }],
  };

  execute(&plan, &mut rt).expect("ok");

  let fs_calls = rt.fs.calls.borrow().clone();
  let proc_calls = rt.proc.calls.borrow().clone();

  assert!(
    fs_calls
      .iter()
      .any(|c| c.starts_with("create_dir_all /repo/.jj/.jjwt-trash")),
    "should create trash dir: {fs_calls:?}"
  );
  assert!(
    fs_calls
      .iter()
      .any(|c| c.starts_with("rename /repo/.worktrees/feat-x")),
    "should rename workspace into trash: {fs_calls:?}"
  );
  assert!(
    proc_calls
      .iter()
      .any(|c| c.starts_with("spawn_detached rm -rf")),
    "should spawn detached rm: {proc_calls:?}"
  );
}