//! Persistent map from a lazyspec doc id to the ClickUp task it materializes,
//! at `.lazyspec/task-map.json`. Mirrors [`IssueMap`](crate::engine::issue_map)
//! for the ClickUp store: it records the external task id and the task's
//! `date_updated` (the timestamp a later write path's optimistic lock compares).
//!
//! ClickUp task ids are opaque alphanumeric strings, not the integer sequence
//! GitHub issue numbers form, so the entry keeps `task_id` as a `String` rather
//! than a number.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

const MAP_PATH: &str = ".lazyspec/task-map.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskMapEntry {
    /// The ClickUp task id (opaque string, e.g. `"86abc123"`).
    pub task_id: String,
    /// ClickUp's `date_updated` for the task (epoch-ms as a string). The
    /// optimistic-lock timestamp the write path compares; empty when unknown.
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskMap {
    #[serde(flatten)]
    entries: HashMap<String, TaskMapEntry>,
}

impl TaskMap {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(MAP_PATH);
        if !path.exists() {
            return Ok(Self {
                entries: HashMap::new(),
            });
        }
        let contents = std::fs::read_to_string(&path)?;
        let entries: HashMap<String, TaskMapEntry> = serde_json::from_str(&contents)?;
        Ok(Self { entries })
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let path = root.join(MAP_PATH);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.entries)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    pub fn insert(
        &mut self,
        id: impl Into<String>,
        task_id: impl Into<String>,
        updated_at: impl Into<String>,
    ) {
        self.entries.insert(
            id.into(),
            TaskMapEntry {
                task_id: task_id.into(),
                updated_at: updated_at.into(),
            },
        );
    }

    pub fn get(&self, id: &str) -> Option<&TaskMapEntry> {
        self.entries.get(id)
    }

    pub fn remove(&mut self, id: &str) {
        self.entries.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_missing_file_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let map = TaskMap::load(tmp.path()).unwrap();
        assert!(map.get("anything").is_none());
    }

    #[test]
    fn insert_and_get() {
        let tmp = TempDir::new().unwrap();
        let mut map = TaskMap::load(tmp.path()).unwrap();
        map.insert("TASK-86abc", "86abc", "1774587145901");
        let entry = map.get("TASK-86abc").unwrap();
        assert_eq!(entry.task_id, "86abc");
        assert_eq!(entry.updated_at, "1774587145901");
    }

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let mut map = TaskMap::load(tmp.path()).unwrap();
        map.insert("TASK-1", "abc", "100");
        map.insert("TASK-2", "def", "200");
        map.save(tmp.path()).unwrap();

        let loaded = TaskMap::load(tmp.path()).unwrap();
        assert_eq!(loaded.get("TASK-1").unwrap().task_id, "abc");
        assert_eq!(loaded.get("TASK-2").unwrap().updated_at, "200");
    }

    #[test]
    fn save_creates_lazyspec_directory() {
        let tmp = TempDir::new().unwrap();
        let mut map = TaskMap::load(tmp.path()).unwrap();
        map.insert("TASK-1", "abc", "100");
        map.save(tmp.path()).unwrap();
        assert!(tmp.path().join(".lazyspec/task-map.json").exists());
    }

    #[test]
    fn remove_drops_entry() {
        let tmp = TempDir::new().unwrap();
        let mut map = TaskMap::load(tmp.path()).unwrap();
        map.insert("TASK-1", "abc", "100");
        map.remove("TASK-1");
        assert!(map.get("TASK-1").is_none());
    }
}
