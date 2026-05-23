/// Deterministic friendly names from input strings (adjective-noun combos).
pub mod codename;
/// Short 3-character base36 hash digest.
pub mod hash;
/// Deterministic port number derived from a string hash.
pub mod hash_port;
/// POSIX-style dirname and basename path decomposition.
pub mod path_parts;
/// Simple branch-name sanitization (replace path separators with dashes).
pub mod sanitize;
/// Database-identifier-safe sanitization with hash suffix.
pub mod sanitize_db;
/// Filename-safe sanitization with hash suffix for collision avoidance.
pub mod sanitize_hash;
