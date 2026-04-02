use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CacheLock {
    entries: BTreeMap<String, String>,
}

impl CacheLock {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(".lazyspec/cache.lock");
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)?;
        let entries: BTreeMap<String, String> = serde_json::from_str(&content)?;
        Ok(Self { entries })
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let dir = root.join(".lazyspec");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("cache.lock");
        let json = serde_json::to_string_pretty(&self.entries)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    pub fn get(&self, doc_key: &str) -> Option<&str> {
        self.entries.get(doc_key).map(|s| s.as_str())
    }

    pub fn set(&mut self, doc_key: &str, sha: &str) {
        self.entries.insert(doc_key.to_string(), sha.to_string());
    }

    pub fn remove(&mut self, doc_key: &str) {
        self.entries.remove(doc_key);
    }

    pub fn keys_for_type(&self, type_prefix: &str) -> Vec<String> {
        let prefix = format!("{}/", type_prefix);
        self.entries
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_cache_lock_round_trip() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Load on empty dir returns empty
        let lock = CacheLock::load(root).unwrap();
        assert!(lock.get("iteration/ITERATION-042").is_none());

        // Set + save + load round-trips
        let mut lock = CacheLock::default();
        lock.set("iteration/ITERATION-042", "abc123");
        lock.set("story/STORY-001", "def456");
        lock.save(root).unwrap();

        let loaded = CacheLock::load(root).unwrap();
        assert_eq!(loaded.get("iteration/ITERATION-042"), Some("abc123"));
        assert_eq!(loaded.get("story/STORY-001"), Some("def456"));

        // Remove + save removes entry
        let mut lock = loaded;
        lock.remove("story/STORY-001");
        lock.save(root).unwrap();

        let loaded = CacheLock::load(root).unwrap();
        assert!(loaded.get("story/STORY-001").is_none());
        assert_eq!(loaded.get("iteration/ITERATION-042"), Some("abc123"));
    }

    #[test]
    fn test_cache_lock_format() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let mut lock = CacheLock::default();
        lock.set("iteration/ITERATION-042", "abc123");
        lock.set("story/STORY-001", "def456");
        lock.save(root).unwrap();

        let raw = std::fs::read_to_string(root.join(".lazyspec/cache.lock")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();

        let obj = parsed.as_object().unwrap();
        assert_eq!(obj.get("iteration/ITERATION-042").unwrap(), "abc123");
        assert_eq!(obj.get("story/STORY-001").unwrap(), "def456");

        // BTreeMap ensures sorted keys
        let keys: Vec<&String> = obj.keys().collect();
        assert_eq!(keys, vec!["iteration/ITERATION-042", "story/STORY-001"]);
    }

    #[test]
    fn test_keys_for_type_filters_by_prefix() {
        let mut lock = CacheLock::default();
        lock.set("iteration/ITERATION-042", "abc123");
        lock.set("iteration/ITERATION-043", "def456");
        lock.set("story/STORY-001", "ghi789");

        let iteration_keys = lock.keys_for_type("iteration");
        assert_eq!(
            iteration_keys,
            vec![
                "iteration/ITERATION-042".to_string(),
                "iteration/ITERATION-043".to_string(),
            ]
        );

        let story_keys = lock.keys_for_type("story");
        assert_eq!(story_keys, vec!["story/STORY-001".to_string()]);

        let empty = lock.keys_for_type("rfc");
        assert!(empty.is_empty());
    }
}
