use anyhow::Result;
use std::path::{Path, PathBuf};

pub trait FileSystem {
    fn read_to_string(&self, path: &Path) -> Result<String>;
    fn write(&self, path: &Path, contents: &str) -> Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> Result<()>;
    fn read_dir(&self, path: &Path) -> Result<Vec<PathBuf>>;
    fn exists(&self, path: &Path) -> bool;
    fn create_dir_all(&self, path: &Path) -> Result<()>;
    fn is_dir(&self, path: &Path) -> bool;
}

pub struct RealFileSystem;

impl FileSystem for RealFileSystem {
    fn read_to_string(&self, path: &Path) -> Result<String> {
        Ok(std::fs::read_to_string(path)?)
    }

    fn write(&self, path: &Path, contents: &str) -> Result<()> {
        Ok(std::fs::write(path, contents)?)
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        Ok(std::fs::rename(from, to)?)
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<PathBuf>> {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(path)? {
            entries.push(entry?.path());
        }
        Ok(entries)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn create_dir_all(&self, path: &Path) -> Result<()> {
        Ok(std::fs::create_dir_all(path)?)
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }
}
