//! Entry point for the `jjwt` CLI binary.
#![cfg(not(tarpaulin_include))]
#![deny(missing_docs)]

use anyhow::Result;
fn main() -> Result<()> {
  if jjwt::completion::maybe_handle_env_completion() {
    return Ok(());
  }

  jjwt::cli::run()
}
