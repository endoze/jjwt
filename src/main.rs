//! Entry point for the `jjwt` CLI binary.
#![cfg(not(tarpaulin_include))]

use anyhow::Result;
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use std::path::PathBuf;

use jjwt::core::types::OutputFormat as CoreOutputFormat;
use jjwt::shell::cmd;

/// CLI-level output format selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
enum OutputFormat {
  /// Human-readable plain text.
  #[default]
  Text,
  /// Machine-readable JSON.
  Json,
  /// Compact status-line format for shell prompts.
  Statusline,
}

/// Converts the CLI output format into the core library's format type.
impl From<OutputFormat> for CoreOutputFormat {
  /// Maps each CLI variant to its core equivalent.
  fn from(o: OutputFormat) -> Self {
    match o {
      OutputFormat::Text => CoreOutputFormat::Text,
      OutputFormat::Json => CoreOutputFormat::Json,
      OutputFormat::Statusline => CoreOutputFormat::Statusline,
    }
  }
}

/// Top-level CLI argument parser.
#[derive(Parser)]
#[command(name = "jjwt", version, about)]
struct Cli {
  /// Change to this directory before running.
  #[arg(short = 'C', long, global = true)]
  chdir: Option<PathBuf>,
  /// Override config file location.
  #[arg(long, global = true)]
  config: Option<PathBuf>,
  /// Verbosity (-v, -vv).
  #[arg(short, long, global = true, action = clap::ArgAction::Count)]
  verbose: u8,
  /// Subcommand to execute.
  #[command(subcommand)]
  cmd: Cmd,
}

/// Available top-level subcommands.
#[derive(Subcommand)]
enum Cmd {
  /// Switch to a different workspace.
  Switch(SwitchCmd),
  /// Remove one or more workspaces.
  Remove(RemoveCmd),
  /// List workspaces and bookmarks.
  List(ListCmd),
  /// Run or inspect hooks.
  Hook(HookCmd),
  /// Manage jjwt configuration.
  Config(ConfigCmd),
  /// Low-level workspace utilities.
  Step(StepCmd),
  /// Catch-all for user-defined aliases. First element is the alias name;
  /// the rest are forwarded to the alias's template as `{{ args }}`.
  #[command(external_subcommand)]
  External(Vec<String>),
}

/// Container for the `step` subcommand group.
#[derive(Args)]
struct StepCmd {
  /// Step sub-subcommand to run.
  #[command(subcommand)]
  sub: StepSub,
}

/// Available step sub-subcommands.
#[derive(Subcommand)]
enum StepSub {
  /// Render a template expression and print the result.
  Eval { template: String },
  /// Run a command in every workspace (tokens template-rendered per workspace).
  ForEach {
    /// Command to run; everything after `--` is captured.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 1..)]
    cmd: Vec<String>,
  },
  /// Run a command tied to the current workspace; killed when the workspace disappears.
  Tether {
    /// Command to run; everything after `--` is captured.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 1..)]
    cmd: Vec<String>,
  },
  /// Remove workspaces whose bookmarks are merged into trunk.
  Prune {
    /// Show what would be pruned without actually removing.
    #[arg(long)]
    dry_run: bool,
    /// Skip configured hooks.
    #[arg(long = "no-hooks")]
    no_hooks: bool,
    /// Output format: `text` (default) or `json`.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
  },
  /// Rename a workspace and move its directory.
  Relocate {
    /// Current workspace name.
    old_name: String,
    /// New workspace name.
    new_name: String,
    /// Also rename the bookmark.
    #[arg(long)]
    rename_bookmark: bool,
    /// Output format: `text` (default) or `json`.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
  },
  /// Generate a commit message with an LLM and set it via `jj describe`.
  Describe {
    /// Show the prompt and generated message without applying.
    #[arg(long)]
    dry_run: bool,
  },
  /// Show diff of current workspace against trunk.
  Diff {
    /// Extra arguments forwarded to `jj diff`.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
  },
  /// Interactive workspace picker with preview.
  Pick,
  /// Copy jj-ignored files from one workspace to another (CoW reflink when available).
  CopyIgnored {
    /// Source workspace name.
    source: String,
    /// Destination workspace name (defaults to current workspace).
    dest: Option<String>,
  },
  /// Manage per-workspace variables (stored in `.jj/jjwt-state.toml`).
  Var {
    #[command(subcommand)]
    sub: VarSub,
  },
}

/// Subcommands for per-workspace variable management.
#[derive(Subcommand)]
enum VarSub {
  /// Set a variable for the current workspace.
  Set {
    /// Variable name.
    key: String,
    /// Variable value.
    value: String,
  },
  /// Get a variable for the current workspace.
  Get {
    /// Variable name.
    key: String,
  },
  /// List all variables for the current workspace.
  List,
  /// Delete a variable for the current workspace.
  Delete {
    /// Variable name.
    key: String,
  },
}

