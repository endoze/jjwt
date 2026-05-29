# Changelog

All notable changes to this project will be documented in this file.

## [0.1.1] - 2026-05-29

### Features

- *(shell)* Add doctor, config check, dry-run, completions, and --full list columns
- *(core)* Add base revision flag and sibling worktree layout
- *(completion)* Use clap_complete dynamic engine
- *(shell)* Stream hook output and echo commands

### Bug Fixes

- *(jj)* Prevent panic when removing a workspace

### Refactoring

- *(core)* Improve type safety, error handling, and dedup utilities
- *(core)* Consolidate hook fields, dry-run types, and error handling
- Improve code quality, ergonomics, and hot-loop performance

### Tasks

- Prep crate for first crates.io publish

### Build

- *(release)* Add cross-platform binary distribution and code quality CI

