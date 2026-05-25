use indexmap::IndexMap;
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use thiserror::Error;

/// Default worktree-path template. Matches worktrunk's default: places
/// worktrees as siblings of the repository root.
pub const DEFAULT_WORKTREE_PATH_TEMPLATE: &str =
  "{{ repo_path }}/../{{ repo }}.{{ branch | sanitize }}";

/// Errors produced by core planning and configuration logic.
#[derive(Debug, Error)]
pub enum CoreError {
  /// TOML configuration could not be parsed.
  #[error("config parse error: {0}")]
  ConfigParse(String),
  /// Minijinja template rendering failed.
  #[error("template render error: {0}")]
  TemplateRender(String),
  /// Named hook does not exist in any configured hook group.
  #[error("hook '{0}' not found in config")]
  HookNotFound(String),
  /// Named hook appears in more than one hook group.
  #[error("hook '{0}' is ambiguous: appears in multiple groups")]
  HookAmbiguous(String),
  /// A workspace with this name is already registered.
  #[error("workspace '{0}' already exists")]
  WorkspaceExists(String),
  /// Target path exists on disk; pass `--clobber` to remove it.
  #[error(
    "path '{0}' already exists at the target workspace location (use --clobber to remove it)"
  )]
  TargetPathOccupied(String),
  /// Target path is nested inside another workspace's directory.
  #[error("path '{0}' is inside another workspace and cannot be clobbered")]
  TargetPathInsideOtherWorkspace(String),
  /// No workspace with this name is registered.
  #[error("workspace '{0}' does not exist")]
  WorkspaceMissing(String),
  /// Workspace has uncommitted changes and `--force` was not given.
  #[error("workspace '{0}' has uncommitted changes (use --force)")]
  WorkspaceDirty(String),
  /// Bookmark is not merged into trunk and forced deletion was not requested.
  #[error("bookmark '{0}' is not fully merged into trunk (use --force)")]
  BookmarkUnmerged(String),
  /// Current directory is not inside a jj repository.
  #[error("not inside a jj repo")]
  NotJjRepo,
  /// No alias with this name exists in the configuration.
  #[error("alias '{0}' not found in config")]
  AliasNotFound(String),
}

/// Top-level configuration parsed from a single `wt.toml` file.
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct Config {
  /// Settings for the `list` subcommand (URL template, summary toggle).
  #[serde(default)]
  pub list: Option<ListConfig>,
  /// Hooks to run before switching to a workspace.
  #[serde(
    rename = "pre-switch",
    default,
    deserialize_with = "crate::core::config::deserialize_hook_groups"
  )]
  pub pre_switch: Vec<HookGroup>,
  /// Hooks to run after switching to a workspace.
  #[serde(
    rename = "post-switch",
    default,
    deserialize_with = "crate::core::config::deserialize_hook_groups"
  )]
  pub post_switch: Vec<HookGroup>,
  /// Hooks to run before a workspace is created.
  #[serde(
    rename = "pre-start",
    default,
    deserialize_with = "crate::core::config::deserialize_hook_groups"
  )]
  pub pre_start: Vec<HookGroup>,
  /// Hooks to run after a workspace is created.
  #[serde(
    rename = "post-start",
    default,
    deserialize_with = "crate::core::config::deserialize_hook_groups"
  )]
  pub post_start: Vec<HookGroup>,
  /// Hooks to run before a workspace is removed.
  #[serde(
    rename = "pre-remove",
    default,
    deserialize_with = "crate::core::config::deserialize_hook_groups"
  )]
  pub pre_remove: Vec<HookGroup>,
  /// Hooks to run after a workspace is removed.
  #[serde(
    rename = "post-remove",
    default,
    deserialize_with = "crate::core::config::deserialize_hook_groups"
  )]
  pub post_remove: Vec<HookGroup>,
  /// When true, workspace directory deletion runs in the background.
  #[serde(rename = "background-remove", default)]
  pub background_remove: Option<bool>,
  /// Custom subcommands. Each entry maps `jjwt <name>` to a template
  /// rendered with the standard hook variables; the result is executed
  /// via `sh -c` with stdio inherited from the parent process.
  #[serde(default)]
  pub aliases: IndexMap<String, String>,
  /// Minijinja template for computing workspace directory paths.
  #[serde(rename = "worktree-path", default)]
  pub worktree_path_template: Option<String>,
  /// LLM commit-message generation settings.
  #[serde(default)]
  pub commit: Option<CommitConfig>,
  /// Per-project overrides in the user config. Keyed by repo identity
  /// (e.g. `github.com/owner/repo`). Only meaningful in user config;
  /// ignored in project config.
  #[serde(default)]
  pub projects: HashMap<String, Config>,
}

