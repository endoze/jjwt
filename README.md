# jjwt

A workspace manager for [jujutsu](https://martinvonz.github.io/jj/) repositories. Inspired by [worktrunk](https://github.com/max-sixty/worktrunk) and config-compatible with it, jjwt is an independent implementation written from scratch in Rust, tailored to jujutsu's native concepts (workspaces, bookmarks, revsets).

## Install

jjwt needs the [jj](https://martinvonz.github.io/jj/) CLI 0.39 or newer on your `PATH`. `jjwt doctor` checks this.

### Cargo

```sh
cargo install jjwt
```

### Homebrew

```sh
brew install endoze/tap/jjwt
```

### Nix

Add jjwt as a flake input and apply its overlay:

```nix
{
  inputs.jjwt.url = "github:endoze/jjwt";

  outputs = { nixpkgs, jjwt, ... }: {
    # Apply the overlay so pkgs.jjwt is available everywhere
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      modules = [{
        nixpkgs.overlays = [ jjwt.overlays.default ];
        environment.systemPackages = [ pkgs.jjwt ];
      }];
    };
  };
}
```

Works the same way in home-manager — apply the overlay, then add `pkgs.jjwt` to `home.packages`.

Or run directly without installing:

```sh
nix run github:endoze/jjwt
```

## Shell setup

Install completions, then the `wt` wrapper. The completion script registers
dynamic completion for both `jjwt` and `wt` in one go — the wrapper itself
is just a shell function that interprets the `cd:` / `exec:` directives
emitted by `jjwt switch`.

### Completions

```sh
# Fish
jjwt config shell completions fish > ~/.config/fish/completions/jjwt.fish

# Bash (add to ~/.bashrc)
eval "$(jjwt config shell completions bash)"

# Zsh (add to ~/.zshrc, before compinit)
jjwt config shell completions zsh > "${fpath[1]}/_jjwt"
```

### `wt` wrapper

This creates a `wt` shell function that wraps `jjwt` so `wt switch` can `cd`
your shell into the target workspace.

```sh
# Fish — sourced at startup so both the function and the completion line take effect
jjwt config shell init fish > ~/.config/fish/conf.d/wt.fish

# Bash (add to ~/.bashrc, AFTER the completions line above)
eval "$(jjwt config shell init bash)"

# Zsh (add to ~/.zshrc, AFTER the completions line above)
eval "$(jjwt config shell init zsh)"
```

## Quick start

```sh
cd my-jj-repo

# Create a config file
jjwt config create --project

# Create a new workspace
jjwt switch --create feat/my-feature

# Switch to a workspace for an existing bookmark (auto-creates)
jjwt switch feat/existing-branch

# List all workspaces
jjwt list

# Remove a workspace
jjwt remove feat/my-feature

# Remove the current workspace
jjwt remove
```

## Commands

### `switch <name>`

Switch to a workspace. If the workspace doesn't exist but a bookmark with that name does, the workspace is created on that bookmark automatically. Use `--create` for a new workspace with no bookmark; jjwt never creates bookmarks, so add one with `jj bookmark` when you want to push.

```sh
jjwt switch feat/login          # switch or auto-create from bookmark
jjwt switch --create new-idea   # create workspace
jjwt switch ^                   # switch to the default workspace
jjwt switch -                   # switch to previous workspace
jjwt switch @                   # current workspace (useful with -x)
jjwt switch pr:42               # checkout a GitHub PR by number
jjwt switch mr:12               # checkout a GitLab MR by number
```

| Flag | Description |
|------|-------------|
| `-c, --create` | Create workspace |
| `-b, --base <rev>` | Base revision for the new workspace: a local bookmark, full commit id, or `^` for `trunk()` (requires `--create`; defaults to `trunk()`) |
| `-x, --execute <cmd>` | Run a template-rendered command after switching |
| `--clobber` | Remove stale directory at target path |
| `--rerun-hooks` | Re-run start hooks even when workspace exists |
| `--no-hooks` | Skip all hooks |
| `--dry-run` | Show what would be done without doing it |
| `--format json` | Output as JSON |

### `remove [names...]`

Remove one or more workspaces. Omit names to remove the current workspace. Changes to tracked files are snapshotted into its working-copy commit before it is forgotten, so they stay in the repo; ignored and untracked files are deleted with the directory. Bookmarks are left alone. The default workspace cannot be removed.

| Flag | Description |
|------|-------------|
| `--no-hooks` | Skip all hooks |
| `--dry-run` | Show what would be done without doing it |
| `--format json` | Output as JSON |

### `list`

Show all workspaces with status, diff stats, and trunk relationship.

```sh
jjwt list                       # table view
jjwt list --full                # add CI status + LLM summaries
jjwt list --format json         # machine-readable output
jjwt list --format statusline   # compact one-liner for shell prompts
```

The table adapts to your terminal width — columns are prioritized and low-value ones drop on narrow terminals.

### `hook`

Run a configured hook manually, or inspect all hooks.

```sh
jjwt hook my-hook               # run a hook by name
jjwt hook --show                # list all configured hooks
jjwt hook --show --expanded     # show hooks with templates rendered
jjwt hook --show --source user  # show only user-level hooks
jjwt hook --var KEY=VAL         # inject extra template variables
```

### `doctor`

Check environment and configuration health.

```sh
jjwt doctor
```

### `config`

Manage jjwt configuration.

```sh
jjwt config create --project    # scaffold .config/wt.toml
jjwt config create --user       # scaffold ~/.config/jjwt/config.toml
jjwt config show                # display resolved config (both layers)
jjwt config check               # validate config syntax and template references
jjwt config shell init fish     # emit shell wrapper function
jjwt config shell completions <shell> # emit shell completion script
```

### `step` utilities

Low-level tools for scripting and automation.

| Command | Description |
|---------|-------------|
| `step eval <template>` | Render a template expression |
| `step for-each -- <cmd>` | Run command in every workspace |
| `step tether -- <cmd>` | Run command tied to current workspace lifecycle |
| `step relocate <old> <new>` | Rename workspace and move its directory |
| `step describe [--dry-run]` | Generate a commit message with an LLM |
| `step pick` | Interactive workspace picker with preview |
| `step copy-ignored <src> [dest]` | Copy jj-ignored files between workspaces (CoW) |
| `step var set/get/list/delete` | Manage per-workspace template variables |

### Aliases

Define custom aliases in your config and invoke them as subcommands:

```toml
[aliases]
greet = "echo Hello from {{ branch }} ({{ args | join(' ') }})"
```

```sh
jjwt greet world    # => Hello from default (world)
```

## Configuration

jjwt reads config from two layers, merged together:

1. **User config** — `~/.config/jjwt/config.toml` (defaults, personal hooks)
2. **Project config** — `.config/wt.toml` in the repo root (shared with team)

User config can also contain `[projects."host/owner/repo"]` blocks that override settings per-repository.

### Example `.config/wt.toml`

```toml
worktree-path = ".worktrees/{{ branch | sanitize }}"
background-remove = true

[pre-start]
setup = "npm install"

[post-switch]
notify = "echo Switched to {{ branch }}"

[aliases]
wip = "jj describe -m 'wip: {{ branch }}'"

[commit.generation]
command = "claude -p --no-session-persistence"

[list]
summary = true
```

### Hooks

Hooks run at lifecycle points: `pre-switch`, `post-switch`, `pre-start`, `post-start`, `pre-remove`, `post-remove`. They support template rendering with variables like `{{ branch }}`, `{{ repo }}`, `{{ worktree_path }}`, `{{ vars.KEY }}`, and more.

Project-sourced hooks require approval on first run (stored in `~/.config/jjwt/approvals.toml`). Set `JJWT_TRUST_PROJECT_HOOKS=1` to bypass in CI.

### Template filters

| Filter | Example | Output |
|--------|---------|--------|
| `sanitize` | `feat/login` | `feat-login` |
| `sanitize_hash` | `feat/login` | `feat-login-a3f` |
| `sanitize_db` | `feat/LOGIN` | `feat_login_a3f` |
| `hash` | `feat/login` | `k7m` |
| `hash_port` | `feat/login` | `14832` |
| `codename` | `feat/login` | `fair-mole` |
| `codename(3)` | `feat/login` | `fair-icy-mole` |
| `dirname` | `src/core/plan.rs` | `src/core` |
| `basename` | `src/core/plan.rs` | `plan.rs` |

## License

MIT licensed. See [LICENSE](https://github.com/endoze/jjwt/blob/master/LICENSE) for details.

Some template filters were ported from [worktrunk](https://github.com/max-sixty/worktrunk) by Maximilian Roos (MIT / Apache-2.0). Attribution is preserved in the respective source files.
