use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use chrono::Local;

use crate::engine::cache_lock::CacheLock;
use crate::engine::config::{Config, TypeDef};
use crate::engine::document::{compose_frontmatter, split_frontmatter, DocMeta, DocType, Status};
use crate::engine::git_ref::GitRefOps;
use crate::engine::store_dispatch::{find_cache_file, write_cache_file, CreatedDoc, DocumentStore};

fn ensure_cache_gitignored(root: &Path) -> Result<()> {
    let gitignore_path = root.join(".lazyspec/.gitignore");
    if let Ok(contents) = std::fs::read_to_string(&gitignore_path) {
        if !contents.lines().any(|line| line.trim() == "cache/") {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&gitignore_path)?;
            if !contents.ends_with('\n') && !contents.is_empty() {
                writeln!(file)?;
            }
            writeln!(file, "cache/")?;
        }
    } else {
        std::fs::create_dir_all(root.join(".lazyspec"))?;
        std::fs::write(&gitignore_path, "cache/\n")?;
    }
    Ok(())
}

pub struct GitRefStore<R: GitRefOps> {
    pub git: R,
    pub root: PathBuf,
    pub config: Config,
    pub reserved_number: Option<u32>,
}

impl<R: GitRefOps> GitRefStore<R> {
    fn ref_prefix(type_name: &str) -> String {
        format!("refs/lazyspec/{}/", type_name)
    }

    fn refname(type_name: &str, id: &str) -> String {
        format!("refs/lazyspec/{}/{}", type_name, id)
    }

    fn doc_key(type_name: &str, id: &str) -> String {
        format!("{}/{}", type_name, id)
    }

    fn next_number_from_refs(&self, type_def: &TypeDef) -> Result<u32> {
        let pattern = Self::ref_prefix(&type_def.name);
        let refs = self.git.list_refs(&self.root, &pattern)?;
        let mut max = 0u32;
        let prefix = format!("{}-", type_def.prefix);
        for (refname, _sha) in &refs {
            if let Some(id_part) = refname.rsplit('/').next() {
                if let Some(rest) = id_part.strip_prefix(&prefix) {
                    let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if let Ok(n) = num_str.parse::<u32>() {
                        max = max.max(n);
                    }
                }
            }
        }
        Ok(max + 1)
    }

    fn build_markdown(
        type_def: &TypeDef,
        title: &str,
        author: &str,
        date: &str,
        status: &str,
        body: &str,
    ) -> String {
        let body_section = if body.is_empty() {
            String::new()
        } else {
            format!("\n{}\n", body)
        };
        format!(
            "---\ntitle: \"{}\"\ntype: {}\nstatus: {}\nauthor: \"{}\"\ndate: {}\ntags: []\nrelated: []\n---\n{}",
            title, type_def.name, status, author, date, body_section
        )
    }
}

impl<R: GitRefOps> DocumentStore for GitRefStore<R> {
    fn create(
        &mut self,
        type_def: &TypeDef,
        title: &str,
        author: &str,
        body: &str,
    ) -> Result<CreatedDoc> {
        ensure_cache_gitignored(&self.root)?;

        let next_num = match self.reserved_number {
            Some(n) => n,
            None => self.next_number_from_refs(type_def)?,
        };
        let id = format!("{}-{:03}", type_def.prefix, next_num);
        let date = Local::now().format("%Y-%m-%d").to_string();

        let content = Self::build_markdown(type_def, title, author, &date, "draft", body);

        let refname = Self::refname(&type_def.name, &id);
        let sha = self
            .git
            .create_ref_commit(&self.root, &refname, &[("doc.md", &content)])?;

        if let Some(coord) = &self.config.coordination {
            self.git.push_ref(&self.root, &coord.remote, &refname)?;
        }

        let meta = DocMeta {
            path: PathBuf::new(),
            title: title.to_string(),
            doc_type: DocType::new(&type_def.name),
            status: Status::Draft,
            author: author.to_string(),
            date: Local::now().date_naive(),
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            id: id.clone(),
        };

        write_cache_file(&self.root, type_def, &meta, body)?;

        let mut lock = CacheLock::load(&self.root)?;
        lock.set(&Self::doc_key(&type_def.name, &id), &sha);
        lock.save(&self.root)?;

        let cache_path = self
            .root
            .join(".lazyspec/cache")
            .join(&type_def.name)
            .join(format!("{}.md", id));
        let relative = cache_path
            .strip_prefix(&self.root)
            .unwrap_or(&cache_path)
            .to_path_buf();

        Ok(CreatedDoc { path: relative, id })
    }

