use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use chrono::Local;

use crate::engine::cache_lock::CacheLock;
use crate::engine::config::{Config, TypeDef};
use crate::engine::document::{
    body_section, compose_frontmatter, split_frontmatter, DocMeta, DocType, Status,
};
use crate::engine::git_ref::GitRefClient;
use crate::engine::store_dispatch::{
    find_cache_file, write_cache_file, CreatedDoc, DocumentStore, PushOutcome,
};

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

/// True when a push error is a live remote *rejecting* the update (the ref
/// moved underneath us) rather than the remote being unreachable/offline.
fn push_was_remote_rejection(err: &anyhow::Error) -> bool {
    let s = err.to_string().to_lowercase();
    [
        "rejected",
        "stale info",
        "non-fast-forward",
        "fetch first",
        "failed to push some refs",
    ]
    .iter()
    .any(|marker| s.contains(marker))
}

fn push_failure_warning(remote: &str, doc_id: &str, err: &anyhow::Error) -> String {
    format!(
        "warning: {doc_id} was saved locally but could not be pushed to remote '{remote}' \
         ({err}). The change is safe in your local git refs; re-run once '{remote}' is reachable to sync."
    )
}

pub struct GitRefStore {
    pub git: Box<dyn GitRefClient>,
    pub root: PathBuf,
    pub config: Config,
    pub remote: String,
    pub reserved_number: Option<u32>,
}

impl GitRefStore {
    /// How many times `create` re-tries a number after a concurrent clone
    /// claims it on the remote before giving up.
    const CREATE_MAX_RETRIES: u8 = 5;

    /// Downcast the boxed client to a concrete mock for test assertions.
    #[cfg(test)]
    fn git_mock(&self) -> &crate::engine::git_ref::test_support::MockGitRefClient {
        (*self.git)
            .as_any()
            .downcast_ref::<crate::engine::git_ref::test_support::MockGitRefClient>()
            .expect("git client is a MockGitRefClient")
    }

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
        format!(
            "---\ntitle: \"{}\"\ntype: {}\nstatus: {}\nauthor: \"{}\"\ndate: {}\ntags: []\nrelated: []\n---{}",
            title, type_def.name, status, author, date, body_section(body)
        )
    }

    /// Turn a push Result into the mutation's outcome. A remote that *rejects*
    /// the push (ref diverged) surfaces as the conflict error (STORY-218 AC2);
    /// an unreachable/offline remote keeps the local write and warns (AC5).
    fn handle_push_result(&self, doc_id: &str, result: Result<()>) -> Result<PushOutcome> {
        match result {
            Ok(()) => Ok(PushOutcome::Synced),
            Err(e) if push_was_remote_rejection(&e) => {
                bail!(
                    "conflict pushing {} to remote '{}': {}",
                    doc_id,
                    self.remote,
                    e
                )
            }
            Err(e) => Ok(PushOutcome::LocalOnly {
                warning: push_failure_warning(&self.remote, doc_id, &e),
            }),
        }
    }

    /// Create the ref commit, cache file, and lock entry for a new doc id,
    /// returning the new commit SHA. Does not push.
    fn materialize_created_doc(
        &mut self,
        type_def: &TypeDef,
        id: &str,
        title: &str,
        author: &str,
        date: &str,
        body: &str,
    ) -> Result<String> {
        let seed_status = type_def.lifecycle.seed_status();
        let content = Self::build_markdown(type_def, title, author, date, seed_status, body);
        let refname = Self::refname(&type_def.name, id);
        let sha = self
            .git
            .create_ref_commit(&self.root, &refname, &[("doc.md", &content)])?;

        let meta = DocMeta {
            path: PathBuf::new(),
            title: title.to_string(),
            doc_type: DocType::new(&type_def.name),
            status: Status::new(seed_status),
            author: author.to_string(),
            date: Local::now().date_naive(),
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            attributes: Default::default(),
            id: id.to_string(),
        };
        write_cache_file(&self.root, type_def, &meta, body)?;

        let mut lock = CacheLock::load(&self.root)?;
        lock.set(&Self::doc_key(&type_def.name, id), &sha);
        lock.save(&self.root)?;
        Ok(sha)
    }

    /// Undo a create attempt whose claim was rejected, so the next number can
    /// be tried against clean local state.
    fn unmaterialize_created_doc(&mut self, type_def: &TypeDef, id: &str) -> Result<()> {
        let refname = Self::refname(&type_def.name, id);
        self.git.delete_ref(&self.root, &refname)?;

        let cache_dir = self.root.join(".lazyspec/cache").join(&type_def.name);
        if let Some(cache_path) = find_cache_file(&cache_dir, id) {
            std::fs::remove_file(&cache_path)?;
        }

        let mut lock = CacheLock::load(&self.root)?;
        lock.remove(&Self::doc_key(&type_def.name, id));
        lock.save(&self.root)?;
        Ok(())
    }

    fn created_doc(&self, type_def: &TypeDef, id: String, push_outcome: PushOutcome) -> CreatedDoc {
        let cache_path = self
            .root
            .join(".lazyspec/cache")
            .join(&type_def.name)
            .join(format!("{}.md", id));
        let relative = cache_path
            .strip_prefix(&self.root)
            .unwrap_or(&cache_path)
            .to_path_buf();
        CreatedDoc {
            path: relative,
            id,
            push_outcome,
        }
    }

    /// Re-commit the current cache content into the ref blob.
    ///
    /// The CLI has already rewritten the cache file's `tags:` block (git-ref docs
    /// materialize under `.lazyspec/cache/<type>/`), so callers pass no diff --
    /// the cache already reflects the change. This persists that content into the
    /// git ref, mirroring the tail of [`DocumentStore::update`]: create a child
    /// commit on the recorded SHA, then CAS-swap the ref. The single-line
    /// `key: value` mutation loop of `update` is deliberately not reused: it
    /// cannot touch a `tags:` sequence block. Finally the new SHA is pushed with
    /// a lease so the ref stays live on the remote.
    pub(crate) fn recommit_cache(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
    ) -> Result<PushOutcome> {
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

        let refname = Self::refname(&type_def.name, doc_id);
        let new_sha = self.git.create_commit(
            &self.root,
            &refname,
            &[("doc.md", &content)],
            Some(&old_sha),
        )?;

        if let Err(e) = self
            .git
            .update_ref(&self.root, &refname, &new_sha, &old_sha)
        {
            bail!("conflict updating {}: {}", doc_id, e);
        }

        let mut lock = CacheLock::load(&self.root)?;
        lock.set(&doc_key, &new_sha);
        lock.save(&self.root)?;

        let push = self.git.push_ref_with_lease(
            &self.root,
            &self.remote,
            &refname,
            &new_sha,
            Some(&old_sha),
        );
        self.handle_push_result(doc_id, push)
    }
}

