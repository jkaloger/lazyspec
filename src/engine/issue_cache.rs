use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::engine::config::TypeDef;
use crate::engine::document::{DocMeta, DocType, Status};
use crate::engine::gh::{type_label, GhIssue, GhIssueReader};
use crate::engine::issue_body::{self, IssueContext};
use crate::engine::issue_map::IssueMap;
use crate::engine::store_dispatch;

#[derive(Debug)]
pub struct FetchResult {
    pub fetched: usize,
    pub new: usize,
    pub removed: usize,
}

#[derive(Debug)]
pub struct RefreshResult {
    pub refreshed: usize,
    pub unchanged: usize,
    pub warnings: Vec<RefreshWarning>,
}

#[derive(Debug)]
pub struct RefreshWarning {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CacheLockEntry {
    pub cached_at: String,
}

pub type CacheLock = HashMap<String, CacheLockEntry>;

pub struct IssueCache {
    root: PathBuf,
}

impl IssueCache {
    pub fn new(root: &Path) -> Self {
        IssueCache {
            root: root.join(".lazyspec").join("cache"),
        }
    }

    fn lock_path(&self) -> PathBuf {
        self.root.parent().unwrap_or(&self.root).join("cache.lock")
    }

    fn doc_path(&self, id: &str, doc_type: &str) -> PathBuf {
        self.root.join(doc_type).join(format!("{}.md", id))
    }

    pub fn read_lock(&self) -> CacheLock {
        let path = self.lock_path();
        let Ok(data) = fs::read_to_string(&path) else {
            return CacheLock::default();
        };
        serde_json::from_str(&data).unwrap_or_default()
    }

    pub fn write_lock(&self, lock: &CacheLock) {
        let path = self.lock_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(lock).unwrap_or_default();
        let _ = fs::write(&path, json);
    }

    pub fn is_fresh(&self, id: &str, ttl: Duration) -> bool {
        let lock = self.read_lock();
        let Some(entry) = lock.get(id) else {
            return false;
        };
        let Ok(cached_at) = entry.cached_at.parse::<DateTime<Utc>>() else {
            return false;
        };
        Utc::now() - cached_at < ttl
    }

    pub fn read_if_fresh(&self, id: &str, doc_type: &str, ttl: Duration) -> Option<String> {
        if !self.is_fresh(id, ttl) {
            return None;
        }
        fs::read_to_string(self.doc_path(id, doc_type)).ok()
    }

    pub fn read_stale(&self, id: &str, doc_type: &str) -> Option<String> {
        fs::read_to_string(self.doc_path(id, doc_type)).ok()
    }

    pub fn write(&self, id: &str, doc_type: &str, content: &str) {
        let path = self.doc_path(id, doc_type);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&path, content);

        let mut lock = self.read_lock();
        lock.insert(
            id.to_string(),
            CacheLockEntry {
                cached_at: Utc::now().to_rfc3339(),
            },
        );
        self.write_lock(&lock);
    }

    pub fn remove(&self, id: &str, doc_type: &str) {
        let path = self.doc_path(id, doc_type);
        let _ = fs::remove_file(&path);

        let mut lock = self.read_lock();
        lock.remove(id);
        self.write_lock(&lock);
    }

