//! Entry point for the `jjwt` CLI binary.
#![cfg(not(tarpaulin_include))]
#![deny(missing_docs)]

use anyhow::Result;
fn main() -> Result<()> {
  jjwt::cli::run()
}