impl DocumentStore for GitRefStore {
    fn create(
        &mut self,
        type_def: &TypeDef,
        title: &str,
        author: &str,
        body: &str,
    ) -> Result<CreatedDoc> {
        ensure_cache_gitignored(&self.root)?;
        let date = Local::now().format("%Y-%m-%d").to_string();

        // A reserved number is claimed by the caller (e.g. the reservation
        // subsystem); bypass cross-clone allocation and materialize it directly.
        if let Some(n) = self.reserved_number {
            let id = format!("{}-{:03}", type_def.prefix, n);
            let sha = self.materialize_created_doc(type_def, &id, title, author, &date, body)?;
            let refname = Self::refname(&type_def.name, &id);
            let push = self
                .git
                .push_new_ref(&self.root, &self.remote, &refname, &sha);
            let outcome = self.handle_push_result(&id, push)?;
            return Ok(self.created_doc(type_def, id, outcome));
        }

        // Cross-clone-safe allocation (STORY-218 AC3): claim the number on the
        // remote with an expect-absent push. A clone that concurrently grabbed
        // the same number makes the remote reject the push, so we fetch the
        // winning ref and retry with the next number, bounded by retries.
        let mut last_rejection = None;
        for _ in 0..Self::CREATE_MAX_RETRIES {
            let next_num = self.next_number_from_refs(type_def)?;
            let id = format!("{}-{:03}", type_def.prefix, next_num);
            let sha = self.materialize_created_doc(type_def, &id, title, author, &date, body)?;
            let refname = Self::refname(&type_def.name, &id);

            match self
                .git
                .push_new_ref(&self.root, &self.remote, &refname, &sha)
            {
                Ok(()) => return Ok(self.created_doc(type_def, id, PushOutcome::Synced)),
                Err(e) if push_was_remote_rejection(&e) => {
                    self.unmaterialize_created_doc(type_def, &id)?;
                    let pattern = format!("{}*", Self::ref_prefix(&type_def.name));
                    self.git.fetch_refs(&self.root, &self.remote, &pattern)?;
                    last_rejection = Some(e);
                }
                Err(e) => {
                    // Remote unreachable: keep the local write and surface the
                    // warning as the outcome, matching the offline semantics of
                    // every other mutation (AC5/ITER-309).
                    let outcome = PushOutcome::LocalOnly {
                        warning: push_failure_warning(&self.remote, &id, &e),
                    };
                    return Ok(self.created_doc(type_def, id, outcome));
                }
            }
        }

        bail!(
            "could not allocate a cross-clone-safe number for {} after {} attempts: \
             remote '{}' kept rejecting the claim ({})",
            type_def.prefix,
            Self::CREATE_MAX_RETRIES,
            self.remote,
            last_rejection
                .map(|e| e.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
        )
    }

    fn update(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        updates: &[(&str, &str)],
    ) -> Result<PushOutcome> {
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

        // Round-trip through serde_yaml (as set_provenance does) so missing
        // keys are inserted rather than dropped and YAML-significant values
        // ("Plan: phase 2") come out properly quoted (AUDIT-018 C3).
        let mut frontmatter: serde_yaml::Value = serde_yaml::from_str(&yaml)?;
        let map = frontmatter
            .as_mapping_mut()
            .ok_or_else(|| anyhow::anyhow!("frontmatter root must be a mapping"))?;
        let mut new_body: Option<String> = None;
        for &(key, value) in updates {
            if key == "body" {
                new_body = Some(value.to_string());
                continue;
            }
            // `assignee` is absent-when-unset; clearing it with "" removes the
            // key rather than writing an empty scalar.
            if key == "assignee" && value.is_empty() {
                map.remove(serde_yaml::Value::String("assignee".to_string()));
                continue;
            }
            map.insert(
                serde_yaml::Value::String(key.to_string()),
                serde_yaml::Value::String(value.to_string()),
            );
        }

        let updated_yaml = serde_yaml::to_string(&frontmatter)?;
        let body = new_body.as_deref().unwrap_or(&existing_body);
        let updated_content = compose_frontmatter(&updated_yaml, &body_section(body));

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

        std::fs::write(&cache_path, &updated_content)?;

        let mut lock = CacheLock::load(&self.root)?;
        lock.set(&doc_key, &new_sha);
        lock.save(&self.root)?;

        let push = self.git.push_ref_with_lease(
            &self.root,
            &self.remote,
            &refname,
            &new_sha,
            Some(&old_sha),
        );
        self.handle_push_result(doc_id, push)
    }

    fn set_provenance(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        provenance: &[String],
    ) -> Result<PushOutcome> {
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

        let updated_content = compose_frontmatter(&new_yaml, &body_section(&existing_body));

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

        std::fs::write(&cache_path, &updated_content)?;

        let mut lock = CacheLock::load(&self.root)?;
        lock.set(&doc_key, &new_sha);
        lock.save(&self.root)?;

        let push = self.git.push_ref_with_lease(
            &self.root,
            &self.remote,
            &refname,
            &new_sha,
            Some(&old_sha),
        );
        self.handle_push_result(doc_id, push)
    }

    fn delete(&mut self, type_def: &TypeDef, doc_id: &str) -> Result<PushOutcome> {
        let refname = Self::refname(&type_def.name, doc_id);
        let doc_key = Self::doc_key(&type_def.name, doc_id);

        let old_sha = CacheLock::load(&self.root)?
            .get(&doc_key)
            .map(|s| s.to_string());

        self.git.delete_ref(&self.root, &refname)?;

        let cache_dir = self.root.join(".lazyspec/cache").join(&type_def.name);
        if let Some(cache_path) = find_cache_file(&cache_dir, doc_id) {
            std::fs::remove_file(&cache_path)?;
        }

        let mut lock = CacheLock::load(&self.root)?;
        lock.remove(&doc_key);
        lock.save(&self.root)?;

        let push =
            self.git
                .delete_remote_ref(&self.root, &self.remote, &refname, old_sha.as_deref());
        self.handle_push_result(doc_id, push)
    }

    fn sync_tags(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        _add: &[String],
        _remove: &[String],
    ) -> Result<PushOutcome> {
        self.recommit_cache(type_def, doc_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{
        Config, DocumentConfig, FilesystemConfig, Naming, NumberingStrategy, StoreBackend,
        Templates, TypeDef, UiConfig,
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
            agents: Vec::new(),
            intent: None,
            authorship: Default::default(),
            lifecycle: Default::default(),
            attributes: Default::default(),
            label_override: None,
            github_issue_tag: None,
            github_issue_type: None,
            status_authority: None,
            clickup_list_id: None,
            clickup_task_type: None,
            clickup_custom_field_map: None,
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
                templates: Templates {
                    dir: ".lazyspec/templates".to_string(),
                },
            },
            relationships: crate::engine::config::starter_relationships(),
            ui: UiConfig::default(),
            rules: vec![],
            ref_count_ceiling: 0,
            certification: Default::default(),
            agents: Default::default(),
            skills: Default::default(),
            web: None,
            git_ref: Default::default(),
        }
    }

    fn make_store(tmp: &TempDir, mock: MockGitRefClient) -> GitRefStore {
        let config = test_config();
        GitRefStore {
            git: Box::new(mock),
            root: tmp.path().to_path_buf(),
            remote: config.git_ref.remote.clone(),
            config,
            reserved_number: None,
        }
    }

    // STORY-218 AC1: the store carries the remote resolved from `[git-ref]`,
    // mirroring how the runtime construction sites wire it from config.
    #[test]
    fn store_carries_configured_remote() {
        let tmp = TempDir::new().unwrap();
        let mut config = test_config();
        config.git_ref.remote = "upstream".to_string();
        let store = GitRefStore {
            git: Box::new(MockGitRefClient::new()),
            root: tmp.path().to_path_buf(),
            remote: config.git_ref.remote.clone(),
            config,
            reserved_number: None,
        };
        assert_eq!(store.remote, "upstream");
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

        let calls = store.git_mock().calls.borrow();
        assert!(calls.iter().any(|c| c.starts_with("list_refs:")));
        assert!(calls
            .iter()
            .any(|c| c.contains("create_ref_commit:refs/lazyspec/iteration/ITERATION-001")));
    }

    // BUG-002: a type whose lifecycle starts at a non-draft state must be born
    // in that first state, not the hardcoded `draft`.
    #[test]
    fn test_git_ref_store_create_seeds_first_lifecycle_state() {
        let tmp = TempDir::new().unwrap();
        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![]))
            .with_create_ref_commit_result(Ok("abc123sha".to_string()));