impl Config {
  /// Iterate (hook_type, group) pairs over every configured hook group.
  /// Used by `jjwt hook` for cross-group lookups and (in 1B.13)
  /// `hook show`.
  pub fn all_hook_groups(&self) -> Vec<(&'static str, &[HookGroup])> {
    vec![
      ("pre-switch", &self.pre_switch),
      ("post-switch", &self.post_switch),
      ("pre-start", &self.pre_start),
      ("post-start", &self.post_start),
      ("pre-remove", &self.pre_remove),
      ("post-remove", &self.post_remove),
    ]
  }
}

/// Which configuration layer a hook originated from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookSource {
  /// Hook defined in the user-level config.
  User,
  /// Hook defined in the project-level config.
  Project,
}

/// Renders the hook source as `"user"` or `"project"`.
impl fmt::Display for HookSource {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      HookSource::User => f.write_str("user"),
      HookSource::Project => f.write_str("project"),
    }
  }
}

/// A hook group paired with which config layer it came from.
#[derive(Debug, Clone)]
pub struct SourcedHookGroup {
  /// Whether this group came from user or project config.
  pub source: HookSource,
  /// Ordered map of hook name to command template.
  pub group: HookGroup,
}

/// The six hook lifecycle slots, generic over the hook representation.
/// `Config` uses `HookSet<HookGroup>` (raw TOML groups);
/// `MergedConfig` uses `HookSet<SourcedHookGroup>` (groups tagged with
/// their config layer).
#[derive(Debug, Clone)]
pub struct HookSet<T> {
  /// Hooks fired before switching workspaces.
  pub pre_switch: Vec<T>,
  /// Hooks fired after switching workspaces.
  pub post_switch: Vec<T>,
  /// Hooks fired before creating a workspace.
  pub pre_start: Vec<T>,
  /// Hooks fired after creating a workspace.
  pub post_start: Vec<T>,
  /// Hooks fired before removing a workspace.
  pub pre_remove: Vec<T>,
  /// Hooks fired after removing a workspace.
  pub post_remove: Vec<T>,
}

impl<T> Default for HookSet<T> {
  fn default() -> Self {
    Self {
      pre_switch: Vec::new(),
      post_switch: Vec::new(),
      pre_start: Vec::new(),
      post_start: Vec::new(),
      pre_remove: Vec::new(),
      post_remove: Vec::new(),
    }
  }
}

impl<T> HookSet<T> {
  /// Iterate (hook_type, groups) pairs over every configured hook slot.
  pub fn all_groups(&self) -> [(&'static str, &[T]); 6] {
    [
      ("pre-switch", &self.pre_switch),
      ("post-switch", &self.post_switch),
      ("pre-start", &self.pre_start),
      ("post-start", &self.post_start),
      ("pre-remove", &self.pre_remove),
      ("post-remove", &self.post_remove),
    ]
  }
}

impl HookSet<SourcedHookGroup> {
  /// Merge a user layer and a project layer into sourced hook groups.
  /// User hooks come first; project hooks are appended.
  fn merge(user: &[HookGroup], project: &[HookGroup]) -> Vec<SourcedHookGroup> {
    user
      .iter()
      .map(|g| SourcedHookGroup {
        source: HookSource::User,
        group: g.clone(),
      })
      .chain(project.iter().map(|g| SourcedHookGroup {
        source: HookSource::Project,
        group: g.clone(),
      }))
      .collect()
  }

