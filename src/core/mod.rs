/// Configuration file parsing and hook deserialization.
pub mod config;
/// Template filters for string transformation (sanitize, hash, codename, etc.).
pub mod filters;
/// List table rendering, JSON output, and display formatting.
pub mod format;
/// Plan construction for switch, remove, list, hook, alias, relocate, and prune.
pub mod plan;
/// Minijinja template rendering with hook/alias variable context.
pub mod template;
/// Core domain types: config, actions, plans, observation state, and CLI args.
pub mod types;
