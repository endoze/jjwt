//!
#![doc = include_str!("../README.md")]
#![deny(missing_docs)]

/// CLI argument definitions and command dispatch.
pub mod cli;
/// Shell completion engine (`COMPLETE=$SHELL jjwt`).
pub mod completion;
/// Core logic for workspace operations, configuration, and hooks.
pub mod core;
/// Shell integration and CLI command implementations.
pub mod shell;
