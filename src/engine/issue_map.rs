use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

const MAP_PATH: &str = ".lazyspec/issue-map.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IssueMapEntry {
    pub issue_number: u64,
    pub updated_at: String,
    /// GraphQL node id (`I_*`) of the issue. Empty when unknown (legacy maps,
    /// or writes that only carry a REST number). Sub-issue mutations key off
    /// this, not `issue_number`.
    #[serde(default)]
    pub node_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueMap {
    #[serde(flatten)]
    entries: HashMap<String, IssueMapEntry>,
}

impl IssueMap {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(MAP_PATH);
        if !path.exists() {
            return Ok(Self {
                entries: HashMap::new(),
            });
        }
        let contents = std::fs::read_to_string(&path)?;
        let entries: HashMap<String, IssueMapEntry> = serde_json::from_str(&contents)?;
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
        number: u64,
        updated_at: impl Into<String>,
        node_id: impl Into<String>,
    ) {
        self.entries.insert(
            id.into(),
            IssueMapEntry {
                issue_number: number,
                updated_at: updated_at.into(),
                node_id: node_id.into(),
            },
        );
    }

    pub fn get(&self, id: &str) -> Option<&IssueMapEntry> {
        self.entries.get(id)
    }

    pub fn remove(&mut self, id: &str) {
        self.entries.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lazyspec-issue-map-{}-{}",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let root = tmp_root("load_missing");
        let map = IssueMap::load(&root).unwrap();
        assert!(map.get("anything").is_none());
    }

    #[test]
    fn insert_and_get() {
        let root = tmp_root("insert_get");
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("ITERATION-042", 87, "2026-03-27T10:00:00Z", "I_node42");

        let entry = map.get("ITERATION-042").unwrap();
        assert_eq!(entry.issue_number, 87);
        assert_eq!(entry.updated_at, "2026-03-27T10:00:00Z");
        assert_eq!(entry.node_id, "I_node42");
    }

    #[test]
    fn node_id_survives_save_load_roundtrip() {
        let root = tmp_root("node_id_roundtrip");
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("ITERATION-050", 50, "ts", "I_kwDOabc");
        map.save(&root).unwrap();

        let loaded = IssueMap::load(&root).unwrap();
        assert_eq!(loaded.get("ITERATION-050").unwrap().node_id, "I_kwDOabc");
    }

    #[test]
    fn missing_node_id_defaults_empty() {
        let root = tmp_root("node_id_default");
        let path = root.join(".lazyspec");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("issue-map.json"),
            r#"{"ITERATION-001": {"issue_number": 1, "updated_at": "ts"}}"#,
        )
        .unwrap();

        let loaded = IssueMap::load(&root).unwrap();
        assert_eq!(loaded.get("ITERATION-001").unwrap().node_id, "");
    }

    #[test]
    fn save_and_load_roundtrip() {
        let root = tmp_root("roundtrip");
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("ITERATION-042", 87, "2026-03-27T10:00:00Z", "I_a");
        map.insert("ITERATION-043", 88, "2026-03-27T10:05:00Z", "I_b");
        map.save(&root).unwrap();

        let loaded = IssueMap::load(&root).unwrap();
        assert_eq!(
            loaded.get("ITERATION-042").unwrap(),
            map.get("ITERATION-042").unwrap()
        );
        assert_eq!(
            loaded.get("ITERATION-043").unwrap(),
            map.get("ITERATION-043").unwrap()
        );
    }

    #[test]
    fn save_creates_lazyspec_directory() {
        let root = tmp_root("creates_dir");
        let lazyspec_dir = root.join(".lazyspec");
        assert!(!lazyspec_dir.exists());

        let mut map = IssueMap::load(&root).unwrap();
        map.insert("STORY-001", 1, "2026-01-01T00:00:00Z", "");
        map.save(&root).unwrap();

        assert!(lazyspec_dir.exists());
        assert!(lazyspec_dir.join("issue-map.json").exists());
    }
}