  /// Merge all six hook slots from user and project configs.
  fn from_config_layers(user: &Config, project: &Config) -> Self {
    Self {
      pre_switch: Self::merge(&user.pre_switch, &project.pre_switch),
      post_switch: Self::merge(&user.post_switch, &project.post_switch),
      pre_start: Self::merge(&user.pre_start, &project.pre_start),
      post_start: Self::merge(&user.post_start, &project.post_start),
      pre_remove: Self::merge(&user.pre_remove, &project.pre_remove),
      post_remove: Self::merge(&user.post_remove, &project.post_remove),
    }
  }

  /// Convert back to plain `HookGroup` vectors, discarding source provenance.
  fn to_raw(&self) -> HookSet<HookGroup> {
    let extract = |groups: &[SourcedHookGroup]| -> Vec<HookGroup> {
      groups.iter().map(|shg| shg.group.clone()).collect()
    };

    HookSet {
      pre_switch: extract(&self.pre_switch),
      post_switch: extract(&self.post_switch),
      pre_start: extract(&self.pre_start),
      post_start: extract(&self.post_start),
      pre_remove: extract(&self.pre_remove),
      post_remove: extract(&self.post_remove),
    }
  }
}

/// User and project configs merged into a single effective configuration.
#[derive(Debug, Clone)]
pub struct MergedConfig {
  /// List subcommand settings (URL template, summary toggle).
  pub list: Option<ListConfig>,
  /// All six hook lifecycle slots, tagged with their config source.
  pub hooks: HookSet<SourcedHookGroup>,
  /// Whether directory deletion should run in the background.
  pub background_remove: Option<bool>,
  /// Custom subcommand aliases (name to template).
  pub aliases: IndexMap<String, String>,
  /// Minijinja template for workspace directory paths. Always populated;
  /// defaults to `DEFAULT_WORKTREE_PATH_TEMPLATE` when neither config
  /// layer provides a value.
  pub worktree_path_template: String,
  /// LLM commit-message generation settings.
  pub commit: Option<CommitConfig>,
}

impl Default for MergedConfig {
  fn default() -> Self {
    Self {
      list: None,
      hooks: HookSet::default(),
      background_remove: None,
      aliases: IndexMap::new(),
      worktree_path_template: DEFAULT_WORKTREE_PATH_TEMPLATE.to_string(),
      commit: None,
    }
  }
}

impl MergedConfig {
  /// Merge a user config (defaults) and a project config (overrides) into a
  /// single `MergedConfig`. Matches worktrunk's layering semantics:
  /// - Scalars: project wins if present, else user.
  /// - Aliases: user entries as base, project entries override per-key.
  /// - Hooks: user hooks first, project hooks appended (both contribute).
  pub fn from_layers(user: Option<&Config>, project: Option<&Config>) -> Self {
    let u = user.cloned().unwrap_or_default();
    let p = project.cloned().unwrap_or_default();

    let hooks = HookSet::from_config_layers(&u, &p);

    let list = p.list.or(u.list);
    let background_remove = p.background_remove.or(u.background_remove);
    let worktree_path_template = p
      .worktree_path_template
      .or(u.worktree_path_template)
      .unwrap_or_else(|| DEFAULT_WORKTREE_PATH_TEMPLATE.to_string());
    let commit = p.commit.or(u.commit);

    let mut aliases = u.aliases;

    for (k, v) in p.aliases {
      aliases.insert(k, v);
    }

    Self {
      list,
      hooks,
      background_remove,
      aliases,
      worktree_path_template,
      commit,
    }
  }

  /// Merge with a per-project override layer from the user config.
  ///
  /// Merge order: user defaults (excluding `projects`) → matching
  /// `projects` entry → project `.config/wt.toml`. The project override
  /// acts as a middle layer: it overrides user defaults but is itself
  /// overridden by the project-local config.
  pub fn from_layers_with_project_id(
    user: Option<&Config>,
    project_id: Option<&str>,
    project: Option<&Config>,
  ) -> Self {
    let project_override = user.zip(project_id).and_then(|(u, id)| u.projects.get(id));

    match project_override {
      Some(po) => {
        let base = Self::from_layers(user, Some(po));
        let base_cfg = base.to_config_lossy();

        Self::from_layers(Some(&base_cfg), project)
      }
      None => Self::from_layers(user, project),
    }
  }