    fn update(&mut self, type_def: &TypeDef, doc_id: &str, updates: &[(&str, &str)]) -> Result<()> {
        let doc_key = Self::doc_key(&type_def.name, doc_id);
        let lock = CacheLock::load(&self.root)?;
        let old_sha = lock
            .get(&doc_key)
            .ok_or_else(|| anyhow::anyhow!("{} not found in cache.lock", doc_id))?
            .to_string();

        let cache_dir = self.root.join(".lazyspec/cache").join(&type_def.name);
        let cache_path = find_cache_file(&cache_dir, doc_id)
            .ok_or_else(|| anyhow::anyhow!("cache file not found for {}", doc_id))?;
        let content = std::fs::read_to_string(&cache_path)?;

        let (yaml, existing_body) = split_frontmatter(&content)?;

        let mut lines: Vec<String> = yaml.lines().map(|l| l.to_string()).collect();
        let mut new_body: Option<String> = None;
        for &(key, value) in updates {
            if key == "body" {
                new_body = Some(value.to_string());
                continue;
            }
            let prefix = format!("{}:", key);
            if let Some(line) = lines
                .iter_mut()
                .find(|l| l.trim_start().starts_with(&prefix))
            {
                *line = format!("{}: {}", key, value);
            }
        }

        let updated_yaml = lines.join("\n");
        let body = new_body.as_deref().unwrap_or(&existing_body);
        let body_trimmed = body.trim_start_matches('\n');
        let body_section = if body_trimmed.is_empty() {
            String::new()
        } else {
            format!("\n{}\n", body_trimmed)
        };
        let updated_content = compose_frontmatter(&updated_yaml, &body_section);

        let refname = Self::refname(&type_def.name, doc_id);
        let new_sha = self.git.create_commit(
            &self.root,
            &refname,
            &[("doc.md", &updated_content)],
            Some(&old_sha),
        )?;

        if let Err(e) = self
            .git
            .update_ref(&self.root, &refname, &new_sha, &old_sha)
        {
            bail!("conflict updating {}: {}", doc_id, e);
        }

        if let Some(coord) = &self.config.coordination {
            self.git.push_ref(&self.root, &coord.remote, &refname)?;
        }

        std::fs::write(&cache_path, &updated_content)?;

        let mut lock = CacheLock::load(&self.root)?;
        lock.set(&doc_key, &new_sha);
        lock.save(&self.root)?;

        Ok(())
    }

    fn set_provenance(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        provenance: &[String],
    ) -> Result<()> {
        let doc_key = Self::doc_key(&type_def.name, doc_id);
        let lock = CacheLock::load(&self.root)?;
        let old_sha = lock
            .get(&doc_key)
            .ok_or_else(|| anyhow::anyhow!("{} not found in cache.lock", doc_id))?
            .to_string();

        let cache_dir = self.root.join(".lazyspec/cache").join(&type_def.name);
        let cache_path = find_cache_file(&cache_dir, doc_id)
            .ok_or_else(|| anyhow::anyhow!("cache file not found for {}", doc_id))?;
        let content = std::fs::read_to_string(&cache_path)?;

        let (yaml, existing_body) = split_frontmatter(&content)?;
        let mut value: serde_yaml::Value = serde_yaml::from_str(&yaml)?;
        let entries: Vec<serde_yaml::Value> = provenance
            .iter()
            .map(|s| serde_yaml::Value::String(s.clone()))
            .collect();
        let map = value
            .as_mapping_mut()
            .ok_or_else(|| anyhow::anyhow!("frontmatter root must be a mapping"))?;
        map.insert(
            serde_yaml::Value::String("provenance".to_string()),
            serde_yaml::Value::Sequence(entries),
        );
        let new_yaml = serde_yaml::to_string(&value)?;

        let body_trimmed = existing_body.trim_start_matches('\n');
        let body_section = if body_trimmed.is_empty() {
            String::new()
        } else {
            format!("\n{}\n", body_trimmed)
        };
        let updated_content = compose_frontmatter(&new_yaml, &body_section);

        let refname = Self::refname(&type_def.name, doc_id);
        let new_sha = self.git.create_commit(
            &self.root,
            &refname,
            &[("doc.md", &updated_content)],
            Some(&old_sha),
        )?;

        if let Err(e) = self
            .git
            .update_ref(&self.root, &refname, &new_sha, &old_sha)
        {
            bail!("conflict updating {}: {}", doc_id, e);
        }

        if let Some(coord) = &self.config.coordination {
            self.git.push_ref(&self.root, &coord.remote, &refname)?;
        }

        std::fs::write(&cache_path, &updated_content)?;

        let mut lock = CacheLock::load(&self.root)?;
        lock.set(&doc_key, &new_sha);
        lock.save(&self.root)?;

        Ok(())
    }

