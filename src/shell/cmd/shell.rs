#![cfg(not(tarpaulin_include))]

use anyhow::Result;
use clap::ValueEnum;

/// Supported shell flavors for the `config shell init` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ShellKind {
  /// Bash shell.
  Bash,
  /// Zsh shell.
  Zsh,
  /// Fish shell.
  Fish,
}

/// Fish wrapper. Supports two output protocols from `jjwt switch`:
///   1. legacy — a single bare path; we `cd` to it.
///   2. directives — lines prefixed `cd:<path>` and (optionally)
///      `exec:<cmd>`; we `cd` then `eval` the command.
const FISH_WRAPPER: &str = r#"function wt
    switch $argv[1]
        case switch remove
            set -l __wt_out (command jjwt $argv[1] $argv[2..])
            or return $status
            set -l __wt_cd ""
            set -l __wt_exec ""
            for line in $__wt_out
                set -l prefix (string sub -l 3 -- $line)
                if test "$prefix" = "cd:"
                    set __wt_cd (string sub -s 4 -- $line)
                else if test (string sub -l 5 -- $line) = "exec:"
                    set __wt_exec (string sub -s 6 -- $line)
                else if test -z "$__wt_cd" -a -n "$line" -a -d "$line"
                    set __wt_cd $line
                end
            end
            if test -n "$__wt_cd" -a -d "$__wt_cd"
                cd "$__wt_cd"
            end
            if test -n "$__wt_exec"
                eval $__wt_exec
            end
        case '*'
            command jjwt $argv
    end
end
"#;

/// Fish completions for workspace names on `switch` and `remove`.
const FISH_COMPLETIONS: &str = r#"complete -c wt -n '__fish_seen_subcommand_from switch remove' -xa '(command jjwt step _complete-workspaces 2>/dev/null)'
"#;

/// POSIX (bash/zsh) wrapper. Same directive protocol as Fish.
const POSIX_WRAPPER: &str = r#"wt() {
  if [ "$1" = "switch" ] || [ "$1" = "remove" ]; then
    local __wt_subcmd="$1"
    shift
    local __wt_out __wt_cd __wt_exec line
    __wt_out=$(command jjwt "$__wt_subcmd" "$@") || return $?
    __wt_cd=""
    __wt_exec=""
    while IFS= read -r line; do
      case "$line" in
        cd:*)   __wt_cd=${line#cd:} ;;
        exec:*) __wt_exec=${line#exec:} ;;
        *)
          if [ -z "$__wt_cd" ] && [ -n "$line" ] && [ -d "$line" ]; then
            __wt_cd=$line
          fi
          ;;
      esac
    done <<EOF
$__wt_out
EOF
    if [ -n "$__wt_cd" ] && [ -d "$__wt_cd" ]; then
      cd "$__wt_cd" || return $?
    fi
    if [ -n "$__wt_exec" ]; then
      eval "$__wt_exec"
    fi
  else
    command jjwt "$@"
  fi
}
"#;

/// Bash completions for workspace names on `switch` and `remove`.
const BASH_COMPLETIONS: &str = r#"_wt_complete() {
  local cur="${COMP_WORDS[COMP_CWORD]}"
  local subcmd="${COMP_WORDS[1]}"
  if [ "$subcmd" = "switch" ] || [ "$subcmd" = "remove" ]; then
    COMPREPLY=($(compgen -W "$(command jjwt step _complete-workspaces 2>/dev/null)" -- "$cur"))
  fi
}
complete -F _wt_complete wt
"#;

/// Zsh completions for workspace names on `switch` and `remove`.
const ZSH_COMPLETIONS: &str = r#"_wt_complete() {
  local cur="${words[CURRENT]}"
  local subcmd="${words[2]}"
  if [ "$subcmd" = "switch" ] || [ "$subcmd" = "remove" ]; then
    local completions
    completions=($(command jjwt step _complete-workspaces 2>/dev/null))
    compadd -a completions
  fi
}
compdef _wt_complete wt
"#;

/// Print the Fish shell wrapper function to stdout.
pub fn run_fish() -> Result<()> {
  print!("{FISH_WRAPPER}");
  print!("{FISH_COMPLETIONS}");

  Ok(())
}

/// Print the Bash shell wrapper function to stdout.
pub fn run_bash() -> Result<()> {
  print!("{POSIX_WRAPPER}");
  print!("{BASH_COMPLETIONS}");

  Ok(())
}

/// Print the Zsh shell wrapper function to stdout.
pub fn run_zsh() -> Result<()> {
  print!("{POSIX_WRAPPER}");
  print!("{ZSH_COMPLETIONS}");

  Ok(())
}

/// Dispatch to the correct shell wrapper emitter based on shell kind.
pub fn dispatch(shell: ShellKind) -> Result<()> {
  match shell {
    ShellKind::Fish => run_fish(),
    ShellKind::Bash => run_bash(),
    ShellKind::Zsh => run_zsh(),
  }
}