  /// Wrap a single `Config` as all-`Project`-sourced. Convenience for
  /// callers that don't need layering (and for migrating existing tests).
  pub fn from_project(cfg: Config) -> Self {
    Self::from_layers(None, Some(&cfg))
  }

  /// Iterate (hook_type, groups) pairs over every configured hook group.
  pub fn all_hook_groups(&self) -> [(&'static str, &[SourcedHookGroup]); 6] {
    self.hooks.all_groups()
  }

  /// Convert back to a plain `Config`, discarding source provenance.
  /// Used internally for multi-layer merges where the intermediate
  /// result feeds into another `from_layers` call.
  fn to_config_lossy(&self) -> Config {
    let raw = self.hooks.to_raw();

    Config {
      list: self.list.clone(),
      pre_switch: raw.pre_switch,
      post_switch: raw.post_switch,
      pre_start: raw.pre_start,
      post_start: raw.post_start,
      pre_remove: raw.pre_remove,
      post_remove: raw.post_remove,
      background_remove: self.background_remove,
      aliases: self.aliases.clone(),
      worktree_path_template: Some(self.worktree_path_template.clone()),
      commit: self.commit.clone(),
      projects: HashMap::new(),
    }
  }
}

/// Configuration for the `list` subcommand.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ListConfig {
  /// Minijinja template rendered into a URL column for each workspace.
  pub url: String,
  /// Enable LLM-generated one-liner summaries in `list --full`.
  #[serde(default)]
  pub summary: Option<bool>,
}

/// Settings for LLM-assisted commit message generation.
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct CommitConfig {
  /// Configuration for the generation subprocess and prompt template.
  pub generation: Option<CommitGenerationConfig>,
}

/// Controls how commit messages are generated via an external LLM command.
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct CommitGenerationConfig {
  /// Shell command that reads a prompt from stdin and writes a commit
  /// message to stdout. Provider-agnostic — any CLI works.
  pub command: Option<String>,
  /// Minijinja template for the LLM prompt. When omitted, a built-in
  /// default is used.
  pub template: Option<String>,
  /// Appended to the default template (ignored when `template` is set).
  /// Intended for project-level guidance in `.config/wt.toml`.
  #[serde(rename = "template-append")]
  pub template_append: Option<String>,
}

/// Ordered map of hook name to command template within a single group.
pub type HookGroup = IndexMap<String, String>;

/// Variables available to minijinja templates during hook/alias rendering.
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
  /// Extra variables from `--var KEY=VAL`.
  pub vars: Vec<(String, String)>,
  /// Per-workspace persistent variables (from `.jj/jjwt-state.toml`).
  /// Accessible in templates as `{{ vars.KEY }}`.
  pub vars_state: HashMap<String, String>,
}

/// A registered jj workspace with its on-disk location and freshness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
  /// Workspace name as registered with jj.
  pub name: String,
  /// Absolute path to the workspace directory.
  pub path: PathBuf,
  /// Whether jj considers this workspace stale (needs update).
  pub stale: bool,
}

/// Snapshot of the repository and workspace state observed by the shell.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObservedState {
  /// Absolute path to the repository root.
  pub repo_root: PathBuf,
  /// Whether the current directory is inside a jj repository.
  pub is_jj_repo: bool,
  /// All registered workspaces with their paths and staleness.
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
  /// Name of the trunk bookmark (e.g. "main", "master"). Used as the
  /// default base revision when creating workspaces.
  pub trunk_bookmark: Option<String>,
}