    fn delete(&mut self, type_def: &TypeDef, doc_id: &str) -> Result<()> {
        let refname = Self::refname(&type_def.name, doc_id);
        if let Some(coord) = &self.config.coordination {
            self.git
                .delete_remote_ref(&self.root, &coord.remote, &refname)?;
        }
        self.git.delete_ref(&self.root, &refname)?;

        let cache_dir = self.root.join(".lazyspec/cache").join(&type_def.name);
        if let Some(cache_path) = find_cache_file(&cache_dir, doc_id) {
            std::fs::remove_file(&cache_path)?;
        }

        let mut lock = CacheLock::load(&self.root)?;
        lock.remove(&Self::doc_key(&type_def.name, doc_id));
        lock.save(&self.root)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{
        Config, CoordinationConfig, Directories, DocumentConfig, FilesystemConfig, Naming,
        NumberingStrategy, StoreBackend, Templates, TypeDef, UiConfig,
    };
    use crate::engine::git_ref::test_support::MockGitRefClient;
    use tempfile::TempDir;

    fn test_type_def() -> TypeDef {
        TypeDef {
            name: "iteration".to_string(),
            plural: "iterations".to_string(),
            dir: "docs/iterations".to_string(),
            prefix: "ITERATION".to_string(),
            icon: None,
            numbering: NumberingStrategy::Incremental,
            subdirectory: false,
            store: StoreBackend::GitRef,
            singleton: false,
            parent_type: None,
        }
    }

    fn test_config() -> Config {
        Config {
            documents: DocumentConfig {
                types: vec![test_type_def()],
                naming: Naming {
                    pattern: "{type}-{n:03}-{title}.md".to_string(),
                },
                sqids: None,
                reserved: None,
                github: None,
            },
            filesystem: FilesystemConfig {
                directories: Directories {
                    rfcs: "docs/rfcs".to_string(),
                    adrs: "docs/adrs".to_string(),
                    stories: "docs/stories".to_string(),
                    iterations: "docs/iterations".to_string(),
                },
                templates: Templates {
                    dir: ".lazyspec/templates".to_string(),
                },
            },
            ui: UiConfig::default(),
            rules: vec![],
            ref_count_ceiling: 0,
            certification: Default::default(),
            coordination: None,
        }
    }

    fn make_store(tmp: &TempDir, mock: MockGitRefClient) -> GitRefStore<MockGitRefClient> {
        GitRefStore {
            git: mock,
            root: tmp.path().to_path_buf(),
            config: test_config(),
            reserved_number: None,
        }
    }

    #[test]
    fn test_git_ref_store_create() {
        let tmp = TempDir::new().unwrap();
        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![]))
            .with_create_ref_commit_result(Ok("abc123sha".to_string()));

        let mut store = make_store(&tmp, mock);
        let td = test_type_def();
        let result = store
            .create(&td, "My Feature", "alice", "some body")
            .unwrap();

        assert_eq!(result.id, "ITERATION-001");
        assert!(result.path.to_string_lossy().contains("ITERATION-001"));