    /// Refresh stale cache entries for a given type with a single `issue_list` call.
    ///
    /// Returns early with zero API calls if all cached documents are fresh.
    /// On API failure, leaves stale cache in place and returns a warning.
    pub fn refresh_stale(
        &self,
        root: &Path,
        type_def: &TypeDef,
        gh: &dyn GhIssueReader,
        repo: &str,
        issue_map: &mut IssueMap,
        ttl: Duration,
    ) -> RefreshResult {
        let cached_ids = self.list_cached(&type_def.name);
        if cached_ids.is_empty() {
            return RefreshResult {
                refreshed: 0,
                unchanged: 0,
                warnings: vec![],
            };
        }

        let any_stale = cached_ids.iter().any(|id| !self.is_fresh(id, ttl));
        if !any_stale {
            return RefreshResult {
                refreshed: 0,
                unchanged: cached_ids.len(),
                warnings: vec![],
            };
        }

        let label = type_label(&type_def.name);
        let labels = vec![label];
        let fields = vec![
            "number".into(),
            "title".into(),
            "body".into(),
            "labels".into(),
            "state".into(),
            "updatedAt".into(),
        ];

        let issues = match gh.issue_list(repo, &labels, &fields, None) {
            Ok(issues) => issues,
            Err(e) => {
                return RefreshResult {
                    refreshed: 0,
                    unchanged: cached_ids.len(),
                    warnings: vec![RefreshWarning {
                        message: format!(
                            "API unreachable for type '{}', serving stale cache: {}",
                            type_def.name, e
                        ),
                    }],
                };
            }
        };

        let mut refreshed = 0usize;
        let mut unchanged = 0usize;

        for issue in &issues {
            let (meta, body) = parse_issue(issue, &type_def.name);
            let id = extract_doc_id(issue, &type_def.name)
                .unwrap_or_else(|| issue.number.to_string());
            let meta = DocMeta { id: id.clone(), ..meta };

            let existing = self.read_stale(&id, &type_def.name);
            let new_content = build_cache_content(&meta, &body);

            if existing.as_deref() == Some(&new_content) {
                unchanged += 1;
            } else {
                if let Err(e) = store_dispatch::write_cache_file(root, type_def, &meta, &body) {
                    // Non-fatal: skip this doc but keep going
                    eprintln!("warning: failed to write cache for {}: {}", id, e);
                    continue;
                }
                refreshed += 1;
            }

            // Update lock timestamp and issue map regardless
            let mut lock = self.read_lock();
            lock.insert(
                id.clone(),
                CacheLockEntry {
                    cached_at: Utc::now().to_rfc3339(),
                },
            );
            self.write_lock(&lock);
            issue_map.insert(&id, issue.number, &issue.updated_at);
        }

        RefreshResult {
            refreshed,
            unchanged,
            warnings: vec![],
        }
    }

    pub fn list_cached(&self, doc_type: &str) -> Vec<String> {
        let dir = self.root.join(doc_type);
        let Ok(entries) = fs::read_dir(&dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                if path.extension().and_then(|s| s.to_str()) != Some("md") {
                    return None;
                }
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            })
            .collect()
    }

    /// Full fetch of all issues for a type, with pagination and cleanup of removed issues.
    pub fn fetch_all(
        &self,
        root: &Path,
        type_def: &TypeDef,
        gh: &dyn GhIssueReader,
        repo: &str,
        issue_map: &mut IssueMap,
    ) -> anyhow::Result<FetchResult> {
        let label = type_label(&type_def.name);
        let labels = vec![label];
        const FETCH_LIMIT: u64 = 500;

        let issues = gh.issue_list(repo, &labels, &[], Some(FETCH_LIMIT))?;

        if issues.len() as u64 == FETCH_LIMIT {
            eprintln!(
                "warning: fetched exactly {} issues for type '{}'; there may be more",
                FETCH_LIMIT, type_def.name
            );
        }

        let previously_cached: std::collections::HashSet<String> =
            self.list_cached(&type_def.name).into_iter().collect();
        let mut fetched_ids = std::collections::HashSet::new();

        let cache_dir = root.join(".lazyspec/cache").join(&type_def.name);
        fs::create_dir_all(&cache_dir)?;

        let mut new_count = 0usize;

        for issue in &issues {
            let (meta, body) = parse_issue(issue, &type_def.name);
            let id = extract_doc_id(issue, &type_def.name)
                .unwrap_or_else(|| issue.number.to_string());
            let meta = DocMeta { id: id.clone(), ..meta };

            if !previously_cached.contains(&id) {
                new_count += 1;
            }

            store_dispatch::write_cache_file(root, type_def, &meta, &body)?;

            let mut lock = self.read_lock();
            lock.insert(
                id.clone(),
                CacheLockEntry {
                    cached_at: Utc::now().to_rfc3339(),
                },
            );
            self.write_lock(&lock);

            issue_map.insert(&id, issue.number, &issue.updated_at);
            fetched_ids.insert(id);
        }

        let removed: Vec<String> = previously_cached
            .difference(&fetched_ids)
            .cloned()
            .collect();

        for id in &removed {
            self.remove(id, &type_def.name);
            issue_map.remove(id);
        }

        Ok(FetchResult {
            fetched: issues.len(),
            new: new_count,
            removed: removed.len(),
        })
    }
}

