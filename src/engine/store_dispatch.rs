use std::cell::RefCell;
use std::path::PathBuf;

use anyhow::{bail, Result};
use chrono::Local;

use crate::engine::config::{Config, StoreBackend, TypeDef};
use crate::engine::document::{DocMeta, DocType, Status};
use crate::engine::gh::{self, GhIssueReader, GhIssueWriter};
use crate::engine::issue_body;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::store::{self, Store};
use crate::engine::template;

#[derive(Debug)]
pub struct CreatedDoc {
    pub path: PathBuf,
    pub id: String,
}

pub trait DocumentStore {
    fn create(
        &self,
        type_def: &TypeDef,
        title: &str,
        author: &str,
        body: &str,
    ) -> Result<CreatedDoc>;

    fn update(
        &self,
        type_def: &TypeDef,
        doc_id: &str,
        updates: &[(&str, &str)],
    ) -> Result<()>;

    fn delete(
        &self,
        type_def: &TypeDef,
        doc_id: &str,
    ) -> Result<()>;
}

pub struct FilesystemStore {
    pub root: PathBuf,
    pub config: Config,
}

impl DocumentStore for FilesystemStore {
    fn create(
        &self,
        type_def: &TypeDef,
        title: &str,
        author: &str,
        _body: &str,
    ) -> Result<CreatedDoc> {
        let path = crate::cli::create::run(
            &self.root,
            &self.config,
            &type_def.name,
            title,
            author,
            |_| {},
        )?;

        let relative = path.strip_prefix(&self.root).unwrap_or(&path).to_path_buf();
        let id = crate::engine::store::extract_id_from_name(
            relative
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(""),
        );

        Ok(CreatedDoc {
            path: relative,
            id,
        })
    }

    fn update(
        &self,
        _type_def: &TypeDef,
        doc_id: &str,
        updates: &[(&str, &str)],
    ) -> Result<()> {
        let store = Store::load(&self.root, &self.config)?;
        crate::cli::update::run(&self.root, &store, doc_id, updates)
    }

    fn delete(
        &self,
        _type_def: &TypeDef,
        doc_id: &str,
    ) -> Result<()> {
        let store = Store::load(&self.root, &self.config)?;
        crate::cli::delete::run(&self.root, &store, doc_id)
    }
}

pub struct GithubIssuesStore<G: GhIssueReader + GhIssueWriter> {
    pub client: G,
    pub root: PathBuf,
    pub repo: String,
    pub config: Config,
    pub issue_map: RefCell<IssueMap>,
    pub issue_cache: IssueCache,
}

impl<G: GhIssueReader + GhIssueWriter> DocumentStore for GithubIssuesStore<G> {
    fn create(
        &self,
        type_def: &TypeDef,
        title: &str,
        author: &str,
        body: &str,
    ) -> Result<CreatedDoc> {
        let cache_dir = self.root.join(".lazyspec/cache").join(&type_def.name);
        std::fs::create_dir_all(&cache_dir)?;

        let numbering = match type_def.numbering {
            crate::engine::config::NumberingStrategy::Sqids => {
                let sqids_config = self.config.documents.sqids.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("type '{}' uses sqids numbering but no sqids config found", type_def.name))?;
                Some((&type_def.numbering, sqids_config))
            }
            _ => None,
        };

        let filename = template::resolve_filename(
            &self.config.documents.naming.pattern,
            &type_def.name,
            title,
            &cache_dir,
            numbering,
            None,
        ).map_err(|e| anyhow::anyhow!("{}", e))?;

        let stem = filename.trim_end_matches(".md");
        let id = store::extract_id_from_name(stem);

        let date = Local::now().date_naive();
        let doc_meta = DocMeta {
            path: PathBuf::new(),
            title: title.to_string(),
            doc_type: DocType::new(&type_def.name),
            status: Status::Draft,
            author: author.to_string(),
            date,
            tags: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            id: id.clone(),
        };

