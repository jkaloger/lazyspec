use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

const MAP_PATH: &str = ".lazyspec/issue-map.json";

/// What kind of GitHub object an entry's `issue_number` refers to. Issues,
/// milestones, and Projects v2 boards each have independent number sequences,
/// so the same number can appear under different kinds in one map. Reverse
/// number lookups must therefore filter by kind to avoid collisions.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    #[default]
    Issue,
    Milestone,
    Project,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IssueMapEntry {
    pub issue_number: u64,
    pub updated_at: String,
    /// GraphQL node id (`I_*`) of the issue. Empty when unknown (legacy maps,
    /// or writes that only carry a REST number). Sub-issue mutations key off
    /// this, not `issue_number`.
    #[serde(default)]
    pub node_id: String,
    /// The GitHub object kind this entry maps to. Defaults to `Issue` so legacy
    /// maps (written before kinds were tracked) deserialize as issues.
    #[serde(default)]
    pub kind: EntryKind,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
        self.insert_kind(id, number, updated_at, node_id, EntryKind::Issue);
    }

    /// Insert an entry of an explicit kind. Milestone and Projects v2 board
    /// entries must use this so reverse number lookups can exclude them.
    pub fn insert_kind(
        &mut self,
        id: impl Into<String>,
        number: u64,
        updated_at: impl Into<String>,
        node_id: impl Into<String>,
        kind: EntryKind,
    ) {
        self.entries.insert(
            id.into(),
            IssueMapEntry {
                issue_number: number,
                updated_at: updated_at.into(),
                node_id: node_id.into(),
                kind,
            },
        );
    }

    pub fn get(&self, id: &str) -> Option<&IssueMapEntry> {
        self.entries.get(id)
    }

    /// Reverse lookup: the lazyspec shorthand id mapped to a given GitHub issue
    /// number, or `None` when no synced doc owns that number. Used to derive the
    /// inverse `targeted-by` relations on milestones.
    ///
    /// Only issue-kind entries are considered: milestone and Projects v2 board
    /// numbers are independent sequences that can collide with issue numbers, so
    /// matching them here would yield a wrong (often self-referential) target.
    /// Issue numbers are unique among issue entries, so the result is
    /// deterministic regardless of map iteration order.
    pub fn shorthand_for_number(&self, number: u64) -> Option<&str> {
        self.entries
            .iter()
            .find(|(_, entry)| entry.kind == EntryKind::Issue && entry.issue_number == number)
            .map(|(id, _)| id.as_str())
    }

    /// Reverse lookup restricted to milestone-kind entries: the `MILESTONE-n`
    /// shorthand mapped to a given GitHub milestone number, or `None` when no
    /// synced milestone owns it. Used at fetch to resolve an issue's native
    /// milestone into a forward `targets` relation. Milestone numbers are a
    /// sequence independent of issue numbers, so this must filter by kind to
    /// avoid matching an unrelated issue that happens to share the number.
    pub fn milestone_shorthand_for_number(&self, number: u64) -> Option<&str> {
        self.entries
            .iter()
            .find(|(_, entry)| entry.kind == EntryKind::Milestone && entry.issue_number == number)
            .map(|(id, _)| id.as_str())
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
    fn shorthand_for_number_ignores_milestone_collision() {
        let root = tmp_root("collision");
        let mut map = IssueMap::load(&root).unwrap();
        // Milestone #3 and issue #3 are independent number sequences and can
        // coexist. The reverse lookup must resolve to the ISSUE shorthand.
        map.insert_kind("MILESTONE-1", 3, "", "", EntryKind::Milestone);
        map.insert("STORY-5", 3, "", "");

        assert_eq!(map.shorthand_for_number(3), Some("STORY-5"));
    }

    #[test]
    fn shorthand_for_number_ignores_project_collision() {
        let root = tmp_root("collision_project");
        let mut map = IssueMap::load(&root).unwrap();
        map.insert_kind("PROJECT-2", 9, "", "", EntryKind::Project);
        map.insert("TICKET-8", 9, "", "");

        assert_eq!(map.shorthand_for_number(9), Some("TICKET-8"));
    }

    #[test]
    fn shorthand_for_number_milestone_only_returns_none() {
        let root = tmp_root("milestone_only");
        let mut map = IssueMap::load(&root).unwrap();
        map.insert_kind("MILESTONE-1", 3, "", "", EntryKind::Milestone);

        assert_eq!(map.shorthand_for_number(3), None);
    }

    #[test]
    fn entry_kind_defaults_to_issue_on_legacy_map() {
        let root = tmp_root("legacy_kind");
        let path = root.join(".lazyspec");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("issue-map.json"),
            r#"{"STORY-5": {"issue_number": 3, "updated_at": "ts"}}"#,
        )
        .unwrap();

        let loaded = IssueMap::load(&root).unwrap();
        assert_eq!(loaded.get("STORY-5").unwrap().kind, EntryKind::Issue);
        assert_eq!(loaded.shorthand_for_number(3), Some("STORY-5"));
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