/// A single step in an execution plan produced by the planner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
  /// Register a new jj workspace at the given path.
  JjWorkspaceAdd {
    /// Workspace name to register.
    name: String,
    /// On-disk path for the new workspace.
    path: PathBuf,
    /// Base revision (bookmark name) to check out after creation. When
    /// set, the new workspace's `@` is reparented from root onto this
    /// revision.
    revision: Option<String>,
  },
  /// Create a jj bookmark pointing at the workspace's working copy.
  JjBookmarkCreate {
    /// Bookmark name to create.
    name: String,
    /// Workspace whose `@` the bookmark targets.
    workspace: String,
  },
  /// Unregister a jj workspace (does not delete files).
  JjWorkspaceForget {
    /// Workspace name to forget.
    name: String,
  },
  /// Delete a jj bookmark.
  JjBookmarkDelete {
    /// Bookmark name to delete.
    name: String,
  },
  /// Bring a stale workspace up to date.
  JjWorkspaceUpdateStale {
    /// Workspace name to update.
    name: String,
  },
  /// Synchronously delete a directory tree.
  DeleteDir {
    /// Path to remove.
    path: PathBuf,
  },
  /// Delete a directory tree in a background process.
  DeleteDirBackground {
    /// Path to remove asynchronously.
    path: PathBuf,
  },
  /// Rename a jj workspace.
  JjWorkspaceRename {
    /// Current workspace name.
    old_name: String,
    /// New workspace name.
    new_name: String,
  },
  /// Move a directory from one path to another.
  RenameDir {
    /// Current path.
    from: PathBuf,
    /// Destination path.
    to: PathBuf,
  },
  /// Rename a jj bookmark.
  JjBookmarkRename {
    /// Current bookmark name.
    old_name: String,
    /// New bookmark name.
    new_name: String,
  },
  /// Execute a rendered hook command in a subprocess.
  RunHook {
    /// Named key of the hook inside its group.
    name: String,
    /// Fully rendered shell command string.
    rendered_cmd: String,
    /// Working directory for the hook subprocess.
    cwd: PathBuf,
    /// Environment variables injected into the hook process.
    env: Vec<(String, String)>,
    /// Which config layer defined this hook.
    source: HookSource,
  },
  /// Run a command with stdio inherited from the parent process. Used by
  /// `jjwt <alias>` and (in 1B.17) `jjwt switch -x`. A non-zero exit
  /// becomes an error so the surrounding plan halts.
  Exec {
    /// Fully rendered shell command string.
    rendered_cmd: String,
    /// Working directory for the exec subprocess.
    cwd: PathBuf,
    /// Environment variables injected into the subprocess.
    env: Vec<(String, String)>,
  },
  /// Print a line to stdout (consumed by the shell wrapper).
  PrintLine(String),
}

/// An ordered sequence of actions to be executed by the runtime.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
  /// Actions to execute in order.
  pub actions: Vec<Action>,
}

impl Plan {
  /// Create an empty plan.
  pub fn new() -> Self {
    Self::default()
  }

  /// Append an action to the end of the plan.
  pub fn push(&mut self, a: Action) {
    self.actions.push(a);
  }
}

/// Output format negotiated by `--format`. Text is the default; JSON is
/// emitted as a single line on the same `PrintLine` action so the runtime
/// is oblivious to the format choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
  /// Human-readable plain text (default).
  #[default]
  Text,
  /// Machine-readable JSON (single line per record).
  Json,
  /// Compact one-line summary for status displays.
  Statusline,
}

/// Arguments for the `switch` subcommand.
#[derive(Debug, Clone, Default)]
pub struct SwitchArgs {
  /// Target workspace name.
  pub name: String,
  /// When true, create the workspace if it does not exist.
  pub create: bool,
  /// Re-run start hooks even though the workspace already exists.
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
  /// Base revision (bookmark name, etc.) for the new workspace. When
  /// omitted, defaults to the trunk bookmark. Only used with `--create`.
  pub base: Option<String>,
  /// Only show what would be done without actually doing it.
  pub dry_run: bool,
  /// Output format (text, JSON, or statusline).
  pub format: OutputFormat,
}