        let issue_body = issue_body::serialize(&doc_meta, body);
        let label = gh::type_label(&type_def.name);
        let issue = self.client.issue_create(&self.repo, title, &issue_body, &[label])?;

        let mut map = self.issue_map.borrow_mut();
        map.insert(&id, issue.number, &issue.updated_at);
        map.save(&self.root)?;
        drop(map);

        let cache_content = format!(
            "---\ntitle: \"{}\"\ntype: {}\nstatus: draft\nauthor: \"{}\"\ndate: {}\ntags: []\nrelated: []\n---\n{}",
            title,
            type_def.name,
            author,
            date,
            if body.is_empty() { String::new() } else { format!("\n{}\n", body) },
        );
        self.issue_cache.write(&id, &type_def.name, &cache_content);

        let cache_path = self.root.join(".lazyspec/cache").join(&type_def.name).join(format!("{}.md", id));
        let relative = cache_path.strip_prefix(&self.root).unwrap_or(&cache_path).to_path_buf();
        Ok(CreatedDoc { path: relative, id })
    }

    fn update(
        &self,
        type_def: &TypeDef,
        doc_id: &str,
        updates: &[(&str, &str)],
    ) -> Result<()> {
        let (issue_number, local_updated_at) = {
            let map = self.issue_map.borrow();
            let entry = map.get(doc_id)
                .ok_or_else(|| anyhow::anyhow!("{} not found in issue map", doc_id))?;
            (entry.issue_number, entry.updated_at.clone())
        };

        let remote_issue = self.client.issue_view(&self.repo, issue_number)?;

        if remote_issue.updated_at != local_updated_at {
            bail!(
                "{} has been modified on GitHub since your last fetch.\n  \
                 Local:  {}\n  \
                 Remote: {}\n\
                 Wait for background sync or restart the TUI to pull the latest version.",
                doc_id,
                local_updated_at,
                remote_issue.updated_at,
            );
        }

        let ctx = issue_body::IssueContext {
            title: remote_issue.title.clone(),
            labels: remote_issue.labels.iter().map(|l| l.name.clone()).collect(),
            is_open: remote_issue.state == "OPEN",
        };
        let (mut meta, mut body) = issue_body::deserialize(&remote_issue.body, &ctx)?;

        let mut new_status: Option<Status> = None;
        for &(key, value) in updates {
            match key {
                "status" => {
                    let s: Status = value.parse()?;
                    new_status = Some(s.clone());
                    meta.status = s;
                }
                "title" => meta.title = value.to_string(),
                "author" => meta.author = value.to_string(),
                "body" => body = value.to_string(),
                _ => bail!("unknown update field: {}", key),
            }
        }

        let new_body = issue_body::serialize(&meta, &body);
        self.client.issue_edit(
            &self.repo,
            issue_number,
            None,
            Some(&new_body),
            &[],
            &[],
        )?;

        if let Some(status) = new_status {
            let should_be_open = matches!(
                status,
                Status::Draft | Status::Review | Status::Accepted | Status::InProgress
            );
            let is_open = remote_issue.state == "OPEN";
            if should_be_open && !is_open {
                self.client.issue_reopen(&self.repo, issue_number)?;
            } else if !should_be_open && is_open {
                self.client.issue_close(&self.repo, issue_number)?;
            }
        }

        let refreshed = self.client.issue_view(&self.repo, issue_number)?;
        let mut map = self.issue_map.borrow_mut();
        map.insert(doc_id, issue_number, &refreshed.updated_at);
        map.save(&self.root)?;
        drop(map);

        let cache_content = format!(
            "---\ntitle: \"{}\"\ntype: {}\nstatus: {}\nauthor: \"{}\"\ndate: {}\ntags: {}\nrelated: {}\n---\n{}",
            meta.title,
            meta.doc_type.as_str(),
            meta.status,
            meta.author,
            meta.date,
            if meta.tags.is_empty() { "[]".to_string() } else {
                format!("[{}]", meta.tags.iter().map(|t| format!("\"{}\"", t)).collect::<Vec<_>>().join(", "))
            },
            if meta.related.is_empty() { "[]".to_string() } else {
                meta.related.iter().map(|r| format!("\n- {}: {}", r.rel_type, r.target)).collect::<Vec<String>>().join("")
            },
            if body.is_empty() { String::new() } else { format!("\n{}\n", body) },
        );
        self.issue_cache.write(doc_id, &type_def.name, &cache_content);

        Ok(())
    }

    fn delete(
        &self,
        type_def: &TypeDef,
        doc_id: &str,
    ) -> Result<()> {
        let (issue_number, local_updated_at) = {
            let map = self.issue_map.borrow();
            let entry = map.get(doc_id)
                .ok_or_else(|| anyhow::anyhow!("{} not found in issue map", doc_id))?;
            (entry.issue_number, entry.updated_at.clone())
        };

        let remote_issue = self.client.issue_view(&self.repo, issue_number)?;

        if remote_issue.updated_at != local_updated_at {
            bail!(
                "{} has been modified on GitHub since your last fetch.\n  \
                 Local:  {}\n  \
                 Remote: {}\n\
                 Wait for background sync or restart the TUI to pull the latest version.",
                doc_id,
                local_updated_at,
                remote_issue.updated_at,
            );
        }

        let deleted_title = format!("[DELETED] {}", remote_issue.title);
        let label = gh::type_label(&type_def.name);
        self.client.issue_edit(
            &self.repo,
            issue_number,
            Some(&deleted_title),
            None,
            &[],
            &[label],
        )?;

        self.client.issue_close(&self.repo, issue_number)?;

        let mut map = self.issue_map.borrow_mut();
        map.remove(doc_id);
        map.save(&self.root)?;
        drop(map);

        self.issue_cache.remove(doc_id, &type_def.name);

        Ok(())
    }
}