        let cache_dir = tmp.path().join(".lazyspec/cache/iteration");
        let cache_file = find_cache_file(&cache_dir, "ITERATION-001");
        assert!(cache_file.is_some(), "cache file should exist");

        let content = std::fs::read_to_string(cache_file.unwrap()).unwrap();
        assert!(content.contains("title: My Feature"));
        assert!(content.contains("status: draft"));
        assert!(content.contains("author: alice"));
        assert!(content.contains("some body"));

        let lock = CacheLock::load(tmp.path()).unwrap();
        assert_eq!(lock.get("iteration/ITERATION-001"), Some("abc123sha"));

        let calls = store.git.calls.borrow();
        assert!(calls.iter().any(|c| c.starts_with("list_refs:")));
        assert!(calls
            .iter()
            .any(|c| c.contains("create_ref_commit:refs/lazyspec/iteration/ITERATION-001")));
    }

    #[test]
    fn test_git_ref_store_create_increments_from_existing() {
        let tmp = TempDir::new().unwrap();
        let existing_refs = vec![
            (
                "refs/lazyspec/iteration/ITERATION-001".to_string(),
                "sha1".to_string(),
            ),
            (
                "refs/lazyspec/iteration/ITERATION-005".to_string(),
                "sha5".to_string(),
            ),
        ];
        let mock = MockGitRefClient::new()
            .with_list_result(Ok(existing_refs))
            .with_create_ref_commit_result(Ok("newsha".to_string()));

        let mut store = make_store(&tmp, mock);
        let td = test_type_def();
        let result = store.create(&td, "Next Thing", "bob", "").unwrap();

        assert_eq!(result.id, "ITERATION-006");
    }

    #[test]
    fn test_git_ref_store_update() {
        let tmp = TempDir::new().unwrap();

        let td = test_type_def();
        let cache_dir = tmp.path().join(".lazyspec/cache/iteration");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let cache_content = "---\ntitle: Old Title\ntype: iteration\nstatus: draft\nauthor: alice\ndate: 2026-04-01\ntags: []\nrelated: []\n---\n\noriginal body\n";
        std::fs::write(cache_dir.join("ITERATION-042.md"), cache_content).unwrap();

        let mut lock = CacheLock::default();
        lock.set("iteration/ITERATION-042", "oldsha");
        lock.save(tmp.path()).unwrap();

        let mock = MockGitRefClient::new()
            .with_create_commit_result(Ok("newsha456".to_string()))
            .with_update_ref_result(Ok(()));

        let mut store = make_store(&tmp, mock);
        store
            .update(&td, "ITERATION-042", &[("status", "accepted")])
            .unwrap();

        let updated = std::fs::read_to_string(cache_dir.join("ITERATION-042.md")).unwrap();
        assert!(
            updated.contains("status: accepted"),
            "status should be updated, got: {}",
            updated
        );
        assert!(updated.contains("title: Old Title"));
        assert!(updated.contains("original body"));

        let lock = CacheLock::load(tmp.path()).unwrap();
        assert_eq!(lock.get("iteration/ITERATION-042"), Some("newsha456"));

        let calls = store.git.calls.borrow();
        let create_call = calls
            .iter()
            .find(|c| c.starts_with("create_commit:"))
            .expect("should call create_commit (not create_ref_commit)");
        assert!(
            create_call.contains("parent=Some(\"oldsha\")"),
            "create_commit should be parented on old SHA, got: {}",
            create_call
        );
        assert!(
            !calls.iter().any(|c| c.starts_with("create_ref_commit:")),
            "should NOT call create_ref_commit, got: {:?}",
            *calls
        );
        let update_call = calls.iter().find(|c| c.starts_with("update_ref:")).unwrap();
        assert!(update_call.contains("newsha456"), "new SHA in update_ref");
        assert!(update_call.contains("oldsha"), "old SHA in update_ref");
    }

    #[test]
    fn test_git_ref_store_update_cas_conflict() {
        let tmp = TempDir::new().unwrap();

        let td = test_type_def();
        let cache_dir = tmp.path().join(".lazyspec/cache/iteration");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let cache_content = "---\ntitle: Title\ntype: iteration\nstatus: draft\nauthor: alice\ndate: 2026-04-01\ntags: []\nrelated: []\n---\n\nbody\n";
        let cache_path = cache_dir.join("ITERATION-042.md");
        std::fs::write(&cache_path, cache_content).unwrap();

        let mut lock = CacheLock::default();
        lock.set("iteration/ITERATION-042", "oldsha");
        lock.save(tmp.path()).unwrap();

        let mock = MockGitRefClient::new()
            .with_create_commit_result(Ok("newsha".to_string()))
            .with_update_ref_result(Err(anyhow::anyhow!("CAS mismatch")));

        let mut store = make_store(&tmp, mock);
        let result = store.update(&td, "ITERATION-042", &[("status", "accepted")]);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("conflict"));

        let unchanged = std::fs::read_to_string(&cache_path).unwrap();
        assert!(
            unchanged.contains("status: draft"),
            "cache should be unchanged on CAS failure"
        );

        let lock = CacheLock::load(tmp.path()).unwrap();
        assert_eq!(
            lock.get("iteration/ITERATION-042"),
            Some("oldsha"),
            "lock should be unchanged on CAS failure"
        );
    }

    #[test]
    fn test_git_ref_store_delete() {
        let tmp = TempDir::new().unwrap();

        let td = test_type_def();
        let cache_dir = tmp.path().join(".lazyspec/cache/iteration");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let cache_path = cache_dir.join("ITERATION-042.md");
        std::fs::write(&cache_path, "---\ntitle: T\ntype: iteration\nstatus: draft\nauthor: a\ndate: 2026-04-01\ntags: []\nrelated: []\n---\n").unwrap();

        let mut lock = CacheLock::default();
        lock.set("iteration/ITERATION-042", "somesha");
        lock.save(tmp.path()).unwrap();

        let mock = MockGitRefClient::new().with_delete_ref_result(Ok(()));

        let mut store = make_store(&tmp, mock);
        store.delete(&td, "ITERATION-042").unwrap();

        assert!(!cache_path.exists(), "cache file should be removed");

        let lock = CacheLock::load(tmp.path()).unwrap();
        assert!(
            lock.get("iteration/ITERATION-042").is_none(),
            "lock entry should be removed"
        );

        let calls = store.git.calls.borrow();
        assert!(calls
            .iter()
            .any(|c| c == "delete_ref:refs/lazyspec/iteration/ITERATION-042"));
    }

    #[test]
    fn test_gitignore_includes_cache() {
        let tmp = TempDir::new().unwrap();
        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![]))
            .with_create_ref_commit_result(Ok("abc123sha".to_string()));

        let mut store = make_store(&tmp, mock);
        let td = test_type_def();
        store.create(&td, "Title", "alice", "").unwrap();

        let gitignore_path = tmp.path().join(".lazyspec/.gitignore");
        assert!(gitignore_path.exists(), ".lazyspec/.gitignore should exist");
        let contents = std::fs::read_to_string(&gitignore_path).unwrap();
        assert!(
            contents.lines().any(|l| l.trim() == "cache/"),
            ".gitignore should contain cache/, got: {}",
            contents
        );
    }

    #[test]
    fn test_gitignore_idempotent() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".lazyspec")).unwrap();
        std::fs::write(tmp.path().join(".lazyspec/.gitignore"), "cache/\n").unwrap();

        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![]))
            .with_create_ref_commit_result(Ok("sha".to_string()));

        let mut store = make_store(&tmp, mock);
        let td = test_type_def();
        store.create(&td, "Title", "alice", "").unwrap();

        let contents = std::fs::read_to_string(tmp.path().join(".lazyspec/.gitignore")).unwrap();
        assert_eq!(
            contents.matches("cache/").count(),
            1,
            "cache/ should appear exactly once, got: {}",
            contents
        );
    }

    #[test]
    fn test_gitignore_appends_to_existing() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".lazyspec")).unwrap();
        std::fs::write(tmp.path().join(".lazyspec/.gitignore"), "*.tmp\n").unwrap();

        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![]))
            .with_create_ref_commit_result(Ok("sha".to_string()));

        let mut store = make_store(&tmp, mock);
        let td = test_type_def();
        store.create(&td, "Title", "alice", "").unwrap();

        let contents = std::fs::read_to_string(tmp.path().join(".lazyspec/.gitignore")).unwrap();
        assert!(
            contents.contains("*.tmp"),
            "should preserve existing entries"
        );
        assert!(
            contents.lines().any(|l| l.trim() == "cache/"),
            "should contain cache/"
        );
    }

    #[test]
    fn create_uses_reserved_number_when_set() {
        let tmp = TempDir::new().unwrap();
        let mock =
            MockGitRefClient::new().with_create_ref_commit_result(Ok("sha_reserved".to_string()));

        let mut store = GitRefStore {
            git: mock,
            root: tmp.path().to_path_buf(),
            config: test_config(),
            reserved_number: Some(42),
        };
        let td = test_type_def();
        let result = store.create(&td, "Reserved Title", "alice", "").unwrap();

        assert_eq!(result.id, "ITERATION-042");

        let calls = store.git.calls.borrow();
        assert!(
            !calls.iter().any(|c| c.starts_with("list_refs:")),
            "should not call list_refs when reserved_number is set, got: {:?}",
            *calls
        );
        assert!(calls
            .iter()
            .any(|c| c.contains("create_ref_commit:refs/lazyspec/iteration/ITERATION-042")));
    }

    #[test]
    fn create_falls_back_to_next_number_from_refs_when_no_reservation() {
        let tmp = TempDir::new().unwrap();
        let existing_refs = vec![(
            "refs/lazyspec/iteration/ITERATION-003".to_string(),
            "sha3".to_string(),
        )];
        let mock = MockGitRefClient::new()
            .with_list_result(Ok(existing_refs))
            .with_create_ref_commit_result(Ok("sha_fallback".to_string()));

        let mut store = GitRefStore {
            git: mock,
            root: tmp.path().to_path_buf(),
            config: test_config(),
            reserved_number: None,
        };
        let td = test_type_def();
        let result = store.create(&td, "Fallback Title", "bob", "").unwrap();

        assert_eq!(result.id, "ITERATION-004");

        let calls = store.git.calls.borrow();
        assert!(
            calls.iter().any(|c| c.starts_with("list_refs:")),
            "should call list_refs when no reserved_number"
        );
    }

    fn test_config_with_coordination() -> Config {
        let mut config = test_config();
        config.coordination = Some(CoordinationConfig {
            remote: "origin".to_string(),
            lease_duration: "60m".to_string(),
            grace_period: "2m".to_string(),
            max_push_retries: 5,
        });
        config
    }

    #[test]
    fn create_pushes_ref_when_coordination_configured() {
        let tmp = TempDir::new().unwrap();
        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![]))
            .with_create_ref_commit_result(Ok("abc123sha".to_string()))
            .with_push_result(Ok(()));

        let mut store = GitRefStore {
            git: mock,
            root: tmp.path().to_path_buf(),
            config: test_config_with_coordination(),
            reserved_number: None,
        };
        let td = test_type_def();
        store.create(&td, "Pushed Doc", "alice", "").unwrap();

        let calls = store.git.calls.borrow();
        assert!(
            calls
                .iter()
                .any(|c| c == "push_ref:origin:refs/lazyspec/iteration/ITERATION-001"),
            "should push doc ref to remote, got: {:?}",
            *calls
        );
    }

    #[test]
    fn create_does_not_push_without_coordination() {
        let tmp = TempDir::new().unwrap();
        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![]))
            .with_create_ref_commit_result(Ok("abc123sha".to_string()));

        let mut store = make_store(&tmp, mock);
        let td = test_type_def();
        store.create(&td, "Local Doc", "alice", "").unwrap();

        let calls = store.git.calls.borrow();
        assert!(
            !calls.iter().any(|c| c.starts_with("push_ref:")),
            "should not push without coordination, got: {:?}",
            *calls
        );
    }

    #[test]
    fn update_pushes_ref_when_coordination_configured() {
        let tmp = TempDir::new().unwrap();
        let td = test_type_def();
        let cache_dir = tmp.path().join(".lazyspec/cache/iteration");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(
            cache_dir.join("ITERATION-042.md"),
            "---\ntitle: Old Title\ntype: iteration\nstatus: draft\nauthor: alice\ndate: 2026-04-01\ntags: []\nrelated: []\n---\n\noriginal body\n",
        ).unwrap();

        let mut lock = CacheLock::default();
        lock.set("iteration/ITERATION-042", "oldsha");
        lock.save(tmp.path()).unwrap();

        let mock = MockGitRefClient::new()
            .with_create_commit_result(Ok("newsha456".to_string()))
            .with_update_ref_result(Ok(()))
            .with_push_result(Ok(()));

        let mut store = GitRefStore {
            git: mock,
            root: tmp.path().to_path_buf(),
            config: test_config_with_coordination(),
            reserved_number: None,
        };
        store
            .update(&td, "ITERATION-042", &[("status", "accepted")])
            .unwrap();

        let calls = store.git.calls.borrow();
        assert!(
            calls
                .iter()
                .any(|c| c == "push_ref:origin:refs/lazyspec/iteration/ITERATION-042"),
            "should push updated ref to remote, got: {:?}",
            *calls
        );
    }

    #[test]
    fn delete_removes_remote_ref_when_coordination_configured() {
        let tmp = TempDir::new().unwrap();
        let td = test_type_def();
        let cache_dir = tmp.path().join(".lazyspec/cache/iteration");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(
            cache_dir.join("ITERATION-042.md"),
            "---\ntitle: T\ntype: iteration\nstatus: draft\nauthor: a\ndate: 2026-04-01\ntags: []\nrelated: []\n---\n",
        ).unwrap();

        let mut lock = CacheLock::default();
        lock.set("iteration/ITERATION-042", "somesha");
        lock.save(tmp.path()).unwrap();

        let mock = MockGitRefClient::new()
            .with_delete_remote_result(Ok(()))
            .with_delete_ref_result(Ok(()));

        let mut store = GitRefStore {
            git: mock,
            root: tmp.path().to_path_buf(),
            config: test_config_with_coordination(),
            reserved_number: None,
        };
        store.delete(&td, "ITERATION-042").unwrap();

        let calls = store.git.calls.borrow();
        assert!(
            calls
                .iter()
                .any(|c| c == "delete_remote_ref:origin:refs/lazyspec/iteration/ITERATION-042"),
            "should delete remote ref, got: {:?}",
            *calls
        );
        let delete_remote_idx = calls
            .iter()
            .position(|c| c.starts_with("delete_remote_ref:"))
            .unwrap();
        let delete_local_idx = calls
            .iter()
            .position(|c| c.starts_with("delete_ref:"))
            .unwrap();
        assert!(
            delete_remote_idx < delete_local_idx,
            "should delete remote before local"
        );
    }

    #[test]
    fn git_ref_set_provenance_writes_yaml_list() {
        let tmp = TempDir::new().unwrap();
        let td = test_type_def();
        let cache_dir = tmp.path().join(".lazyspec/cache/iteration");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let cache_content = "---\ntitle: Title\ntype: iteration\nstatus: draft\nauthor: alice\ndate: 2026-04-01\ntags: []\nrelated: []\n---\n\nbody\n";
        std::fs::write(cache_dir.join("ITERATION-042.md"), cache_content).unwrap();

        let mut lock = CacheLock::default();
        lock.set("iteration/ITERATION-042", "oldsha");
        lock.save(tmp.path()).unwrap();

        let mock = MockGitRefClient::new()
            .with_create_commit_result(Ok("newsha789".to_string()))
            .with_update_ref_result(Ok(()));

        let mut store = make_store(&tmp, mock);
        store
            .set_provenance(&td, "ITERATION-042", &["A".to_string(), "B".to_string()])
            .unwrap();

        let updated = std::fs::read_to_string(cache_dir.join("ITERATION-042.md")).unwrap();
        let (yaml, _) = split_frontmatter(&updated).unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
        let prov = parsed["provenance"].as_sequence().expect("provenance seq");
        assert_eq!(prov.len(), 2);
        assert_eq!(prov[0].as_str().unwrap(), "A");
        assert_eq!(prov[1].as_str().unwrap(), "B");
    }

    #[test]
    fn git_ref_set_provenance_replaces_existing() {
        let tmp = TempDir::new().unwrap();
        let td = test_type_def();
        let cache_dir = tmp.path().join(".lazyspec/cache/iteration");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let cache_content = "---\ntitle: Title\ntype: iteration\nstatus: draft\nauthor: alice\ndate: 2026-04-01\ntags: []\nprovenance:\n- X\nrelated: []\n---\n\nbody\n";
        std::fs::write(cache_dir.join("ITERATION-042.md"), cache_content).unwrap();

        let mut lock = CacheLock::default();
        lock.set("iteration/ITERATION-042", "oldsha");
        lock.save(tmp.path()).unwrap();

        let mock = MockGitRefClient::new()
            .with_create_commit_result(Ok("newsha".to_string()))
            .with_update_ref_result(Ok(()));

        let mut store = make_store(&tmp, mock);
        store
            .set_provenance(&td, "ITERATION-042", &["Y".to_string(), "Z".to_string()])
            .unwrap();

        let updated = std::fs::read_to_string(cache_dir.join("ITERATION-042.md")).unwrap();
        let (yaml, _) = split_frontmatter(&updated).unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
        let prov = parsed["provenance"].as_sequence().expect("provenance seq");
        assert_eq!(prov.len(), 2);
        assert_eq!(prov[0].as_str().unwrap(), "Y");
        assert_eq!(prov[1].as_str().unwrap(), "Z");
    }

    #[test]
    fn git_ref_set_provenance_uses_old_sha_for_ff() {
        let tmp = TempDir::new().unwrap();
        let td = test_type_def();
        let cache_dir = tmp.path().join(".lazyspec/cache/iteration");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let cache_content = "---\ntitle: Title\ntype: iteration\nstatus: draft\nauthor: alice\ndate: 2026-04-01\ntags: []\nrelated: []\n---\n\nbody\n";
        std::fs::write(cache_dir.join("ITERATION-042.md"), cache_content).unwrap();

        let mut lock = CacheLock::default();
        lock.set("iteration/ITERATION-042", "abc123");
        lock.save(tmp.path()).unwrap();

        let mock = MockGitRefClient::new()
            .with_create_commit_result(Ok("newsha".to_string()))
            .with_update_ref_result(Ok(()));

        let mut store = make_store(&tmp, mock);
        store
            .set_provenance(&td, "ITERATION-042", &["A".to_string()])
            .unwrap();

        let calls = store.git.calls.borrow();
        let create_call = calls
            .iter()
            .find(|c| c.starts_with("create_commit:"))
            .expect("create_commit should be called");
        assert!(
            create_call.contains("parent=Some(\"abc123\")"),
            "create_commit should be parented on old SHA, got: {}",
            create_call
        );

        let lock = CacheLock::load(tmp.path()).unwrap();
        assert_eq!(lock.get("iteration/ITERATION-042"), Some("newsha"));
    }

    #[test]
    fn delete_does_not_touch_remote_without_coordination() {
        let tmp = TempDir::new().unwrap();
        let td = test_type_def();
        let cache_dir = tmp.path().join(".lazyspec/cache/iteration");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(
            cache_dir.join("ITERATION-042.md"),
            "---\ntitle: T\ntype: iteration\nstatus: draft\nauthor: a\ndate: 2026-04-01\ntags: []\nrelated: []\n---\n",
        ).unwrap();

        let mut lock = CacheLock::default();
        lock.set("iteration/ITERATION-042", "somesha");
        lock.save(tmp.path()).unwrap();

        let mock = MockGitRefClient::new().with_delete_ref_result(Ok(()));

        let mut store = make_store(&tmp, mock);
        store.delete(&td, "ITERATION-042").unwrap();

        let calls = store.git.calls.borrow();
        assert!(
            !calls.iter().any(|c| c.starts_with("delete_remote_ref:")),
            "should not touch remote without coordination, got: {:?}",
            *calls
        );
    }
}
