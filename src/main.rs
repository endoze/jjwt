use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use jjwt::shell::cmd;

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
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Switch(SwitchCmd),
    Remove(RemoveCmd),
    List,
    Hook(HookCmd),
    Config(ConfigCmd),
}

#[derive(Args)]
struct SwitchCmd {
    name: String,
    #[arg(long)]
    create: bool,
    #[arg(long)]
    rerun_hooks: bool,
}

#[derive(Args)]
struct RemoveCmd {
    name: String,
    #[arg(long)]
    force: bool,
}

#[derive(Args)]
struct HookCmd {
    name: String,
}

#[derive(Args)]
struct ConfigCmd {
    #[command(subcommand)]
    sub: ConfigSub,
}

#[derive(Subcommand)]
enum ConfigSub {
    Shell(ConfigShellCmd),
}

#[derive(Args)]
struct ConfigShellCmd {
    #[command(subcommand)]
    sub: ConfigShellSub,
}

#[derive(Subcommand)]
enum ConfigShellSub {
    Init(ConfigShellInitCmd),
}

#[derive(Args)]
struct ConfigShellInitCmd {
    shell: String,
}

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
        Cmd::Switch(s) => cmd::switch::run(cwd, config, s.name, s.create, s.rerun_hooks),
        Cmd::Remove(r) => cmd::remove::run(cwd, config, r.name, r.force),
        Cmd::List => cmd::list::run(cwd, config),
        Cmd::Hook(h) => cmd::hook::run(cwd, config, h.name),
        Cmd::Config(c) => match c.sub {
            ConfigSub::Shell(s) => match s.sub {
                ConfigShellSub::Init(i) => {
                    if i.shell != "fish" {
                        return Err(anyhow::anyhow!("only fish is supported, got: {}", i.shell));
                    }

                    cmd::shell::run_fish()
                }
            },
        },
    }
}
