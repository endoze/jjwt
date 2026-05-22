use anyhow::Result;
use std::path::{Path, PathBuf};

pub trait Fs {
  fn exists(&self, path: &Path) -> bool;
  fn remove_dir_all(&self, path: &Path) -> Result<()>;
  fn current_dir(&self) -> Result<PathBuf>;
}

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
}
