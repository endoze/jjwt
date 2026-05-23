/// User-defined alias execution.
pub mod alias;
/// Scaffold a new config file (project or user).
pub mod config_create;
/// Display resolved config layers.
pub mod config_show;
/// Run a named hook in the current workspace.
pub mod hook;
/// Display configured hooks and their expansions.
pub mod hook_show;
/// List workspaces with status details.
pub mod list;
/// Remove one or more workspaces.
pub mod remove;
/// Emit shell wrapper functions for `wt` integration.
pub mod shell;
/// Copy jj-ignored files between workspaces.
pub mod step_copy_ignored;
/// Generate and apply an LLM commit message.
pub mod step_describe;
/// Show diff between trunk and current workspace.
pub mod step_diff;
/// Evaluate a template expression in the current context.
pub mod step_eval;
/// Run a command in every workspace.
pub mod step_for_each;
/// Interactive fuzzy workspace picker.
pub mod step_pick;
/// Remove workspaces whose bookmarks are merged into trunk.
pub mod step_prune;
/// Rename/move a workspace and optionally its bookmark.
pub mod step_relocate;
/// Tie a process lifetime to the current workspace directory.
pub mod step_tether;
/// Per-workspace key-value variable management.
pub mod step_var;
/// Switch to (or create) a workspace.
pub mod switch;