        let mut store = make_store(&tmp, mock);
        let mut td = test_type_def();
        td.lifecycle = crate::engine::config::Lifecycle {
            states: vec!["reported".into(), "triaged".into(), "fixed".into()],
            edges: vec![],
        };
        let result = store.create(&td, "Broken", "alice", "body").unwrap();

        let cache_dir = tmp.path().join(".lazyspec/cache/iteration");
        let cache_file = find_cache_file(&cache_dir, &result.id).unwrap();
        let content = std::fs::read_to_string(cache_file).unwrap();
        assert!(
            content.contains("status: reported"),
            "doc should be seeded with the first lifecycle state, got: {}",
            content
        );
        assert!(!content.contains("status: draft"));
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

    // STORY-223 AC4: git-ref (like filesystem) is a local store with no remote
    // open/closed concept. Its status comes straight from frontmatter and is
    // never coerced by the github open/closed -> first-active/terminal mapping.
    // A doc stored at an intermediate `in-progress` state reads/updates back at
    // `in-progress`, not remapped to the lifecycle's first-active or terminal.
    #[test]
    fn test_git_ref_store_status_not_overridden_by_remote_state_logic() {
        let tmp = TempDir::new().unwrap();

        let mut td = test_type_def();
        td.lifecycle = crate::engine::config::Lifecycle {
            states: vec!["backlog".into(), "in-progress".into(), "shipped".into()],
            edges: vec![],
        };

        let cache_dir = tmp.path().join(".lazyspec/cache/iteration");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let cache_content = "---\ntitle: Feature\ntype: iteration\nstatus: in-progress\nauthor: alice\ndate: 2026-04-01\ntags: []\nrelated: []\n---\n\nbody\n";
        std::fs::write(cache_dir.join("ITERATION-042.md"), cache_content).unwrap();

        let mut lock = CacheLock::default();
        lock.set("iteration/ITERATION-042", "oldsha");
        lock.save(tmp.path()).unwrap();

        let mock = MockGitRefClient::new()
            .with_create_commit_result(Ok("newsha".to_string()))
            .with_update_ref_result(Ok(()));

        let mut store = make_store(&tmp, mock);
        // Update an unrelated field; status must be left exactly as stored.
        store
            .update(&td, "ITERATION-042", &[("title", "Renamed")])
            .unwrap();

        let updated = std::fs::read_to_string(cache_dir.join("ITERATION-042.md")).unwrap();
        assert!(
            updated.contains("status: in-progress"),
            "git-ref status must not be remapped to first-active/terminal, got: {}",
            updated
        );
        assert!(!updated.contains("status: backlog"));
        assert!(!updated.contains("status: shipped"));
    }

