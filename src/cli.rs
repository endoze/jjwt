//! CLI argument definitions and command dispatch for `jjwt`.
#![cfg(not(tarpaulin_include))]

use anyhow::Result;
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use std::path::PathBuf;

use crate::core::types::{
  HookSource, ListOptions, OutputFormat as CoreOutputFormat, RemoveArgs, SwitchArgs,
};
use crate::shell::cmd;
use crate::shell::cmd::shell::ShellKind;

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
  /// Check environment and configuration health.
  Doctor,
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
  Eval {
    /// Template source string to render.
    template: String,
  },
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
    #[arg(add = crate::completion::workspace_completer())]
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
  /// Interactive workspace picker with preview.
  Pick,
  /// Copy jj-ignored files from one workspace to another (CoW reflink when available).
  CopyIgnored {
    /// Source workspace name.
    #[arg(add = crate::completion::workspace_completer())]
    source: String,
    /// Destination workspace name (defaults to current workspace).
    #[arg(add = crate::completion::workspace_completer())]
    dest: Option<String>,
  },
  /// Manage per-workspace variables (stored in `.jj/jjwt-state.toml`).
  Var {
    /// `var` sub-subcommand (set / get / list / delete).
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
  #[arg(add = crate::completion::workspace_completer_restricted())]
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
  /// Base revision for the new workspace (bookmark name, change ID, etc.).
  /// Defaults to the trunk bookmark. Only used with --create.
  #[arg(short = 'b', long, requires = "create", add = crate::completion::bookmark_completer())]
  base: Option<String>,
  /// Skip configured hooks for this invocation.
  #[arg(long = "no-hooks")]
  no_hooks: bool,
  /// Deprecated alias for `--no-hooks`.
  #[arg(long = "no-verify", hide = true)]
  no_verify: bool,
  /// Show what would be done without actually doing it.
  #[arg(long)]
  dry_run: bool,
  /// Output format: `text` (default) or `json`.
  #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
  format: OutputFormat,
}

/// Arguments for the `remove` command.
#[derive(Args)]
struct RemoveCmd {
  /// Workspaces to remove. When omitted, defaults to the current workspace.
  #[arg(num_args = 0.., add = crate::completion::workspace_completer())]
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
  /// Show what would be done without actually doing it.
  #[arg(long)]
  dry_run: bool,
  /// Output format: `text` (default) or `json`.
  #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
  format: OutputFormat,
}

/// Arguments for the `list` command.
#[derive(Args)]
struct ListCmd {
  /// Include local bookmarks that don't have a workspace.
  #[arg(long)]
  bookmarks: bool,
  /// Include remote-only bookmarks.
  #[arg(long)]
  remotes: bool,
  /// Show additional columns (CI, URL, Commit, Age, Summary).
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

/// Converts the CLI hook source filter into the core library's `HookSource`.
impl From<HookSourceArg> for HookSource {
  fn from(s: HookSourceArg) -> Self {
    match s {
      HookSourceArg::User => HookSource::User,
      HookSourceArg::Project => HookSource::Project,
    }
  }
}

/// Arguments for the `hook` command.
#[derive(Args)]
struct HookCmd {
  /// Hook name to run. Omit to use --show.
  #[arg(add = crate::completion::hook_completer())]
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
  /// Validate the configuration.
  Check,
  /// Shell integration helpers.
  Shell(ConfigShellCmd),
  /// Scaffold a new configuration file.
  Create(ConfigCreateCmd),
  /// Display the resolved configuration.
  Show,
}

/// Arguments for the `config completions` command.
#[derive(Args)]
struct ConfigShellCompletionsCmd {
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
  /// Generate shell completion scripts.
  Completions(ConfigShellCompletionsCmd),
}

/// Arguments for the `config shell init` command.
#[derive(Args)]
struct ConfigShellInitCmd {
  /// Shell flavor to emit init code for.
  #[arg(value_enum)]
  shell: ShellKind,
}

/// Build the clap `Command` tree for `jjwt`. Exposed so the completion engine
/// can walk the same tree without re-deriving it.
pub fn command() -> clap::Command {
  Cli::command()
}

/// Parses CLI arguments and dispatches to the appropriate command handler.
pub fn run() -> Result<()> {
  let mut cli = Cli::parse();

  let cwd_owned: PathBuf = match cli.chdir.take() {
    Some(p) => p,
    None => std::env::current_dir()?,
  };
  let cwd = cwd_owned.as_path();

  let config = cli.config.as_deref();

  match cli.cmd {
    Cmd::Switch(s) => cmd::switch::run(
      cwd,
      config,
      SwitchArgs {
        name: s.name,
        create: s.create,
        rerun_hooks: s.rerun_hooks,
        no_hooks: s.no_hooks || s.no_verify,
        execute: s.execute,
        clobber: s.clobber,
        base: s.base,
        dry_run: s.dry_run,
        format: s.format.into(),
      },
    ),
    Cmd::Remove(r) => cmd::remove::run(
      cwd,
      config,
      r.names,
      RemoveArgs {
        force: r.force,
        no_hooks: r.no_hooks || r.no_verify,
        no_delete_branch: r.no_delete_branch,
        force_delete: r.force_delete,
        dry_run: r.dry_run,
        format: r.format.into(),
      },
    ),
    Cmd::List(l) => cmd::list::run(
      cwd,
      config,
      ListOptions {
        include_bookmarks: l.bookmarks,
        include_remotes: l.remotes,
        full: l.full,
      },
      l.format.into(),
    ),
    Cmd::Hook(h) => {
      if h.show {
        let source_filter = h.source.map(HookSource::from);

        cmd::hook_show::run(cwd, config, h.expanded, h.format.into(), source_filter)
      } else {
        let name = h
          .name
          .ok_or_else(|| anyhow::anyhow!("hook name required (or use --show)"))?;

        cmd::hook::run(cwd, config, name, h.vars)
      }
    }
    Cmd::Doctor => cmd::doctor::run(cwd),
    Cmd::Config(c) => match c.sub {
      ConfigSub::Check => cmd::config_check::run(cwd, config),
      ConfigSub::Shell(s) => match s.sub {
        ConfigShellSub::Init(i) => cmd::shell::dispatch(i.shell),
        ConfigShellSub::Completions(c) => {
          use std::io::Write as _;

          let out = match c.shell {
            Shell::Fish => cmd::shell::registration_fish(),
            Shell::Bash => cmd::shell::registration_bash(),
            Shell::Zsh => cmd::shell::registration_zsh(),
            _ => String::new(),
          };

          let stdout = std::io::stdout();
          let mut handle = stdout.lock();

          handle.write_all(out.as_bytes())?;

          Ok(())
        }
      },
      ConfigSub::Create(c) => cmd::config_create::run(cwd, c.project, c.user),
      ConfigSub::Show => cmd::config_show::run(cwd, config),
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
