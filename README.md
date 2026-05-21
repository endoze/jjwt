# jjwt

A small Rust CLI that reads worktrunk-compatible `.config/wt.toml` and
manages jujutsu workspaces in `.worktrees/<name>/`. Drop-in replacement
for the worktrunk subcommands actually used in this workflow.

## Install

```fish
cargo install --git https://github.com/endoze/jjwt
jjwt config shell init fish > ~/.config/fish/functions/wt.fish
```

## Subcommands

- `jjwt switch <name> [--create] [--rerun-hooks]`
- `jjwt remove <name> [--force]`
- `jjwt list`
- `jjwt hook <hook-name>`
- `jjwt config shell init fish`

## License

Dual-licensed under MIT or Apache-2.0.
