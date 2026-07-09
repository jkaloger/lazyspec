//! Persistent cache of ClickUp per-status colours, at
//! `.lazyspec/status-colors.json`. Mirrors [`TaskMap`](crate::engine::task_map):
//! keyed `type_name -> { status_name -> hex }`, captured during sync and
//! resolved by renderers (TUI/CLI/web) in later iterations.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

const MAP_PATH: &str = ".lazyspec/status-colors.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatusColors {
    #[serde(flatten)]
    types: HashMap<String, HashMap<String, String>>,
}

impl StatusColors {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(MAP_PATH);
        if !path.exists() {
            return Ok(Self {
                types: HashMap::new(),
            });
        }
        let contents = std::fs::read_to_string(&path)?;
        let types: HashMap<String, HashMap<String, String>> = serde_json::from_str(&contents)?;
        Ok(Self { types })
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let path = root.join(MAP_PATH);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.types)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    pub fn set_type(&mut self, type_name: impl Into<String>, colors: HashMap<String, String>) {
        self.types.insert(type_name.into(), colors);
    }

    pub fn get(&self, type_name: &str, status: &str) -> Option<&str> {
        self.types
            .get(type_name)
            .and_then(|statuses| statuses.get(status))
            .map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_missing_file_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let colors = StatusColors::load(tmp.path()).unwrap();
        assert!(colors.get("story", "pending").is_none());
    }

    #[test]
    fn set_type_and_get() {
        let tmp = TempDir::new().unwrap();
        let mut colors = StatusColors::load(tmp.path()).unwrap();
        colors.set_type(
            "story",
            HashMap::from([("pending".to_string(), "#f00".to_string())]),
        );
        assert_eq!(colors.get("story", "pending"), Some("#f00"));
    }

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let mut colors = StatusColors::load(tmp.path()).unwrap();
        colors.set_type(
            "story",
            HashMap::from([("pending".to_string(), "#f00".to_string())]),
        );
        colors.set_type(
            "iteration",
            HashMap::from([("done".to_string(), "#0f0".to_string())]),
        );
        colors.save(tmp.path()).unwrap();

        let loaded = StatusColors::load(tmp.path()).unwrap();
        assert_eq!(loaded.get("story", "pending"), Some("#f00"));
        assert_eq!(loaded.get("iteration", "done"), Some("#0f0"));
        assert!(tmp.path().join(".lazyspec/status-colors.json").exists());
    }

    #[test]
    fn get_unknown_type_or_status_returns_none() {
        let tmp = TempDir::new().unwrap();
        let mut colors = StatusColors::load(tmp.path()).unwrap();
        colors.set_type(
            "story",
            HashMap::from([("pending".to_string(), "#f00".to_string())]),
        );
        assert!(colors.get("audit", "pending").is_none());
        assert!(colors.get("story", "done").is_none());
    }

    #[test]
    fn save_writes_cache_under_lazyspec_and_leaves_config_untouched() {
        let tmp = TempDir::new().unwrap();
        let mut colors = StatusColors::load(tmp.path()).unwrap();
        colors.set_type(
            "story",
            HashMap::from([("pending".to_string(), "#f00".to_string())]),
        );
        colors.save(tmp.path()).unwrap();
        assert!(tmp.path().join(".lazyspec/status-colors.json").exists());
        assert!(!tmp.path().join(".lazyspec.toml").exists());
    }
}
