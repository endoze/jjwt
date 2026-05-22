use indexmap::IndexMap;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
  #[error("config parse error: {0}")]
  ConfigParse(String),
  #[error("template render error: {0}")]
  TemplateRender(String),
  #[error("hook '{0}' not found in config")]
  HookNotFound(String),
  #[error("hook '{0}' is ambiguous: appears in multiple groups")]
  HookAmbiguous(String),
  #[error("workspace '{0}' already exists")]
  WorkspaceExists(String),
  #[error("workspace '{0}' does not exist")]
  WorkspaceMissing(String),
  #[error("workspace '{0}' has uncommitted changes (use --force)")]
  WorkspaceDirty(String),
  #[error("bookmark '{0}' is not fully merged into trunk (use --force)")]
  BookmarkUnmerged(String),
  #[error("not inside a jj repo")]
  NotJjRepo,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct Config {
  #[serde(default)]
  pub list: Option<ListConfig>,
  #[serde(rename = "pre-start", default)]
  pub pre_start: Vec<HookGroup>,
  #[serde(rename = "pre-remove", default)]
  pub pre_remove: Vec<HookGroup>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ListConfig {
  pub url: String,
}

pub type HookGroup = IndexMap<String, String>;

#[derive(Debug, Clone)]
pub struct RenderContext {
  pub branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
  pub name: String,
  pub path: PathBuf,
  pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedState {
  pub repo_root: PathBuf,
  pub is_jj_repo: bool,
  pub workspaces: Vec<Workspace>,
  /// Whether the target workspace path already exists on disk (for switch --create).
  pub target_path_exists: bool,
  /// `jj status` output non-empty for the target workspace (for remove).
  pub target_workspace_dirty: bool,
  /// Whether the bookmark's target is an ancestor of trunk (for remove).
  pub target_bookmark_merged: bool,
  /// Whether the bookmark exists at all (for remove).
  pub target_bookmark_exists: bool,
  /// Workspace name that `target_name` resolves to when it isn't itself a
  /// workspace. Set when `target_name` equals the trunk bookmark, in which
  /// case it resolves to "default". Mirrors worktrunk's behavior of using
  /// the default branch name to address the root worktree.
  pub target_resolved_workspace: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
  JjWorkspaceAdd {
    name: String,
    path: PathBuf,
  },
  JjBookmarkCreate {
    name: String,
    workspace: String,
  },
  JjWorkspaceForget {
    name: String,
  },
  JjBookmarkDelete {
    name: String,
  },
  JjWorkspaceUpdateStale {
    name: String,
  },
  DeleteDir {
    path: PathBuf,
  },
  RunHook {
    name: String,
    rendered_cmd: String,
    cwd: PathBuf,
    env: Vec<(String, String)>,
  },
  PrintLine(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
  pub actions: Vec<Action>,
}

impl Plan {
  pub fn new() -> Self {
    Self::default()
  }
  pub fn push(&mut self, a: Action) {
    self.actions.push(a);
  }
}

#[derive(Debug, Clone)]
pub struct SwitchArgs {
  pub name: String,
  pub create: bool,
  pub rerun_hooks: bool,
}

#[derive(Debug, Clone)]
pub struct RemoveArgs {
  pub name: String,
  pub force: bool,
}

#[derive(Debug, Clone)]
pub struct HookArgs {
  pub name: String,
  pub current_workspace: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrunkRel {
  /// Bookmark's @ equals trunk exactly.
  IsTrunk,
  /// Bookmark's @ is an ancestor of trunk (merged in).
  Ancestor,
  /// Diverged from trunk (both ahead and behind).
  Diverged,
  /// Strictly ahead of trunk.
  Ahead,
  /// Strictly behind trunk.
  Behind,
  /// No measurable relationship (e.g. unborn).
  None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StatusFlags {
  /// The `@` commit has a non-empty diff vs its parent (jj analog of
  /// worktrunk's "staged" indicator).
  pub has_changes: bool,
  /// Tracked files have working-copy modifications.
  pub modified: bool,
  /// Untracked files present.
  pub untracked: bool,
  /// Workspace is stale.
  pub stale: bool,
  /// Working copy has conflicts.
  pub conflicts: bool,
  /// The bookmark has a remote-tracking variant (e.g. `<name>@origin`).
  pub has_remote: bool,
  /// Relationship of this workspace's `@` to trunk.
  pub vs_trunk: Option<TrunkRel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LineDiff {
  pub added: u32,
  pub removed: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AheadBehind {
  pub ahead: u32,
  pub behind: u32,
}

/// Per-workspace details gathered by the shell from `jj` for rendering
/// the list table. Pure data — the core never reads from `jj`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkspaceDetails {
  pub modified: bool,
  pub untracked: bool,
  pub conflicts: bool,
  /// `@` equals `trunk()` exactly.
  pub is_trunk: bool,
  /// `@` is an ancestor of `trunk()`.
  pub is_ancestor_of_trunk: bool,
  /// Short change ID (8 chars).
  pub commit_short: String,
  /// Seconds since `@`'s committer timestamp.
  pub age_seconds: i64,
  /// First line of `@`'s description.
  pub message_first_line: String,
  /// Working-copy line diff (`jj diff -r @ --stat`).
  pub head_added: u32,
  pub head_removed: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedListRow {
  pub workspace: Workspace,
  pub details: WorkspaceDetails,
  pub ahead: u32,
  pub behind: u32,
  /// True when the bookmark for this workspace has a remote-tracking
  /// variant (e.g. `<name>@origin`).
  pub has_remote_bookmark: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObservedListState {
  pub repo_root: PathBuf,
  pub is_jj_repo: bool,
  /// Name of the workspace whose path contains cwd, if any.
  pub current_workspace: Option<String>,
  pub rows: Vec<ObservedListRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRow {
  /// Workspace name (also bookmark name by jjwt convention).
  pub name: String,
  /// Absolute on-disk path of the workspace.
  pub path: PathBuf,
  /// Rendered from `[list].url`; "" if no config.
  pub url: String,
  /// Workspace whose path contains cwd.
  pub is_current: bool,
  /// Workspace name is "default" (lives at repo root).
  pub is_default: bool,
  pub status: StatusFlags,
  /// Working-copy line diff (`jj diff -r @ --stat`).
  pub head_diff: LineDiff,
  pub vs_trunk: AheadBehind,
  /// 8-char short change ID for the workspace's `@`.
  pub commit: String,
  /// Pre-formatted relative age (e.g. "9h", "2w", "1mo").
  pub age: String,
  /// First line of `@`'s description.
  pub message: String,
}