    #[test]
    fn update_body_keeps_a_blank_line_after_the_frontmatter() {
        let tmp = TempDir::new().unwrap();

        let td = test_type_def();
        seed_doc(&tmp, SEED_CACHE, "oldsha");

        let mock = MockGitRefClient::new()
            .with_create_commit_result(Ok("newsha456".to_string()))
            .with_update_ref_result(Ok(()));

        let mut store = make_store(&tmp, mock);
        store
            .update(&td, "ITERATION-042", &[("body", "## Problem\n\nBroke.")])
            .unwrap();

        let updated = std::fs::read_to_string(
            tmp.path()
                .join(".lazyspec/cache/iteration/ITERATION-042.md"),
        )
        .unwrap();
        assert!(
            updated.ends_with("---\n\n## Problem\n\nBroke.\n"),
            "body glued to the delimiter: {:?}",
            updated
        );
    }

    #[test]
    fn update_without_a_body_keeps_the_blank_line_after_the_frontmatter() {
        let tmp = TempDir::new().unwrap();

        let td = test_type_def();
        seed_doc(&tmp, SEED_CACHE, "oldsha");

        let mock = MockGitRefClient::new()
            .with_create_commit_result(Ok("newsha456".to_string()))
            .with_update_ref_result(Ok(()));

        let mut store = make_store(&tmp, mock);
        store
            .update(&td, "ITERATION-042", &[("status", "accepted")])
            .unwrap();

        let updated = std::fs::read_to_string(
            tmp.path()
                .join(".lazyspec/cache/iteration/ITERATION-042.md"),
        )
        .unwrap();
        assert!(
            updated.ends_with("---\n\nbody\n"),
            "separator blank line lost: {:?}",
            updated
        );
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

        let calls = store.git_mock().calls.borrow();
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

    // AUDIT-018 C3 / STORY-210 AC2: updating a key not yet present in the
    // frontmatter must insert it, not silently drop it while returning Ok.
    #[test]
    fn test_git_ref_store_update_inserts_missing_key() {
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
            .with_create_commit_result(Ok("newsha".to_string()))
            .with_update_ref_result(Ok(()));

        let mut store = make_store(&tmp, mock);
        store
            .update(&td, "ITERATION-042", &[("priority", "high")])
            .unwrap();

        let updated = std::fs::read_to_string(cache_dir.join("ITERATION-042.md")).unwrap();
        let (yaml, _) = split_frontmatter(&updated).unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(
            parsed["priority"].as_str(),
            Some("high"),
            "new key must be inserted, got: {}",
            updated
        );
        assert_eq!(parsed["title"].as_str(), Some("Title"));
    }

    // STORY-222 AC2: assignee is settable on a git-ref doc via `update` (no
    // hand-edit), and clearing with "" removes the key.
    #[test]
    fn test_git_ref_store_update_sets_and_clears_assignee() {
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
            .with_create_commit_result(Ok("sha1".to_string()))
            .with_update_ref_result(Ok(()));
        let mut store = make_store(&tmp, mock);
        store
            .update(&td, "ITERATION-042", &[("assignee", "alice")])
            .unwrap();

        let updated = std::fs::read_to_string(cache_dir.join("ITERATION-042.md")).unwrap();
        let (yaml, _) = split_frontmatter(&updated).unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed["assignee"].as_str(), Some("alice"));

        let mock = MockGitRefClient::new()
            .with_create_commit_result(Ok("sha2".to_string()))
            .with_update_ref_result(Ok(()));
        let mut store = make_store(&tmp, mock);
        store
            .update(&td, "ITERATION-042", &[("assignee", "")])
            .unwrap();

        let cleared = std::fs::read_to_string(cache_dir.join("ITERATION-042.md")).unwrap();
        assert!(
            !cleared.contains("assignee:"),
            "clearing with empty string must remove the key, got:\n{cleared}"
        );
    }

