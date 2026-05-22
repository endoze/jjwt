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

  fn workspace_add(&self, _r: &Path, n: &str, p: &Path) -> Result<()> {
    self
      .calls
      .borrow_mut()
      .push(format!("workspace_add {n} {}", p.display()));

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

  fn workspace_details(&self, _r: &Path, _w: &str) -> Result<WorkspaceDetails> {
    Ok(WorkspaceDetails::default())
  }

  fn workspace_ahead_behind_trunk(&self, _r: &Path, _w: &str) -> Result<(u32, u32)> {
    Ok((0, 0))
  }

  fn bookmarks_with_remote(&self, _r: &Path) -> Result<std::collections::HashSet<String>> {
    Ok(std::collections::HashSet::new())
  }

  fn trunk_bookmark(&self, _r: &Path) -> Result<Option<String>> {
    Ok(None)
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