/// Arguments for the `switch` command.
#[derive(Args)]
struct SwitchCmd {
  /// Target workspace name.
  name: String,
  /// Create the workspace if it does not exist.
  #[arg(short, long)]
  create: bool,
  /// Re-run post-switch hooks even if already in the workspace.
  #[arg(long)]
  rerun_hooks: bool,
  /// Run a command after switching. Template-rendered; the shell wrapper
  /// executes it inside the destination workspace.
  #[arg(short = 'x', long)]
  execute: Option<String>,
  /// Remove a stale directory at the target workspace path before creating.
  #[arg(long)]
  clobber: bool,
  /// Skip configured hooks for this invocation.
  #[arg(long = "no-hooks")]
  no_hooks: bool,
  /// Deprecated alias for `--no-hooks`.
  #[arg(long = "no-verify", hide = true)]
  no_verify: bool,
  /// Output format: `text` (default) or `json`.
  #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
  format: OutputFormat,
}

/// Arguments for the `remove` command.
#[derive(Args)]
struct RemoveCmd {
  /// Workspaces to remove. When omitted, defaults to the current workspace.
  #[arg(num_args = 0..)]
  names: Vec<String>,
  /// Force worktree removal: bypass the "uncommitted changes" check.
  #[arg(short, long)]
  force: bool,
  /// Keep the bookmark even if it is merged into trunk.
  #[arg(long = "no-delete-branch")]
  no_delete_branch: bool,
  /// Delete the bookmark even when not merged (worktrunk's `-D`).
  #[arg(short = 'D', long = "force-delete")]
  force_delete: bool,
  /// Skip configured hooks for this invocation.
  #[arg(long = "no-hooks")]
  no_hooks: bool,
  /// Deprecated alias for `--no-hooks`.
  #[arg(long = "no-verify", hide = true)]
  no_verify: bool,
  /// Output format: `text` (default) or `json`.
  #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
  format: OutputFormat,
}

/// Arguments for the `list` command.
#[derive(Args)]
struct ListCmd {
  /// Include local bookmarks that don't have a workspace.
  #[arg(long)]
  branches: bool,
  /// Include remote-only bookmarks.
  #[arg(long)]
  remotes: bool,
  /// Reserved for additional columns (CI / summary). Phase 1: flag is
  /// plumbed through, but the column set is unchanged.
  #[arg(long)]
  full: bool,
  /// Output format: `text` (default) or `json`.
  #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
  format: OutputFormat,
}

/// Filter for which config source a hook originates from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum HookSourceArg {
  /// User-level configuration.
  User,
  /// Project-level configuration.
  Project,
}

/// Arguments for the `hook` command.
#[derive(Args)]
struct HookCmd {
  /// Hook name to run. Omit to use --show.
  name: Option<String>,
  /// List all configured hooks.
  #[arg(long)]
  show: bool,
  /// Render templates with current workspace context (requires --show).
  #[arg(long, requires = "show")]
  expanded: bool,
  /// Filter hooks by config source (requires --show).
  #[arg(long, value_enum, requires = "show")]
  source: Option<HookSourceArg>,
  /// Set a template variable for hook rendering (repeatable).
  #[arg(long = "var", value_name = "KEY=VAL")]
  vars: Vec<String>,
  /// Output format: `text` (default) or `json`.
  #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
  format: OutputFormat,
}

/// Arguments for the `config` command.
#[derive(Args)]
struct ConfigCmd {
  /// Config sub-subcommand to run.
  #[command(subcommand)]
  sub: ConfigSub,
}

/// Available config sub-subcommands.
#[derive(Subcommand)]
enum ConfigSub {
  /// Shell integration helpers.
  Shell(ConfigShellCmd),
  /// Scaffold a new configuration file.
  Create(ConfigCreateCmd),
  /// Display the resolved configuration.
  Show,
  /// Generate shell completion scripts.
  Completions(ConfigCompletionsCmd),
}

/// Arguments for the `config completions` command.
#[derive(Args)]
struct ConfigCompletionsCmd {
  /// Shell to generate completions for.
  shell: Shell,
}

/// Arguments for the `config create` command.
#[derive(Args)]
struct ConfigCreateCmd {
  /// Write a project config at `.config/wt.toml`.
  #[arg(long)]
  project: bool,
  /// Write a user config at `~/.config/jjwt/config.toml`.
  #[arg(long)]
  user: bool,
}