    // AUDIT-018 C3 / STORY-210 AC2: values with YAML-significant characters
    // (`--title 'Plan: phase 2'`) must be serialized so the frontmatter still
    // parses and round-trips the exact value.
    #[test]
    fn test_git_ref_store_update_quotes_yaml_significant_values() {
        let tmp = TempDir::new().unwrap();

        let td = test_type_def();
        let cache_dir = tmp.path().join(".lazyspec/cache/iteration");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let cache_content = "---\ntitle: Old Title\ntype: iteration\nstatus: draft\nauthor: alice\ndate: 2026-04-01\ntags: []\nrelated: []\n---\n\nbody\n";
        std::fs::write(cache_dir.join("ITERATION-042.md"), cache_content).unwrap();

        let mut lock = CacheLock::default();
        lock.set("iteration/ITERATION-042", "oldsha");
        lock.save(tmp.path()).unwrap();

        let mock = MockGitRefClient::new()
            .with_create_commit_result(Ok("newsha".to_string()))
            .with_update_ref_result(Ok(()));

        let mut store = make_store(&tmp, mock);
        store
            .update(&td, "ITERATION-042", &[("title", "Plan: phase 2")])
            .unwrap();

        let updated = std::fs::read_to_string(cache_dir.join("ITERATION-042.md")).unwrap();
        let (yaml, _) = split_frontmatter(&updated).unwrap();
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&yaml).expect("frontmatter must still parse as YAML");
        assert_eq!(
            parsed["title"].as_str(),
            Some("Plan: phase 2"),
            "colon-containing title must round-trip, got: {}",
            updated
        );
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