pub fn write_cache_file(
    root: &std::path::Path,
    type_def: &TypeDef,
    meta: &DocMeta,
    body: &str,
) -> Result<()> {
    let cache_dir = root.join(".lazyspec/cache").join(&type_def.name);
    std::fs::create_dir_all(&cache_dir)?;
    let cache_path = find_cache_file(&cache_dir, &meta.id)
        .unwrap_or_else(|| cache_dir.join(format!("{}.md", meta.id)));

    let tags_str = if meta.tags.is_empty() {
        "[]".to_string()
    } else {
        format!("[{}]", meta.tags.iter().map(|t| format!("\"{}\"", t)).collect::<Vec<_>>().join(", "))
    };

    let related_str = if meta.related.is_empty() {
        "[]".to_string()
    } else {
        let lines: Vec<String> = meta.related.iter().map(|r| {
            format!("\n- {}: {}", r.rel_type, r.target)
        }).collect();
        lines.join("")
    };

    let cache_content = format!(
        "---\ntitle: \"{}\"\ntype: {}\nstatus: {}\nauthor: \"{}\"\ndate: {}\ntags: {}\nrelated: {}\n---\n{}",
        meta.title,
        meta.doc_type.as_str(),
        meta.status,
        meta.author,
        meta.date,
        tags_str,
        related_str,
        if body.is_empty() { String::new() } else { format!("\n{}\n", body) },
    );
    std::fs::write(&cache_path, &cache_content)?;
    Ok(())
}

fn find_cache_file(cache_dir: &std::path::Path, doc_id: &str) -> Option<PathBuf> {
    let prefix = format!("{}-", doc_id);
    let exact = format!("{}.md", doc_id);
    std::fs::read_dir(cache_dir).ok()?.find_map(|entry| {
        let entry = entry.ok()?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name == exact || name.starts_with(&prefix) {
            Some(entry.path())
        } else {
            None
        }
    })
}

