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
  #[error(
    "path '{0}' already exists at the target workspace location (use --clobber to remove it)"
  )]
  TargetPathOccupied(String),
  #[error("path '{0}' is inside another workspace and cannot be clobbered")]
  TargetPathInsideOtherWorkspace(String),
  #[error("workspace '{0}' does not exist")]
  WorkspaceMissing(String),
  #[error("workspace '{0}' has uncommitted changes (use --force)")]
  WorkspaceDirty(String),
  #[error("bookmark '{0}' is not fully merged into trunk (use --force)")]
  BookmarkUnmerged(String),
  #[error("not inside a jj repo")]
  NotJjRepo,
  #[error("alias '{0}' not found in config")]
  AliasNotFound(String),
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct Config {
  #[serde(default)]
  pub list: Option<ListConfig>,
  #[serde(
    rename = "pre-switch",
    default,
    deserialize_with = "crate::core::config::deserialize_hook_groups"
  )]
  pub pre_switch: Vec<HookGroup>,
  #[serde(
    rename = "post-switch",
    default,
    deserialize_with = "crate::core::config::deserialize_hook_groups"
  )]
  pub post_switch: Vec<HookGroup>,
  #[serde(
    rename = "pre-start",
    default,
    deserialize_with = "crate::core::config::deserialize_hook_groups"
  )]
  pub pre_start: Vec<HookGroup>,
  #[serde(
    rename = "post-start",
    default,
    deserialize_with = "crate::core::config::deserialize_hook_groups"
  )]
  pub post_start: Vec<HookGroup>,
  #[serde(
    rename = "pre-remove",
    default,
    deserialize_with = "crate::core::config::deserialize_hook_groups"
  )]
  pub pre_remove: Vec<HookGroup>,
  #[serde(
    rename = "post-remove",
    default,
    deserialize_with = "crate::core::config::deserialize_hook_groups"
  )]
  pub post_remove: Vec<HookGroup>,
  #[serde(rename = "background-remove", default)]
  pub background_remove: Option<bool>,
  /// Custom subcommands. Each entry maps `jjwt <name>` to a template
  /// rendered with the standard hook variables; the result is executed
  /// via `sh -c` with stdio inherited from the parent process.
  #[serde(default)]
  pub aliases: IndexMap<String, String>,
  #[serde(rename = "worktree-path", default)]
  pub worktree_path_template: Option<String>,
}

