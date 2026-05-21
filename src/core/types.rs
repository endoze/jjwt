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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    JjWorkspaceAdd { name: String, path: PathBuf },
    JjBookmarkCreate { name: String, workspace: String },
    JjWorkspaceForget { name: String },
    JjBookmarkDelete { name: String },
    JjWorkspaceUpdateStale { name: String },
    DeleteDir { path: PathBuf },
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
    pub fn new() -> Self { Self::default() }
    pub fn push(&mut self, a: Action) { self.actions.push(a); }
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