        let calls = store.git_mock().calls.borrow();
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
            git: Box::new(mock),
            root: tmp.path().to_path_buf(),
            remote: "origin".to_string(),
            config: test_config(),
            reserved_number: Some(42),
        };
        let td = test_type_def();
        let result = store.create(&td, "Reserved Title", "alice", "").unwrap();

        assert_eq!(result.id, "ITERATION-042");

        let calls = store.git_mock().calls.borrow();
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
            git: Box::new(mock),
            root: tmp.path().to_path_buf(),
            remote: "origin".to_string(),
            config: test_config(),
            reserved_number: None,
        };
        let td = test_type_def();
        let result = store.create(&td, "Fallback Title", "bob", "").unwrap();

        assert_eq!(result.id, "ITERATION-004");

        let calls = store.git_mock().calls.borrow();
        assert!(
            calls.iter().any(|c| c.starts_with("list_refs:")),
            "should call list_refs when no reserved_number"
        );
    }

    // AC: git-ref tag add/remove re-commits the ref. The CLI has already
    // rewritten the cache `tags:`; sync_tags re-commits that content into the
    // ref blob (create_commit + update_ref) and the lock advances.
    #[test]
    fn sync_tags_recommits_cache_content() {
        let tmp = TempDir::new().unwrap();
        let td = test_type_def();
        let cache_dir = tmp.path().join(".lazyspec/cache/iteration");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(
            cache_dir.join("ITERATION-042.md"),
            "---\ntitle: T\ntype: iteration\nstatus: draft\nauthor: a\ndate: 2026-04-01\ntags:\n- security\nrelated: []\n---\n\nbody\n",
        )
        .unwrap();

        let mut lock = CacheLock::default();
        lock.set("iteration/ITERATION-042", "oldsha");
        lock.save(tmp.path()).unwrap();

        let mock = MockGitRefClient::new()
            .with_create_commit_result(Ok("newsha456".to_string()))
            .with_update_ref_result(Ok(()));

        let mut store = make_store(&tmp, mock);
        store
            .sync_tags(&td, "ITERATION-042", &["security".to_string()], &[])
            .unwrap();

        // The committed ref blob is the current cache content, carrying the tag.
        let blobs = store.git_mock().committed_blobs.borrow();
        assert_eq!(blobs.len(), 1, "should commit exactly once");
        assert!(
            blobs[0].contains("security"),
            "committed ref blob should contain the tag, got: {}",
            blobs[0]
        );
        drop(blobs);

        let calls = store.git_mock().calls.borrow();
        assert!(
            calls.iter().any(|c| c.starts_with("create_commit:")),
            "should re-commit locally, got: {:?}",
            *calls
        );
        assert!(
            calls.iter().any(|c| c.starts_with("update_ref:")),
            "should CAS-swap the ref, got: {:?}",
            *calls
        );
        drop(calls);

        let lock = CacheLock::load(tmp.path()).unwrap();
        assert_eq!(lock.get("iteration/ITERATION-042"), Some("newsha456"));
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

        let calls = store.git_mock().calls.borrow();
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

    // Helper: seed a cache file + lock entry for an existing git-ref doc so a
    // mutation has something to update/push.
    fn seed_doc(tmp: &TempDir, cache_content: &str, sha: &str) {
        let cache_dir = tmp.path().join(".lazyspec/cache/iteration");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(cache_dir.join("ITERATION-042.md"), cache_content).unwrap();
        let mut lock = CacheLock::default();
        lock.set("iteration/ITERATION-042", sha);
        lock.save(tmp.path()).unwrap();
    }

    const SEED_CACHE: &str = "---\ntitle: Title\ntype: iteration\nstatus: draft\nauthor: alice\ndate: 2026-04-01\ntags: []\nrelated: []\n---\n\nbody\n";

    // STORY-218 AC2/AC3: a successful create claims the new ref on the remote
    // with an expect-absent push.
    #[test]
    fn create_pushes_ref_to_remote() {
        let tmp = TempDir::new().unwrap();
        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![]))
            .with_create_ref_commit_result(Ok("abc123sha".to_string()))
            .with_push_new_ref_result(Ok(()));

        let mut store = make_store(&tmp, mock);
        let td = test_type_def();
        let result = store.create(&td, "My Feature", "alice", "body").unwrap();

        // BUG-006: a create whose push reaches the remote reports `Synced`.
        assert_eq!(result.push_outcome, PushOutcome::Synced);

        let calls = store.git_mock().calls.borrow();
        assert!(
            calls.iter().any(|c| c
                == "push_new_ref:origin:refs/lazyspec/iteration/ITERATION-001:new_sha=abc123sha"),
            "create should claim the new ref on the remote, got: {:?}",
            *calls
        );
    }

    // STORY-218 AC3: a concurrent clone claiming the same number makes the
    // remote reject the push; create refetches and retries the next number.
    #[test]
    fn create_collision_retries_to_next_number() {
        let tmp = TempDir::new().unwrap();
        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![]))
            .with_create_ref_commit_result(Ok("sha1".to_string()))
            .with_push_new_ref_result(Err(anyhow::anyhow!("! [rejected] (stale info)")))
            .with_list_result(Ok(vec![(
                "refs/lazyspec/iteration/ITERATION-001".to_string(),
                "winner".to_string(),
            )]))
            .with_create_ref_commit_result(Ok("sha2".to_string()))
            .with_push_new_ref_result(Ok(()));

        let mut store = make_store(&tmp, mock);
        let td = test_type_def();
        let result = store.create(&td, "Racy", "alice", "").unwrap();

        assert_eq!(result.id, "ITERATION-002");

        let calls = store.git_mock().calls.borrow();
        assert!(
            calls.iter().any(|c| c.starts_with("fetch_refs:")),
            "a rejected claim should refetch before retrying, got: {:?}",
            *calls
        );
        assert_eq!(
            calls
                .iter()
                .filter(|c| c.starts_with("push_new_ref:"))
                .count(),
            2,
            "should push twice: rejected then accepted, got: {:?}",
            *calls
        );
        drop(calls);

        let lock = CacheLock::load(tmp.path()).unwrap();
        assert!(
            lock.get("iteration/ITERATION-001").is_none(),
            "the rejected number's lock entry should be cleaned up"
        );
        assert_eq!(lock.get("iteration/ITERATION-002"), Some("sha2"));
    }

    // STORY-218 AC3: if every retry keeps colliding, create fails rather than
    // silently duplicating a number.
    #[test]
    fn create_exhausts_retries_and_errors() {
        let tmp = TempDir::new().unwrap();
        let mut mock = MockGitRefClient::new();
        for _ in 0..GitRefStore::CREATE_MAX_RETRIES {
            mock = mock
                .with_list_result(Ok(vec![]))
                .with_create_ref_commit_result(Ok("sha".to_string()))
                .with_push_new_ref_result(Err(anyhow::anyhow!("! [rejected] (stale info)")));
        }

        let mut store = make_store(&tmp, mock);
        let td = test_type_def();
        let result = store.create(&td, "Always Losing", "alice", "");

        assert!(result.is_err(), "exhausted retries must error");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("after") && err.contains("attempts"),
            "error should mention retry exhaustion, got: {}",
            err
        );

        let calls = store.git_mock().calls.borrow();
        assert_eq!(
            calls
                .iter()
                .filter(|c| c.starts_with("push_new_ref:"))
                .count(),
            GitRefStore::CREATE_MAX_RETRIES as usize,
            "should attempt exactly CREATE_MAX_RETRIES pushes, got: {:?}",
            *calls
        );
    }

    // STORY-218 AC3/AC5: an unreachable remote keeps the local write, warns,
    // and does not retry (there is no competing claim to lose to).
    #[test]
    fn create_offline_falls_back_to_local_with_warning() {
        let tmp = TempDir::new().unwrap();
        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![]))
            .with_create_ref_commit_result(Ok("localsha".to_string()))
            .with_push_new_ref_result(Err(anyhow::anyhow!(
                "fatal: Could not read from remote repository"
            )));

        let mut store = make_store(&tmp, mock);
        let td = test_type_def();
        let result = store
            .create(&td, "Offline Feature", "alice", "body")
            .unwrap();

        assert_eq!(result.id, "ITERATION-001");

        // BUG-006: the unreachable-remote outcome is reported (not printed and
        // swallowed), carrying a warning that names the remote and the doc id.
        let warning = match &result.push_outcome {
            PushOutcome::LocalOnly { warning } => warning,
            other => panic!("offline create should report LocalOnly, got: {:?}", other),
        };
        assert!(
            warning.contains("origin"),
            "warning should mention the remote, got: {}",
            warning
        );
        assert!(
            warning.contains("ITERATION-001"),
            "warning should mention the doc id, got: {}",
            warning
        );

        let cache_dir = tmp.path().join(".lazyspec/cache/iteration");
        assert!(
            find_cache_file(&cache_dir, "ITERATION-001").is_some(),
            "offline create should keep the local cache file"
        );

        let lock = CacheLock::load(tmp.path()).unwrap();
        assert_eq!(
            lock.get("iteration/ITERATION-001"),
            Some("localsha"),
            "offline create should keep the local lock entry"
        );

        let calls = store.git_mock().calls.borrow();
        assert!(
            !calls.iter().any(|c| c.starts_with("fetch_refs:")),
            "an unreachable remote must not trigger a collision refetch, got: {:?}",
            *calls
        );
        assert_eq!(
            calls
                .iter()
                .filter(|c| c.starts_with("push_new_ref:"))
                .count(),
            1,
            "offline should not retry, got: {:?}",
            *calls
        );
    }

    // STORY-218 AC2: update pushes the new SHA with a lease on the old SHA.
    #[test]
    fn update_pushes_with_lease() {
        let tmp = TempDir::new().unwrap();
        seed_doc(&tmp, SEED_CACHE, "oldsha");

        let mock = MockGitRefClient::new()
            .with_create_commit_result(Ok("newsha".to_string()))
            .with_update_ref_result(Ok(()))
            .with_push_with_lease_result(Ok(()));

        let mut store = make_store(&tmp, mock);
        let outcome = store
            .update(&test_type_def(), "ITERATION-042", &[("status", "accepted")])
            .unwrap();

        // BUG-006: an update whose push reaches the remote reports `Synced`.
        assert_eq!(outcome, PushOutcome::Synced);

        let calls = store.git_mock().calls.borrow();
        assert!(
            calls.iter().any(|c| c
                == "push_ref_with_lease:origin:refs/lazyspec/iteration/ITERATION-042:new_sha=newsha:expected_old=Some(\"oldsha\")"),
            "update should push with lease, got: {:?}",
            *calls
        );
    }

    // STORY-218 AC2: set_provenance pushes the new SHA with a lease.
    #[test]
    fn set_provenance_pushes_with_lease() {
        let tmp = TempDir::new().unwrap();
        seed_doc(&tmp, SEED_CACHE, "oldsha");

        let mock = MockGitRefClient::new()
            .with_create_commit_result(Ok("newsha".to_string()))
            .with_update_ref_result(Ok(()))
            .with_push_with_lease_result(Ok(()));

        let mut store = make_store(&tmp, mock);
        store
            .set_provenance(&test_type_def(), "ITERATION-042", &["A".to_string()])
            .unwrap();

        let calls = store.git_mock().calls.borrow();
        assert!(
            calls.iter().any(|c| c
                == "push_ref_with_lease:origin:refs/lazyspec/iteration/ITERATION-042:new_sha=newsha:expected_old=Some(\"oldsha\")"),
            "set_provenance should push with lease, got: {:?}",
            *calls
        );
    }

    // STORY-218 AC2: sync_tags (via recommit_cache) pushes with a lease.
    #[test]
    fn sync_tags_pushes_with_lease() {
        let tmp = TempDir::new().unwrap();
        seed_doc(&tmp, SEED_CACHE, "oldsha");

        let mock = MockGitRefClient::new()
            .with_create_commit_result(Ok("newsha".to_string()))
            .with_update_ref_result(Ok(()))
            .with_push_with_lease_result(Ok(()));

        let mut store = make_store(&tmp, mock);
        store
            .sync_tags(
                &test_type_def(),
                "ITERATION-042",
                &["security".to_string()],
                &[],
            )
            .unwrap();

        let calls = store.git_mock().calls.borrow();
        assert!(
            calls.iter().any(|c| c
                == "push_ref_with_lease:origin:refs/lazyspec/iteration/ITERATION-042:new_sha=newsha:expected_old=Some(\"oldsha\")"),
            "sync_tags should push with lease, got: {:?}",
            *calls
        );
    }

    // STORY-218 AC2: delete pushes a remote deletion with a lease on old SHA.
    #[test]
    fn delete_pushes_delete_remote() {
        let tmp = TempDir::new().unwrap();
        seed_doc(&tmp, SEED_CACHE, "somesha");

        let mock = MockGitRefClient::new()
            .with_delete_ref_result(Ok(()))
            .with_delete_remote_result(Ok(()));

        let mut store = make_store(&tmp, mock);
        store.delete(&test_type_def(), "ITERATION-042").unwrap();

        let calls = store.git_mock().calls.borrow();
        assert!(
            calls.iter().any(|c| c
                == "delete_remote_ref:origin:refs/lazyspec/iteration/ITERATION-042:expected_old=Some(\"somesha\")"),
            "delete should push a remote deletion, got: {:?}",
            *calls
        );
    }

    // STORY-218 AC5 (F5): an unreachable remote keeps the local write and warns
    // instead of failing the mutation.
    #[test]
    fn update_offline_keeps_local_write_and_warns() {
        let tmp = TempDir::new().unwrap();
        seed_doc(&tmp, SEED_CACHE, "oldsha");

        let mock = MockGitRefClient::new()
            .with_create_commit_result(Ok("newsha".to_string()))
            .with_update_ref_result(Ok(()))
            .with_push_with_lease_result(Err(anyhow::anyhow!(
                "fatal: Could not read from remote repository"
            )));

        let mut store = make_store(&tmp, mock);
        let result = store.update(&test_type_def(), "ITERATION-042", &[("status", "accepted")]);
        assert!(result.is_ok(), "offline push must not fail the mutation");

        // BUG-006: the unreachable-remote outcome is reported, not swallowed.
        let outcome = result.unwrap();
        assert!(
            matches!(outcome, PushOutcome::LocalOnly { .. }),
            "offline update should report LocalOnly, got: {:?}",
            outcome
        );

        let cache_dir = tmp.path().join(".lazyspec/cache/iteration");
        let updated = std::fs::read_to_string(cache_dir.join("ITERATION-042.md")).unwrap();
        assert!(
            updated.contains("status: accepted"),
            "local write must be kept on offline push, got: {}",
            updated
        );

        let lock = CacheLock::load(tmp.path()).unwrap();
        assert_eq!(
            lock.get("iteration/ITERATION-042"),
            Some("newsha"),
            "lock should advance even when the push is offline"
        );

        let calls = store.git_mock().calls.borrow();
        assert!(
            calls.iter().any(|c| c.starts_with("push_ref_with_lease:")),
            "a push should still have been attempted, got: {:?}",
            *calls
        );
    }

    // STORY-218 AC2: a remote that *rejects* the push (ref diverged) surfaces as
    // a conflict error.
    #[test]
    fn update_remote_rejection_is_conflict() {
        let tmp = TempDir::new().unwrap();
        seed_doc(&tmp, SEED_CACHE, "oldsha");

        let mock = MockGitRefClient::new()
            .with_create_commit_result(Ok("newsha".to_string()))
            .with_update_ref_result(Ok(()))
            .with_push_with_lease_result(Err(anyhow::anyhow!(
                "! [rejected] (stale info)\nerror: failed to push some refs"
            )));

        let mut store = make_store(&tmp, mock);
        let result = store.update(&test_type_def(), "ITERATION-042", &[("status", "accepted")]);
        assert!(result.is_err(), "a rejected push must fail the mutation");
        assert!(
            result.unwrap_err().to_string().contains("conflict"),
            "rejection should surface as a conflict"
        );
    }

    #[test]
    fn push_was_remote_rejection_classifies_markers() {
        assert!(push_was_remote_rejection(&anyhow::anyhow!(
            "! [rejected] (stale info)"
        )));
        assert!(!push_was_remote_rejection(&anyhow::anyhow!(
            "fatal: could not resolve host github.com"
        )));
    }

    #[test]
    fn push_failure_warning_mentions_remote_doc_and_retry() {
        let msg = push_failure_warning("origin", "ITERATION-042", &anyhow::anyhow!("network down"));
        assert!(msg.contains("origin"));
        assert!(msg.contains("ITERATION-042"));
        assert!(msg.contains("re-run"));
    }
}
