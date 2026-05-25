# Changelog

All notable changes to this project will be documented in this file.

## [0.1.0] - 2026-05-25

### Features

- *(core)* Define shared types and error enum
- *(core)* Parse wt.toml config with ordered hooks
- *(core)* Port hash_port filter from worktrunk
- *(core)* Port sanitize filter from worktrunk
- *(core)* Render templates with hash_port and sanitize filters
- *(core)* Plan switch (create, existing, stale, rerun-hooks)
- *(core)* Plan remove (dirty/unmerged guards, force bypass)
- *(core)* Plan single hook with unique-name resolution
- *(core)* Plan list and format_list output
- *(shell)* Define Jj, Fs, Proc trait abstractions
- *(shell)* Implement JjCli by shelling out to jj
- *(shell)* Observe ObservedState from real Jj/Fs
- *(shell)* Runtime executes plan against Jj/Fs/Proc traits
- Wire clap CLI to subcommand glue and config loader
- *(core)* Enrich list output with workspace details
- *(core)* Port worktrunk feature set to jjwt
- *(core)* Add forge integration, workspace commands, and config options
- *(core)* Add adaptive terminal-width layout to list output
- *(core)* Add layered config merging and hook source filtering
- *(core)* Add workspace variables, hook approvals, CI status, and picker
- *(llm)* Add LLM integration for commit messages and list summaries
- *(release)* Add packaging, CI, docs, and license cleanup
- *(shell)* Add doctor, config check, dry-run, completions, and --full list columns
- *(core)* Add base revision flag and sibling worktree layout

### Bug Fixes

- *(shell)* Preserve stale flag and use correct path for default workspace

### Refactoring

- *(core)* Rename Branch to Bookmark and remove step diff
- *(core)* Improve type safety, error handling, and dedup utilities
- *(core)* Consolidate hook fields, dry-run types, and error handling
- Improve code quality, ergonomics, and hot-loop performance

### Documentation

- Add README and dual MIT/Apache-2.0 license

### Performance

- *(core)* Parallelize per-workspace queries in observe_prune

### Testing

- *(core)* Byte-exact fidelity snapshots vs worktrunk
- *(e2e)* Smoke test switch --create against real jj repo

### Tasks

- Scaffold cargo project and module layout

### Build

- *(deps)* Bump sha2, thiserror, toml, and which to latest majors
- *(release)* Add cross-platform binary distribution and code quality CI

