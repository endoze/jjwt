//! Shell-level adapters and command implementations for jjwt.

/// User approval tracking for project hook commands.
pub mod approvals;
/// CI status queries via GitHub/GitLab CLIs.
pub mod ci;
/// Top-level CLI command dispatch modules.
pub mod cmd;
/// Config file discovery and loading across user/project layers.
pub mod config_loader;
/// Filesystem abstraction for testability.
pub mod fs;
/// Trait and CLI-based implementation for jj operations.
pub mod jj;
/// In-process jj backend using jj-lib.
pub mod jj_lib;
/// LLM integration for commit message and summary generation.
pub mod llm;
/// On-disk cache for LLM-generated summaries.
pub mod llm_cache;
/// Observation queries that gather jj repo state without side effects.
pub mod observe;
/// Process spawning abstraction for testability.
pub mod proc;
/// Plan executor that runs action sequences against real backends.
pub mod runtime;
/// Persistent per-repo state (previous workspace, variables).
pub mod state;
/// Background trash directory cleanup for deferred removals.
pub mod trash;
