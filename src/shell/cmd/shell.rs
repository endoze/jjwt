use anyhow::{Result, bail};

/// Fish wrapper. Supports two output protocols from `jjwt switch`:
///   1. legacy — a single bare path; we `cd` to it.
///   2. directives — lines prefixed `cd:<path>` and (optionally)
///      `exec:<cmd>`; we `cd` then `eval` the command.
const FISH_WRAPPER: &str = r#"function wt
    switch $argv[1]
        case switch
            set -l __wt_out (command jjwt switch $argv[2..])
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

const POSIX_WRAPPER: &str = r#"wt() {
  if [ "$1" = "switch" ]; then
    shift
    local __wt_out __wt_cd __wt_exec line
    __wt_out=$(command jjwt switch "$@") || return $?
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

pub fn run_fish() -> Result<()> {
  print!("{FISH_WRAPPER}");

  Ok(())
}

pub fn run_bash() -> Result<()> {
  print!("{POSIX_WRAPPER}");

  Ok(())
}

pub fn run_zsh() -> Result<()> {
  print!("{POSIX_WRAPPER}");

  Ok(())
}

pub fn dispatch(shell: &str) -> Result<()> {
  match shell {
    "fish" => run_fish(),
    "bash" => run_bash(),
    "zsh" => run_zsh(),
    other => bail!("unsupported shell '{other}' (supported: fish, bash, zsh)"),
  }
}