/// Arguments for the `remove` subcommand.
#[derive(Debug, Clone, Default)]
pub struct RemoveArgs {
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
  /// Only show what would be done without actually doing it.
  pub dry_run: bool,
  /// Output format (text, JSON, or statusline).
  pub format: OutputFormat,
}

/// Arguments for the `hook` subcommand (manual hook invocation).
#[derive(Debug, Clone)]
pub struct HookArgs {
  /// Named key of the hook to run.
  pub name: String,
  /// Workspace to use as context for template rendering.
  pub current_workspace: String,
  /// Extra template variables from `--var KEY=VAL`.
  pub vars: Vec<(String, String)>,
}

/// Arguments for a custom alias invocation.
#[derive(Debug, Clone)]
pub struct AliasArgs {
  /// Alias name to look up in config.
  pub name: String,
  /// Tokens forwarded from the CLI; bound to `{{ args }}` in the template.
  pub forwarded: Vec<String>,
}

/// Arguments for the `relocate` subcommand (rename a workspace).
#[derive(Debug, Clone, Default)]
pub struct RelocateArgs {
  /// Current workspace name.
  pub old_name: String,
  /// Desired new workspace name.
  pub new_name: String,
  /// Also rename the associated bookmark.
  pub rename_bookmark: bool,
  /// Output format (text, JSON, or statusline).
  pub format: OutputFormat,
}

/// Arguments for the `prune` subcommand (bulk-remove merged workspaces).
#[derive(Debug, Clone, Default)]
pub struct PruneArgs {
  /// Only report what would be pruned; do not modify anything.
  pub dry_run: bool,
  /// Skip all hooks during pruning.
  pub no_hooks: bool,
  /// Output format (text, JSON, or statusline).
  pub format: OutputFormat,
}

/// Relationship of a bookmark to the trunk branch.
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

/// Per-workspace status indicators for list display.
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

/// Lines added and removed in a diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LineDiff {
  /// Number of lines added.
  pub added: u32,
  /// Number of lines removed.
  pub removed: u32,
}

/// Commit distance ahead of and behind trunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AheadBehind {
  /// Commits ahead of trunk.
  pub ahead: u32,
  /// Commits behind trunk.
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
  /// Tracked files have working-copy modifications.
  pub modified: bool,
  /// Untracked files present.
  pub untracked: bool,
  /// Working copy has unresolved conflicts.
  pub conflicts: bool,
  /// Short change ID (8 chars).
  pub commit_short: String,
  /// Seconds since `@`'s committer timestamp.
  pub age_seconds: i64,
  /// First line of `@`'s description.
  pub message_first_line: String,
  /// Lines added in the working-copy diff.
  pub head_added: u32,
  /// Lines removed in the working-copy diff.
  pub head_removed: u32,
}

/// CI check status for a workspace's bookmark, queried from gh/glab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CiStatus {
  /// All checks passed.
  Pass,
  /// One or more checks failed.
  Fail,
  /// Checks are still running.
  Pending,
  /// No CI information available.
  #[default]
  None,
}

/// Renders the CI status as a lowercase string.
impl fmt::Display for CiStatus {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      CiStatus::Pass => f.write_str("pass"),
      CiStatus::Fail => f.write_str("fail"),
      CiStatus::Pending => f.write_str("pending"),
      CiStatus::None => f.write_str("none"),
    }
  }
}

/// Raw per-workspace data observed by the shell for list rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedListRow {
  /// The workspace identity and path.
  pub workspace: Workspace,
  /// Working-copy status and commit metadata.
  pub details: WorkspaceDetails,
  /// Commits ahead of trunk.
  pub ahead: u32,
  /// Commits behind trunk.
  pub behind: u32,
  /// True when the bookmark for this workspace has a remote-tracking
  /// variant (e.g. `<name>@origin`).
  pub has_remote_bookmark: bool,
  /// CI check status from forge CLI (gh/glab). Only populated when
  /// `--full` is used.
  pub ci_status: CiStatus,
  /// LLM-generated one-liner summary. Only populated when `--full` is
  /// used and `[list] summary = true`.
  pub summary: String,
}

