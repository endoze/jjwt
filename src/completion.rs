//! Shell completion engine — `COMPLETE=$SHELL jjwt` integration.
//!
//! When the binary is invoked with the `COMPLETE` environment variable set,
//! [`maybe_handle_env_completion`] intercepts before normal CLI dispatch and
//! emits either a registration script (no args after `--`) or completion
//! candidates (args provided). Per-arg completers attached via
//! `#[arg(add = ...)]` in `src/cli.rs` produce dynamic candidates from the
//! current jj repo state and merged config.
#![cfg(not(tarpaulin_include))]

use std::cell::{Cell, RefCell};
use std::ffi::{OsStr, OsString};
use std::io::Write;

use clap::Command;
use clap_complete::engine::{ArgValueCompleter, CompletionCandidate, ValueCompleter};
use clap_complete::env::CompleteEnv;

use crate::shell::config_loader::load_merged_config;
use crate::shell::jj::Jj;
use crate::shell::jj_lib::JjLib;

/// Handle shell-initiated completion requests via `COMPLETE=$SHELL jjwt`.
///
/// Returns `true` when completion was handled (caller should exit), `false`
/// when normal CLI dispatch should proceed.
pub fn maybe_handle_env_completion() -> bool {
  let Some(shell_name) = std::env::var_os("COMPLETE") else {
    return false;
  };

  if shell_name.is_empty() || shell_name == "0" {
    return false;
  }

  let mut args: Vec<OsString> = std::env::args_os().collect();

  CONTEXT.with(|ctx| *ctx.borrow_mut() = Some(CompletionContext { args: args.clone() }));
  SUPPRESS_ALL.with(|s| s.set(false));

  args.remove(0);
  let escape_index = args
    .iter()
    .position(|a| *a == "--")
    .map(|i| i + 1)
    .unwrap_or(args.len());

  args.drain(0..escape_index);

  let current_dir = std::env::current_dir().ok();

  if args.is_empty() {
    let all_args: Vec<OsString> = std::env::args_os().collect();
    let _ = CompleteEnv::with_factory(completion_command)
      .try_complete(all_args, current_dir.as_deref());

    CONTEXT.with(|ctx| ctx.borrow_mut().take());

    return true;
  }

  let mut cmd = completion_command();

  cmd.build();

  let index: usize = std::env::var("_CLAP_COMPLETE_INDEX")
    .ok()
    .and_then(|i| i.parse().ok())
    .unwrap_or_else(|| args.len() - 1);

  let current_word = args.get(index).map(|s| s.to_string_lossy().into_owned());

  let completions =
    match clap_complete::engine::complete(&mut cmd, args.clone(), index, current_dir.as_deref()) {
      Ok(c) => c,
      Err(_) => {
        CONTEXT.with(|ctx| ctx.borrow_mut().take());

        return true;
      }
    };

  // If a completer decided to fully suppress the slot (e.g. `switch --create`
  // wants a fresh workspace name), drop clap's flag fallbacks too — the user
  // shouldn't be encouraged to keep typing flags here.
  let completions = if SUPPRESS_ALL.with(|s| s.get()) {
    Vec::new()
  } else {
    completions
  };

  let shell_name = shell_name.to_string_lossy();
  // Bash doesn't filter COMPREPLY by prefix — its programmable completion
  // passes the array verbatim. Fish/zsh apply their own matching, so they
  // receive all candidates.
  let completions = if shell_name.as_ref() == "bash" {
    let prefix = current_word.as_deref().unwrap_or("").to_owned();

    if prefix.is_empty() {
      completions
    } else {
      completions
        .into_iter()
        .filter(|c| c.get_value().to_string_lossy().starts_with(&*prefix))
        .collect()
    }
  } else {
    completions
  };

  let ifs = std::env::var("_CLAP_IFS").ok();
  let separator = ifs.as_deref().unwrap_or("\n");

  let help_sep = match shell_name.as_ref() {
    "zsh" => Some(":"),
    "fish" | "nu" => Some("\t"),
    _ => None,
  };

  let mut stdout = std::io::stdout();

  for (i, candidate) in completions.iter().enumerate() {
    if i != 0 {
      let _ = write!(stdout, "{separator}");
    }

    let value = candidate.get_value().to_string_lossy();

    match (help_sep, candidate.get_help()) {
      (Some(sep), Some(help)) => {
        let _ = write!(stdout, "{value}{sep}{help}");
      }
      _ => {
        let _ = write!(stdout, "{value}");
      }
    }
  }

  CONTEXT.with(|ctx| ctx.borrow_mut().take());

  true
}

/// Workspace name completer for positional args (no context-based filtering).
pub(crate) fn workspace_completer() -> ArgValueCompleter {
  ArgValueCompleter::new(WorkspaceCompleter {
    suppress_on_create: false,
  })
}

/// Workspace name completer that suppresses candidates when `--create`/`-c`
/// is present (the target workspace doesn't exist yet).
pub(crate) fn workspace_completer_restricted() -> ArgValueCompleter {
  ArgValueCompleter::new(WorkspaceCompleter {
    suppress_on_create: true,
  })
}

/// Local bookmark name completer.
pub(crate) fn bookmark_completer() -> ArgValueCompleter {
  ArgValueCompleter::new(BookmarkCompleter)
}

/// Hook name completer — merges configured hook names across all six lifecycle
/// slots from user + project config.
pub(crate) fn hook_completer() -> ArgValueCompleter {
  ArgValueCompleter::new(HookCompleter)
}

/// Alias name completer — names from the merged user + project config.
#[allow(dead_code)]
pub(crate) fn alias_completer() -> ArgValueCompleter {
  ArgValueCompleter::new(AliasCompleter)
}