impl Config {
  /// Iterate (hook_type, group) pairs over every configured hook group.
  /// Used by `jjwt hook` for cross-group lookups and (in 1B.13)
  /// `hook show`.
  pub fn all_hook_groups(&self) -> Vec<(&'static str, &[HookGroup])> {
    vec![
      ("pre-switch", self.pre_switch.as_slice()),
      ("post-switch", self.post_switch.as_slice()),
      ("pre-start", self.pre_start.as_slice()),
      ("post-start", self.post_start.as_slice()),
      ("pre-remove", self.pre_remove.as_slice()),
      ("post-remove", self.post_remove.as_slice()),
    ]
  }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ListConfig {
  pub url: String,
}

pub type HookGroup = IndexMap<String, String>;

#[derive(Debug, Clone, Default)]
pub struct RenderContext {
  /// Branch/workspace name the operation targets.
  pub branch: String,
  /// Absolute path of the workspace this template is being rendered for.
  pub worktree_path: Option<PathBuf>,
  /// Workspace directory name (`basename(worktree_path)` when present).
  pub worktree_name: Option<String>,
  /// Repository root directory name.
  pub repo: Option<String>,
  /// Absolute path of the repository root.
  pub repo_path: Option<PathBuf>,
  /// Directory the hook command will run in (often the same as
  /// `worktree_path`; differs for some hook types we don't yet emit).
  pub cwd: Option<PathBuf>,
  /// Hook type being rendered, e.g. `pre-start`.
  pub hook_type: Option<String>,
  /// Named key of the hook command inside its group.
  pub hook_name: Option<String>,
  /// Tokens forwarded from the CLI to a manually-invoked hook
  /// (`jjwt hook <type> -- <args>`).
  pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
  pub name: String,
  pub path: PathBuf,
  pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObservedState {
  pub repo_root: PathBuf,
  pub is_jj_repo: bool,
  pub workspaces: Vec<Workspace>,
  /// Workspace whose path contains cwd (deepest match), if any.
  pub current_workspace: Option<String>,
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
  DeleteDirBackground {
    path: PathBuf,
  },
  JjWorkspaceRename {
    old_name: String,
    new_name: String,
  },
  RenameDir {
    from: PathBuf,
    to: PathBuf,
  },
  JjBookmarkRename {
    old_name: String,
    new_name: String,
  },
  RunHook {
    name: String,
    rendered_cmd: String,
    cwd: PathBuf,
    env: Vec<(String, String)>,
  },
  /// Run a command with stdio inherited from the parent process. Used by
  /// `jjwt <alias>` and (in 1B.17) `jjwt switch -x`. A non-zero exit
  /// becomes an error so the surrounding plan halts.
  Exec {
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

/// Output format negotiated by `--format`. Text is the default; JSON is
/// emitted as a single line on the same `PrintLine` action so the runtime
/// is oblivious to the format choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
  #[default]
  Text,
  Json,
}

#[derive(Debug, Clone, Default)]
pub struct SwitchArgs {
  pub name: String,
  pub create: bool,
  pub rerun_hooks: bool,
  /// Skip all hooks for this invocation. Set by `--no-hooks`
  /// (and the deprecated `--no-verify` alias).
  pub no_hooks: bool,
  /// Optional command template to run after switching. Equivalent to
  /// worktrunk's `-x`. The template is expanded with the standard hook
  /// variables; the rendered command is emitted to the shell wrapper as
  /// an `exec:` directive.
  pub execute: Option<String>,
  /// Remove a stale directory at the target workspace path before
  /// creating the workspace. Worktrunk's `--clobber`. Refused when the
  /// stale path lives inside another registered workspace.
  pub clobber: bool,
  pub format: OutputFormat,
}

#[derive(Debug, Clone, Default)]
pub struct RemoveArgs {
  pub name: String,
  /// Force worktree removal: bypass the "uncommitted changes" check.
  /// Worktrunk's `-f`.
  pub force: bool,
  /// Skip all hooks for this invocation.
  pub no_hooks: bool,
  /// Never delete the bookmark, even if it is merged into trunk.
  /// Worktrunk's `--no-delete-branch`.
  pub no_delete_branch: bool,
  /// Delete the bookmark even when not merged into trunk. Worktrunk's
  /// `-D` / `--force-delete`.
  pub force_delete: bool,
  pub format: OutputFormat,
}

#[derive(Debug, Clone)]
pub struct HookArgs {
  pub name: String,
  pub current_workspace: String,
}

#[derive(Debug, Clone)]
pub struct AliasArgs {
  pub name: String,
  /// Tokens forwarded from the CLI; bound to `{{ args }}` in the template.
  pub forwarded: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RelocateArgs {
  pub old_name: String,
  pub new_name: String,
  pub rename_bookmark: bool,
  pub format: OutputFormat,
}

#[derive(Debug, Clone, Default)]
pub struct PruneArgs {
  pub dry_run: bool,
  pub no_hooks: bool,
  pub format: OutputFormat,
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

/// Commit metadata and diff stats for a workspace's `@`, gathered in batch
/// via a single `jj log` call across all workspaces. Includes fields that
/// previously required separate `jj status` and `jj diff --stat` calls.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommitInfo {
  /// Short change ID (8 chars).
  pub commit_short: String,
  /// Seconds since `@`'s committer timestamp.
  pub age_seconds: i64,
  /// First line of `@`'s description.
  pub message_first_line: String,
  /// Working copy has unresolved conflicts.
  pub conflicts: bool,
  /// Lines added in `@`'s diff vs parent.
  pub head_added: u32,
  /// Lines removed in `@`'s diff vs parent.
  pub head_removed: u32,
}

/// Per-workspace details gathered by the shell from `jj` for rendering
/// the list table. Pure data — the core never reads from `jj`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkspaceDetails {
  pub modified: bool,
  pub untracked: bool,
  pub conflicts: bool,
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

/// State for the prune command: all workspaces with their merge status.
#[derive(Debug, Clone, Default)]
pub struct ObservedPruneState {
  pub repo_root: PathBuf,
  pub is_jj_repo: bool,
  pub current_workspace: Option<String>,
  pub workspaces: Vec<Workspace>,
  /// Per-workspace: (bookmark_exists, bookmark_merged, workspace_dirty).
  pub workspace_status: Vec<(String, bool, bool, bool)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObservedListState {
  pub repo_root: PathBuf,
  pub is_jj_repo: bool,
  /// Name of the workspace whose path contains cwd, if any.
  pub current_workspace: Option<String>,
  pub rows: Vec<ObservedListRow>,
  /// Names of bookmarks without a workspace, only populated when the
  /// caller asked for `--branches`.
  pub extra_branch_names: Vec<String>,
  /// Names of remote-only bookmarks, only populated when the caller
  /// asked for `--remotes`. Format: bare local name (the `@<remote>`
  /// suffix is stripped).
  pub extra_remote_only_names: Vec<String>,
}

/// What kind of row this is. `Workspace` rows have a real path and full
/// observation details. `Branch` rows are bookmarks without a workspace
/// (either local-only-no-worktree or remote-only) and have empty
/// working-copy state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListRowKind {
  #[default]
  Workspace,
  Branch,
}

/// Options that gate which rows `observe_list` collects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ListOptions {
  /// Include local bookmarks that don't have a workspace.
  pub include_branches: bool,
  /// Include remote-only bookmarks (`<name>@<remote>` with no local).
  pub include_remotes: bool,
  /// Reserved for later — adds extra columns. Phase 1 plumbs the flag;
  /// the renderer keeps the existing column layout.
  pub full: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRow {
  /// Workspace name (also bookmark name by jjwt convention).
  pub name: String,
  /// Absolute on-disk path of the workspace (empty for `Branch` rows).
  pub path: PathBuf,
  pub kind: ListRowKind,
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
