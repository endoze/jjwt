#![cfg(not(tarpaulin_include))]

use anyhow::Result;
use std::path::{Path, PathBuf};

/// Filesystem operations abstraction for testability.
pub trait Fs {
  /// Check whether a path exists on disk.
  fn exists(&self, path: &Path) -> bool;
  /// Recursively remove a directory and all its contents.
  fn remove_dir_all(&self, path: &Path) -> Result<()>;
  /// Return the current working directory.
  fn current_dir(&self) -> Result<PathBuf>;
  /// Rename (move) a file or directory.
  fn rename(&self, from: &Path, to: &Path) -> Result<()>;
  /// Create a directory and all missing parent directories.
  fn create_dir_all(&self, path: &Path) -> Result<()>;
}

/// Real filesystem implementation delegating to `std::fs`.
pub struct RealFs;

impl Fs for RealFs {
  fn exists(&self, path: &Path) -> bool {
    path.exists()
  }

  fn remove_dir_all(&self, path: &Path) -> Result<()> {
    std::fs::remove_dir_all(path).map_err(Into::into)
  }

  fn current_dir(&self) -> Result<PathBuf> {
    std::env::current_dir().map_err(Into::into)
  }

  fn rename(&self, from: &Path, to: &Path) -> Result<()> {
    std::fs::rename(from, to).map_err(Into::into)
  }

  fn create_dir_all(&self, path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(Into::into)
  }
}
