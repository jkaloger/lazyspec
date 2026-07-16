use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::engine::fs::atomic_write;

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
        match serde_json::from_str::<BTreeMap<String, String>>(&content) {
            Ok(entries) => Ok(Self { entries }),
            Err(_) => {
                // Migration: old format had { "key": { "cached_at": "..." } } or mixed values.
                // A file that parses as neither format is corrupt: hard error,
                // never a silent default, so freshness state is not erased.
                let raw: BTreeMap<String, serde_json::Value> = serde_json::from_str(&content)
                    .with_context(|| {
                        format!(
                            "cache lock at {} is corrupt; fix or delete the file",
                            path.display()
                        )
                    })?;
                let entries = raw
                    .into_iter()
                    .filter_map(|(k, v)| match v {
                        serde_json::Value::String(s) => Some((k, s)),
                        serde_json::Value::Object(map) => map
                            .get("cached_at")
                            .and_then(|v| v.as_str())
                            .map(|s| (k, s.to_string())),
                        _ => None,
                    })
                    .collect();
                let lock = Self { entries };
                lock.save(root)?;
                Ok(lock)
            }
        }
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let dir = root.join(".lazyspec");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("cache.lock");
        let json = serde_json::to_string_pretty(&self.entries)?;
        atomic_write(&path, &json)
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
    fn test_load_corrupt_lock_errors_and_leaves_file_unchanged() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let dir = root.join(".lazyspec");
        std::fs::create_dir_all(&dir).unwrap();

        // A truncated write: not valid JSON in either format.
        let corrupt = r#"{"iteration/ITERATION-042": "abc12"#;
        std::fs::write(dir.join("cache.lock"), corrupt).unwrap();

        let err = CacheLock::load(root).unwrap_err();
        assert!(err.to_string().contains("corrupt"), "got: {err}");

        // The corrupt file is untouched: no defaulted lock persisted over it.
        let raw = std::fs::read_to_string(dir.join("cache.lock")).unwrap();
        assert_eq!(raw, corrupt);
    }

    #[test]
    fn test_migration_from_old_cached_at_format() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let dir = root.join(".lazyspec");
        std::fs::create_dir_all(&dir).unwrap();

        // Old format: { "key": { "cached_at": "timestamp" } }
        let old_json = r#"{
  "STORY-10": { "cached_at": "2026-03-27T10:00:00+00:00" },
  "STORY-11": { "cached_at": "2026-03-28T12:00:00+00:00" }
}"#;
        std::fs::write(dir.join("cache.lock"), old_json).unwrap();

        let lock = CacheLock::load(root).unwrap();
        assert_eq!(lock.get("STORY-10"), Some("2026-03-27T10:00:00+00:00"));
        assert_eq!(lock.get("STORY-11"), Some("2026-03-28T12:00:00+00:00"));

        // Verify it was re-saved in new format
        let raw = std::fs::read_to_string(dir.join("cache.lock")).unwrap();
        let parsed: BTreeMap<String, String> = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn test_migration_mixed_old_and_string_values() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let dir = root.join(".lazyspec");
        std::fs::create_dir_all(&dir).unwrap();

        // Mixed format: some old wrapped entries, some plain strings
        let mixed_json = r#"{
  "STORY-10": { "cached_at": "2026-03-27T10:00:00+00:00" },
  "iteration/ITERATION-042": "abc123"
}"#;
        std::fs::write(dir.join("cache.lock"), mixed_json).unwrap();

        let lock = CacheLock::load(root).unwrap();
        assert_eq!(lock.get("STORY-10"), Some("2026-03-27T10:00:00+00:00"));
        assert_eq!(lock.get("iteration/ITERATION-042"), Some("abc123"));
    }

    #[test]
    fn test_both_backends_coexist() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let mut lock = CacheLock::default();
        // IssueCache-style: doc ID -> RFC3339 timestamp
        lock.set("STORY-10", "2026-03-27T10:00:00+00:00");
        lock.set("STORY-11", "2026-03-28T12:00:00+00:00");
        // git-ref-style: type/id -> SHA
        lock.set("iteration/ITERATION-042", "abc123");
        lock.set("story/STORY-001", "def456");
        lock.save(root).unwrap();

        let loaded = CacheLock::load(root).unwrap();
        assert_eq!(loaded.get("STORY-10"), Some("2026-03-27T10:00:00+00:00"));
        assert_eq!(loaded.get("STORY-11"), Some("2026-03-28T12:00:00+00:00"));
        assert_eq!(loaded.get("iteration/ITERATION-042"), Some("abc123"));
        assert_eq!(loaded.get("story/STORY-001"), Some("def456"));
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