fn parse_issue(issue: &GhIssue, type_name: &str) -> (DocMeta, String) {
    let ctx = IssueContext {
        title: issue.title.clone(),
        labels: issue.labels.iter().map(|l| l.name.clone()).collect(),
        is_open: issue.state.eq_ignore_ascii_case("open"),
    };

    if let Ok((meta, body)) = issue_body::deserialize(&issue.body, &ctx) {
        return (meta, body);
    }

    let status = if issue.state.eq_ignore_ascii_case("open") {
        Status::Draft
    } else {
        Status::Complete
    };

    let meta = DocMeta {
        path: PathBuf::new(),
        title: issue.title.clone(),
        doc_type: DocType::new(type_name),
        status,
        author: "unknown".to_string(),
        date: Utc::now().date_naive(),
        tags: issue
            .labels
            .iter()
            .filter(|l| !l.name.starts_with("lazyspec:"))
            .map(|l| l.name.clone())
            .collect(),
        related: vec![],
        validate_ignore: false,
        virtual_doc: false,
        id: String::new(),
    };

    (meta, issue.body.clone())
}

fn extract_doc_id(issue: &GhIssue, type_name: &str) -> Option<String> {
    let prefix = type_name.to_uppercase();
    let pattern = format!("{}-", prefix);

    for text in [&issue.title, &issue.body] {
        for word in text.split_whitespace() {
            if let Some(rest) = word.strip_prefix(&pattern) {
                let id_part: String = rest.chars().take_while(|c| c.is_alphanumeric()).collect();
                if !id_part.is_empty() {
                    return Some(format!("{}-{}", prefix, id_part));
                }
            }
        }
    }

    None
}