#[derive(Clone, Copy)]
struct WorkspaceCompleter {
  suppress_on_create: bool,
}

impl ValueCompleter for WorkspaceCompleter {
  fn complete(&self, current: &OsStr) -> Vec<CompletionCandidate> {
    if current.to_str().is_some_and(|s| s.starts_with('-')) {
      return Vec::new();
    }

    if self.suppress_on_create && suppress_with_create() {
      SUPPRESS_ALL.with(|s| s.set(true));

      return Vec::new();
    }

    let Ok(cwd) = std::env::current_dir() else {
      return Vec::new();
    };

    let Ok(jj) = JjLib::new(&cwd) else {
      return Vec::new();
    };

    let Ok(repo_root) = jj.repo_root(&cwd) else {
      return Vec::new();
    };

    let workspaces = match jj.workspace_list(&repo_root) {
      Ok(w) => w,
      Err(_) => return Vec::new(),
    };

    workspaces
      .into_iter()
      .map(|w| CompletionCandidate::new(w.name))
      .collect()
  }
}

#[derive(Clone, Copy)]
struct BookmarkCompleter;

impl ValueCompleter for BookmarkCompleter {
  fn complete(&self, current: &OsStr) -> Vec<CompletionCandidate> {
    if current.to_str().is_some_and(|s| s.starts_with('-')) {
      return Vec::new();
    }

    let Ok(cwd) = std::env::current_dir() else {
      return Vec::new();
    };

    let Ok(jj) = JjLib::new(&cwd) else {
      return Vec::new();
    };

    let Ok(repo_root) = jj.repo_root(&cwd) else {
      return Vec::new();
    };

    let bookmarks = match jj.bookmarks_local(&repo_root) {
      Ok(b) => b,
      Err(_) => return Vec::new(),
    };

    bookmarks.into_iter().map(CompletionCandidate::new).collect()
  }
}

#[derive(Clone, Copy)]
struct HookCompleter;

impl ValueCompleter for HookCompleter {
  fn complete(&self, current: &OsStr) -> Vec<CompletionCandidate> {
    if current.to_str().is_some_and(|s| s.starts_with('-')) {
      return Vec::new();
    }

    let Ok(cwd) = std::env::current_dir() else {
      return Vec::new();
    };

    let Ok(cfg) = load_merged_config(&cwd, None) else {
      return Vec::new();
    };

    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    for (_hook_type, groups) in cfg.all_hook_groups() {
      for shg in groups {
        for name in shg.group.keys() {
          seen.insert(name.clone());
        }
      }
    }

    seen.into_iter().map(CompletionCandidate::new).collect()
  }
}

#[derive(Clone, Copy)]
struct AliasCompleter;

impl ValueCompleter for AliasCompleter {
  fn complete(&self, current: &OsStr) -> Vec<CompletionCandidate> {
    if current.to_str().is_some_and(|s| s.starts_with('-')) {
      return Vec::new();
    }

    let Ok(cwd) = std::env::current_dir() else {
      return Vec::new();
    };

    let Ok(cfg) = load_merged_config(&cwd, None) else {
      return Vec::new();
    };

    cfg
      .aliases
      .keys()
      .cloned()
      .map(CompletionCandidate::new)
      .collect()
  }
}

/// Check the cached full argv for `--create` or `-c`. Used to suppress
/// workspace completion on `switch --create <name>` where the workspace
/// doesn't yet exist.
fn suppress_with_create() -> bool {
  CONTEXT.with(|ctx| {
    ctx
      .borrow()
      .as_ref()
      .is_some_and(|ctx| ctx.contains("--create") || ctx.contains("-c"))
  })
}

struct CompletionContext {
  args: Vec<OsString>,
}

impl CompletionContext {
  fn contains(&self, needle: &str) -> bool {
    self
      .args
      .iter()
      .any(|arg| arg.to_string_lossy().as_ref() == needle)
  }
}

// `ValueCompleter::complete()` only receives the current word — but suppression
// rules (e.g. hiding workspaces when `--create` is present) need the full argv.
// Stash it in thread-local during `maybe_handle_env_completion`.
thread_local! {
  static CONTEXT: RefCell<Option<CompletionContext>> = const { RefCell::new(None) };
  /// Set by a completer when it wants to wipe the entire candidate list, including
  /// clap's flag fallbacks. Read once after `engine::complete` returns.
  static SUPPRESS_ALL: Cell<bool> = const { Cell::new(false) };
}

/// Build the clap `Command` used during completion. Mirrors the live CLI tree
/// but injects configured alias names as top-level subcommand stubs so
/// `jjwt <Tab>` offers them.
fn completion_command() -> Command {
  inject_alias_subcommands(crate::cli::command())
}

/// Inject configured alias names as subcommand stubs into the clap tree.
///
/// Aliases are dispatched at runtime via `#[command(external_subcommand)]` —
/// they don't appear in the static clap tree. For completion they need to
/// show up as siblings of built-in subcommands. Stubs only carry an `about`
/// string; arg semantics aren't represented (alias forwarding eats everything
/// after the name).
fn inject_alias_subcommands(mut cmd: Command) -> Command {
  let cwd = std::env::current_dir().unwrap_or_default();

  let Ok(cfg) = load_merged_config(&cwd, None) else {
    return cmd;
  };

  for name in cfg.aliases.keys() {
    if cmd.get_subcommands().any(|s| s.get_name() == name.as_str()) {
      continue;
    }

    let leaked: &'static str = Box::leak(name.clone().into_boxed_str());

    cmd = cmd.subcommand(Command::new(leaked).about("alias"));
  }

  cmd
}