/// State for the prune command: all workspaces with their merge status.
#[derive(Debug, Clone, Default)]
pub struct ObservedPruneState {
  /// Absolute path to the repository root.
  pub repo_root: PathBuf,
  /// Whether the current directory is inside a jj repository.
  pub is_jj_repo: bool,
  /// Name of the workspace containing cwd, if any.
  pub current_workspace: Option<String>,
  /// All registered workspaces.
  pub workspaces: Vec<Workspace>,
  /// Per-workspace: (bookmark_exists, bookmark_merged, workspace_dirty).
  pub workspace_status: Vec<(String, bool, bool, bool)>,
}

/// Presentation hints observed from the terminal environment. The shell
/// constructs this from I/O (terminal detection, `NO_COLOR`, terminal
/// size) and passes it into the core as plain data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DisplayHints {
  /// Whether to emit ANSI escape sequences.
  pub styled: bool,
  /// Terminal width in columns, if known. `None` means unbounded (e.g.
  /// piped output).
  pub term_width: Option<u16>,
}

/// Full observed state for the list subcommand.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObservedListState {
  /// Absolute path to the repository root.
  pub repo_root: PathBuf,
  /// Whether the current directory is inside a jj repository.
  pub is_jj_repo: bool,
  /// Name of the workspace whose path contains cwd, if any.
  pub current_workspace: Option<String>,
  /// Per-workspace observation data.
  pub rows: Vec<ObservedListRow>,
  /// Names of bookmarks without a workspace, only populated when the
  /// caller asked for `--bookmarks`.
  pub extra_bookmark_names: Vec<String>,
  /// Names of remote-only bookmarks, only populated when the caller
  /// asked for `--remotes`. Format: bare local name (the `@<remote>`
  /// suffix is stripped).
  pub extra_remote_only_names: Vec<String>,
  /// Whether `--full` mode is active (show all columns).
  pub full: bool,
}

/// What kind of row this is. `Workspace` rows have a real path and full
/// observation details. `Bookmark` rows are bookmarks without a workspace
/// (either local-only-no-worktree or remote-only) and have empty
/// working-copy state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListRowKind {
  /// Row represents a registered jj workspace.
  #[default]
  Workspace,
  /// Row represents a bookmark without a workspace.
  Bookmark,
}

/// Options that gate which rows `observe_list` collects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ListOptions {
  /// Include local bookmarks that don't have a workspace.
  pub include_bookmarks: bool,
  /// Include remote-only bookmarks (`<name>@<remote>` with no local).
  pub include_remotes: bool,
  /// Show additional columns (CI, URL, Commit, Age, Summary) and query
  /// CI status and LLM summaries when enabled.
  pub full: bool,
}

/// A fully resolved row for the list table, ready for rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRow {
  /// Workspace name (also bookmark name by jjwt convention).
  pub name: String,
  /// Absolute on-disk path of the workspace (empty for `Branch` rows).
  pub path: PathBuf,
  /// Relative display path for the list table (e.g. ".", "./sibling.feat").
  pub display_path: String,
  /// Whether this row represents a workspace or a standalone branch.
  pub kind: ListRowKind,
  /// Rendered from `[list].url`; "" if no config.
  pub url: String,
  /// Workspace whose path contains cwd.
  pub is_current: bool,
  /// Workspace name is "default" (lives at repo root).
  pub is_default: bool,
  /// Working-copy and trunk-relationship status indicators.
  pub status: StatusFlags,
  /// Working-copy line diff (`jj diff -r @ --stat`).
  pub head_diff: LineDiff,
  /// Commits ahead of and behind trunk.
  pub vs_trunk: AheadBehind,
  /// 8-char short change ID for the workspace's `@`.
  pub commit: String,
  /// Pre-formatted relative age (e.g. "9h", "2w", "1mo").
  pub age: String,
  /// First line of `@`'s description.
  pub message: String,
  /// CI check status from forge CLI.
  pub ci_status: CiStatus,
  /// LLM-generated one-liner summary.
  pub summary: String,
}
