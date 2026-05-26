#![cfg(not(tarpaulin_include))]

use anyhow::Result;
use clap::ValueEnum;
use clap_complete::env::{Bash, EnvCompleter, Fish, Zsh};

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

/// Fish completion registration. Emits dynamic-engine `complete` lines for
/// both `jjwt` and `wt` so tab completion in either invocation calls back
/// into the binary with `COMPLETE=fish`.
pub fn registration_fish() -> String {
  build_registration(&Fish, &["jjwt", "wt"])
}

/// Bash completion registration. Same shape as fish.
pub fn registration_bash() -> String {
  build_registration(&Bash, &["jjwt", "wt"])
}

/// Zsh completion registration. Same shape as fish.
pub fn registration_zsh() -> String {
  build_registration(&Zsh, &["jjwt", "wt"])
}

/// Run clap_complete's per-shell registration script generator once per bin.
/// All registrations share a single completer ("jjwt"), so `wt switch <Tab>`
/// also calls the `jjwt` binary at completion time.
fn build_registration<C: EnvCompleter + ?Sized>(shell: &C, bins: &[&str]) -> String {
  let mut buf: Vec<u8> = Vec::new();

  for bin in bins {
    // Errors here would only fire on I/O failures to an in-memory Vec, which
    // can't happen. Bubble them out as an empty string in the impossible case.
    let _ = shell.write_registration("COMPLETE", "jjwt", bin, "jjwt", &mut buf);
  }

  String::from_utf8(buf).unwrap_or_default()
}

/// Print the Fish shell wrapper function to stdout. Completion registration is
/// installed separately via `jjwt config shell completions fish`.
pub fn run_fish() -> Result<()> {
  print!("{FISH_WRAPPER}");

  Ok(())
}

/// Print the Bash shell wrapper function to stdout. Completion registration is
/// installed separately via `jjwt config shell completions bash`.
pub fn run_bash() -> Result<()> {
  print!("{POSIX_WRAPPER}");

  Ok(())
}

/// Print the Zsh shell wrapper function to stdout. Completion registration is
/// installed separately via `jjwt config shell completions zsh`.
pub fn run_zsh() -> Result<()> {
  print!("{POSIX_WRAPPER}");

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