/// Arguments for the `config shell` command.
#[derive(Args)]
struct ConfigShellCmd {
  /// Shell sub-subcommand to run.
  #[command(subcommand)]
  sub: ConfigShellSub,
}

/// Available shell integration sub-subcommands.
#[derive(Subcommand)]
enum ConfigShellSub {
  /// Emit shell initialization code.
  Init(ConfigShellInitCmd),
}

/// Arguments for the `config shell init` command.
#[derive(Args)]
struct ConfigShellInitCmd {
  /// Shell name (e.g. `bash`, `zsh`, `fish`).
  shell: String,
}

/// Parses CLI arguments and dispatches to the appropriate command handler.
fn main() -> Result<()> {
  let cli = Cli::parse();

  let cwd_owned;
  let cwd = if let Some(p) = &cli.chdir {
    p.as_path()
  } else {
    cwd_owned = std::env::current_dir()?;
    cwd_owned.as_path()
  };

  let config = cli.config.as_deref();

  match cli.cmd {
    Cmd::Switch(s) => cmd::switch::run(
      cwd,
      config,
      s.name,
      s.create,
      s.rerun_hooks,
      s.no_hooks || s.no_verify,
      s.execute,
      s.clobber,
      s.format.into(),
    ),
    Cmd::Remove(r) => cmd::remove::run(
      cwd,
      config,
      r.names,
      r.force,
      r.no_hooks || r.no_verify,
      r.no_delete_branch,
      r.force_delete,
      r.format.into(),
    ),
    Cmd::List(l) => cmd::list::run(
      cwd,
      config,
      jjwt::core::types::ListOptions {
        include_branches: l.branches,
        include_remotes: l.remotes,
        full: l.full,
      },
      l.format.into(),
    ),
    Cmd::Hook(h) => {
      if h.show {
        let source_filter = h.source.map(|s| match s {
          HookSourceArg::User => jjwt::core::types::HookSource::User,
          HookSourceArg::Project => jjwt::core::types::HookSource::Project,
        });

        cmd::hook_show::run(cwd, config, h.expanded, h.format.into(), source_filter)
      } else {
        let name = h
          .name
          .ok_or_else(|| anyhow::anyhow!("hook name required (or use --show)"))?;

        cmd::hook::run(cwd, config, name, h.vars)
      }
    }
    Cmd::Config(c) => match c.sub {
      ConfigSub::Shell(s) => match s.sub {
        ConfigShellSub::Init(i) => cmd::shell::dispatch(&i.shell),
      },
      ConfigSub::Create(c) => cmd::config_create::run(cwd, c.project, c.user),
      ConfigSub::Show => cmd::config_show::run(cwd, config),
      ConfigSub::Completions(c) => {
        clap_complete::generate(c.shell, &mut Cli::command(), "jjwt", &mut std::io::stdout());

        Ok(())
      }
    },
    Cmd::Step(s) => match s.sub {
      StepSub::Eval { template } => cmd::step_eval::run(cwd, &template),
      StepSub::ForEach { cmd: argv } => cmd::step_for_each::run(cwd, argv),
      StepSub::Tether { cmd: argv } => {
        let code = cmd::step_tether::run(cwd, argv)?;

        std::process::exit(code);
      }
      StepSub::Prune {
        dry_run,
        no_hooks,
        format,
      } => cmd::step_prune::run(cwd, config, dry_run, no_hooks, format.into()),
      StepSub::Relocate {
        old_name,
        new_name,
        rename_bookmark,
        format,
      } => cmd::step_relocate::run(
        cwd,
        config,
        old_name,
        new_name,
        rename_bookmark,
        format.into(),
      ),
      StepSub::Describe { dry_run } => cmd::step_describe::run(cwd, config, dry_run),
      StepSub::Diff { args } => {
        let code = cmd::step_diff::run(cwd, args)?;

        std::process::exit(code);
      }
      StepSub::Pick => cmd::step_pick::run(cwd),
      StepSub::CopyIgnored { source, dest } => {
        cmd::step_copy_ignored::run(cwd, &source, dest.as_deref())
      }
      StepSub::Var { sub } => match sub {
        VarSub::Set { key, value } => cmd::step_var::run_set(cwd, &key, &value),
        VarSub::Get { key } => cmd::step_var::run_get(cwd, &key),
        VarSub::List => cmd::step_var::run_list(cwd),
        VarSub::Delete { key } => cmd::step_var::run_delete(cwd, &key),
      },
    },
    Cmd::External(parts) => {
      let mut it = parts.into_iter();
      let name = it
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing subcommand name"))?;
      let forwarded: Vec<String> = it.collect();

      cmd::alias::run(cwd, config, name, forwarded)
    }
  }
}