fn build_cache_content(meta: &DocMeta, body: &str) -> String {
    let tags_str = if meta.tags.is_empty() {
        "[]".to_string()
    } else {
        format!(
            "[{}]",
            meta.tags
                .iter()
                .map(|t| format!("\"{}\"", t))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    let related_str = if meta.related.is_empty() {
        "[]".to_string()
    } else {
        let lines: Vec<String> = meta
            .related
            .iter()
            .map(|r| format!("\n- {}: {}", r.rel_type, r.target))
            .collect();
        lines.join("")
    };

    format!(
        "---\ntitle: \"{}\"\ntype: {}\nstatus: {}\nauthor: \"{}\"\ndate: {}\ntags: {}\nrelated: {}\n---\n{}",
        meta.title,
        meta.doc_type.as_str(),
        meta.status,
        meta.author,
        meta.date,
        tags_str,
        related_str,
        body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use crate::engine::config::{NumberingStrategy, StoreBackend};
    use crate::engine::gh::{GhIssueReader, GhLabel};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    fn make_cache() -> (IssueCache, TempDir) {
        let tmp = TempDir::new().unwrap();
        let cache = IssueCache {
            root: tmp.path().join(".lazyspec").join("cache"),
        };
        (cache, tmp)
    }

    fn story_type_def() -> TypeDef {
        TypeDef {
            name: "story".to_string(),
            plural: "stories".to_string(),
            dir: "docs/story".to_string(),
            prefix: "STORY".to_string(),
            icon: None,
            numbering: NumberingStrategy::default(),
            subdirectory: false,
            store: StoreBackend::GithubIssues,
        }
    }

    fn make_gh_issue(number: u64, title: &str, body: &str, labels: &[&str]) -> GhIssue {
        GhIssue {
            number,
            url: format!("https://github.com/owner/repo/issues/{}", number),
            title: title.to_string(),
            body: body.to_string(),
            labels: labels
                .iter()
                .map(|l| GhLabel {
                    name: l.to_string(),
                    color: String::new(),
                })
                .collect(),
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
        }
    }

    struct MockReader {
        issues: Vec<GhIssue>,
        fail: bool,
        list_call_count: AtomicUsize,
    }

    impl MockReader {
        fn new(issues: Vec<GhIssue>) -> Self {
            Self {
                issues,
                fail: false,
                list_call_count: AtomicUsize::new(0),
            }
        }

        fn failing() -> Self {
            Self {
                issues: vec![],
                fail: true,
                list_call_count: AtomicUsize::new(0),
            }
        }

        fn call_count(&self) -> usize {
            self.list_call_count.load(Ordering::SeqCst)
        }
    }

    impl GhIssueReader for MockReader {
        fn issue_list(
            &self,
            _repo: &str,
            _labels: &[String],
            _json_fields: &[String],
            _limit: Option<u64>,
        ) -> Result<Vec<GhIssue>> {
            self.list_call_count.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                anyhow::bail!("API unreachable");
            }
            Ok(self.issues.clone())
        }

        fn issue_view(&self, _repo: &str, _number: u64) -> Result<GhIssue> {
            unimplemented!()
        }
    }

    #[test]
    fn test_issue_cache_write_and_fresh_read() {
        let (cache, _tmp) = make_cache();
        let ttl = Duration::seconds(60);

        cache.write("ITERATION-042", "iteration", "# Iteration 042\nSome content");

        let result = cache.read_if_fresh("ITERATION-042", "iteration", ttl);
        assert_eq!(
            result,
            Some("# Iteration 042\nSome content".to_string())
        );

        let doc_path = cache.doc_path("ITERATION-042", "iteration");
        assert!(doc_path.exists());

        let lock = cache.read_lock();
        assert!(lock.contains_key("ITERATION-042"));
    }

    #[test]
    fn test_issue_cache_stale_returns_none_from_fresh() {
        let (cache, _tmp) = make_cache();
        let ttl = Duration::seconds(60);

        cache.write("STORY-075", "story", "# Story 075\nStale content");

        // Backdate the cached_at to 2 minutes ago
        let mut lock = cache.read_lock();
        let two_min_ago = Utc::now() - Duration::seconds(120);
        lock.get_mut("STORY-075").unwrap().cached_at = two_min_ago.to_rfc3339();
        cache.write_lock(&lock);

        let fresh = cache.read_if_fresh("STORY-075", "story", ttl);
        assert_eq!(fresh, None);

        let stale = cache.read_stale("STORY-075", "story");
        assert_eq!(stale, Some("# Story 075\nStale content".to_string()));
    }

    #[test]
    fn test_issue_cache_cold_returns_none() {
        let (cache, _tmp) = make_cache();
        let ttl = Duration::seconds(60);

        assert_eq!(cache.read_if_fresh("NONEXISTENT-001", "rfc", ttl), None);
        assert_eq!(cache.read_stale("NONEXISTENT-001", "rfc"), None);
    }

    #[test]
    fn test_issue_cache_remove_deletes_file_and_lock_entry() {
        let (cache, _tmp) = make_cache();

        cache.write("ITERATION-001", "iteration", "content one");
        cache.write("ITERATION-002", "iteration", "content two");

        cache.remove("ITERATION-001", "iteration");

        assert!(!cache.doc_path("ITERATION-001", "iteration").exists());
        assert!(cache.doc_path("ITERATION-002", "iteration").exists());

        let lock = cache.read_lock();
        assert!(!lock.contains_key("ITERATION-001"));
        assert!(lock.contains_key("ITERATION-002"));
        assert_eq!(lock.len(), 1);
    }

    // --- refresh_stale tests ---

    fn backdate_all(cache: &IssueCache, ids: &[&str]) {
        let mut lock = cache.read_lock();
        let old = (Utc::now() - Duration::seconds(300)).to_rfc3339();
        for id in ids {
            if let Some(entry) = lock.get_mut(*id) {
                entry.cached_at = old.clone();
            }
        }
        cache.write_lock(&lock);
    }

    #[test]
    fn test_refresh_stale_fetches_all_via_issue_list() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();
        let ttl = Duration::seconds(60);

        // Seed 3 stale cache entries
        cache.write("STORY-001", "story", "old content 1");
        cache.write("STORY-002", "story", "old content 2");
        cache.write("STORY-003", "story", "old content 3");
        backdate_all(&cache, &["STORY-001", "STORY-002", "STORY-003"]);

        let gh = MockReader::new(vec![
            make_gh_issue(10, "STORY-001 First story", "Body 1", &["lazyspec:story"]),
            make_gh_issue(11, "STORY-002 Second story", "Body 2", &["lazyspec:story"]),
            make_gh_issue(12, "STORY-003 Third story", "Body 3", &["lazyspec:story"]),
        ]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let result = cache.refresh_stale(
            tmp.path(),
            &type_def,
            &gh,
            "owner/repo",
            &mut issue_map,
            ttl,
        );

        assert_eq!(gh.call_count(), 1, "should make exactly one issue_list call");
        assert_eq!(result.refreshed, 3);
        assert!(result.warnings.is_empty());

        // All 3 cache files should exist and lock entries should be fresh
        for id in &["STORY-001", "STORY-002", "STORY-003"] {
            assert!(
                cache.is_fresh(id, ttl),
                "cache entry {} should be fresh after refresh",
                id
            );
        }

        // Issue map should be updated
        assert_eq!(issue_map.get("STORY-001").unwrap().issue_number, 10);
        assert_eq!(issue_map.get("STORY-002").unwrap().issue_number, 11);
        assert_eq!(issue_map.get("STORY-003").unwrap().issue_number, 12);
    }

    #[test]
    fn test_refresh_stale_skips_api_when_all_fresh() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();
        let ttl = Duration::seconds(60);

        // Seed 3 fresh cache entries (default write sets cached_at to now)
        cache.write("STORY-001", "story", "content 1");
        cache.write("STORY-002", "story", "content 2");
        cache.write("STORY-003", "story", "content 3");

        let gh = MockReader::new(vec![]);
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let result = cache.refresh_stale(
            tmp.path(),
            &type_def,
            &gh,
            "owner/repo",
            &mut issue_map,
            ttl,
        );

        assert_eq!(gh.call_count(), 0, "should not call API when all fresh");
        assert_eq!(result.refreshed, 0);
        assert_eq!(result.unchanged, 3);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_refresh_stale_returns_stale_on_api_failure() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();
        let ttl = Duration::seconds(60);

        // Seed stale cache entries
        cache.write("STORY-001", "story", "stale content 1");
        cache.write("STORY-002", "story", "stale content 2");
        backdate_all(&cache, &["STORY-001", "STORY-002"]);

        let gh = MockReader::failing();
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let result = cache.refresh_stale(
            tmp.path(),
            &type_def,
            &gh,
            "owner/repo",
            &mut issue_map,
            ttl,
        );

        assert_eq!(result.refreshed, 0);
        assert_eq!(result.unchanged, 2);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].message.contains("API unreachable"));

        // Stale content should still be readable
        assert_eq!(
            cache.read_stale("STORY-001", "story"),
            Some("stale content 1".to_string())
        );
        assert_eq!(
            cache.read_stale("STORY-002", "story"),
            Some("stale content 2".to_string())
        );
    }

    // --- fetch_all tests ---

    #[test]
    fn test_fetch_all_populates_cache_with_frontmatter() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();

        let gh = MockReader::new(vec![
            make_gh_issue(10, "STORY-001 First story", "Body 1", &["lazyspec:story"]),
            make_gh_issue(11, "STORY-002 Second story", "Body 2", &["lazyspec:story"]),
            make_gh_issue(12, "STORY-003 Third story", "Body 3", &["lazyspec:story"]),
        ]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let result = cache
            .fetch_all(tmp.path(), &type_def, &gh, "owner/repo", &mut issue_map)
            .unwrap();

        assert_eq!(result.fetched, 3);
        assert_eq!(result.new, 3);
        assert_eq!(result.removed, 0);

        // All cache files exist with parseable frontmatter
        let cache_dir = tmp.path().join(".lazyspec/cache/story");
        for id in &["STORY-001", "STORY-002", "STORY-003"] {
            let path = cache_dir.join(format!("{}.md", id));
            assert!(path.exists(), "cache file for {} should exist", id);
            let content = std::fs::read_to_string(&path).unwrap();
            assert!(content.contains("title:"), "should have title frontmatter");
            assert!(content.contains("type: story"), "should have type frontmatter");
            assert!(content.contains("status:"), "should have status frontmatter");
        }

        // cache.lock updated
        let ttl = Duration::seconds(60);
        for id in &["STORY-001", "STORY-002", "STORY-003"] {
            assert!(cache.is_fresh(id, ttl), "cache.lock for {} should be fresh", id);
        }

        // issue map entries
        assert_eq!(issue_map.get("STORY-001").unwrap().issue_number, 10);
        assert_eq!(issue_map.get("STORY-002").unwrap().issue_number, 11);
        assert_eq!(issue_map.get("STORY-003").unwrap().issue_number, 12);

        // Verify Store::load can find the documents
        use crate::engine::store::Store;
        use crate::engine::config::{Config, GithubConfig};
        use crate::engine::document::DocType;
        let mut config = Config::default();
        config.documents.types = vec![story_type_def()];
        config.documents.github = Some(GithubConfig {
            repo: Some("owner/repo".to_string()),
            cache_ttl: 60,
        });
        let store = Store::load(tmp.path(), &config).unwrap();
        let filter = crate::engine::store::Filter {
            doc_type: Some(DocType::new("story")),
            status: None,
            tag: None,
        };
        let docs = store.list(&filter);
        assert_eq!(docs.len(), 3);
    }

    #[test]
    fn test_fetch_all_cleans_up_removed_issues() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();

        // Pre-populate cache with 3 docs
        let initial_gh = MockReader::new(vec![
            make_gh_issue(10, "STORY-001 First", "Body 1", &["lazyspec:story"]),
            make_gh_issue(11, "STORY-002 Second", "Body 2", &["lazyspec:story"]),
            make_gh_issue(12, "STORY-003 Third", "Body 3", &["lazyspec:story"]),
        ]);
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        cache
            .fetch_all(tmp.path(), &type_def, &initial_gh, "owner/repo", &mut issue_map)
            .unwrap();

        // Second fetch returns only 2 of the 3
        let updated_gh = MockReader::new(vec![
            make_gh_issue(10, "STORY-001 First", "Body 1 updated", &["lazyspec:story"]),
            make_gh_issue(11, "STORY-002 Second", "Body 2 updated", &["lazyspec:story"]),
        ]);
        let result = cache
            .fetch_all(tmp.path(), &type_def, &updated_gh, "owner/repo", &mut issue_map)
            .unwrap();

        assert_eq!(result.fetched, 2);
        assert_eq!(result.removed, 1);

        // STORY-003 should be gone
        let cache_dir = tmp.path().join(".lazyspec/cache/story");
        assert!(cache_dir.join("STORY-001.md").exists());
        assert!(cache_dir.join("STORY-002.md").exists());
        assert!(!cache_dir.join("STORY-003.md").exists());

        // cache.lock should not contain STORY-003
        let lock = cache.read_lock();
        assert!(lock.contains_key("STORY-001"));
        assert!(lock.contains_key("STORY-002"));
        assert!(!lock.contains_key("STORY-003"));

        // issue map should not contain STORY-003
        assert!(issue_map.get("STORY-001").is_some());
        assert!(issue_map.get("STORY-002").is_some());
        assert!(issue_map.get("STORY-003").is_none());
    }
}