pub fn dispatch_for_type<'a, G: GhIssueReader + GhIssueWriter>(
    type_def: &TypeDef,
    fs_store: &'a FilesystemStore,
    gh_store: Option<&'a GithubIssuesStore<G>>,
) -> Result<&'a dyn DocumentStore> {
    match type_def.store {
        StoreBackend::Filesystem => Ok(fs_store as &dyn DocumentStore),
        StoreBackend::GithubIssues => match gh_store {
            Some(s) => Ok(s as &dyn DocumentStore),
            None => bail!(
                "type '{}' uses github-issues store but no GitHub backend is configured",
                type_def.name
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{
        Config, NumberingStrategy, StoreBackend, TypeDef,
    };
    use crate::engine::gh::{GhIssueReader, GhIssueWriter, GhIssue, GhLabel};
    use crate::engine::issue_map::IssueMap;
    use std::path::PathBuf;

    fn test_type_def(store: StoreBackend) -> TypeDef {
        TypeDef {
            name: "rfc".to_string(),
            plural: "rfcs".to_string(),
            dir: "docs/rfcs".to_string(),
            prefix: "RFC".to_string(),
            icon: None,
            numbering: NumberingStrategy::Incremental,
            subdirectory: false,
            store,
        }
    }

    use std::cell::Cell;

    struct MockGhClient {
        view_issue: RefCell<Option<GhIssue>>,
        closed: Cell<bool>,
        reopened: Cell<bool>,
        last_edit_title: RefCell<Option<String>>,
        last_edit_body: RefCell<Option<String>>,
        last_edit_labels_remove: RefCell<Vec<String>>,
    }

    impl MockGhClient {
        fn new() -> Self {
            Self {
                view_issue: RefCell::new(None),
                closed: Cell::new(false),
                reopened: Cell::new(false),
                last_edit_title: RefCell::new(None),
                last_edit_body: RefCell::new(None),
                last_edit_labels_remove: RefCell::new(vec![]),
            }
        }

        fn with_view_issue(mut self, issue: GhIssue) -> Self {
            self.view_issue = RefCell::new(Some(issue));
            self
        }
    }

    impl GhIssueReader for MockGhClient {
        fn issue_list(
            &self,
            _repo: &str,
            _labels: &[String],
            _json_fields: &[String],
            _limit: Option<u64>,
        ) -> Result<Vec<GhIssue>> {
            Ok(vec![])
        }

        fn issue_view(&self, _repo: &str, number: u64) -> Result<GhIssue> {
            if let Some(issue) = self.view_issue.borrow().as_ref() {
                return Ok(issue.clone());
            }
            Ok(GhIssue {
                number,
                url: String::new(),
                title: String::new(),
                body: String::new(),
                labels: vec![],
                state: "OPEN".to_string(),
                updated_at: String::new(),
            })
        }
    }

    impl GhIssueWriter for MockGhClient {
        fn issue_create(
            &self,
            _repo: &str,
            title: &str,
            body: &str,
            labels: &[String],
        ) -> Result<GhIssue> {
            Ok(GhIssue {
                number: 1,
                url: "https://github.com/test/repo/issues/1".to_string(),
                title: title.to_string(),
                body: body.to_string(),
                labels: labels
                    .iter()
                    .map(|l| GhLabel {
                        name: l.clone(),
                        color: String::new(),
                    })
                    .collect(),
                state: "OPEN".to_string(),
                updated_at: "2026-03-27T00:00:00Z".to_string(),
            })
        }

        fn issue_edit(
            &self,
            _repo: &str,
            _number: u64,
            title: Option<&str>,
            body: Option<&str>,
            _labels_add: &[String],
            labels_remove: &[String],
        ) -> Result<()> {
            *self.last_edit_title.borrow_mut() = title.map(|s| s.to_string());
            *self.last_edit_body.borrow_mut() = body.map(|s| s.to_string());
            *self.last_edit_labels_remove.borrow_mut() = labels_remove.to_vec();
            Ok(())
        }

        fn issue_close(&self, _repo: &str, _number: u64) -> Result<()> {
            self.closed.set(true);
            Ok(())
        }

        fn issue_reopen(&self, _repo: &str, _number: u64) -> Result<()> {
            self.reopened.set(true);
            Ok(())
        }

        fn label_create(
            &self,
            _repo: &str,
            _name: &str,
            _description: &str,
            _color: &str,
        ) -> Result<()> {
            Ok(())
        }

        fn label_ensure(
            &self,
            _repo: &str,
            _name: &str,
            _description: &str,
            _color: &str,
        ) -> Result<()> {
            Ok(())
        }
    }

    fn tmp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lazyspec-store-dispatch-{}-{}",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn filesystem_create_produces_file() {
        let root = tmp_root("fs_create");
        let config = Config::default();

        let fs_store = FilesystemStore {
            root: root.clone(),
            config: config.clone(),
        };

        let td = test_type_def(StoreBackend::Filesystem);
        let result = fs_store.create(&td, "test doc", "author", "").unwrap();

        assert!(!result.id.is_empty());
        assert!(result.path.to_string_lossy().contains("RFC"));
        assert!(root.join(&result.path).exists());
    }

    #[test]
    fn filesystem_create_and_delete() {
        let root = tmp_root("fs_create_delete");
        let config = Config::default();

        let fs_store = FilesystemStore {
            root: root.clone(),
            config: config.clone(),
        };

        let td = test_type_def(StoreBackend::Filesystem);
        let created = fs_store.create(&td, "to delete", "author", "").unwrap();
        assert!(root.join(&created.path).exists());

        fs_store.delete(&td, &created.id).unwrap();
        assert!(!root.join(&created.path).exists());
    }

    #[test]
    fn filesystem_create_and_update() {
        let root = tmp_root("fs_create_update");
        let config = Config::default();

        let fs_store = FilesystemStore {
            root: root.clone(),
            config: config.clone(),
        };

        let td = test_type_def(StoreBackend::Filesystem);
        let created = fs_store.create(&td, "to update", "author", "").unwrap();

        fs_store
            .update(&td, &created.id, &[("status", "accepted")])
            .unwrap();

        let content = std::fs::read_to_string(root.join(&created.path)).unwrap();
        assert!(content.contains("status: accepted"));
    }

    #[test]
    fn github_issues_create_produces_cache_file() {
        let root = tmp_root("gh_create");
        let gh_store = GithubIssuesStore {
            client: MockGhClient::new(),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: RefCell::new(IssueMap::load(&root).unwrap()),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        let result = gh_store.create(&td, "my title", "author", "body text").unwrap();

        assert_eq!(result.id, "RFC-001");
        assert!(result.path.to_string_lossy().contains(".lazyspec/cache/rfc/"));
        assert!(root.join(&result.path).exists());

        let content = std::fs::read_to_string(root.join(&result.path)).unwrap();
        assert!(content.contains("title: \"my title\""));
        assert!(content.contains("type: rfc"));
        assert!(content.contains("status: draft"));
        assert!(content.contains("author: \"author\""));
        assert!(content.contains("body text"));
    }

    #[test]
    fn github_issues_create_updates_issue_map() {
        let root = tmp_root("gh_create_map");
        let gh_store = GithubIssuesStore {
            client: MockGhClient::new(),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: RefCell::new(IssueMap::load(&root).unwrap()),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store.create(&td, "mapped", "author", "").unwrap();

        let map = gh_store.issue_map.borrow();
        let entry = map.get("RFC-001").expect("issue map entry should exist");
        assert_eq!(entry.issue_number, 1);
        assert_eq!(entry.updated_at, "2026-03-27T00:00:00Z");
    }

    #[test]
    fn github_issues_create_persists_issue_map() {
        let root = tmp_root("gh_create_persist");
        let gh_store = GithubIssuesStore {
            client: MockGhClient::new(),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: RefCell::new(IssueMap::load(&root).unwrap()),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store.create(&td, "persist", "author", "").unwrap();

        let reloaded = IssueMap::load(&root).unwrap();
        assert!(reloaded.get("RFC-001").is_some());
    }

    #[test]
    fn github_issues_create_increments_id() {
        let root = tmp_root("gh_create_incr");
        let gh_store = GithubIssuesStore {
            client: MockGhClient::new(),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: RefCell::new(IssueMap::load(&root).unwrap()),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        let first = gh_store.create(&td, "first", "author", "").unwrap();
        let second = gh_store.create(&td, "second", "author", "").unwrap();

        assert_eq!(first.id, "RFC-001");
        assert_eq!(second.id, "RFC-002");
    }

    fn make_issue_body(author: &str, date: &str, status: Option<&str>, body: &str) -> String {
        let status_line = match status {
            Some(s) => format!("\nstatus: {}", s),
            None => String::new(),
        };
        let body_part = if body.is_empty() {
            String::new()
        } else {
            format!("\n\n{}", body)
        };
        format!(
            "<!-- lazyspec\n---\nauthor: {}\ndate: {}{}\n---\n-->{}",
            author, date, status_line, body_part
        )
    }

    #[test]
    fn github_issues_update_success() {
        let root = tmp_root("gh_update_ok");
        let issue_body = make_issue_body("agent-7", "2026-03-27", None, "original body");
        let view_issue = GhIssue {
            number: 42,
            url: String::new(),
            title: "My RFC".to_string(),
            body: issue_body,
            labels: vec![GhLabel { name: "lazyspec:rfc".to_string(), color: String::new() }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z");

        let gh_store = GithubIssuesStore {
            client,
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: RefCell::new(map),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store
            .update(&td, "RFC-001", &[("status", "accepted")])
            .unwrap();
    }

    #[test]
    fn github_issues_update_optimistic_lock_failure() {
        let root = tmp_root("gh_update_lock");
        let issue_body = make_issue_body("agent-7", "2026-03-27", None, "");
        let view_issue = GhIssue {
            number: 42,
            url: String::new(),
            title: "My RFC".to_string(),
            body: issue_body,
            labels: vec![GhLabel { name: "lazyspec:rfc".to_string(), color: String::new() }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:45:00Z".to_string(),
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z");

        let gh_store = GithubIssuesStore {
            client,
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: RefCell::new(map),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        let err = gh_store
            .update(&td, "RFC-001", &[("status", "accepted")])
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("has been modified on GitHub"), "got: {}", msg);
        assert!(msg.contains("2026-03-27T10:00:00Z"));
        assert!(msg.contains("2026-03-27T10:45:00Z"));
        assert!(msg.contains("background sync"));
    }

    #[test]
    fn github_issues_update_status_complete_closes_issue() {
        let root = tmp_root("gh_update_close");
        let issue_body = make_issue_body("agent-7", "2026-03-27", None, "");
        let view_issue = GhIssue {
            number: 42,
            url: String::new(),
            title: "My RFC".to_string(),
            body: issue_body,
            labels: vec![GhLabel { name: "lazyspec:rfc".to_string(), color: String::new() }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z");

        let gh_store = GithubIssuesStore {
            client,
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: RefCell::new(map),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store
            .update(&td, "RFC-001", &[("status", "complete")])
            .unwrap();
        assert!(gh_store.client.closed.get());
        assert!(!gh_store.client.reopened.get());
    }

    #[test]
    fn github_issues_update_status_draft_reopens_issue() {
        let root = tmp_root("gh_update_reopen");
        let issue_body = make_issue_body("agent-7", "2026-03-27", None, "");
        let view_issue = GhIssue {
            number: 42,
            url: String::new(),
            title: "My RFC".to_string(),
            body: issue_body,
            labels: vec![GhLabel { name: "lazyspec:rfc".to_string(), color: String::new() }],
            state: "CLOSED".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z");

        let gh_store = GithubIssuesStore {
            client,
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: RefCell::new(map),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store
            .update(&td, "RFC-001", &[("status", "draft")])
            .unwrap();
        assert!(gh_store.client.reopened.get());
        assert!(!gh_store.client.closed.get());
    }

    #[test]
    fn github_issues_update_not_in_map() {
        let root = tmp_root("gh_update_nomap");
        let gh_store = GithubIssuesStore {
            client: MockGhClient::new(),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: RefCell::new(IssueMap::load(&root).unwrap()),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        let err = gh_store
            .update(&td, "RFC-999", &[("status", "accepted")])
            .unwrap_err();
        assert!(err.to_string().contains("not found in issue map"));
    }

    #[test]
    fn github_issues_delete_success() {
        let root = tmp_root("gh_delete_ok");
        let issue_body = make_issue_body("agent-7", "2026-03-27", None, "some content");
        let view_issue = GhIssue {
            number: 42,
            url: String::new(),
            title: "My RFC".to_string(),
            body: issue_body,
            labels: vec![GhLabel { name: "lazyspec:rfc".to_string(), color: String::new() }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z");

        let gh_store = GithubIssuesStore {
            client,
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: RefCell::new(map),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store.delete(&td, "RFC-001").unwrap();

        assert!(gh_store.client.closed.get());
        let title = gh_store.client.last_edit_title.borrow();
        assert_eq!(title.as_deref(), Some("[DELETED] My RFC"));
        let labels_remove = gh_store.client.last_edit_labels_remove.borrow();
        assert_eq!(*labels_remove, vec!["lazyspec:rfc".to_string()]);
        assert!(gh_store.issue_map.borrow().get("RFC-001").is_none());
    }

    #[test]
    fn github_issues_delete_optimistic_lock_failure() {
        let root = tmp_root("gh_delete_lock");
        let view_issue = GhIssue {
            number: 42,
            url: String::new(),
            title: "My RFC".to_string(),
            body: String::new(),
            labels: vec![],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:45:00Z".to_string(),
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z");

        let gh_store = GithubIssuesStore {
            client,
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: RefCell::new(map),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        let err = gh_store.delete(&td, "RFC-001").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("has been modified on GitHub"), "got: {}", msg);
        assert!(!gh_store.client.closed.get());
    }

    #[test]
    fn github_issues_delete_not_in_map() {
        let root = tmp_root("gh_delete_nomap");
        let gh_store = GithubIssuesStore {
            client: MockGhClient::new(),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: RefCell::new(IssueMap::load(&root).unwrap()),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        let err = gh_store.delete(&td, "RFC-999").unwrap_err();
        assert!(err.to_string().contains("not found in issue map"));
    }

    #[test]
    fn github_issues_delete_removes_cache_file() {
        let root = tmp_root("gh_delete_cache");
        let view_issue = GhIssue {
            number: 42,
            url: String::new(),
            title: "My RFC".to_string(),
            body: String::new(),
            labels: vec![],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z");

        let cache_dir = root.join(".lazyspec/cache/rfc");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let cache_file = cache_dir.join("RFC-001.md");
        std::fs::write(&cache_file, "cached content").unwrap();
        assert!(cache_file.exists());

        let gh_store = GithubIssuesStore {
            client,
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: RefCell::new(map),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store.delete(&td, "RFC-001").unwrap();
        assert!(!cache_file.exists());
    }

    #[test]
    fn dispatch_routes_to_filesystem() {
        let root = tmp_root("dispatch_fs");
        let config = Config::default();

        let fs_store = FilesystemStore {
            root: root.clone(),
            config,
        };

        let td = test_type_def(StoreBackend::Filesystem);
        let store = dispatch_for_type::<MockGhClient>(&td, &fs_store, None).unwrap();

        // Should succeed (routed to filesystem)
        let result = store.create(&td, "dispatched", "author", "");
        assert!(result.is_ok());
    }

    #[test]
    fn dispatch_routes_to_github() {
        let root = tmp_root("dispatch_gh");
        let config = Config::default();

        let fs_store = FilesystemStore {
            root: root.clone(),
            config,
        };

        let gh_store = GithubIssuesStore {
            client: MockGhClient::new(),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: RefCell::new(IssueMap::load(&root).unwrap()),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        let store = dispatch_for_type(&td, &fs_store, Some(&gh_store)).unwrap();

        let result = store.create(&td, "dispatched", "author", "");
        assert!(result.is_ok());
    }

    #[test]
    fn github_issues_update_body_success() {
        let root = tmp_root("gh_update_body");
        let issue_body = make_issue_body("agent-7", "2026-03-27", None, "original body");
        let view_issue = GhIssue {
            number: 42,
            url: String::new(),
            title: "My RFC".to_string(),
            body: issue_body,
            labels: vec![GhLabel { name: "lazyspec:rfc".to_string(), color: String::new() }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z");

        let gh_store = GithubIssuesStore {
            client,
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: RefCell::new(map),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store
            .update(&td, "RFC-001", &[("body", "new content")])
            .unwrap();

        let captured = gh_store.client.last_edit_body.borrow();
        let body_str = captured.as_deref().expect("issue_edit should have been called with body");
        assert!(body_str.contains("new content"), "body should contain 'new content', got: {}", body_str);
        assert!(body_str.contains("<!-- lazyspec"), "body should be wrapped in issue_body format");
    }

    #[test]
    fn github_issues_update_body_with_status() {
        let root = tmp_root("gh_update_body_status");
        let issue_body = make_issue_body("agent-7", "2026-03-27", None, "old body");
        let view_issue = GhIssue {
            number: 42,
            url: String::new(),
            title: "My RFC".to_string(),
            body: issue_body,
            labels: vec![GhLabel { name: "lazyspec:rfc".to_string(), color: String::new() }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z");

        let gh_store = GithubIssuesStore {
            client,
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: RefCell::new(map),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store
            .update(&td, "RFC-001", &[("body", "new"), ("status", "complete")])
            .unwrap();

        let captured = gh_store.client.last_edit_body.borrow();
        let body_str = captured.as_deref().expect("issue_edit should have been called with body");
        assert!(body_str.contains("new"), "body should contain updated text");
        assert!(gh_store.client.closed.get(), "issue should be closed for status=complete");
    }

    #[test]
    fn github_issues_update_body_optimistic_lock_failure() {
        let root = tmp_root("gh_update_body_lock");
        let issue_body = make_issue_body("agent-7", "2026-03-27", None, "some body");
        let view_issue = GhIssue {
            number: 42,
            url: String::new(),
            title: "My RFC".to_string(),
            body: issue_body,
            labels: vec![GhLabel { name: "lazyspec:rfc".to_string(), color: String::new() }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:45:00Z".to_string(),
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z");

        let gh_store = GithubIssuesStore {
            client,
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: RefCell::new(map),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        let err = gh_store
            .update(&td, "RFC-001", &[("body", "new content")])
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("modified on GitHub"), "got: {}", msg);
    }

    #[test]
    fn filesystem_update_rejects_body() {
        let root = tmp_root("fs_update_body");
        let config = Config::default();

        let fs_store = FilesystemStore {
            root: root.clone(),
            config: config.clone(),
        };

        let td = test_type_def(StoreBackend::Filesystem);
        let created = fs_store.create(&td, "test doc", "author", "").unwrap();

        let err = fs_store
            .update(&td, &created.id, &[("body", "content")])
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not supported for filesystem documents"), "got: {}", msg);
    }

    #[test]
    fn dispatch_github_without_backend_errors() {
        let root = tmp_root("dispatch_no_gh");
        let config = Config::default();

        let fs_store = FilesystemStore {
            root: root.clone(),
            config,
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        let result = dispatch_for_type::<MockGhClient>(&td, &fs_store, None);
        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("no GitHub backend"));
    }
}
