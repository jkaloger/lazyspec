use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use anyhow::{bail, Result};
use chrono::Local;
use serde::Serialize;

use crate::engine::clickup_cache;
use crate::engine::config::{Config, Lifecycle, StoreBackend, TypeDef};
use crate::engine::document::{compose_frontmatter, AttrValue, DocMeta, DocType, Status};
use crate::engine::gh::{self, GhClient, GhGraphql, GhMilestoneClient, GhProjectsClient, GqlVar};
use crate::engine::gh_schema::{try_org_then_user, GhSchemaSnapshot};
use crate::engine::issue_body;
use crate::engine::issue_cache::{self, IssueCache};
use crate::engine::issue_map::IssueMap;
use crate::engine::store::{self, Store};
use crate::engine::task_map::TaskMap;
use crate::engine::template;

#[derive(Serialize)]
struct CacheFrontmatter {
    title: String,
    #[serde(rename = "type")]
    doc_type: String,
    status: String,
    author: String,
    date: String,
    tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    assignee: Option<String>,
    provenance: Vec<String>,
    related: Vec<BTreeMap<String, String>>,
    /// Custom attributes are flattened to top-level frontmatter keys so the
    /// cache loader's `parse_with_schema` (which reads undeclared top-level keys)
    /// coerces them back to typed values on read-back.
    #[serde(flatten)]
    attributes: BTreeMap<String, AttrValue>,
}

/// Outcome of a mutation's push to a remote. Only `git-ref`-backed stores push
/// asynchronously after a local write (see `GitRefStore`); every other backend's
/// mutation is synced synchronously as part of the API call itself (a REST/
/// GraphQL write that either succeeds or the whole mutation errors), so those
/// backends always report `Synced`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PushOutcome {
    #[default]
    Synced,
    LocalOnly {
        warning: String,
    },
}

impl PushOutcome {
    pub fn is_synced(&self) -> bool {
        matches!(self, PushOutcome::Synced)
    }

    pub fn warning(&self) -> Option<&str> {
        match self {
            PushOutcome::Synced => None,
            PushOutcome::LocalOnly { warning } => Some(warning),
        }
    }
}

#[derive(Debug)]
pub struct CreatedDoc {
    pub path: PathBuf,
    pub id: String,
    pub push_outcome: PushOutcome,
}

pub trait DocumentStore: gh::AsAny {
    fn create(
        &mut self,
        type_def: &TypeDef,
        title: &str,
        author: &str,
        body: &str,
    ) -> Result<CreatedDoc>;

    fn update(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        updates: &[(&str, &str)],
    ) -> Result<PushOutcome>;

    fn delete(&mut self, type_def: &TypeDef, doc_id: &str) -> Result<PushOutcome>;

    fn set_provenance(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        provenance: &[String],
    ) -> Result<PushOutcome>;

    /// Propagate a tag mutation to the backend, mirroring
    /// [`gh::GhIssueWriter::issue_edit`]'s `(labels_add, labels_remove)` shape.
    /// The CLI has already rewritten the local frontmatter (the source of truth
    /// for filesystem docs, the cache for materialized backends); this is the
    /// remote half. Each backend must propagate or `bail!`, so a new backend is
    /// forced to make a tag decision at compile time rather than silently
    /// dropping the mutation.
    fn sync_tags(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        add: &[String],
        remove: &[String],
    ) -> Result<PushOutcome>;
}

pub struct FilesystemStore {
    pub root: PathBuf,
    pub config: Config,
}

/// A store placeholder registered for a backend that the current config cannot
/// serve (e.g. a GitHub backend with no `[github]` config). Every operation
/// fails with `message`, so a misconfigured type still errors clearly at
/// dispatch time rather than silently or with a generic "not registered".
struct UnavailableStore {
    message: String,
}

impl DocumentStore for UnavailableStore {
    fn create(&mut self, _: &TypeDef, _: &str, _: &str, _: &str) -> Result<CreatedDoc> {
        bail!("{}", self.message)
    }
    fn update(&mut self, _: &TypeDef, _: &str, _: &[(&str, &str)]) -> Result<PushOutcome> {
        bail!("{}", self.message)
    }
    fn delete(&mut self, _: &TypeDef, _: &str) -> Result<PushOutcome> {
        bail!("{}", self.message)
    }
    fn set_provenance(&mut self, _: &TypeDef, _: &str, _: &[String]) -> Result<PushOutcome> {
        bail!("{}", self.message)
    }
    fn sync_tags(
        &mut self,
        _: &TypeDef,
        _: &str,
        _: &[String],
        _: &[String],
    ) -> Result<PushOutcome> {
        bail!("{}", self.message)
    }
}

/// ClickUp-backed store, registered under [`StoreBackend::ClickupTasks`]. The
/// read path (fetch + materialize) lives in
/// [`clickup_cache`](crate::engine::clickup_cache); this struct carries the
/// boxed client, credential, and bindings the *write* path (create/update/
/// delete = archive) will use in later RFC-056 stories. Until those land, the
/// write methods fail loudly rather than silently no-op.
pub struct ClickupTasksStore {
    pub client: Box<dyn crate::engine::clickup::ClickupClient>,
    pub root: PathBuf,
    #[allow(dead_code)]
    pub config: Config,
    /// The ClickUp token the write path authenticates with. The registry leaves
    /// this `None` (so building the registry never touches the keychain); the
    /// CLI create branch loads it before dispatching a write.
    pub token: Option<crate::engine::credentials::Token>,
}

impl ClickupTasksStore {
    const WRITE_UNIMPLEMENTED: &'static str =
        "this clickup-tasks write path is not implemented yet (later RFC-056 story)";

    const NO_TOKEN: &'static str =
        "no ClickUp token found; run `lazyspec setup clickup` before creating \
         clickup-tasks documents";

    /// Optimistic-lock pre-write check (RFC-056 §Caching/id-mapping).
    ///
    /// Before a `PUT` (update or advance), re-fetch the task (`GET /task/{id}`)
    /// and compare ClickUp's current `date_updated` against the `TaskMap`
    /// baseline recorded at the last create/fetch/write. If the remote is
    /// *newer*, an external change landed since we synced -- proceeding would
    /// clobber it -- so reject with a conflict error and perform no write. The
    /// comparison is on integer epoch-ms, never string equality: ClickUp returns
    /// `date_updated` as an epoch-ms string (`"1774587145901"`), so a raw string
    /// compare would misorder unequal-length values.
    ///
    /// An empty or unparseable baseline means there is no timestamp to race
    /// against (e.g. a create whose echo carried no `date_updated`); the check is
    /// skipped and the write proceeds, matching the GitHub store's "just pushed
    /// -> accept remote" posture.
    fn check_optimistic_lock(
        &self,
        token: &str,
        doc_id: &str,
        task_id: &str,
        baseline: &str,
    ) -> Result<()> {
        let Ok(local_ms) = baseline.trim().parse::<i64>() else {
            return Ok(());
        };

        let remote = self.client.get_task(token, task_id)?;
        let remote_ms = remote.date_updated.unwrap_or(0);

        if remote_ms > local_ms {
            bail!(
                "{} changed on ClickUp since your last fetch; \
                 run `lazyspec fetch` and retry.\n  \
                 Local baseline: {}\n  \
                 Remote updated: {}",
                doc_id,
                local_ms,
                remote_ms,
            );
        }

        Ok(())
    }
}

impl DocumentStore for ClickupTasksStore {
    /// Create a task in the bound ClickUp List and mirror it into the local
    /// cache. Mirrors [`GithubIssuesStore::create`]: POST the remote task, use
    /// its returned id to form the doc id, materialize the echoed task into a
    /// cache file (reusing the read path's [`clickup_cache::task_to_doc`]), and
    /// record it in the [`TaskMap`] (with `date_updated` as the optimistic-lock
    /// baseline for later writes).
    fn create(
        &mut self,
        type_def: &TypeDef,
        title: &str,
        _author: &str,
        body: &str,
    ) -> Result<CreatedDoc> {
        let token = self
            .token
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("{}", Self::NO_TOKEN))?;
        let list_id = type_def.clickup_list_id.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "type '{}' is clickup-tasks but has no clickup_list_id configured",
                type_def.name
            )
        })?;

        // A create carries no attributes or status yet, so only name + body are
        // sent; ClickUp assigns the List's default status. The type's configured
        // task type (if any) stamps the new task's custom_item_id.
        let payload = clickup_cache::build_task_create(
            title,
            body,
            None,
            &BTreeMap::new(),
            type_def.clickup_task_type,
        );
        let task = self.client.create_task(token.expose(), list_id, &payload)?;

        let id = type_def.make_id(&task.id);
        let (meta, doc_body) = clickup_cache::task_to_doc(&task, type_def, &id);
        write_cache_file(&self.root, type_def, &meta, &doc_body)?;

        let updated_at = task
            .date_updated
            .map(|ms| ms.to_string())
            .unwrap_or_default();
        let mut task_map = TaskMap::load(&self.root)?;
        task_map.insert(&id, &task.id, updated_at);
        task_map.save(&self.root)?;

        let cache_path = self
            .root
            .join(".lazyspec/cache")
            .join(&type_def.name)
            .join(format!("{}.md", id));
        let relative = cache_path
            .strip_prefix(&self.root)
            .unwrap_or(&cache_path)
            .to_path_buf();
        Ok(CreatedDoc {
            path: relative,
            id,
            push_outcome: PushOutcome::Synced,
        })
    }

    /// Edit the bound ClickUp task from a doc's `update` and re-materialize the
    /// change into the cache -- the native-field round-trip (RFC-056 §Field
    /// mapping). Resolve the task id from the [`TaskMap`], map the changed
    /// fields to a partial [`TaskUpdate`], `PUT /task/{id}`, then rewrite the
    /// cache file and `TaskMap.updated_at` from the *returned* task so a
    /// subsequent read reflects the new native values. A `status` change (an
    /// `advance`) rides the same path: it maps to the payload's raw status
    /// string, is `PUT` verbatim, and re-materializes from ClickUp's echo
    /// (RFC-056 §Status handling); lazyspec applies no local transition gate.
    fn update(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        updates: &[(&str, &str)],
    ) -> Result<PushOutcome> {
        let token = self
            .token
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("{}", Self::NO_TOKEN))?;

        let mut task_map = TaskMap::load(&self.root)?;
        let (task_id, baseline) = task_map
            .get(doc_id)
            .map(|e| (e.task_id.clone(), e.updated_at.clone()))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "{} is not mapped to a ClickUp task; run `lazyspec fetch` before updating",
                    doc_id
                )
            })?;

        // Optimistic lock: reject before the PUT if ClickUp has moved on since
        // our recorded baseline, so a stale local doc never clobbers a concurrent
        // external change. Applies equally to an `advance` (a status-only update).
        self.check_optimistic_lock(token.expose(), doc_id, &task_id, &baseline)?;

        let payload = clickup_cache::build_task_update(updates);
        let task = self
            .client
            .update_task(token.expose(), &task_id, &payload)?;

        // Re-materialize from the task ClickUp echoed back: the round-trip AC
        // wants the cache (and a subsequent read) to reflect the updated native
        // fields, and this keeps the same doc id.
        let id = doc_id.to_string();
        let (meta, doc_body) = clickup_cache::task_to_doc(&task, type_def, &id);
        write_cache_file(&self.root, type_def, &meta, &doc_body)?;

        let updated_at = task
            .date_updated
            .map(|ms| ms.to_string())
            .unwrap_or_default();
        task_map.insert(&id, &task.id, updated_at);
        task_map.save(&self.root)?;

        Ok(PushOutcome::Synced)
    }
    /// Delete a ClickUp-backed doc by *archiving* its task (RFC-056 §Design):
    /// resolve the task id from the [`TaskMap`] and `PUT /task/{id}` with
    /// `{"archived": true}` -- never a hard delete. The cache file and the
    /// `TaskMap` entry are deliberately *not* evicted here: an archived task
    /// drops out of `task_list`, so the next `fetch` (which owns the whole type
    /// dir) removes the doc from the cache and the map. Eagerly deleting them now
    /// would only race that authoritative sync.
    fn delete(&mut self, _type_def: &TypeDef, doc_id: &str) -> Result<PushOutcome> {
        let token = self
            .token
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("{}", Self::NO_TOKEN))?;

        let task_map = TaskMap::load(&self.root)?;
        let task_id = task_map
            .get(doc_id)
            .map(|e| e.task_id.clone())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "{} is not mapped to a ClickUp task; run `lazyspec fetch` before deleting",
                    doc_id
                )
            })?;

        self.client.archive_task(token.expose(), &task_id)?;

        Ok(PushOutcome::Synced)
    }
    fn set_provenance(&mut self, _: &TypeDef, _: &str, _: &[String]) -> Result<PushOutcome> {
        bail!("{}", Self::WRITE_UNIMPLEMENTED)
    }
    /// The ClickUp tag write path (task tags) is not implemented yet, so fail
    /// loudly at the trait seam rather than silently dropping the mutation.
    /// Deferred to the RFC-056 write path.
    fn sync_tags(
        &mut self,
        _: &TypeDef,
        _: &str,
        _: &[String],
        _: &[String],
    ) -> Result<PushOutcome> {
        bail!("{}", Self::WRITE_UNIMPLEMENTED)
    }
}

impl DocumentStore for FilesystemStore {
    fn create(
        &mut self,
        type_def: &TypeDef,
        title: &str,
        author: &str,
        _body: &str,
    ) -> Result<CreatedDoc> {
        let path = crate::engine::fs_ops::create_document(
            &self.root,
            &self.config,
            &type_def.name,
            &type_def.dir,
            &type_def.prefix,
            title,
            author,
            &type_def.numbering,
            type_def.subdirectory,
            |_| {},
        )?;

        let relative = path.strip_prefix(&self.root).unwrap_or(&path).to_path_buf();
        let id = crate::engine::store::extract_id_from_name(
            relative.file_stem().and_then(|s| s.to_str()).unwrap_or(""),
        );

        Ok(CreatedDoc {
            path: relative,
            id,
            push_outcome: PushOutcome::Synced,
        })
    }

    fn update(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        updates: &[(&str, &str)],
    ) -> Result<PushOutcome> {
        let store = Store::load(&self.root, &self.config)?;
        crate::engine::fs_ops::update_document_with_type(
            &self.root,
            &store,
            doc_id,
            updates,
            Some(type_def),
        )
        .map(|()| PushOutcome::Synced)
    }

    fn delete(&mut self, _type_def: &TypeDef, doc_id: &str) -> Result<PushOutcome> {
        let store = Store::load(&self.root, &self.config)?;
        crate::engine::fs_ops::delete_document(&self.root, &store, doc_id)
            .map(|()| PushOutcome::Synced)
    }

    fn set_provenance(
        &mut self,
        _type_def: &TypeDef,
        doc_id: &str,
        provenance: &[String],
    ) -> Result<PushOutcome> {
        let store = Store::load(&self.root, &self.config)?;
        let doc = store
            .get(std::path::Path::new(doc_id))
            .or_else(|| store.resolve_shorthand(doc_id).ok())
            .ok_or_else(|| anyhow::anyhow!("could not resolve document: {}", doc_id))?;
        let full_path = self.root.join(&doc.path);

        let entries: Vec<serde_yaml::Value> = provenance
            .iter()
            .map(|s| serde_yaml::Value::String(s.clone()))
            .collect();

        crate::engine::document::rewrite_frontmatter(
            &full_path,
            &crate::engine::fs::RealFileSystem,
            |val| {
                let map = val
                    .as_mapping_mut()
                    .ok_or_else(|| anyhow::anyhow!("frontmatter root must be a mapping"))?;
                map.insert(
                    serde_yaml::Value::String("provenance".to_string()),
                    serde_yaml::Value::Sequence(entries.clone()),
                );
                Ok(())
            },
        )
        .map(|()| PushOutcome::Synced)
    }

    /// No-op: the on-disk document is the source of truth and the CLI already
    /// rewrote its frontmatter `tags`. There is no remote to propagate to.
    fn sync_tags(
        &mut self,
        _: &TypeDef,
        _: &str,
        _: &[String],
        _: &[String],
    ) -> Result<PushOutcome> {
        Ok(PushOutcome::Synced)
    }
}

/// The Projects v2 board numbers a doc is a member of, read from its `related`
/// relations whose relationship declares `github_native = "membership"`.
/// Targets are board doc ids (`PROJECT-n`).
fn membership_board_numbers(config: &Config, meta: &DocMeta) -> Vec<u64> {
    meta.related
        .iter()
        .filter(|r| {
            config
                .relationship_by_name(&r.rel_type.to_string())
                .and_then(|rel| rel.github_native.as_deref())
                == Some("membership")
        })
        .filter_map(|r| board_number(&r.target).ok())
        .collect()
}

/// Read the per-item project field values for `meta` across every board it is a
/// member of and inject them into `meta.attributes` keyed
/// `PROJECT-{number}.{field_name}`. Standalone (no store) so read-path callers
/// that only hold a borrowed graphql client (e.g. the fetch loop) can inject
/// without owning a client. A missing node id or zero memberships is a no-op.
pub fn inject_project_fields_for_meta(
    client: &dyn GhGraphql,
    repo: &str,
    issue_map: &IssueMap,
    config: &Config,
    meta: &mut DocMeta,
) -> Result<()> {
    let boards = membership_board_numbers(config, meta);
    if boards.is_empty() {
        return Ok(());
    }
    let Some(node_id) = issue_map
        .get(&meta.id)
        .map(|e| e.node_id.clone())
        .filter(|n| !n.is_empty())
    else {
        return Ok(());
    };

    let values = client.project_item_fields(repo, &node_id)?;
    for v in &values {
        if !boards.contains(&v.project_number) {
            continue;
        }
        let key = format!("PROJECT-{}.{}", v.project_number, v.field_name);
        meta.attributes.insert(key, gh::gh_field_to_attr(&v.value));
    }
    Ok(())
}

pub struct GithubIssuesStore {
    pub client: Box<dyn GhClient>,
    pub root: PathBuf,
    pub repo: String,
    pub config: Config,
    pub issue_map: IssueMap,
    pub issue_cache: IssueCache,
}

impl GithubIssuesStore {
    /// Downcast the boxed client to a concrete mock for test assertions.
    #[cfg(test)]
    fn mock(&self) -> &crate::engine::gh::test_support::MockGhClient {
        (*self.client)
            .as_any()
            .downcast_ref::<crate::engine::gh::test_support::MockGhClient>()
            .expect("client is a MockGhClient")
    }

    /// Push the current cache file content to GitHub.
    ///
    /// Reads the cache file for `doc_id`, parses its frontmatter and body,
    /// re-serializes into the GitHub issue body format, and pushes via
    /// `issue_edit`. Used after local cache writes (e.g. link/unlink) to
    /// sync relationship changes back to GitHub.
    pub fn push_cache(&mut self, type_def: &TypeDef, doc_id: &str) -> Result<()> {
        let cache_dir = self.root.join(".lazyspec/cache").join(&type_def.name);
        let cache_path = find_cache_file(&cache_dir, doc_id)
            .ok_or_else(|| anyhow::anyhow!("cache file not found for {}", doc_id))?;
        let content = std::fs::read_to_string(&cache_path)?;
        let meta = DocMeta::parse(&content)?;
        let body = DocMeta::extract_body(&content)?;

        let (issue_number, _remote_issue) = self.check_lock(doc_id)?;

        let new_body = issue_body::serialize(&meta, &body);
        self.client
            .issue_edit(&self.repo, issue_number, None, Some(&new_body), &[], &[])?;

        let node_id = self.existing_node_id(doc_id);
        self.issue_map.insert(doc_id, issue_number, "", node_id);
        self.issue_map.save(&self.root)?;
        self.issue_cache.touch_lock(doc_id)?;

        Ok(())
    }

    /// Re-mirror the cache body to GitHub after a native-relation edge write
    /// (milestone / membership) WITHOUT the optimistic body lock.
    ///
    /// Native relations are last-write-wins: the field PATCH (e.g. milestone
    /// association, Projects v2 membership) has already been applied and is
    /// authoritative. This re-pushes the known-good local cache body the caller
    /// just wrote, so an out-of-band `updated_at` advance (e.g. a remote comment)
    /// cannot abort the mirror after the native PATCH already landed -- avoiding
    /// the half-applied state where the remote edge exists but the cache and
    /// `updated_at` baseline never reconcile.
    ///
    /// Unlike [`Self::push_cache`], this records the remote's CURRENT
    /// `updated_at` (rather than clearing it) so the next ordinary body write has
    /// an accurate lock baseline.
    pub fn resync_after_native_edge(&mut self, type_def: &TypeDef, doc_id: &str) -> Result<()> {
        let cache_dir = self.root.join(".lazyspec/cache").join(&type_def.name);
        let cache_path = find_cache_file(&cache_dir, doc_id)
            .ok_or_else(|| anyhow::anyhow!("cache file not found for {}", doc_id))?;
        let content = std::fs::read_to_string(&cache_path)?;
        let meta = DocMeta::parse(&content)?;
        let body = DocMeta::extract_body(&content)?;

        let issue_number = self
            .issue_map
            .get(doc_id)
            .map(|e| e.issue_number)
            .ok_or_else(|| anyhow::anyhow!("{} not found in issue map", doc_id))?;

        // Read the remote ONCE to capture its current timestamp. Last-write-wins:
        // the remote is authoritative for `updated_at`; we never reject on it.
        let remote_issue = self.client.issue_view(&self.repo, issue_number)?;

        let new_body = issue_body::serialize(&meta, &body);
        self.client
            .issue_edit(&self.repo, issue_number, None, Some(&new_body), &[], &[])?;

        self.issue_map.insert(
            doc_id,
            issue_number,
            &remote_issue.updated_at,
            &remote_issue.id,
        );
        self.issue_map.save(&self.root)?;
        self.issue_cache.touch_lock(doc_id)?;

        Ok(())
    }

    /// Merge a single ordinary (non-native) issue-to-issue relation delta into
    /// the remote issue body WITHOUT the optimistic body lock.
    ///
    /// Ordinary relations (`implements`, `blocks`, ...) round-trip through the
    /// GitHub issue body's `related:` block. A whole-cache [`Self::push_cache`]
    /// would clobber the remote body (prose + any remote-added relations) with
    /// stale local cache, and its [`Self::check_lock`] rejects on an unrelated
    /// out-of-band `updated_at` bump (e.g. a remote comment) even though the
    /// relation edit does not contend the body.
    ///
    /// Instead this reads the remote body ONCE, applies just the `(rel, target)`
    /// delta to its `related` (insert-if-absent on `set`, retain-drop on unset),
    /// preserves the remote prose, and pushes the merged body. `set == true`
    /// with the relation already present is a no-op (dedup): no `issue_edit`.
    /// Like [`Self::resync_after_native_edge`] it records the remote's current
    /// `updated_at` and never rejects on it.
    pub fn merge_relation_to_remote(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        rel_str: &str,
        target_id: &str,
        set: bool,
    ) -> Result<()> {
        let issue_number = self
            .issue_map
            .get(doc_id)
            .map(|e| e.issue_number)
            .ok_or_else(|| anyhow::anyhow!("{} not found in issue map", doc_id))?;

        let remote_issue = self.client.issue_view(&self.repo, issue_number)?;

        let ctx = issue_body::IssueContext {
            title: remote_issue.title.clone(),
            labels: remote_issue.labels.iter().map(|l| l.name.clone()).collect(),
            is_open: remote_issue.state == "OPEN",
            known_types: self
                .config
                .documents
                .types
                .iter()
                .map(issue_body::TypeMatchRule::from)
                .collect(),
            issue_type: remote_issue.issue_type.clone(),
            default_type: type_def.name.clone(),
            attr_defs: type_def.attributes.clone(),
            open_status: type_def
                .effective_lifecycle()
                .first_active_status()
                .to_string(),
            closed_status: type_def.effective_lifecycle().terminal_status().to_string(),
        };
        // A body without a lazyspec comment (GitHub-authored issue) is adopted:
        // synthesize meta from the issue's own fields and keep the whole body
        // as prose, so this write plants the comment.
        let (mut remote_meta, remote_prose) = issue_body::deserialize(&remote_issue.body, &ctx)
            .unwrap_or_else(|_| {
                (
                    issue_cache::fallback_meta(&remote_issue, &ctx),
                    remote_issue.body.clone(),
                )
            });

        let rel_type = crate::engine::document::RelationType::new(rel_str);
        let already_present = remote_meta
            .related
            .iter()
            .any(|r| r.rel_type == rel_type && r.target == target_id);

        if set {
            // Dedup: an already-present relation is a no-op -- no remote write.
            if already_present {
                return Ok(());
            }
            remote_meta.related.push(crate::engine::document::Relation {
                rel_type,
                target: target_id.to_string(),
            });
        } else {
            remote_meta
                .related
                .retain(|r| !(r.rel_type == rel_type && r.target == target_id));
        }

        let new_body = issue_body::serialize(&remote_meta, &remote_prose);
        self.client
            .issue_edit(&self.repo, issue_number, None, Some(&new_body), &[], &[])?;

        self.issue_map.insert(
            doc_id,
            issue_number,
            &remote_issue.updated_at,
            &remote_issue.id,
        );
        self.issue_map.save(&self.root)?;
        self.issue_cache.touch_lock(doc_id)?;

        Ok(())
    }

    /// The currently-mapped GraphQL node id for `doc_id`, or empty when unknown.
    /// Used to preserve the node id across re-inserts that only clear
    /// `updated_at`.
    fn existing_node_id(&self, doc_id: &str) -> String {
        self.issue_map
            .get(doc_id)
            .map(|e| e.node_id.clone())
            .unwrap_or_default()
    }

    /// Fetch the remote issue and check the optimistic lock.
    ///
    /// If `updated_at` is empty (we just pushed), accept the remote state and
    /// record its timestamp. Otherwise, reject if the remote has been modified
    /// since our last fetch.
    fn check_lock(&mut self, doc_id: &str) -> Result<(u64, gh::GhIssue)> {
        let entry = self
            .issue_map
            .get(doc_id)
            .ok_or_else(|| anyhow::anyhow!("{} not found in issue map", doc_id))?;
        let issue_number = entry.issue_number;
        let local_updated_at = entry.updated_at.clone();

        let remote_issue = self.client.issue_view(&self.repo, issue_number)?;

        if local_updated_at.is_empty() {
            // We pushed recently; accept remote state and record timestamp.
            self.issue_map.insert(
                doc_id,
                issue_number,
                &remote_issue.updated_at,
                &remote_issue.id,
            );
            self.issue_map.save(&self.root)?;
        } else if remote_issue.updated_at != local_updated_at {
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

        Ok((issue_number, remote_issue))
    }

    /// Push the native issue-type to GitHub via a single `updateIssue` mutation.
    /// `type_id` is `Some(id)` to set the type or `None` to clear it
    /// (`issueTypeId: null`). The issue node id is resolved over GraphQL.
    fn push_issue_type(&self, issue_number: u64, type_id: Option<&str>) -> Result<()> {
        let (owner, name) = split_owner_repo(&self.repo)?;
        let id_resp = self.client.graphql(
            ISSUE_NODE_ID_QUERY,
            &[
                ("owner", GqlVar::Str(owner.to_string())),
                ("name", GqlVar::Str(name.to_string())),
                ("number", GqlVar::Int(issue_number as i64)),
            ],
        )?;
        let issue_id = id_resp
            .pointer("/data/repository/issue/id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                anyhow::anyhow!("could not resolve issue node id for #{}", issue_number)
            })?
            .to_string();

        match type_id {
            Some(id) => {
                self.client.graphql(
                    UPDATE_ISSUE_TYPE_MUTATION,
                    &[
                        ("issueId", GqlVar::Str(issue_id)),
                        ("issueTypeId", GqlVar::Str(id.to_string())),
                    ],
                )?;
            }
            None => {
                // gh cannot pass a null variable, so the clear value is inlined.
                self.client.graphql(
                    CLEAR_ISSUE_TYPE_MUTATION,
                    &[("issueId", GqlVar::Str(issue_id))],
                )?;
            }
        }
        Ok(())
    }

    /// Materialize a subdirectory document type (`index.md` parent + sibling
    /// child `.md` files) into GitHub issues, closing the silent-drop gap where
    /// only the `index.md` parent reached GitHub.
    ///
    /// Loads the filesystem [`Store`] to resolve the parent's `index.md` path
    /// and its path-sorted children (loader order). For the parent and each
    /// child not yet in the issue map, runs the create steps
    /// (label_ensure -> issue_create -> issue_map.insert -> write_cache_file).
    /// Returns the parent issue number + node id and the ordered children, ready
    /// to feed a [`crate::engine::gh_subissue::SubIssuePlan`].
    pub fn materialize_subdir(
        &mut self,
        type_def: &TypeDef,
        parent_doc_id: &str,
    ) -> Result<MaterializeResult> {
        let store = self.load_source_store(type_def)?;
        let parent_meta = store
            .resolve_shorthand(parent_doc_id)
            .ok()
            .or_else(|| store.resolve_relation_target(parent_doc_id))
            .ok_or_else(|| anyhow::anyhow!("could not resolve subdir parent: {}", parent_doc_id))?;
        let parent_path = parent_meta.path.clone();

        let (parent_issue, parent_node) =
            self.materialize_one(type_def, parent_doc_id, parent_meta)?;

        let child_paths: Vec<PathBuf> = store.children_of(&parent_path).to_vec();
        let mut children = Vec::new();
        for (order_index, child_path) in child_paths.iter().enumerate() {
            let child_meta = store
                .get(child_path)
                .ok_or_else(|| anyhow::anyhow!("child doc vanished: {}", child_path.display()))?;
            let child_id = child_meta.id.clone();
            let (issue_number, node_id) = self.materialize_one(type_def, &child_id, child_meta)?;
            children.push(MaterializedChild {
                child_id,
                issue_number,
                node_id,
                order_index,
            });
        }

        Ok(MaterializeResult {
            parent_id: parent_doc_id.to_string(),
            parent_issue,
            parent_node,
            children,
        })
    }

    /// Load a [`Store`] over the type's *source* directory (`type_def.dir`).
    /// `Store::load` routes github-issues types to the cache dir, but subdir
    /// children are authored on disk in the source dir; this scans there so the
    /// loader sees the `index.md` parent and its sibling children.
    fn load_source_store(&self, type_def: &TypeDef) -> Result<Store> {
        let mut source_config = self.config.clone();
        for td in &mut source_config.documents.types {
            if td.name == type_def.name {
                td.store = StoreBackend::Filesystem;
            }
        }
        Store::load(&self.root, &source_config)
    }

    /// Ensure a single doc is on GitHub: reuse its existing issue when already
    /// mapped, otherwise create one (label_ensure -> issue_create ->
    /// issue_map.insert -> write_cache_file). Returns `(issue_number, node_id)`.
    fn materialize_one(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        meta: &DocMeta,
    ) -> Result<(u64, String)> {
        if let Some(entry) = self.issue_map.get(doc_id) {
            return Ok((entry.issue_number, entry.node_id.clone()));
        }

        let body = std::fs::read_to_string(self.root.join(&meta.path))
            .ok()
            .and_then(|c| DocMeta::extract_body(&c).ok())
            .unwrap_or_default();

        let issue_body = issue_body::serialize(meta, &body);
        let labels = type_def.github_create_labels();
        let color = gh::deterministic_color(&type_def.name);
        let description = format!("lazyspec document type: {}", type_def.name);
        for label in &labels {
            self.client
                .label_ensure(&self.repo, label, &description, &color)?;
        }
        let issue = self
            .client
            .issue_create(&self.repo, &meta.title, &issue_body, &labels)?;

        let materialized_meta = DocMeta {
            id: doc_id.to_string(),
            ..meta.clone()
        };
        self.issue_map
            .insert(doc_id, issue.number, &issue.updated_at, &issue.id);
        self.issue_map.save(&self.root)?;
        write_cache_file(&self.root, type_def, &materialized_meta, &body)?;
        self.issue_cache.touch_lock(doc_id)?;

        Ok((issue.number, issue.id))
    }

    /// Read the per-item project field values for this doc across every board it
    /// is a member of, and inject them into `meta.attributes` keyed
    /// `PROJECT-{number}.{field_name}`. The issue's GraphQL node id comes from
    /// the issue map; a missing node id or zero memberships is a no-op. Values
    /// are coerced [`AttrValue`]s (never `Raw`); the board number namespaces the
    /// key so the same field name on two boards cannot collide.
    pub fn inject_project_fields(&self, meta: &mut DocMeta) -> Result<()> {
        inject_project_fields_for_meta(
            self.client.as_graphql(),
            &self.repo,
            &self.issue_map,
            &self.config,
            meta,
        )
    }

    /// Write (or clear) one `PROJECT-{number}.{field}` project field for the
    /// issue. Resolution order, all ids never names:
    ///   1. field id (+ option/iteration id) FROM the `gh-schema.json` snapshot
    ///      offline -- an unknown option/iteration rejects here BEFORE any
    ///      mutation;
    ///   2. project node id -- reused from the issue map binding for `PROJECT-n`
    ///      when present, otherwise a fresh org/user lookup;
    ///   3. project item id for this issue on that board (live);
    ///   4. `updateProjectV2ItemFieldValue`, or `clearProjectV2ItemFieldValue`
    ///      for an empty value (a distinct mutation, never an empty-string text).
    fn set_project_field(
        &self,
        content_node_id: &str,
        project_number: u64,
        field_name: &str,
        value: &str,
    ) -> Result<()> {
        let snapshot = GhSchemaSnapshot::load(&self.root);
        let field_id = snapshot
            .field_id(project_number, field_name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "unknown project field '{}' on board #{} (refresh the schema snapshot)",
                    field_name,
                    project_number
                )
            })?
            .to_string();
        let data_type = snapshot
            .project_fields
            .iter()
            .find(|f| f.project_number == project_number && f.field_name == field_name)
            .map(|f| f.data_type.clone())
            .unwrap_or_default();

        // Empty value clears the field, regardless of kind. Resolve ids and
        // dispatch the distinct clear mutation.
        let input: Option<gh::GhFieldValueInput> = if value.is_empty() {
            None
        } else {
            Some(self.resolve_field_value_input(&snapshot, &field_id, &data_type, value)?)
        };

        let project_id = self.resolve_project_node_id(project_number)?;
        let item_id = self.resolve_project_item_id(content_node_id, &project_id)?;

        match input {
            Some(v) => {
                self.client
                    .update_project_v2_item_field_value(&project_id, &item_id, &field_id, &v)
            }
            None => self
                .client
                .clear_project_field(&project_id, &item_id, &field_id),
        }
    }

    /// Build the typed [`gh::GhFieldValueInput`] for a field write, resolving
    /// single-select option / iteration ids FROM the snapshot (offline). An
    /// unknown option/iteration is an error here, before any mutation.
    fn resolve_field_value_input(
        &self,
        snapshot: &GhSchemaSnapshot,
        field_id: &str,
        data_type: &str,
        value: &str,
    ) -> Result<gh::GhFieldValueInput> {
        match data_type {
            "SINGLE_SELECT" => {
                let opt = snapshot.option_id(field_id, value).ok_or_else(|| {
                    anyhow::anyhow!(
                        "unknown option '{}' for project field (not in schema snapshot)",
                        value
                    )
                })?;
                Ok(gh::GhFieldValueInput::SingleSelect(opt.to_string()))
            }
            "ITERATION" => {
                let iter = snapshot.iteration_id(field_id, value).ok_or_else(|| {
                    anyhow::anyhow!(
                        "unknown iteration '{}' for project field (not in schema snapshot)",
                        value
                    )
                })?;
                Ok(gh::GhFieldValueInput::Iteration(iter.to_string()))
            }
            "NUMBER" => {
                let n: f64 = value.parse().map_err(|_| {
                    anyhow::anyhow!("project number field expects a number: '{}'", value)
                })?;
                Ok(gh::GhFieldValueInput::Number(n))
            }
            "DATE" => {
                let d = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| {
                    anyhow::anyhow!("project date field expects YYYY-MM-DD: '{}'", value)
                })?;
                Ok(gh::GhFieldValueInput::Date(d))
            }
            // TEXT and any other text-shaped field.
            _ => Ok(gh::GhFieldValueInput::Text(value.to_string())),
        }
    }

    /// The project node id for `project_number`: reused from the issue map
    /// binding (`PROJECT-n`) when present, else a fresh org-then-user lookup.
    fn resolve_project_node_id(&self, project_number: u64) -> Result<String> {
        let board_doc_id = format!("PROJECT-{}", project_number);
        if let Some(id) = self
            .issue_map
            .get(&board_doc_id)
            .map(|e| e.node_id.clone())
            .filter(|n| !n.is_empty())
        {
            return Ok(id);
        }
        let owner = owner_of(&self.repo)?;
        resolve_project_id_live(self.client.as_graphql(), owner, project_number)
    }

    /// The project item id for the issue (`content_node_id`) on the board with
    /// node id `project_id`, looked up live over GraphQL.
    fn resolve_project_item_id(&self, content_node_id: &str, project_id: &str) -> Result<String> {
        if content_node_id.is_empty() {
            bail!("issue has no GitHub node id; cannot resolve its project item");
        }
        let resp = self.client.graphql(
            PROJECT_ITEM_ID_QUERY,
            &[("id", GqlVar::Str(content_node_id.to_string()))],
        )?;
        resp.pointer("/data/node/projectItems/nodes")
            .and_then(|v| v.as_array())
            .and_then(|arr| {
                arr.iter().find_map(|n| {
                    let pid = n.pointer("/project/id").and_then(|v| v.as_str())?;
                    if pid == project_id {
                        n.get("id").and_then(|v| v.as_str()).map(String::from)
                    } else {
                        None
                    }
                })
            })
            .ok_or_else(|| anyhow::anyhow!("issue is not an item on board {}", project_id))
    }

    /// Materialize a subdir parent + children, then reconcile their native
    /// sub-issue links over GraphQL. The single entry point used by both the
    /// `create` path and cache refresh of subdir types.
    pub fn sync_subissues(
        &mut self,
        type_def: &TypeDef,
        parent_doc_id: &str,
    ) -> Result<MaterializeResult> {
        let result = self.materialize_subdir(type_def, parent_doc_id)?;
        let plan = result.to_plan(type_def);
        crate::engine::gh_subissue::reconcile_subissues(
            self.client.as_graphql(),
            &self.repo,
            &plan,
        )?;
        Ok(result)
    }

    /// Create a child as a real GitHub issue and bind it as a native sub-issue
    /// of `parent_doc_id` at create time. The parent must already be a github
    /// issue (it is itself a github-issues doc); an unmapped parent aborts before
    /// the bind. The add is idempotent via
    /// [`crate::engine::gh_subissue::reconcile_subissues`], which fetches the
    /// remote sub-issue set and only issues the missing edge.
    pub fn create_child_subissue(
        &mut self,
        type_def: &TypeDef,
        parent_doc_id: &str,
        title: &str,
        author: &str,
        body: &str,
    ) -> Result<CreatedDoc> {
        let parent_node = self
            .issue_map
            .get(parent_doc_id)
            .map(|e| e.node_id.clone())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "parent {} is not a github issue (no issue-map entry); \
                     cannot bind a native sub-issue",
                    parent_doc_id
                )
            })?;

        let created = self.create(type_def, title, author, body)?;

        let child_node = self
            .issue_map
            .get(&created.id)
            .map(|e| e.node_id.clone())
            .ok_or_else(|| {
                anyhow::anyhow!("child {} missing from issue map after create", created.id)
            })?;

        let plan = crate::engine::gh_subissue::SubIssuePlan {
            parent_id: parent_doc_id.to_string(),
            parent_node,
            parent_store: type_def.store.clone(),
            children: vec![crate::engine::gh_subissue::PlannedChild {
                doc_id: created.id.clone(),
                node_id: child_node,
                store: type_def.store.clone(),
                order_index: 0,
            }],
        };
        crate::engine::gh_subissue::reconcile_subissues(
            self.client.as_graphql(),
            &self.repo,
            &plan,
        )?;

        Ok(created)
    }
}

/// One structural child materialized by [`GithubIssuesStore::materialize_subdir`].
#[derive(Debug, Clone, PartialEq)]
pub struct MaterializedChild {
    pub child_id: String,
    pub issue_number: u64,
    pub node_id: String,
    pub order_index: usize,
}

/// Outcome of materializing a subdir parent and its children into GitHub issues.
#[derive(Debug, Clone)]
pub struct MaterializeResult {
    pub parent_id: String,
    pub parent_issue: u64,
    pub parent_node: String,
    pub children: Vec<MaterializedChild>,
}

impl MaterializeResult {
    /// Build the sub-issue reconcile plan for this materialization. The parent
    /// and its structural children all belong to the same subdir `type_def`, so
    /// they share its [`StoreBackend`]; the same-store guard in
    /// [`crate::engine::gh_subissue::reconcile_subissues`] enforces that.
    pub fn to_plan(&self, type_def: &TypeDef) -> crate::engine::gh_subissue::SubIssuePlan {
        use crate::engine::gh_subissue::{PlannedChild, SubIssuePlan};
        SubIssuePlan {
            parent_id: self.parent_id.clone(),
            parent_node: self.parent_node.clone(),
            parent_store: type_def.store.clone(),
            children: self
                .children
                .iter()
                .map(|c| PlannedChild {
                    doc_id: c.child_id.clone(),
                    node_id: c.node_id.clone(),
                    store: type_def.store.clone(),
                    order_index: c.order_index,
                })
                .collect(),
        }
    }
}

const ISSUE_NODE_ID_QUERY: &str = "query($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) { issue(number: $number) { id } } }";

const UPDATE_ISSUE_TYPE_MUTATION: &str = "mutation($issueId: ID!, $issueTypeId: ID!) { updateIssue(input: {id: $issueId, issueTypeId: $issueTypeId}) { issue { id } } }";

const CLEAR_ISSUE_TYPE_MUTATION: &str = "mutation($issueId: ID!) { updateIssue(input: {id: $issueId, issueTypeId: null}) { issue { id } } }";

fn split_owner_repo(repo: &str) -> Result<(&str, &str)> {
    repo.split_once('/')
        .filter(|(o, n)| !o.is_empty() && !n.is_empty())
        .ok_or_else(|| anyhow::anyhow!("repo '{}' must be in owner/name form", repo))
}

/// Parse a `PROJECT-{number}.{field}` attribute key into `(number, field)`.
/// Returns `None` for any key that does not match the namespaced project-field
/// shape, so ordinary attributes fall through to the body round-trip.
pub fn parse_project_field_key(key: &str) -> Option<(u64, &str)> {
    let rest = key.strip_prefix("PROJECT-")?;
    let (num_str, field) = rest.split_once('.')?;
    if field.is_empty() {
        return None;
    }
    let number = num_str.parse::<u64>().ok()?;
    Some((number, field))
}

const PROJECT_ITEM_ID_QUERY: &str = "query($id: ID!) { node(id: $id) { ... on Issue { projectItems(first: 100) { nodes { id project { id } } } } } }";

/// Live org-then-user resolve of a Projects v2 board number to its node id.
/// Used by the issue store's field-write path when the issue map has no cached
/// board binding.
fn resolve_project_id_live(client: &dyn GhGraphql, owner: &str, number: u64) -> Result<String> {
    let (_kind, id_node) = try_org_then_user(
        client,
        PROJECT_NODE_ID_ORG_QUERY,
        PROJECT_NODE_ID_USER_QUERY,
        &[
            ("owner", GqlVar::Str(owner.to_string())),
            ("number", GqlVar::Int(number as i64)),
        ],
        "/data/organization/projectV2/id",
        "/data/user/projectV2/id",
    )
    .map_err(|_| anyhow::anyhow!("Projects v2 board #{} not found under '{}'", number, owner))?;
    id_node
        .as_str()
        .map(String::from)
        .ok_or_else(|| anyhow::anyhow!("Projects v2 board #{} not found under '{}'", number, owner))
}

impl DocumentStore for GithubIssuesStore {
    fn create(
        &mut self,
        type_def: &TypeDef,
        title: &str,
        author: &str,
        body: &str,
    ) -> Result<CreatedDoc> {
        let cache_dir = self.root.join(".lazyspec/cache").join(&type_def.name);
        std::fs::create_dir_all(&cache_dir)?;

        // Create the GitHub issue first so the issue number becomes the doc ID.
        let date = Local::now().date_naive();
        let placeholder_meta = DocMeta {
            path: PathBuf::new(),
            title: title.to_string(),
            doc_type: DocType::new(&type_def.name),
            status: Status::new(type_def.effective_lifecycle().seed_status()),
            author: author.to_string(),
            date,
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            attributes: Default::default(),
            id: String::new(),
        };

        let issue_body = issue_body::serialize(&placeholder_meta, body);
        let labels = type_def.github_create_labels();
        let color = gh::deterministic_color(&type_def.name);
        let description = format!("lazyspec document type: {}", type_def.name);
        for label in &labels {
            self.client
                .label_ensure(&self.repo, label, &description, &color)?;
        }

        // Resolve the native issue-type offline before any remote write, so an
        // unresolvable name aborts before `issue_create` fires.
        let resolved_issue_type_id: Option<String> = match type_def.github_issue_type.as_deref() {
            Some(name) => {
                let snapshot = GhSchemaSnapshot::load(&self.root);
                let id = snapshot.issue_type_id(name).ok_or_else(|| {
                    if snapshot.issue_types.is_empty() {
                        let (owner, _) = split_owner_repo(&self.repo)
                            .unwrap_or((self.repo.as_str(), ""));
                        anyhow::anyhow!(
                            "native issue types require an organization-owned repository; '{}' has none",
                            owner
                        )
                    } else {
                        anyhow::anyhow!(
                            "invalid issue_type '{}': not a known GitHub issue type",
                            name
                        )
                    }
                })?;
                Some(id.to_string())
            }
            None => None,
        };

        let issue = self
            .client
            .issue_create(&self.repo, title, &issue_body, &labels)?;

        if let Some(resolved_id) = resolved_issue_type_id.as_deref() {
            self.push_issue_type(issue.number, Some(resolved_id))?;
        }

        // Use the GitHub issue number as the document number.
        let issue_num_str = issue.number.to_string();
        let filename = template::resolve_filename(
            &self.config.documents.naming.pattern,
            &type_def.prefix,
            title,
            &cache_dir,
            None,
            Some(&issue_num_str),
        )
        .map_err(|e| anyhow::anyhow!("{}", e))?;

        let stem = filename.trim_end_matches(".md");
        let id = store::extract_id_from_name(stem);

        let doc_meta = DocMeta {
            id: id.clone(),
            ..placeholder_meta
        };

        self.issue_map
            .insert(&id, issue.number, &issue.updated_at, &issue.id);
        self.issue_map.save(&self.root)?;

        write_cache_file(&self.root, type_def, &doc_meta, body)?;
        self.issue_cache.touch_lock(&id)?;

        // Subdir types: co-materialize sibling children into issues and bind
        // them as native sub-issues. Best-effort during create -- children may
        // not yet be on disk; cache refresh reconciles the settled state.
        if type_def.subdirectory {
            let _ = self.sync_subissues(type_def, &id);
        }

        let cache_path = self
            .root
            .join(".lazyspec/cache")
            .join(&type_def.name)
            .join(format!("{}.md", id));
        let relative = cache_path
            .strip_prefix(&self.root)
            .unwrap_or(&cache_path)
            .to_path_buf();
        Ok(CreatedDoc {
            path: relative,
            id,
            push_outcome: PushOutcome::Synced,
        })
    }

    fn update(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        updates: &[(&str, &str)],
    ) -> Result<PushOutcome> {
        let (issue_number, remote_issue) = self.check_lock(doc_id)?;

        let ctx = issue_body::IssueContext {
            title: remote_issue.title.clone(),
            labels: remote_issue.labels.iter().map(|l| l.name.clone()).collect(),
            is_open: remote_issue.state == "OPEN",
            known_types: self
                .config
                .documents
                .types
                .iter()
                .map(issue_body::TypeMatchRule::from)
                .collect(),
            issue_type: remote_issue.issue_type.clone(),
            default_type: type_def.name.clone(),
            attr_defs: type_def.attributes.clone(),
            open_status: type_def
                .effective_lifecycle()
                .first_active_status()
                .to_string(),
            closed_status: type_def.effective_lifecycle().terminal_status().to_string(),
        };
        let (mut meta, mut body) = issue_body::deserialize(&remote_issue.body, &ctx)?;

        let mut new_status: Option<Status> = None;
        let mut attr_updates: Vec<(&str, &str)> = Vec::new();
        let mut issue_type_update: Option<&str> = None;
        let mut assignee_update: Option<&str> = None;
        let mut project_field_updates: Vec<(u64, &str, &str)> = Vec::new();
        for &(key, value) in updates {
            // `PROJECT-n.<field>` is a per-board project field, not a body
            // attribute: route it to a GraphQL field mutation, never the HTML
            // -comment round-trip.
            if let Some((number, field)) = parse_project_field_key(key) {
                project_field_updates.push((number, field, value));
                continue;
            }
            match key {
                "status" => {
                    let s: Status = value.parse()?;
                    new_status = Some(s.clone());
                    meta.status = s;
                }
                "title" => meta.title = value.to_string(),
                "author" => meta.author = value.to_string(),
                "body" => body = value.to_string(),
                // The native issue-type lives in GitHub's `issueType` field, not
                // the issue-body HTML comment, so it is kept out of attr_updates.
                "issue_type" => issue_type_update = Some(value),
                // Assignee is a native GitHub field (`assignees`), written through
                // a dedicated `gh issue edit` mutation -- never the body comment.
                "assignee" => assignee_update = Some(value),
                _ => attr_updates.push((key, value)),
            }
        }

        // Project field writes are independent of the issue-body round-trip:
        // resolve ids + validate offline first, then mutate, then return without
        // touching the issue body or optimistic lock.
        if !project_field_updates.is_empty() {
            let content_node_id = self.existing_node_id(doc_id);
            for (number, field, value) in &project_field_updates {
                self.set_project_field(&content_node_id, *number, field, value)?;
            }
            if attr_updates.is_empty()
                && new_status.is_none()
                && issue_type_update.is_none()
                && assignee_update.is_none()
                && !updates.iter().any(|(k, _)| matches!(*k, "title" | "body"))
            {
                return Ok(PushOutcome::Synced);
            }
        }
        if !attr_updates.is_empty() {
            crate::engine::document::apply_attrs(type_def, &mut meta, &attr_updates)?;
        }

        // Resolve (and validate) the native issue-type offline before any remote
        // write, so an invalid value rejects without an issue_edit or mutation.
        // `Some(Some(id))` sets the type, `Some(None)` clears it, `None` leaves
        // it untouched.
        let issue_type_change: Option<Option<String>> = match issue_type_update {
            Some("") => Some(None),
            Some(name) => {
                let snapshot = GhSchemaSnapshot::load(&self.root);
                let id = snapshot.issue_type_id(name).ok_or_else(|| {
                    if snapshot.issue_types.is_empty() {
                        let (owner, _) = split_owner_repo(&self.repo)
                            .unwrap_or((self.repo.as_str(), ""));
                        anyhow::anyhow!(
                            "native issue types require an organization-owned repository; '{}' has none",
                            owner
                        )
                    } else {
                        anyhow::anyhow!(
                            "invalid issue_type '{}': not a known GitHub issue type",
                            name
                        )
                    }
                })?;
                Some(Some(id.to_string()))
            }
            None => None,
        };

        let new_body = issue_body::serialize(&meta, &body);
        self.client
            .issue_edit(&self.repo, issue_number, None, Some(&new_body), &[], &[])?;

        if let Some(status) = new_status {
            let should_be_open = issue_body::status_maps_to_open(
                status.as_str(),
                type_def.effective_lifecycle().terminal_status(),
            );
            let is_open = remote_issue.state == "OPEN";
            if should_be_open && !is_open {
                self.client.issue_reopen(&self.repo, issue_number)?;
            } else if !should_be_open && is_open {
                self.client.issue_close(&self.repo, issue_number)?;
            }
        }

        if let Some(type_id) = issue_type_change {
            self.push_issue_type(issue_number, type_id.as_deref())?;
        }

        // Assignee native edge (STORY-222 AC4): diff the requested assignee
        // against the remote's current one (first entry) and push only the delta
        // via `gh issue edit`, then reflect it locally. The remote `assignees`
        // field is the edge of record; the body comment never carries it.
        if let Some(value) = assignee_update {
            let requested = (!value.is_empty()).then(|| value.to_string());
            let current = remote_issue.assignees.first().map(|a| a.login.clone());
            if requested != current {
                let add: Vec<String> = requested.iter().cloned().collect();
                let remove: Vec<String> = current.into_iter().collect();
                self.client
                    .issue_set_assignee(&self.repo, issue_number, &add, &remove)?;
            }
            meta.assignee = requested;
        }

        // Clear updated_at: we just pushed, so our stored timestamp is stale.
        // The next edit's pre-flight fetch will record the fresh timestamp.
        let node_id = self.existing_node_id(doc_id);
        self.issue_map.insert(doc_id, issue_number, "", node_id);
        self.issue_map.save(&self.root)?;

        let meta = DocMeta {
            id: doc_id.to_string(),
            ..meta
        };
        write_cache_file(&self.root, type_def, &meta, &body)?;
        self.issue_cache.touch_lock(doc_id)?;

        Ok(PushOutcome::Synced)
    }

    fn set_provenance(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        provenance: &[String],
    ) -> Result<PushOutcome> {
        let (issue_number, remote_issue) = self.check_lock(doc_id)?;

        let ctx = issue_body::IssueContext {
            title: remote_issue.title.clone(),
            labels: remote_issue.labels.iter().map(|l| l.name.clone()).collect(),
            is_open: remote_issue.state == "OPEN",
            known_types: self
                .config
                .documents
                .types
                .iter()
                .map(issue_body::TypeMatchRule::from)
                .collect(),
            issue_type: remote_issue.issue_type.clone(),
            default_type: type_def.name.clone(),
            attr_defs: type_def.attributes.clone(),
            open_status: type_def
                .effective_lifecycle()
                .first_active_status()
                .to_string(),
            closed_status: type_def.effective_lifecycle().terminal_status().to_string(),
        };
        let (mut meta, body) = issue_body::deserialize(&remote_issue.body, &ctx)?;
        meta.provenance = provenance.to_vec();

        let new_body = issue_body::serialize(&meta, &body);
        self.client
            .issue_edit(&self.repo, issue_number, None, Some(&new_body), &[], &[])?;

        let node_id = self.existing_node_id(doc_id);
        self.issue_map.insert(doc_id, issue_number, "", node_id);
        self.issue_map.save(&self.root)?;

        let meta = DocMeta {
            id: doc_id.to_string(),
            ..meta
        };
        write_cache_file(&self.root, type_def, &meta, &body)?;
        self.issue_cache.touch_lock(doc_id)?;

        Ok(PushOutcome::Synced)
    }

    fn delete(&mut self, type_def: &TypeDef, doc_id: &str) -> Result<PushOutcome> {
        let (issue_number, remote_issue) = self.check_lock(doc_id)?;

        let deleted_title = format!("[DELETED] {}", remote_issue.title);
        let label = type_def.github_label();
        self.client.issue_edit(
            &self.repo,
            issue_number,
            Some(&deleted_title),
            None,
            &[],
            &[label],
        )?;

        self.client.issue_close(&self.repo, issue_number)?;

        self.issue_map.remove(doc_id);
        self.issue_map.save(&self.root)?;

        self.issue_cache.remove(doc_id, &type_def.name)?;

        Ok(PushOutcome::Synced)
    }

    /// Sync tag mutations to GitHub as issue labels.
    ///
    /// Ensures each added label exists (creating it with a deterministic color),
    /// then applies the add/remove label deltas via `issue_edit`, and touches the
    /// cache lock. Skips the optimistic `check_lock` gate because adding/removing
    /// a label is an atomic GitHub operation independent of the issue body state.
    fn sync_tags(
        &mut self,
        _type_def: &TypeDef,
        doc_id: &str,
        add: &[String],
        remove: &[String],
    ) -> Result<PushOutcome> {
        let issue_number = self
            .issue_map
            .get(doc_id)
            .map(|e| e.issue_number)
            .ok_or_else(|| anyhow::anyhow!("{} not found in issue map", doc_id))?;

        if !add.is_empty() {
            for tag in add {
                self.client
                    .label_ensure(&self.repo, tag, "", &gh::deterministic_color(tag))?;
            }
            self.client
                .issue_edit(&self.repo, issue_number, None, None, add, &[])?;
        }
        if !remove.is_empty() {
            self.client
                .issue_edit(&self.repo, issue_number, None, None, &[], remove)?;
        }

        self.issue_cache.touch_lock(doc_id)?;

        Ok(PushOutcome::Synced)
    }
}

/// Map a milestone REST `state` back to a lifecycle status via the type's
/// lifecycle (STORY-223 AC1): `"closed"` -> the type's terminal state, anything
/// else -> the type's first active state. Uses the same
/// [`Lifecycle::first_active_status`]/[`Lifecycle::terminal_status`] derivation
/// as the github-issues read path, so a milestone-backed type inherits its
/// remote open/closed state into its own lifecycle. Replaces the former
/// hardcoded `in-progress`/`complete` mapping.
pub fn milestone_state_to_status(state: &str, lifecycle: &Lifecycle) -> Status {
    if state.eq_ignore_ascii_case("closed") {
        Status::new(lifecycle.terminal_status())
    } else {
        Status::new(lifecycle.first_active_status())
    }
}

/// Progress as a 0..=100 percentage of closed issues over the total, or `None`
/// when there are no issues. Computed at read time only; never stored or
/// writable.
pub fn percent_complete(open: u64, closed: u64) -> Option<u8> {
    let total = open + closed;
    if total == 0 {
        None
    } else {
        Some((closed * 100 / total) as u8)
    }
}

/// A milestone document store backed by the GitHub milestones REST API. The
/// milestone number is the document id (`make_id(number)`), mirroring the
/// github-issues backend. Write policy is last-write-wins + refresh: pushes
/// happen unconditionally, then the milestone is re-read into the cache; there
/// is no optimistic lock.
pub struct GithubMilestonesStore {
    pub client: Box<dyn GhMilestoneClient>,
    pub root: PathBuf,
    pub repo: String,
    pub config: Config,
    pub issue_map: IssueMap,
}

impl GithubMilestonesStore {
    /// Downcast the boxed client to a concrete mock for test assertions.
    #[cfg(test)]
    fn mock(&self) -> &crate::engine::gh::test_support::MockGhMilestoneClient {
        (*self.client)
            .as_any()
            .downcast_ref::<crate::engine::gh::test_support::MockGhMilestoneClient>()
            .expect("client is a MockGhMilestoneClient")
    }

    fn resolve_number(&self, doc_id: &str) -> Result<u64> {
        self.issue_map
            .get(doc_id)
            .map(|e| e.issue_number)
            .ok_or_else(|| anyhow::anyhow!("{} not found in milestone map", doc_id))
    }

    fn meta_from_milestone(
        &self,
        type_def: &TypeDef,
        id: &str,
        milestone: &gh::GhMilestone,
        author: &str,
    ) -> DocMeta {
        let mut attributes: std::collections::BTreeMap<String, AttrValue> = Default::default();
        if let Some(due) = &milestone.due_on {
            attributes.insert("due_on".to_string(), AttrValue::Str(due.clone()));
        }
        attributes.insert(
            "open_issues".to_string(),
            AttrValue::Int(milestone.open_issues as i64),
        );
        attributes.insert(
            "closed_issues".to_string(),
            AttrValue::Int(milestone.closed_issues as i64),
        );

        // The milestone's `targeted-by` relations are derived virtually in
        // `build_links` as the reverse of each issue's forward `targets` edge
        // (read from the issue's native milestone at fetch). They are never
        // stored on the cached milestone doc, so `related` is always empty here.
        DocMeta {
            path: PathBuf::new(),
            title: milestone.title.clone(),
            doc_type: DocType::new(&type_def.name),
            status: milestone_state_to_status(&milestone.state, &type_def.effective_lifecycle()),
            author: author.to_string(),
            date: Local::now().date_naive(),
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            attributes,
            id: id.to_string(),
        }
    }
}

impl DocumentStore for GithubMilestonesStore {
    fn create(
        &mut self,
        type_def: &TypeDef,
        title: &str,
        author: &str,
        body: &str,
    ) -> Result<CreatedDoc> {
        let cache_dir = self.root.join(".lazyspec/cache").join(&type_def.name);
        std::fs::create_dir_all(&cache_dir)?;

        let milestone = self
            .client
            .milestone_create(&self.repo, title, body, None, "open")?;

        let id = type_def.make_id(milestone.number);
        self.issue_map.insert_kind(
            &id,
            milestone.number,
            "",
            "",
            crate::engine::issue_map::EntryKind::Milestone,
        );
        self.issue_map.save(&self.root)?;

        let meta = self.meta_from_milestone(type_def, &id, &milestone, author);
        write_cache_file(&self.root, type_def, &meta, body)?;

        let cache_path = cache_dir.join(format!("{}.md", id));
        let relative = cache_path
            .strip_prefix(&self.root)
            .unwrap_or(&cache_path)
            .to_path_buf();
        Ok(CreatedDoc {
            path: relative,
            id,
            push_outcome: PushOutcome::Synced,
        })
    }

    fn update(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        updates: &[(&str, &str)],
    ) -> Result<PushOutcome> {
        let number = self.resolve_number(doc_id)?;

        let mut title: Option<String> = None;
        let mut description: Option<String> = None;
        let mut due_on: Option<String> = None;
        let mut state: Option<String> = None;
        for &(key, value) in updates {
            match key {
                "title" => title = Some(value.to_string()),
                "body" | "description" => description = Some(value.to_string()),
                "due_on" => due_on = Some(value.to_string()),
                "status" => {
                    let s: Status = value.parse()?;
                    // Route through the single ITERATION-318 open/closed
                    // classification so a transition into the type's terminal
                    // state (custom or default) closes the milestone.
                    let open = issue_body::status_maps_to_open(
                        s.as_str(),
                        type_def.effective_lifecycle().terminal_status(),
                    );
                    state = Some(if open { "open" } else { "closed" }.to_string());
                }
                // `percent_complete` is a computed read-only field; reject writes
                // so it is never PATCHed to GitHub.
                "percent_complete" => {
                    bail!("percent_complete is computed from issue counts and cannot be set")
                }
                other => bail!("unknown milestone field '{}'", other),
            }
        }

        // Last-write-wins: push the changed fields unconditionally (no lock).
        self.client.milestone_edit(
            &self.repo,
            number,
            title.as_deref(),
            description.as_deref(),
            due_on.as_deref(),
            state.as_deref(),
        )?;

        // Refresh: re-read the milestone and rewrite the cache from remote.
        let milestone = self.client.milestone_view(&self.repo, number)?;
        let meta = self.meta_from_milestone(type_def, doc_id, &milestone, "");
        write_cache_file(&self.root, type_def, &meta, &milestone.description)?;

        Ok(PushOutcome::Synced)
    }

    fn delete(&mut self, type_def: &TypeDef, doc_id: &str) -> Result<PushOutcome> {
        let number = self.resolve_number(doc_id)?;
        self.client.milestone_delete(&self.repo, number)?;

        self.issue_map.remove(doc_id);
        self.issue_map.save(&self.root)?;

        let cache_dir = self.root.join(".lazyspec/cache").join(&type_def.name);
        if let Some(path) = find_cache_file(&cache_dir, doc_id) {
            let _ = std::fs::remove_file(path);
        }
        Ok(PushOutcome::Synced)
    }

    fn set_provenance(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        provenance: &[String],
    ) -> Result<PushOutcome> {
        // Milestones have no provenance field on GitHub; keep it in the local
        // cache frontmatter only.
        let cache_dir = self.root.join(".lazyspec/cache").join(&type_def.name);
        let cache_path = find_cache_file(&cache_dir, doc_id)
            .ok_or_else(|| anyhow::anyhow!("cache file not found for {}", doc_id))?;

        let entries: Vec<serde_yaml::Value> = provenance
            .iter()
            .map(|s| serde_yaml::Value::String(s.clone()))
            .collect();

        crate::engine::document::rewrite_frontmatter(
            &cache_path,
            &crate::engine::fs::RealFileSystem,
            |val| {
                let map = val
                    .as_mapping_mut()
                    .ok_or_else(|| anyhow::anyhow!("frontmatter root must be a mapping"))?;
                map.insert(
                    serde_yaml::Value::String("provenance".to_string()),
                    serde_yaml::Value::Sequence(entries.clone()),
                );
                Ok(())
            },
        )
        .map(|()| PushOutcome::Synced)
    }

    /// No-op: labels are an issue concept, not a milestone concept. Tag
    /// mutations stay in the local cache frontmatter only.
    fn sync_tags(
        &mut self,
        _: &TypeDef,
        _: &str,
        _: &[String],
        _: &[String],
    ) -> Result<PushOutcome> {
        Ok(PushOutcome::Synced)
    }
}

const PROJECT_NODE_ID_ORG_QUERY: &str = "query($owner: String!, $number: Int!) { organization(login: $owner) { projectV2(number: $number) { id } } }";

const PROJECT_NODE_ID_USER_QUERY: &str = "query($owner: String!, $number: Int!) { user(login: $owner) { projectV2(number: $number) { id } } }";

const OWNER_NODE_ID_ORG_QUERY: &str =
    "query($owner: String!) { organization(login: $owner) { id } }";

const OWNER_NODE_ID_USER_QUERY: &str = "query($owner: String!) { user(login: $owner) { id } }";

const CREATE_PROJECT_V2_MUTATION: &str = "mutation($ownerId: ID!, $title: String!) { createProjectV2(input: { ownerId: $ownerId, title: $title }) { projectV2 { id number } } }";

/// Resolve an owner login to its GraphQL node id, trying the organization root
/// first then the user root (mirrors [`resolve_project_id_live`]). The
/// `createProjectV2` mutation needs the owner's *node id*, not the login.
fn resolve_owner_node_id(client: &dyn GhGraphql, owner: &str) -> Result<String> {
    let (_kind, id_node) = try_org_then_user(
        client,
        OWNER_NODE_ID_ORG_QUERY,
        OWNER_NODE_ID_USER_QUERY,
        &[("owner", GqlVar::Str(owner.to_string()))],
        "/data/organization/id",
        "/data/user/id",
    )
    .map_err(|_| anyhow::anyhow!("owner '{}' not found as org or user", owner))?;
    id_node
        .as_str()
        .map(String::from)
        .ok_or_else(|| anyhow::anyhow!("owner '{}' not found as org or user", owner))
}

/// True when a GraphQL response signals the `project` token scope is missing:
/// a top-level `errors[]` entry whose type or message names insufficient
/// scopes, the `project` scope, an inaccessible resource, or a missing
/// permission. Board creation needs the `project` scope, which `repo` does not
/// grant.
fn missing_project_scope(resp: &serde_json::Value) -> bool {
    let Some(errors) = resp.pointer("/errors").and_then(|v| v.as_array()) else {
        return false;
    };
    errors.iter().any(|e| {
        let kind = e.get("type").and_then(|v| v.as_str()).unwrap_or_default();
        let msg = e
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_lowercase();
        kind == "INSUFFICIENT_SCOPES"
            || msg.contains("`project` scope")
            || msg.contains("project scope")
            || msg.contains("resource not accessible")
            || msg.contains("does not have permission")
    })
}

/// Parse the `owner` half of a `owner/repo` string. The github-projects backend
/// resolves boards under this owner.
fn owner_of(repo: &str) -> Result<&str> {
    repo.split_once('/')
        .map(|(o, _)| o)
        .filter(|o| !o.is_empty())
        .ok_or_else(|| anyhow::anyhow!("repo '{}' must be in owner/name form", repo))
}

/// Extract the numeric board number from a board doc id (`PROJECT-7` -> 7, or a
/// bare `7`). Project boards are addressed by their GitHub Projects v2 number.
pub fn board_number(doc_id: &str) -> Result<u64> {
    let suffix = doc_id.rsplit('-').next().unwrap_or(doc_id);
    suffix
        .parse::<u64>()
        .map_err(|_| anyhow::anyhow!("'{}' does not name a Projects v2 board number", doc_id))
}

/// A project-board document store backed by GitHub Projects v2 (GraphQL).
/// `create` authors a board via `createProjectV2` and binds the returned number
/// as the doc id (`PROJECT-n`); `delete` bails (boards are removed on GitHub).
/// `update`/`set_provenance` resolve the board node id without mutating the
/// board. The owner type (org vs user) is auto-detected by trying the
/// organization root first, then falling back to the user root.
pub struct GithubProjectsStore {
    pub client: Box<dyn GhProjectsClient>,
    pub root: PathBuf,
    pub repo: String,
    pub config: Config,
    pub issue_map: IssueMap,
}

impl GithubProjectsStore {
    /// Downcast the boxed client to a concrete mock for test assertions.
    #[cfg(test)]
    fn mock(&self) -> &crate::engine::gh::test_support::MockGhClient {
        (*self.client)
            .as_any()
            .downcast_ref::<crate::engine::gh::test_support::MockGhClient>()
            .expect("client is a MockGhClient")
    }

    /// Resolve a Projects v2 board number to its GraphQL node id. Tries the
    /// organization root first, then the user root. A board number that exists
    /// under neither (both `projectV2` null) is a not-found error. NEVER issues a
    /// create mutation.
    pub fn resolve_board(&self, owner: &str, number: u64) -> Result<String> {
        let org_resp = self.client.graphql(
            PROJECT_NODE_ID_ORG_QUERY,
            &[
                ("owner", GqlVar::Str(owner.to_string())),
                ("number", GqlVar::Int(number as i64)),
            ],
        )?;
        if let Some(id) = org_resp
            .pointer("/data/organization/projectV2/id")
            .and_then(|v| v.as_str())
        {
            return Ok(id.to_string());
        }

        let user_resp = self.client.graphql(
            PROJECT_NODE_ID_USER_QUERY,
            &[
                ("owner", GqlVar::Str(owner.to_string())),
                ("number", GqlVar::Int(number as i64)),
            ],
        )?;
        if let Some(id) = user_resp
            .pointer("/data/user/projectV2/id")
            .and_then(|v| v.as_str())
        {
            return Ok(id.to_string());
        }

        bail!(
            "Projects v2 board #{} not found under owner '{}'",
            number,
            owner
        )
    }

    /// Resolve a board doc id to its node id and materialize a cache file holding
    /// it for offline lookup. Records the number+node id in the issue map.
    fn bind_board(&mut self, type_def: &TypeDef, doc_id: &str) -> Result<String> {
        let owner = owner_of(&self.repo)?.to_string();
        let number = board_number(doc_id)?;
        let node_id = self.resolve_board(&owner, number)?;

        self.issue_map.insert_kind(
            doc_id,
            number,
            "",
            node_id.clone(),
            crate::engine::issue_map::EntryKind::Project,
        );
        self.issue_map.save(&self.root)?;

        let meta = DocMeta {
            path: PathBuf::new(),
            title: doc_id.to_string(),
            doc_type: DocType::new(&type_def.name),
            status: Status::new(type_def.effective_lifecycle().seed_status()),
            author: String::new(),
            date: Local::now().date_naive(),
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            attributes: Default::default(),
            id: doc_id.to_string(),
        };
        write_cache_file(&self.root, type_def, &meta, &node_id)?;

        Ok(node_id)
    }
}

impl DocumentStore for GithubProjectsStore {
    fn create(
        &mut self,
        type_def: &TypeDef,
        title: &str,
        _author: &str,
        _body: &str,
    ) -> Result<CreatedDoc> {
        let owner = owner_of(&self.repo)?.to_string();
        let owner_id = resolve_owner_node_id(self.client.as_graphql(), &owner)?;

        let resp = self.client.graphql(
            CREATE_PROJECT_V2_MUTATION,
            &[
                ("ownerId", GqlVar::Str(owner_id)),
                ("title", GqlVar::Str(title.to_string())),
            ],
        )?;

        // Board creation needs the `project` token scope; `repo` does not grant
        // it. Detect the scope-missing signal BEFORE any persist so the failure
        // path writes no doc and no issue-map entry.
        if missing_project_scope(&resp) || resp.pointer("/data/createProjectV2").is_none() {
            if missing_project_scope(&resp) {
                bail!("Projects v2 board creation needs the `project` token scope; run `gh auth refresh -s project`");
            }
            bail!("createProjectV2 returned no board number");
        }

        let number = resp
            .pointer("/data/createProjectV2/projectV2/number")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("createProjectV2 returned no board number"))?;
        let node_id = resp
            .pointer("/data/createProjectV2/projectV2/id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("createProjectV2 returned no board number"))?
            .to_string();

        let doc_id = format!("PROJECT-{}", number);

        self.issue_map.insert_kind(
            &doc_id,
            number,
            "",
            node_id.clone(),
            crate::engine::issue_map::EntryKind::Project,
        );
        self.issue_map.save(&self.root)?;

        let meta = DocMeta {
            path: PathBuf::new(),
            title: doc_id.clone(),
            doc_type: DocType::new(&type_def.name),
            status: Status::new(type_def.effective_lifecycle().seed_status()),
            author: String::new(),
            date: Local::now().date_naive(),
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            attributes: Default::default(),
            id: doc_id.clone(),
        };
        write_cache_file(&self.root, type_def, &meta, &node_id)?;

        let cache_path = self
            .root
            .join(".lazyspec/cache")
            .join(&type_def.name)
            .join(format!("{}.md", doc_id));
        let relative = cache_path
            .strip_prefix(&self.root)
            .unwrap_or(&cache_path)
            .to_path_buf();
        Ok(CreatedDoc {
            path: relative,
            id: doc_id,
            push_outcome: PushOutcome::Synced,
        })
    }

    fn update(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        _updates: &[(&str, &str)],
    ) -> Result<PushOutcome> {
        // Boards are not mutated from lazyspec; resolving the node id is the only
        // side effect, so an out-of-date binding refreshes.
        self.bind_board(type_def, doc_id)?;
        Ok(PushOutcome::Synced)
    }

    fn delete(&mut self, _type_def: &TypeDef, _doc_id: &str) -> Result<PushOutcome> {
        bail!("github-projects backend does not delete boards; boards are managed on GitHub")
    }

    fn set_provenance(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        _provenance: &[String],
    ) -> Result<PushOutcome> {
        self.bind_board(type_def, doc_id)?;
        Ok(PushOutcome::Synced)
    }

    /// No-op: labels are an issue concept, not a Projects v2 board concept. Tag
    /// mutations stay in the local cache frontmatter only.
    fn sync_tags(
        &mut self,
        _: &TypeDef,
        _: &str,
        _: &[String],
        _: &[String],
    ) -> Result<PushOutcome> {
        Ok(PushOutcome::Synced)
    }
}

pub fn write_cache_file(
    root: &std::path::Path,
    type_def: &TypeDef,
    meta: &DocMeta,
    body: &str,
) -> Result<()> {
    if meta.id.is_empty() {
        anyhow::bail!("refusing cache write for empty doc id");
    }
    let cache_dir = root.join(".lazyspec/cache").join(&type_def.name);
    std::fs::create_dir_all(&cache_dir)?;
    let cache_path = find_cache_file(&cache_dir, &meta.id)
        .unwrap_or_else(|| cache_dir.join(format!("{}.md", meta.id)));

    let cache_content = render_cache_content(meta, body)?;
    crate::engine::fs::atomic_write(&cache_path, &cache_content)?;
    Ok(())
}

pub(crate) fn find_cache_file(cache_dir: &std::path::Path, doc_id: &str) -> Option<PathBuf> {
    if let Some(flat) = find_cache_file_flat(cache_dir, doc_id) {
        return Some(flat);
    }
    // Nested layout: a child lives inside a parent-id folder as `NN-<doc_id>.md`,
    // and a parent-with-children lives at `<doc_id>/index.md`.
    std::fs::read_dir(cache_dir).ok()?.find_map(|entry| {
        let entry = entry.ok()?;
        if !entry.file_type().ok()?.is_dir() {
            return None;
        }
        let folder = entry.path();
        if entry.file_name().to_string_lossy() == doc_id {
            let index = folder.join("index.md");
            if index.exists() {
                return Some(index);
            }
        }
        find_nested_child(&folder, doc_id)
    })
}

fn find_cache_file_flat(cache_dir: &std::path::Path, doc_id: &str) -> Option<PathBuf> {
    let prefix = format!("{}-", doc_id);
    let exact = format!("{}.md", doc_id);
    std::fs::read_dir(cache_dir).ok()?.find_map(|entry| {
        let entry = entry.ok()?;
        if entry.file_type().ok()?.is_dir() {
            return None;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name == exact || name.starts_with(&prefix) {
            Some(entry.path())
        } else {
            None
        }
    })
}

/// Find a nested child `NN-<doc_id>.md` inside a parent folder.
fn find_nested_child(folder: &std::path::Path, doc_id: &str) -> Option<PathBuf> {
    std::fs::read_dir(folder).ok()?.find_map(|entry| {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            return None;
        }
        let stem = path.file_stem().and_then(|s| s.to_str())?;
        if store::strip_order_prefix(stem) == Some(doc_id) {
            Some(path)
        } else {
            None
        }
    })
}

/// Zero-padded order prefix width: lexicographic sort must equal numeric sort, so
/// the width grows with the child count (`>=2` digits, `3` when more than 99).
fn order_width(total: usize) -> usize {
    if total > 99 {
        3
    } else {
        2
    }
}

fn child_cache_filename(order: usize, total: usize, child_id: &str) -> String {
    format!(
        "{:0width$}-{}.md",
        order,
        child_id,
        width = order_width(total)
    )
}

/// Write a parent that has children to `<type>/<PARENT>/index.md`.
pub fn write_cache_parent(
    root: &std::path::Path,
    type_def: &TypeDef,
    meta: &DocMeta,
    body: &str,
) -> Result<()> {
    if meta.id.is_empty() {
        anyhow::bail!("refusing cache write for empty doc id");
    }
    let folder = root
        .join(".lazyspec/cache")
        .join(&type_def.name)
        .join(&meta.id);
    std::fs::create_dir_all(&folder)?;
    let cache_path = folder.join("index.md");
    let content = render_cache_content(meta, body)?;
    crate::engine::fs::atomic_write(&cache_path, &content)?;
    Ok(())
}

/// Write a child to `<type>/<PARENT>/NN-<child-id>.md`, where `order` is the child's
/// zero-based position and `total` the sibling count (sets the `NN` padding width).
pub fn write_cache_child(
    root: &std::path::Path,
    type_def: &TypeDef,
    parent_id: &str,
    order: usize,
    total: usize,
    meta: &DocMeta,
    body: &str,
) -> Result<()> {
    if meta.id.is_empty() {
        anyhow::bail!("refusing cache write for empty doc id");
    }
    if parent_id.is_empty() {
        anyhow::bail!("refusing nested cache write for empty parent id");
    }
    let folder = root
        .join(".lazyspec/cache")
        .join(&type_def.name)
        .join(parent_id);
    std::fs::create_dir_all(&folder)?;
    let cache_path = folder.join(child_cache_filename(order, total, &meta.id));
    let content = render_cache_content(meta, body)?;
    crate::engine::fs::atomic_write(&cache_path, &content)?;
    Ok(())
}

/// Test-only access to the local cache serializer, used by `document.rs` to
/// assert the `assignee` skip-when-`None` behaviour (AC6).
#[cfg(test)]
pub(crate) fn render_cache_content_for_test(meta: &DocMeta, body: &str) -> String {
    render_cache_content(meta, body).unwrap()
}

fn render_cache_content(meta: &DocMeta, body: &str) -> Result<String> {
    let frontmatter = CacheFrontmatter {
        title: meta.title.clone(),
        doc_type: meta.doc_type.as_str().to_string(),
        status: meta.status.to_string(),
        author: meta.author.clone(),
        date: meta.date.to_string(),
        tags: meta.tags.clone(),
        assignee: meta.assignee.clone(),
        provenance: meta.provenance.clone(),
        related: meta
            .related
            .iter()
            .map(|r| {
                let mut m = BTreeMap::new();
                m.insert(r.rel_type.to_string(), r.target.clone());
                m
            })
            .collect(),
        attributes: meta.attributes.clone(),
    };
    let yaml = serde_yaml::to_string(&frontmatter)?;
    let body_section = if body.is_empty() {
        String::new()
    } else {
        format!("\n{}\n", body)
    };
    Ok(compose_frontmatter(&yaml, &body_section))
}

#[allow(clippy::too_many_arguments)]
/// A non-generic lookup from a [`StoreBackend`] to the [`DocumentStore`] that
/// serves it.
///
/// This replaces the former `dispatch_for_type` closed generic match, which was
/// generic over one client type parameter per backend (`G`, `R`, `M`, `P`).
/// That shape forced every new backend to add another generic parameter and
/// forced every construction site to name all of them. The registry erases the
/// concrete client type behind `&mut dyn DocumentStore`, so backends register
/// by [`StoreBackend`] key and a new backend (e.g. `ClickupTasks`) is added by
/// registering it, not by growing a generic signature.
///
/// The registry owns each backend's store as a boxed trait object. A new
/// backend (e.g. `ClickupTasks`) is added by registering it in
/// [`build_registry`], not by growing a generic signature or adding an `if`
/// branch to every command. Lookup is keyed on [`TypeDef::store`].
#[derive(Default)]
pub struct DocumentStoreRegistry {
    stores: HashMap<StoreBackend, Box<dyn DocumentStore>>,
}

impl DocumentStoreRegistry {
    pub fn new() -> Self {
        Self {
            stores: HashMap::new(),
        }
    }

    /// Register the store serving `backend`. A later registration for the same
    /// backend replaces the earlier one.
    pub fn register(&mut self, backend: StoreBackend, store: Box<dyn DocumentStore>) {
        self.stores.insert(backend, store);
    }

    /// Resolve the store for `type_def`'s backend, or error if no store is
    /// registered for it.
    pub fn for_type(&mut self, type_def: &TypeDef) -> Result<&mut dyn DocumentStore> {
        match self.stores.get_mut(&type_def.store) {
            Some(store) => Ok(store.as_mut()),
            None => bail!(
                "type '{}' uses {} store but no {} backend is registered",
                type_def.name,
                type_def.store,
                type_def.store
            ),
        }
    }

    /// The store registered for `backend`, if any. Primarily for tests that
    /// assert routing did not touch another backend.
    #[cfg(test)]
    pub fn get(&self, backend: StoreBackend) -> Option<&dyn DocumentStore> {
        self.stores.get(&backend).map(|s| s.as_ref())
    }
}

/// Build the production store registry for `root`/`config`: one boxed
/// [`DocumentStore`] per backend the config can serve. The GitHub backends are
/// registered only when `[github]` and a repo are configured; the git-ref store
/// is registered with no reserved number (number reservation is a create-time
/// concern handled by the caller). This is the single place a new backend is
/// wired into production `update`/`delete`/`set_provenance` dispatch.
pub fn build_registry(root: &std::path::Path, config: &Config) -> DocumentStoreRegistry {
    let mut registry = DocumentStoreRegistry::new();

    registry.register(
        StoreBackend::Filesystem,
        Box::new(FilesystemStore {
            root: root.to_path_buf(),
            config: config.clone(),
        }),
    );

    let github = config.documents.github.as_ref();
    match github.and_then(|g| g.repo.clone()) {
        Some(repo) => {
            let issue_map = IssueMap::load(root).unwrap_or_default();
            registry.register(
                StoreBackend::GithubIssues,
                Box::new(GithubIssuesStore {
                    client: Box::new(gh::GhCli::new()),
                    root: root.to_path_buf(),
                    repo: repo.clone(),
                    config: config.clone(),
                    issue_map: issue_map.clone(),
                    issue_cache: IssueCache::new(root),
                }),
            );
            registry.register(
                StoreBackend::GithubMilestones,
                Box::new(GithubMilestonesStore {
                    client: Box::new(gh::GhCli::new()),
                    root: root.to_path_buf(),
                    repo: repo.clone(),
                    config: config.clone(),
                    issue_map: issue_map.clone(),
                }),
            );
            registry.register(
                StoreBackend::GithubProjects,
                Box::new(GithubProjectsStore {
                    client: Box::new(gh::GhCli::new()),
                    root: root.to_path_buf(),
                    repo,
                    config: config.clone(),
                    issue_map,
                }),
            );
        }
        // No usable GitHub config: register each GitHub backend as a store that
        // fails on use with the same message the inline dispatch produced, so a
        // GitHub-backed type still errors clearly (rather than a generic
        // "backend not registered") without eagerly failing unrelated types.
        None => {
            let reason = if github.is_none() {
                "no [github] config found"
            } else {
                "no github.repo configured"
            };
            for backend in [
                StoreBackend::GithubIssues,
                StoreBackend::GithubMilestones,
                StoreBackend::GithubProjects,
            ] {
                let message = format!("type uses {} store but {}", backend, reason);
                registry.register(backend, Box::new(UnavailableStore { message }));
            }
        }
    }

    registry.register(
        StoreBackend::GitRef,
        Box::new(crate::engine::git_ref_store::GitRefStore {
            git: Box::new(crate::engine::git_ref::GitCli),
            root: root.to_path_buf(),
            config: config.clone(),
            remote: config.git_ref.remote.clone(),
            reserved_number: None,
        }),
    );

    // ClickUp is registered the new way (the first backend to do so): a boxed
    // trait object keyed by [`StoreBackend`], no generic param. The read path is
    // inline in `cli::fetch`; the write path is a later RFC-056 story, so the
    // write methods currently fail loudly. The token stays unloaded here to keep
    // `build_registry` free of keychain I/O on every command; the write path
    // will load it when it lands.
    //
    // Only register the real store when a clickup-tasks type actually exists:
    // `ClickupHttpClient::new()` eagerly builds a reqwest client (system CA
    // load), so registering it unconditionally would touch the network stack on
    // every command and panic in CA-less environments. Mirrors the poll seam's
    // `has_clickup_types` gate in `tui::infra::event_loop`.
    let has_clickup_types = config
        .documents
        .types
        .iter()
        .any(|t| t.store == StoreBackend::ClickupTasks);
    if has_clickup_types {
        registry.register(
            StoreBackend::ClickupTasks,
            Box::new(ClickupTasksStore {
                client: Box::new(crate::engine::clickup::ClickupHttpClient::new()),
                root: root.to_path_buf(),
                config: config.clone(),
                token: None,
            }),
        );
    } else {
        registry.register(
            StoreBackend::ClickupTasks,
            Box::new(UnavailableStore {
                message: format!(
                    "type uses {} store but no clickup-tasks type is configured",
                    StoreBackend::ClickupTasks
                ),
            }),
        );
    }

    registry
}

/// Load the ClickUp credential and bind a token-bearing [`ClickupTasksStore`] --
/// the single place the write path's token load + store construction lives
/// (STORY-212 AC3).
///
/// [`build_registry`] deliberately registers the ClickUp store with `token: None`
/// to keep registry construction free of keychain I/O; every write path
/// (create/update/delete and the TUI external-edit push) calls here instead to
/// load the credential and bind a store that can actually authenticate. `action`
/// names the operation for the missing-token error
/// ("creating"/"updating"/"deleting"/"editing"). The client factory and token
/// loader are injected so a test drives the `ClickupClient` seam with a
/// `FakeClickupClient` and a scripted token without a keychain or the network
/// (DICTUM-002); production passes `ClickupHttpClient::new` and the global
/// credential store. The token is loaded (and checked present) before the client
/// is built, so a missing credential fails loud without constructing a client.
pub fn clickup_write_store<C: crate::engine::clickup::ClickupClient + 'static>(
    root: &std::path::Path,
    config: &Config,
    action: &str,
    client_factory: impl FnOnce() -> C,
    token_loader: impl FnOnce() -> Result<Option<crate::engine::credentials::Token>>,
) -> Result<ClickupTasksStore> {
    let token = token_loader()?.ok_or_else(|| {
        anyhow::anyhow!(
            "no ClickUp token found; run `lazyspec setup clickup` before {} \
             clickup-tasks documents",
            action
        )
    })?;
    Ok(ClickupTasksStore {
        client: Box::new(client_factory()),
        root: root.to_path_buf(),
        config: config.clone(),
        token: Some(token),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{Config, NumberingStrategy, StoreBackend, TypeDef};
    use crate::engine::gh::{
        test_support::{MockGhClient, MockGhMilestoneClient},
        GhIssue, GhLabel,
    };
    use crate::engine::git_ref::test_support::MockGitRefClient;
    use crate::engine::git_ref_store::GitRefStore;
    use crate::engine::issue_map::IssueMap;

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
            clickup_list_id: None,
            clickup_task_type: None,
            clickup_custom_field_map: None,
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

        let mut fs_store = FilesystemStore {
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

        let mut fs_store = FilesystemStore {
            root: root.clone(),
            config: config.clone(),
        };

        let td = test_type_def(StoreBackend::Filesystem);
        let created = fs_store.create(&td, "to delete", "author", "").unwrap();
        assert!(root.join(&created.path).exists());

        fs_store.delete(&td, &created.id).unwrap();
        assert!(!root.join(&created.path).exists());
    }

    // AC: Filesystem tag mutation unchanged. Propagation is a no-op -- the CLI
    // already rewrote the on-disk frontmatter, and there is no remote.
    #[test]
    fn filesystem_sync_tags_is_noop() {
        let root = tmp_root("fs_sync_tags");
        let mut fs_store = FilesystemStore {
            root: root.clone(),
            config: Config::default(),
        };
        let td = test_type_def(StoreBackend::Filesystem);

        fs_store
            .sync_tags(&td, "RFC-1", &["security".to_string()], &[])
            .unwrap();
        fs_store
            .sync_tags(&td, "RFC-1", &[], &["security".to_string()])
            .unwrap();
    }

    // AC: GitHub-issues tag add. Each added label is ensured, then applied via a
    // single labels-add issue_edit; no labels are removed.
    #[test]
    fn github_issues_sync_tags_add_ensures_and_applies_label() {
        let root = tmp_root("gh_sync_tags_add");
        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("RFC-1", 87, "", "");
        let mut gh_store = GithubIssuesStore {
            client: Box::new(MockGhClient::new()),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map,
            issue_cache: IssueCache::new(&root),
        };
        let td = test_type_def(StoreBackend::GithubIssues);

        gh_store
            .sync_tags(&td, "RFC-1", &["security".to_string()], &[])
            .unwrap();

        let mock = gh_store.mock();
        assert_eq!(
            *mock.last_ensure_label_names.borrow(),
            vec!["security".to_string()],
            "should ensure the added label exists"
        );
        assert_eq!(
            *mock.last_edit_labels_add.borrow(),
            vec!["security".to_string()],
            "should apply the label to the issue"
        );
        assert!(
            mock.last_edit_labels_remove.borrow().is_empty(),
            "an add must not remove any labels"
        );
    }

    // AC: GitHub-issues tag remove. Removal applies a labels-remove issue_edit
    // and never ensures/creates a label.
    #[test]
    fn github_issues_sync_tags_remove_drops_label() {
        let root = tmp_root("gh_sync_tags_remove");
        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("RFC-1", 87, "", "");
        let mut gh_store = GithubIssuesStore {
            client: Box::new(MockGhClient::new()),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map,
            issue_cache: IssueCache::new(&root),
        };
        let td = test_type_def(StoreBackend::GithubIssues);

        gh_store
            .sync_tags(&td, "RFC-1", &[], &["security".to_string()])
            .unwrap();

        let mock = gh_store.mock();
        assert_eq!(
            *mock.last_edit_labels_remove.borrow(),
            vec!["security".to_string()],
            "should remove the label from the issue"
        );
        assert!(
            mock.last_ensure_label_names.borrow().is_empty(),
            "a remove must not ensure/create a label"
        );
        assert!(
            mock.last_edit_labels_add.borrow().is_empty(),
            "a remove must not add any labels"
        );
    }

    fn clickup_type_def(list_id: Option<&str>) -> TypeDef {
        let mut td = test_type_def(StoreBackend::ClickupTasks);
        td.name = "task".to_string();
        td.prefix = "TASK".to_string();
        td.clickup_list_id = list_id.map(|s| s.to_string());
        td
    }

    fn clickup_user() -> crate::engine::clickup::ClickupUser {
        crate::engine::clickup::ClickupUser {
            id: 1,
            username: "Jack".to_string(),
            email: String::new(),
        }
    }

    fn scripted_task(json: &str) -> crate::engine::clickup::ClickupTask {
        serde_json::from_str(json).unwrap()
    }

    // AC: ClickUp tag mutation fails loudly. The write path is unimplemented, so
    // sync_tags must error (not silently succeed) at the trait seam.
    #[test]
    fn clickup_sync_tags_fails_loudly() {
        use crate::engine::clickup::FakeClickupClient;

        let root = tmp_root("clickup_sync_tags");
        let td = clickup_type_def(Some("list123"));
        let mut store = ClickupTasksStore {
            client: Box::new(FakeClickupClient::valid(clickup_user())),
            root,
            config: Config::default(),
            token: None,
        };

        let err = store
            .sync_tags(&td, "TASK-1", &["security".to_string()], &[])
            .unwrap_err();
        assert!(
            err.to_string().contains("not implemented"),
            "expected a not-implemented error, got: {err}"
        );
    }

    #[test]
    fn clickup_create_posts_task_and_mirrors_to_cache_and_map() {
        use crate::engine::clickup::FakeClickupClient;
        use crate::engine::credentials::Token;

        let root = tmp_root("clickup_create");
        let td = clickup_type_def(Some("list123"));

        let created = scripted_task(
            r#"{
                "id": "90abc",
                "name": "My task",
                "status": {"status": "open"},
                "date_updated": "1774587145901",
                "markdown_description": "the body",
                "creator": {"username": "Jack"}
            }"#,
        );
        let fake = FakeClickupClient::valid(clickup_user()).with_created_task(created);
        let calls = fake.create_calls();

        let mut store = ClickupTasksStore {
            client: Box::new(fake),
            root: root.clone(),
            config: Config::default(),
            token: Some(Token::new("pk_x")),
        };

        let result = store
            .create(&td, "My task", "ignored-author", "the body")
            .unwrap();

        // Doc id is the type prefix + the returned ClickUp task id.
        assert_eq!(result.id, "TASK-90abc");

        // The echoed task is materialized into a cache file, github-issues shape.
        let cache = root.join(".lazyspec/cache/task/TASK-90abc.md");
        let content = std::fs::read_to_string(&cache).unwrap();
        assert!(content.contains("title: My task"), "got:\n{content}");
        assert!(content.contains("status: open"), "got:\n{content}");
        assert!(content.contains("the body"), "got:\n{content}");

        // The task map records the task id + date_updated (the lock baseline).
        let map = TaskMap::load(&root).unwrap();
        let entry = map.get("TASK-90abc").unwrap();
        assert_eq!(entry.task_id, "90abc");
        assert_eq!(entry.updated_at, "1774587145901");

        // Exactly one POST, to the bound List, with the mapped payload.
        let recorded = calls.borrow();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, "list123");
        assert_eq!(recorded[0].1.name, "My task");
        assert_eq!(recorded[0].1.markdown_content, Some("the body".to_string()));
        // A create sends no status; ClickUp assigns the List default.
        assert_eq!(recorded[0].1.status, None);
    }

    #[test]
    fn clickup_create_stamps_configured_task_type_into_payload() {
        use crate::engine::clickup::FakeClickupClient;
        use crate::engine::credentials::Token;

        let root = tmp_root("clickup_create_task_type");
        let mut td = clickup_type_def(Some("list123"));
        td.clickup_task_type = Some(1001);

        let created = scripted_task(
            r#"{"id":"90abc","name":"My task","status":{"status":"open"},"custom_item_id":1001}"#,
        );
        let fake = FakeClickupClient::valid(clickup_user()).with_created_task(created);
        let calls = fake.create_calls();

        let mut store = ClickupTasksStore {
            client: Box::new(fake),
            root,
            config: Config::default(),
            token: Some(Token::new("pk_x")),
        };

        store.create(&td, "My task", "author", "body").unwrap();

        let recorded = calls.borrow();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].1.custom_item_id, Some(1001));
    }

    #[test]
    fn clickup_create_without_task_type_omits_custom_item_id() {
        use crate::engine::clickup::FakeClickupClient;
        use crate::engine::credentials::Token;

        let root = tmp_root("clickup_create_no_task_type");
        let td = clickup_type_def(Some("list123"));
        assert!(td.clickup_task_type.is_none());

        let created =
            scripted_task(r#"{"id":"90abc","name":"My task","status":{"status":"open"}}"#);
        let fake = FakeClickupClient::valid(clickup_user()).with_created_task(created);
        let calls = fake.create_calls();

        let mut store = ClickupTasksStore {
            client: Box::new(fake),
            root,
            config: Config::default(),
            token: Some(Token::new("pk_x")),
        };

        store.create(&td, "My task", "author", "body").unwrap();

        let recorded = calls.borrow();
        assert_eq!(recorded[0].1.custom_item_id, None);
    }

    #[test]
    fn clickup_create_without_token_errors() {
        use crate::engine::clickup::FakeClickupClient;

        let root = tmp_root("clickup_create_no_token");
        let td = clickup_type_def(Some("list123"));
        let fake = FakeClickupClient::valid(clickup_user());

        let mut store = ClickupTasksStore {
            client: Box::new(fake),
            root,
            config: Config::default(),
            token: None,
        };

        let err = store.create(&td, "t", "a", "b").unwrap_err();
        assert!(err.to_string().contains("setup clickup"), "got: {err}");
    }

    #[test]
    fn clickup_create_without_list_id_errors() {
        use crate::engine::clickup::FakeClickupClient;
        use crate::engine::credentials::Token;

        let root = tmp_root("clickup_create_no_list");
        let td = clickup_type_def(None);
        let fake = FakeClickupClient::valid(clickup_user());

        let mut store = ClickupTasksStore {
            client: Box::new(fake),
            root,
            config: Config::default(),
            token: Some(Token::new("pk_x")),
        };

        let err = store.create(&td, "t", "a", "b").unwrap_err();
        assert!(err.to_string().contains("clickup_list_id"), "got: {err}");
    }

    #[test]
    fn clickup_update_puts_native_fields_and_round_trips_through_cache() {
        use crate::engine::clickup::FakeClickupClient;
        use crate::engine::credentials::Token;

        let root = tmp_root("clickup_update_roundtrip");
        let td = clickup_type_def(Some("list123"));

        // The doc is already mapped to a ClickUp task (an earlier fetch/create),
        // with a stale lock baseline the edit must bump.
        let mut map = TaskMap::load(&root).unwrap();
        map.insert("TASK-90abc", "90abc", "1700000000000");
        map.save(&root).unwrap();

        // ClickUp echoes the *updated* task: new priority/estimate/due and a
        // fresh date_updated. The round-trip reads these back out of the cache.
        let updated = scripted_task(
            r#"{
                "id": "90abc",
                "name": "Renamed task",
                "status": {"status": "in progress"},
                "priority": {"priority": "high"},
                "due_date": "1748541600000",
                "time_estimate": "3600000",
                "date_updated": "1774587145901",
                "markdown_description": "edited body",
                "creator": {"username": "Jack"}
            }"#,
        );
        // The pre-write optimistic-lock fetch sees the same date_updated the
        // baseline recorded (no external change), so the write proceeds.
        let remote_unchanged = scripted_task(
            r#"{
                "id": "90abc",
                "name": "My task",
                "status": {"status": "open"},
                "date_updated": "1700000000000"
            }"#,
        );
        let fake = FakeClickupClient::valid(clickup_user())
            .with_viewed_task(remote_unchanged)
            .with_updated_task(updated);
        let calls = fake.update_calls();

        let mut store = ClickupTasksStore {
            client: Box::new(fake),
            root: root.clone(),
            config: Config::default(),
            token: Some(Token::new("pk_x")),
        };

        store
            .update(
                &td,
                "TASK-90abc",
                &[
                    ("title", "Renamed task"),
                    ("body", "edited body"),
                    ("priority", "high"),
                    ("due", "1748541600000"),
                    ("estimate", "3600000"),
                ],
            )
            .unwrap();

        // Exactly one PUT, to the resolved task id, with the mapped native shape.
        let recorded = calls.borrow();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, "90abc");
        let payload = &recorded[0].1;
        assert_eq!(payload.name, Some("Renamed task".to_string()));
        assert_eq!(payload.markdown_content, Some("edited body".to_string()));
        assert_eq!(payload.priority, Some(2));
        assert_eq!(payload.due_date, Some(1_748_541_600_000));
        assert_eq!(payload.time_estimate, Some(3_600_000));

        // Round-trip: the cache now reflects the updated native field values.
        let cache = root.join(".lazyspec/cache/task/TASK-90abc.md");
        let content = std::fs::read_to_string(&cache).unwrap();
        assert!(content.contains("title: Renamed task"), "got:\n{content}");
        assert!(content.contains("priority: high"), "got:\n{content}");
        assert!(content.contains("estimate: 3600000"), "got:\n{content}");
        assert!(content.contains("due: 1748541600000"), "got:\n{content}");
        assert!(content.contains("edited body"), "got:\n{content}");

        // The lock baseline is bumped to the returned task's date_updated.
        let map = TaskMap::load(&root).unwrap();
        let entry = map.get("TASK-90abc").unwrap();
        assert_eq!(entry.task_id, "90abc");
        assert_eq!(entry.updated_at, "1774587145901");
    }

    #[test]
    fn clickup_advance_puts_raw_status_and_round_trips_through_cache() {
        use crate::engine::clickup::FakeClickupClient;
        use crate::engine::credentials::Token;

        let root = tmp_root("clickup_advance_status");
        let td = clickup_type_def(Some("list123"));

        // An existing mapped doc at "open" with a stale lock baseline.
        let mut map = TaskMap::load(&root).unwrap();
        map.insert("TASK-90abc", "90abc", "1700000000000");
        map.save(&root).unwrap();

        // ClickUp echoes the task at its new status with a fresh date_updated.
        let advanced = scripted_task(
            r#"{
                "id": "90abc",
                "name": "My task",
                "status": {"status": "in progress"},
                "date_updated": "1774587145901",
                "markdown_description": "the body",
                "creator": {"username": "Jack"}
            }"#,
        );
        // The pre-write optimistic-lock fetch matches the baseline, so the
        // status push proceeds.
        let remote_unchanged = scripted_task(
            r#"{
                "id": "90abc",
                "name": "My task",
                "status": {"status": "open"},
                "date_updated": "1700000000000"
            }"#,
        );
        let fake = FakeClickupClient::valid(clickup_user())
            .with_viewed_task(remote_unchanged)
            .with_updated_task(advanced);
        let calls = fake.update_calls();

        let mut store = ClickupTasksStore {
            client: Box::new(fake),
            root: root.clone(),
            config: Config::default(),
            token: Some(Token::new("pk_x")),
        };

        // An advance is a status-only update; the CLI's local gate is bypassed
        // for clickup-tasks, so the store simply pushes the raw string.
        store
            .update(&td, "TASK-90abc", &[("status", "in progress")])
            .unwrap();

        // Exactly one PUT, to the resolved task id, carrying the raw status only.
        let recorded = calls.borrow();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, "90abc");
        let payload = &recorded[0].1;
        assert_eq!(payload.status, Some("in progress".to_string()));
        // A status-only advance touches no native field.
        assert_eq!(payload.name, None);
        assert_eq!(payload.markdown_content, None);
        assert_eq!(payload.priority, None);

        // Round-trip: the cache now reflects the new raw status verbatim.
        let cache = root.join(".lazyspec/cache/task/TASK-90abc.md");
        let content = std::fs::read_to_string(&cache).unwrap();
        assert!(content.contains("status: in progress"), "got:\n{content}");

        // The lock baseline is bumped to the returned task's date_updated.
        let map = TaskMap::load(&root).unwrap();
        let entry = map.get("TASK-90abc").unwrap();
        assert_eq!(entry.task_id, "90abc");
        assert_eq!(entry.updated_at, "1774587145901");
    }

    // STORY-222 AC4: `update --assignee <id>` on a clickup-tasks doc PUTs the
    // native `assignees {add, rem}` payload via `update_task` and re-materializes
    // the echoed task into the cache.
    #[test]
    fn clickup_update_assignee_puts_add_rem_payload_via_update_task() {
        use crate::engine::clickup::FakeClickupClient;
        use crate::engine::credentials::Token;

        let root = tmp_root("clickup_update_assignee");
        let td = clickup_type_def(Some("list123"));

        let mut map = TaskMap::load(&root).unwrap();
        map.insert("TASK-90abc", "90abc", "1700000000000");
        map.save(&root).unwrap();

        let remote_unchanged = scripted_task(
            r#"{
                "id": "90abc",
                "name": "My task",
                "status": {"status": "open"},
                "date_updated": "1700000000000"
            }"#,
        );
        // ClickUp echoes the task with the new assignee.
        let updated = scripted_task(
            r#"{
                "id": "90abc",
                "name": "My task",
                "status": {"status": "open"},
                "date_updated": "1774587145901",
                "assignees": [{"username": "carol"}]
            }"#,
        );
        let fake = FakeClickupClient::valid(clickup_user())
            .with_viewed_task(remote_unchanged)
            .with_updated_task(updated);
        let calls = fake.update_calls();

        let mut store = ClickupTasksStore {
            client: Box::new(fake),
            root: root.clone(),
            config: Config::default(),
            token: Some(Token::new("pk_x")),
        };

        store
            .update(&td, "TASK-90abc", &[("assignee", "183")])
            .unwrap();

        let recorded = calls.borrow();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, "90abc");
        assert_eq!(
            recorded[0].1.assignees,
            Some(crate::engine::clickup::TaskAssigneeUpdate {
                add: vec![183],
                rem: vec![],
            })
        );

        // Re-materialized cache reflects the echoed assignee.
        let content =
            std::fs::read_to_string(root.join(".lazyspec/cache/task/TASK-90abc.md")).unwrap();
        assert!(content.contains("assignee: carol"), "got:\n{content}");
    }

    #[test]
    fn clickup_update_without_token_errors() {
        use crate::engine::clickup::FakeClickupClient;

        let root = tmp_root("clickup_update_no_token");
        let td = clickup_type_def(Some("list123"));
        let fake = FakeClickupClient::valid(clickup_user());

        let mut store = ClickupTasksStore {
            client: Box::new(fake),
            root,
            config: Config::default(),
            token: None,
        };

        let err = store
            .update(&td, "TASK-90abc", &[("priority", "high")])
            .unwrap_err();
        assert!(err.to_string().contains("setup clickup"), "got: {err}");
    }

    #[test]
    fn clickup_update_unmapped_doc_errors() {
        use crate::engine::clickup::FakeClickupClient;
        use crate::engine::credentials::Token;

        let root = tmp_root("clickup_update_unmapped");
        let td = clickup_type_def(Some("list123"));
        let fake = FakeClickupClient::valid(clickup_user());

        let mut store = ClickupTasksStore {
            client: Box::new(fake),
            root,
            config: Config::default(),
            token: Some(Token::new("pk_x")),
        };

        let err = store
            .update(&td, "TASK-missing", &[("priority", "high")])
            .unwrap_err();
        assert!(
            err.to_string().contains("not mapped to a ClickUp task"),
            "got: {err}"
        );
    }

    #[test]
    fn clickup_update_rejects_stale_write_and_performs_no_put() {
        use crate::engine::clickup::FakeClickupClient;
        use crate::engine::credentials::Token;

        let root = tmp_root("clickup_update_stale_conflict");
        let td = clickup_type_def(Some("list123"));

        // The doc's recorded baseline lags behind ClickUp: an external change
        // advanced the task's date_updated after our last fetch.
        let mut map = TaskMap::load(&root).unwrap();
        map.insert("TASK-90abc", "90abc", "1700000000000");
        map.save(&root).unwrap();

        // The pre-write fetch sees a *newer* date_updated than the baseline.
        let remote_changed = scripted_task(
            r#"{
                "id": "90abc",
                "name": "Changed elsewhere",
                "status": {"status": "in progress"},
                "date_updated": "1774587145901"
            }"#,
        );
        // No update_task is scripted: if the store reached the PUT it would fail
        // for a different reason, but the conflict must stop it before that.
        let fake = FakeClickupClient::valid(clickup_user()).with_viewed_task(remote_changed);
        let update_calls = fake.update_calls();
        let view_calls = fake.view_calls();

        let mut store = ClickupTasksStore {
            client: Box::new(fake),
            root: root.clone(),
            config: Config::default(),
            token: Some(Token::new("pk_x")),
        };

        let err = store
            .update(&td, "TASK-90abc", &[("title", "My local edit")])
            .unwrap_err();

        // The error names the conflict and points at the recovery path.
        let msg = err.to_string();
        assert!(msg.contains("changed on ClickUp"), "got: {msg}");
        assert!(msg.contains("lazyspec fetch"), "got: {msg}");

        // No clobber: the pre-write fetch fired, but no PUT followed.
        assert_eq!(view_calls.borrow().len(), 1, "expected exactly one GET");
        assert!(
            update_calls.borrow().is_empty(),
            "a conflicting write must not PUT"
        );

        // The recorded baseline is left untouched -- nothing was overwritten.
        let map = TaskMap::load(&root).unwrap();
        assert_eq!(map.get("TASK-90abc").unwrap().updated_at, "1700000000000");
    }

    #[test]
    fn clickup_advance_rejects_stale_write_and_performs_no_put() {
        use crate::engine::clickup::FakeClickupClient;
        use crate::engine::credentials::Token;

        let root = tmp_root("clickup_advance_stale_conflict");
        let td = clickup_type_def(Some("list123"));

        let mut map = TaskMap::load(&root).unwrap();
        map.insert("TASK-90abc", "90abc", "1700000000000");
        map.save(&root).unwrap();

        let remote_changed = scripted_task(
            r#"{
                "id": "90abc",
                "name": "Changed elsewhere",
                "status": {"status": "review"},
                "date_updated": "1774587145901"
            }"#,
        );
        let fake = FakeClickupClient::valid(clickup_user()).with_viewed_task(remote_changed);
        let update_calls = fake.update_calls();

        let mut store = ClickupTasksStore {
            client: Box::new(fake),
            root: root.clone(),
            config: Config::default(),
            token: Some(Token::new("pk_x")),
        };

        // An advance routes through update; the optimistic lock guards it too.
        let err = store
            .update(&td, "TASK-90abc", &[("status", "in progress")])
            .unwrap_err();

        assert!(err.to_string().contains("changed on ClickUp"), "got: {err}");
        assert!(
            update_calls.borrow().is_empty(),
            "a conflicting advance must not PUT the status"
        );
    }

    #[test]
    fn clickup_update_proceeds_when_remote_matches_baseline() {
        use crate::engine::clickup::FakeClickupClient;
        use crate::engine::credentials::Token;

        let root = tmp_root("clickup_update_lock_ok");
        let td = clickup_type_def(Some("list123"));

        let mut map = TaskMap::load(&root).unwrap();
        map.insert("TASK-90abc", "90abc", "1700000000000");
        map.save(&root).unwrap();

        // Remote date_updated equals the baseline: no external change, so the
        // write is allowed through and the PUT fires.
        let remote_unchanged = scripted_task(
            r#"{
                "id": "90abc",
                "name": "My task",
                "status": {"status": "open"},
                "date_updated": "1700000000000"
            }"#,
        );
        let updated = scripted_task(
            r#"{
                "id": "90abc",
                "name": "Edited",
                "status": {"status": "open"},
                "date_updated": "1774587145901",
                "markdown_description": "new body"
            }"#,
        );
        let fake = FakeClickupClient::valid(clickup_user())
            .with_viewed_task(remote_unchanged)
            .with_updated_task(updated);
        let update_calls = fake.update_calls();
        let view_calls = fake.view_calls();

        let mut store = ClickupTasksStore {
            client: Box::new(fake),
            root: root.clone(),
            config: Config::default(),
            token: Some(Token::new("pk_x")),
        };

        store
            .update(&td, "TASK-90abc", &[("body", "new body")])
            .unwrap();

        // The lock check fetched, then the write proceeded with a single PUT.
        assert_eq!(view_calls.borrow().len(), 1);
        assert_eq!(update_calls.borrow().len(), 1);

        // The baseline advanced to the returned task's fresh date_updated.
        let map = TaskMap::load(&root).unwrap();
        assert_eq!(map.get("TASK-90abc").unwrap().updated_at, "1774587145901");
    }

    #[test]
    fn clickup_update_empty_baseline_skips_lock_check_and_writes() {
        use crate::engine::clickup::FakeClickupClient;
        use crate::engine::credentials::Token;

        let root = tmp_root("clickup_update_no_baseline");
        let td = clickup_type_def(Some("list123"));

        // No baseline timestamp (e.g. a create whose echo carried no
        // date_updated): there is nothing to race against, so the lock is skipped
        // and no pre-write GET is issued.
        let mut map = TaskMap::load(&root).unwrap();
        map.insert("TASK-90abc", "90abc", "");
        map.save(&root).unwrap();

        let updated = scripted_task(
            r#"{
                "id": "90abc",
                "name": "Edited",
                "status": {"status": "open"},
                "date_updated": "1774587145901",
                "markdown_description": "new body"
            }"#,
        );
        // get_task is intentionally left unscripted (it would error if called),
        // proving the empty baseline short-circuits before the fetch.
        let fake = FakeClickupClient::valid(clickup_user()).with_updated_task(updated);
        let update_calls = fake.update_calls();
        let view_calls = fake.view_calls();

        let mut store = ClickupTasksStore {
            client: Box::new(fake),
            root: root.clone(),
            config: Config::default(),
            token: Some(Token::new("pk_x")),
        };

        store
            .update(&td, "TASK-90abc", &[("body", "new body")])
            .unwrap();

        assert!(view_calls.borrow().is_empty(), "no baseline -> no GET");
        assert_eq!(update_calls.borrow().len(), 1);
    }

    #[test]
    fn clickup_delete_archives_task_and_leaves_cache_and_map_for_sync() {
        use crate::engine::clickup::FakeClickupClient;
        use crate::engine::credentials::Token;

        let root = tmp_root("clickup_delete_archive");
        let td = clickup_type_def(Some("list123"));

        // An existing mapped, cached doc (from an earlier fetch/create).
        let mut map = TaskMap::load(&root).unwrap();
        map.insert("TASK-90abc", "90abc", "1774587145901");
        map.save(&root).unwrap();
        let cache = root.join(".lazyspec/cache/task/TASK-90abc.md");
        std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
        std::fs::write(&cache, "---\ntitle: My task\n---\nbody\n").unwrap();

        let fake = FakeClickupClient::valid(clickup_user());
        let archive_calls = fake.archive_calls();

        let mut store = ClickupTasksStore {
            client: Box::new(fake),
            root: root.clone(),
            config: Config::default(),
            token: Some(Token::new("pk_x")),
        };

        store.delete(&td, "TASK-90abc").unwrap();

        // Exactly one archive (PUT /task/{id} {"archived":true}) for the resolved
        // task id; there is no hard-delete method on the client to call.
        let recorded = archive_calls.borrow();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0], "90abc");

        // Archive is not an eager local eviction: the cache file and the map
        // entry remain, and the next `fetch` (task gone from the list) removes
        // them (RFC-056 §Design).
        assert!(
            cache.exists(),
            "archive must not eagerly delete the cache file"
        );
        let map = TaskMap::load(&root).unwrap();
        assert!(
            map.get("TASK-90abc").is_some(),
            "archive must not eagerly drop the TaskMap entry"
        );
    }

    #[test]
    fn clickup_delete_without_token_errors() {
        use crate::engine::clickup::FakeClickupClient;

        let root = tmp_root("clickup_delete_no_token");
        let td = clickup_type_def(Some("list123"));
        let fake = FakeClickupClient::valid(clickup_user());
        let archive_calls = fake.archive_calls();

        let mut store = ClickupTasksStore {
            client: Box::new(fake),
            root,
            config: Config::default(),
            token: None,
        };

        let err = store.delete(&td, "TASK-90abc").unwrap_err();
        assert!(err.to_string().contains("setup clickup"), "got: {err}");
        assert!(
            archive_calls.borrow().is_empty(),
            "a missing token must not archive"
        );
    }

    #[test]
    fn clickup_delete_unmapped_doc_errors() {
        use crate::engine::clickup::FakeClickupClient;
        use crate::engine::credentials::Token;

        let root = tmp_root("clickup_delete_unmapped");
        let td = clickup_type_def(Some("list123"));
        let fake = FakeClickupClient::valid(clickup_user());
        let archive_calls = fake.archive_calls();

        let mut store = ClickupTasksStore {
            client: Box::new(fake),
            root,
            config: Config::default(),
            token: Some(Token::new("pk_x")),
        };

        let err = store.delete(&td, "TASK-missing").unwrap_err();
        assert!(
            err.to_string().contains("not mapped to a ClickUp task"),
            "got: {err}"
        );
        assert!(
            archive_calls.borrow().is_empty(),
            "an unmapped doc must not archive"
        );
    }

    #[test]
    fn clickup_delete_surfaces_archive_error() {
        use crate::engine::clickup::{ClickupError, FakeClickupClient};
        use crate::engine::credentials::Token;

        let root = tmp_root("clickup_delete_archive_err");
        let td = clickup_type_def(Some("list123"));

        let mut map = TaskMap::load(&root).unwrap();
        map.insert("TASK-90abc", "90abc", "1774587145901");
        map.save(&root).unwrap();

        let fake = FakeClickupClient::valid(clickup_user())
            .failing_archive(ClickupError::Upstream { status: 500 });

        let mut store = ClickupTasksStore {
            client: Box::new(fake),
            root: root.clone(),
            config: Config::default(),
            token: Some(Token::new("pk_x")),
        };

        let err = store.delete(&td, "TASK-90abc").unwrap_err();
        assert!(
            err.to_string().contains("ClickUp server error"),
            "got: {err}"
        );

        // A failed archive left the local state intact for a retry.
        let map = TaskMap::load(&root).unwrap();
        assert!(map.get("TASK-90abc").is_some());
    }

    #[test]
    fn filesystem_create_and_update() {
        let root = tmp_root("fs_create_update");
        let config = Config::default();

        let mut fs_store = FilesystemStore {
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
        let mut gh_store = GithubIssuesStore {
            client: Box::new(MockGhClient::new()),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: IssueMap::load(&root).unwrap(),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        let result = gh_store
            .create(&td, "my title", "author", "body text")
            .unwrap();

        assert_eq!(result.id, "RFC-1");
        assert!(result
            .path
            .to_string_lossy()
            .contains(".lazyspec/cache/rfc/"));
        assert!(root.join(&result.path).exists());

        // Issue body sent to GH should NOT contain author: in lazyspec comment
        let create_body = gh_store.mock().last_create_body.borrow();
        let create_body_str = create_body
            .as_deref()
            .expect("issue_create should have been called");
        assert!(
            create_body_str.contains("<!-- lazyspec"),
            "body should have lazyspec comment"
        );
        assert!(
            !create_body_str.contains("author:"),
            "issue body should not contain author: in lazyspec comment, got: {}",
            create_body_str
        );

        // Cache file should still have author in frontmatter
        let content = std::fs::read_to_string(root.join(&result.path)).unwrap();
        let (yaml, _) = crate::engine::document::split_frontmatter(&content).unwrap();
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&yaml).expect("valid YAML frontmatter");
        assert_eq!(parsed["title"].as_str().unwrap(), "my title");
        assert_eq!(parsed["type"].as_str().unwrap(), "rfc");
        // STORY-224: a github-issues type with no declared lifecycle is born at the
        // store's canonical first-active state `open`, not the empty-fallback `draft`.
        assert_eq!(parsed["status"].as_str().unwrap(), "open");
        assert_eq!(parsed["author"].as_str().unwrap(), "author");
        assert!(content.contains("body text"));
    }

    #[test]
    fn github_issues_create_seeds_first_lifecycle_state() {
        // BUG-007: a github-backed type whose lifecycle starts at `reported`
        // must be born `reported`, not the hardcoded `draft`.
        let root = tmp_root("gh_create_seed_first_state");
        let mut gh_store = GithubIssuesStore {
            client: Box::new(MockGhClient::new()),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: IssueMap::load(&root).unwrap(),
            issue_cache: IssueCache::new(&root),
        };

        let mut td = test_type_def(StoreBackend::GithubIssues);
        td.lifecycle = crate::engine::config::Lifecycle {
            states: vec!["reported".to_string(), "triaged".to_string()],
            edges: vec![],
        };
        let result = gh_store.create(&td, "a bug", "author", "").unwrap();

        let content = std::fs::read_to_string(root.join(&result.path)).unwrap();
        let (yaml, _) = crate::engine::document::split_frontmatter(&content).unwrap();
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&yaml).expect("valid YAML frontmatter");
        assert_eq!(parsed["status"].as_str().unwrap(), "reported");
    }

    #[test]
    fn github_issues_create_default_lifecycle_still_draft() {
        // The default lifecycle's states[0] is `draft`, so a default-lifecycle
        // github type is unchanged by BUG-007's seeding.
        let root = tmp_root("gh_create_default_lifecycle_draft");
        let mut gh_store = GithubIssuesStore {
            client: Box::new(MockGhClient::new()),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: IssueMap::load(&root).unwrap(),
            issue_cache: IssueCache::new(&root),
        };

        let mut td = test_type_def(StoreBackend::GithubIssues);
        td.lifecycle = crate::engine::config::default_lifecycle();
        let result = gh_store.create(&td, "a doc", "author", "").unwrap();

        let content = std::fs::read_to_string(root.join(&result.path)).unwrap();
        let (yaml, _) = crate::engine::document::split_frontmatter(&content).unwrap();
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&yaml).expect("valid YAML frontmatter");
        assert_eq!(parsed["status"].as_str().unwrap(), "draft");
    }

    #[test]
    fn github_issues_create_updates_issue_map() {
        let root = tmp_root("gh_create_map");
        let mut gh_store = GithubIssuesStore {
            client: Box::new(MockGhClient::new()),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: IssueMap::load(&root).unwrap(),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store.create(&td, "mapped", "author", "").unwrap();

        let entry = gh_store
            .issue_map
            .get("RFC-1")
            .expect("issue map entry should exist");
        assert_eq!(entry.issue_number, 1);
        assert_eq!(entry.updated_at, "2026-03-27T00:00:00Z");
    }

    #[test]
    fn github_issues_create_uses_label_override() {
        let root = tmp_root("gh_create_label_override");
        let mut gh_store = GithubIssuesStore {
            client: Box::new(MockGhClient::new()),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: IssueMap::load(&root).unwrap(),
            issue_cache: IssueCache::new(&root),
        };

        let mut td = test_type_def(StoreBackend::GithubIssues);
        td.label_override = Some("Ticket".to_string());
        gh_store.create(&td, "custom", "author", "").unwrap();

        assert_eq!(
            *gh_store.mock().last_create_labels.borrow(),
            vec!["Ticket".to_string()]
        );
    }

    #[test]
    fn github_issues_create_with_native_type_attaches_no_labels() {
        // BUG-010: a type whose `github_issue_type` is set is classified by the
        // native GitHub issue type, so no `lazyspec:{name}` identity label is
        // attached. The native type is still pushed.
        let root = tmp_root("gh_create_native_type_no_labels");
        write_bug_snapshot(&root);
        let td = TypeDef {
            github_issue_type: Some("Bug".to_string()),
            github_issue_tag: None,
            ..test_type_def(StoreBackend::GithubIssues)
        };

        let mut gh_store = GithubIssuesStore {
            client: Box::new(MockGhClient::new().with_graphql_responses(vec![
                issue_node_id_response(),
                serde_json::json!({"data": {"updateIssue": {"issue": {"id": "I_node1"}}}}),
            ])),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: IssueMap::load(&root).unwrap(),
            issue_cache: IssueCache::new(&root),
        };

        gh_store
            .create(&td, "my title", "author", "body text")
            .unwrap();

        assert!(
            gh_store.mock().last_create_labels.borrow().is_empty(),
            "native-typed issue must carry zero labels, got: {:?}",
            gh_store.mock().last_create_labels.borrow()
        );
        let calls = gh_store.mock().graphql_calls.borrow();
        let mutations = calls
            .iter()
            .filter(|(q, _)| q.contains("updateIssue"))
            .count();
        assert_eq!(mutations, 1, "native issue type must still be pushed");
    }

    #[test]
    fn github_issues_create_with_native_type_and_tag_attaches_only_tag() {
        // BUG-010: when both `github_issue_type` and `github_issue_tag` are set,
        // only the tag label is attached (no `lazyspec:{name}` identity label).
        let root = tmp_root("gh_create_native_type_with_tag");
        write_bug_snapshot(&root);
        let td = TypeDef {
            github_issue_type: Some("Bug".to_string()),
            github_issue_tag: Some("bug".to_string()),
            ..test_type_def(StoreBackend::GithubIssues)
        };

        let mut gh_store = GithubIssuesStore {
            client: Box::new(MockGhClient::new().with_graphql_responses(vec![
                issue_node_id_response(),
                serde_json::json!({"data": {"updateIssue": {"issue": {"id": "I_node1"}}}}),
            ])),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: IssueMap::load(&root).unwrap(),
            issue_cache: IssueCache::new(&root),
        };

        gh_store
            .create(&td, "my title", "author", "body text")
            .unwrap();

        assert_eq!(
            *gh_store.mock().last_create_labels.borrow(),
            vec!["bug".to_string()],
            "both set -> only the tag label attached"
        );
    }

    #[test]
    fn github_issues_create_persists_issue_map() {
        let root = tmp_root("gh_create_persist");
        let mut gh_store = GithubIssuesStore {
            client: Box::new(MockGhClient::new()),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: IssueMap::load(&root).unwrap(),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store.create(&td, "persist", "author", "").unwrap();

        let reloaded = IssueMap::load(&root).unwrap();
        assert!(reloaded.get("RFC-1").is_some());
    }

    #[test]
    fn github_issues_create_increments_id() {
        let root = tmp_root("gh_create_incr");
        let mut gh_store = GithubIssuesStore {
            client: Box::new(MockGhClient::new()),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: IssueMap::load(&root).unwrap(),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        let first = gh_store.create(&td, "first", "author", "").unwrap();
        let second = gh_store.create(&td, "second", "author", "").unwrap();

        assert_eq!(first.id, "RFC-1");
        assert_eq!(second.id, "RFC-2");
    }

    #[test]
    fn github_issues_create_uses_prefix_not_name() {
        let root = tmp_root("gh_create_prefix");
        let mut gh_store = GithubIssuesStore {
            client: Box::new(MockGhClient::new()),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: IssueMap::load(&root).unwrap(),
            issue_cache: IssueCache::new(&root),
        };

        let td = TypeDef {
            name: "github".to_string(),
            plural: "gh".to_string(),
            dir: "docs/gh".to_string(),
            prefix: "GH".to_string(),
            icon: None,
            numbering: NumberingStrategy::Incremental,
            subdirectory: false,
            store: StoreBackend::GithubIssues,
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
            clickup_list_id: None,
            clickup_task_type: None,
            clickup_custom_field_map: None,
        };

        let result = gh_store.create(&td, "test prefix", "author", "").unwrap();
        assert_eq!(result.id, "GH-1");
        assert!(
            result.path.to_string_lossy().contains("GH-1"),
            "path should use prefix GH, got: {}",
            result.path.display()
        );
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
            id: String::new(),
            url: String::new(),
            title: "My RFC".to_string(),
            body: issue_body,
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        let outcome = gh_store
            .update(&td, "RFC-001", &[("status", "accepted")])
            .unwrap();

        // BUG-006: a non-pushing backend syncs synchronously as part of the API
        // call, so its mutation always reports `Synced` (never `LocalOnly`).
        assert_eq!(outcome, PushOutcome::Synced);

        // Re-serialized body sent to GH should not contain author:
        let captured = gh_store.mock().last_edit_body.borrow();
        let body_str = captured
            .as_deref()
            .expect("issue_edit should have been called with body");
        assert!(
            !body_str.contains("author:"),
            "re-serialized issue body should not contain author:, got: {}",
            body_str
        );
    }

    // STORY-222 AC4: `update --assignee X` on a github-issues doc fires the
    // native `gh issue edit` assignee mutation, diffing against the remote's
    // current assignee (add the new login, remove the old), and reflects the new
    // assignee in the local cache. The body comment never carries assignee.
    #[test]
    fn github_issues_update_assignee_fires_native_edit_and_diffs() {
        let root = tmp_root("gh_update_assignee");
        let issue_body = make_issue_body("agent-7", "2026-03-27", None, "original body");
        let view_issue = GhIssue {
            number: 42,
            id: String::new(),
            url: String::new(),
            title: "My RFC".to_string(),
            body: issue_body,
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![crate::engine::gh::GhAssignee {
                login: "bob".to_string(),
            }],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store
            .update(&td, "RFC-001", &[("assignee", "carol")])
            .unwrap();

        // Diff vs remote: add the requested login, remove the current one.
        let recorded = gh_store.mock().last_set_assignee.borrow().clone();
        assert_eq!(
            recorded,
            Some((vec!["carol".to_string()], vec!["bob".to_string()])),
            "assignee edit should add carol and remove bob"
        );

        // The re-serialized body must NOT carry an assignee (native field).
        let body = gh_store.mock().last_edit_body.borrow();
        assert!(
            !body.as_deref().unwrap_or("").contains("assignee"),
            "assignee must stay out of the issue body comment"
        );

        // Local cache reflects the new assignee.
        let cache = std::fs::read_to_string(
            root.join(".lazyspec/cache")
                .join(&td.name)
                .join("RFC-001.md"),
        )
        .unwrap();
        assert!(
            cache.contains("assignee: carol"),
            "cache should reflect the new assignee, got:\n{cache}"
        );
    }

    // STORY-222 AC4: setting the assignee to the login already on the remote is a
    // no-op -- no `gh issue edit` assignee mutation fires (empty diff).
    #[test]
    fn github_issues_update_assignee_unchanged_is_noop() {
        let root = tmp_root("gh_update_assignee_noop");
        let issue_body = make_issue_body("agent-7", "2026-03-27", None, "body");
        let view_issue = GhIssue {
            number: 42,
            id: String::new(),
            url: String::new(),
            title: "My RFC".to_string(),
            body: issue_body,
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![crate::engine::gh::GhAssignee {
                login: "carol".to_string(),
            }],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store
            .update(&td, "RFC-001", &[("assignee", "carol")])
            .unwrap();

        assert!(
            gh_store.mock().last_set_assignee.borrow().is_none(),
            "assignee unchanged vs remote must not fire an edit"
        );
    }

    #[test]
    fn github_issues_update_optimistic_lock_failure() {
        let root = tmp_root("gh_update_lock");
        let issue_body = make_issue_body("agent-7", "2026-03-27", None, "");
        let view_issue = GhIssue {
            number: 42,
            id: String::new(),
            url: String::new(),
            title: "My RFC".to_string(),
            body: issue_body,
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:45:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
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

    // ITERATION-222: the conflict-free native-relation resync re-mirrors the
    // cache body WITHOUT the optimistic lock. An out-of-band remote updated_at
    // bump (e.g. a comment) must not abort it; the issue-map baseline reconciles
    // to the remote's CURRENT timestamp (not cleared, not left stale).
    #[test]
    fn resync_after_native_edge_ignores_updated_at_and_records_fresh() {
        let root = tmp_root("gh_resync_native");
        let cache_dir = root.join(".lazyspec/cache/rfc");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let cache_content = concat!(
            "---\n",
            "title: My RFC\n",
            "type: rfc\n",
            "status: draft\n",
            "author: agent-7\n",
            "date: 2026-03-27\n",
            "tags: []\n",
            "related:\n",
            "- targets: MILESTONE-3\n",
            "---\n",
            "Some body text.\n",
        );
        std::fs::write(cache_dir.join("RFC-001.md"), cache_content).unwrap();

        // Remote moved to 10:45 since our last fetch at 10:00.
        let remote_body = make_issue_body("agent-7", "2026-03-27", None, "Some body text.");
        let view_issue = GhIssue {
            number: 42,
            id: "I_node42".to_string(),
            url: String::new(),
            title: "My RFC".to_string(),
            body: remote_body,
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:45:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store
            .resync_after_native_edge(&td, "RFC-001")
            .expect("resync must not abort on an out-of-band updated_at bump");

        // The body was re-mirrored (issue_edit ran).
        assert!(
            gh_store.mock().last_edit_body.borrow().is_some(),
            "issue_edit should have been called"
        );

        // Baseline reconciled to the remote's fresh timestamp.
        let reloaded = IssueMap::load(&root).unwrap();
        let entry = reloaded.get("RFC-001").unwrap();
        assert_eq!(entry.updated_at, "2026-03-27T10:45:00Z");
        assert_eq!(entry.node_id, "I_node42");
    }

    #[test]
    fn github_issues_update_status_complete_closes_issue() {
        let root = tmp_root("gh_update_close");
        let issue_body = make_issue_body("agent-7", "2026-03-27", None, "");
        let view_issue = GhIssue {
            number: 42,
            id: String::new(),
            url: String::new(),
            title: "My RFC".to_string(),
            body: issue_body,
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store
            .update(&td, "RFC-001", &[("status", "complete")])
            .unwrap();
        assert!(gh_store.mock().closed.get());
        assert!(!gh_store.mock().reopened.get());
    }

    #[test]
    fn github_issues_update_status_draft_reopens_issue() {
        let root = tmp_root("gh_update_reopen");
        let issue_body = make_issue_body("agent-7", "2026-03-27", None, "");
        let view_issue = GhIssue {
            number: 42,
            id: String::new(),
            url: String::new(),
            title: "My RFC".to_string(),
            body: issue_body,
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "CLOSED".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store
            .update(&td, "RFC-001", &[("status", "draft")])
            .unwrap();
        assert!(gh_store.mock().reopened.get());
        assert!(!gh_store.mock().closed.get());
    }

    fn custom_lifecycle() -> Lifecycle {
        let edge = |from: &str, to: &str| crate::engine::config::Edge {
            from: from.into(),
            to: to.into(),
        };
        Lifecycle {
            states: vec!["backlog".into(), "doing".into(), "shipped".into()],
            edges: vec![edge("backlog", "doing"), edge("doing", "shipped")],
        }
    }

    fn open_issue_store(root: &std::path::Path) -> GithubIssuesStore {
        let issue_body = make_issue_body("agent-7", "2026-03-27", None, "");
        let view_issue = GhIssue {
            number: 42,
            id: String::new(),
            url: String::new(),
            title: "My RFC".to_string(),
            body: issue_body,
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };
        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");
        GithubIssuesStore {
            client: Box::new(client),
            root: root.to_path_buf(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(root),
        }
    }

    // BUG-008 write half: transition into a CUSTOM terminal state (not
    // `complete`) closes the remote issue via the lifecycle-aware classifier.
    #[test]
    fn github_issues_update_custom_terminal_status_closes_issue() {
        let root = tmp_root("gh_update_close_custom");
        let mut gh_store = open_issue_store(&root);
        let mut td = test_type_def(StoreBackend::GithubIssues);
        td.lifecycle = custom_lifecycle();

        gh_store
            .update(&td, "RFC-001", &[("status", "shipped")])
            .unwrap();
        assert!(gh_store.mock().closed.get(), "custom terminal must close");
        assert!(!gh_store.mock().reopened.get());
    }

    // A transition into a custom NON-terminal (intermediate) state must NOT
    // close the open issue -- the pre-319 non-lifecycle-aware classifier wrongly
    // treated any non-canonical status as closed.
    #[test]
    fn github_issues_update_custom_intermediate_status_does_not_close() {
        let root = tmp_root("gh_update_intermediate_custom");
        let mut gh_store = open_issue_store(&root);
        let mut td = test_type_def(StoreBackend::GithubIssues);
        td.lifecycle = custom_lifecycle();

        gh_store
            .update(&td, "RFC-001", &[("status", "doing")])
            .unwrap();
        assert!(
            !gh_store.mock().closed.get(),
            "custom intermediate must not close"
        );
        assert!(!gh_store.mock().reopened.get());
    }

    #[test]
    fn github_issues_update_not_in_map() {
        let root = tmp_root("gh_update_nomap");
        let mut gh_store = GithubIssuesStore {
            client: Box::new(MockGhClient::new()),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: IssueMap::load(&root).unwrap(),
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
            id: String::new(),
            url: String::new(),
            title: "My RFC".to_string(),
            body: issue_body,
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store.delete(&td, "RFC-001").unwrap();

        assert!(gh_store.mock().closed.get());
        let title = gh_store.mock().last_edit_title.borrow();
        assert_eq!(title.as_deref(), Some("[DELETED] My RFC"));
        let labels_remove = gh_store.mock().last_edit_labels_remove.borrow();
        assert_eq!(*labels_remove, vec!["lazyspec:rfc".to_string()]);
        assert!(gh_store.issue_map.get("RFC-001").is_none());
    }

    #[test]
    fn github_issues_delete_optimistic_lock_failure() {
        let root = tmp_root("gh_delete_lock");
        let view_issue = GhIssue {
            number: 42,
            id: String::new(),
            url: String::new(),
            title: "My RFC".to_string(),
            body: String::new(),
            labels: vec![],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:45:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        let err = gh_store.delete(&td, "RFC-001").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("has been modified on GitHub"), "got: {}", msg);
        assert!(!gh_store.mock().closed.get());
    }

    #[test]
    fn github_issues_delete_not_in_map() {
        let root = tmp_root("gh_delete_nomap");
        let mut gh_store = GithubIssuesStore {
            client: Box::new(MockGhClient::new()),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: IssueMap::load(&root).unwrap(),
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
            id: String::new(),
            url: String::new(),
            title: "My RFC".to_string(),
            body: String::new(),
            labels: vec![],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let cache_dir = root.join(".lazyspec/cache/rfc");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let cache_file = cache_dir.join("RFC-001.md");
        std::fs::write(&cache_file, "cached content").unwrap();
        assert!(cache_file.exists());

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store.delete(&td, "RFC-001").unwrap();
        assert!(!cache_file.exists());
    }

    fn milestone_store(
        root: &std::path::Path,
        client: MockGhMilestoneClient,
    ) -> GithubMilestonesStore {
        GithubMilestonesStore {
            client: Box::new(client),
            root: root.to_path_buf(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: IssueMap::load(root).unwrap(),
        }
    }

    // AC1: create calls milestone_create with title/description, id == make_id(number),
    // issue_map maps doc_id -> number, cache file written.
    #[test]
    fn milestone_create_writes_cache_and_maps_number() {
        let root = tmp_root("ms_create");
        let mut store = milestone_store(&root, MockGhMilestoneClient::new());
        let td = test_type_def(StoreBackend::GithubMilestones);

        let created = store
            .create(&td, "v1.0", "author", "first release")
            .unwrap();

        assert_eq!(created.id, td.make_id(1));
        assert_eq!(store.mock().create_calls.get(), 1);
        let ms = &store.mock().milestones.borrow()[0];
        assert_eq!(ms.title, "v1.0");
        assert_eq!(ms.description, "first release");

        let entry = store.issue_map.get(&created.id).unwrap();
        assert_eq!(entry.issue_number, 1);

        assert!(root.join(&created.path).exists());
    }

    // ITERATION-225 (AC2): the cached milestone doc carries NO stored relations;
    // `targeted-by` is derived virtually in `build_links` as the reverse of each
    // issue's forward `targets` edge, never written here.
    #[test]
    fn milestone_meta_stores_no_relations() {
        let root = tmp_root("ms_no_stored_relations");
        let client = MockGhMilestoneClient::new();
        let store = milestone_store(&root, client);
        let td = test_type_def(StoreBackend::GithubMilestones);

        let milestone = crate::engine::gh::GhMilestone {
            number: 7,
            title: "v1.0".to_string(),
            description: String::new(),
            due_on: None,
            state: "open".to_string(),
            open_issues: 3,
            closed_issues: 0,
            url: String::new(),
        };

        let meta = store.meta_from_milestone(&td, "MILESTONE-7", &milestone, "author");
        assert!(
            meta.related.is_empty(),
            "milestone doc must store no relations; targeted-by is virtual"
        );
    }

    // AC2: update [title, body, due_on] -> milestone_edit records changed fields;
    // re-read via milestone_view returns updates; cache reflects them.
    #[test]
    fn milestone_update_round_trips_changed_fields() {
        let root = tmp_root("ms_update");
        let mut store = milestone_store(&root, MockGhMilestoneClient::new());
        let td = test_type_def(StoreBackend::GithubMilestones);

        let created = store.create(&td, "v1.0", "author", "old desc").unwrap();
        store
            .update(
                &td,
                &created.id,
                &[
                    ("title", "v2.0"),
                    ("body", "new desc"),
                    ("due_on", "2026-09-01T00:00:00Z"),
                ],
            )
            .unwrap();

        let edit = store.mock().last_edit.borrow();
        let edit = edit.as_ref().unwrap();
        assert_eq!(edit.title.as_deref(), Some("v2.0"));
        assert_eq!(edit.description.as_deref(), Some("new desc"));
        assert_eq!(edit.due_on.as_deref(), Some("2026-09-01T00:00:00Z"));

        let viewed = store.client.milestone_view("owner/repo", 1).unwrap();
        assert_eq!(viewed.title, "v2.0");
        assert_eq!(viewed.description, "new desc");

        let cache = std::fs::read_to_string(root.join(&created.path)).unwrap();
        assert!(cache.contains("v2.0"), "cache title updated: {cache}");
        assert!(cache.contains("new desc"), "cache body updated: {cache}");
        assert!(
            cache.contains("2026-09-01T00:00:00Z"),
            "cache due_on updated: {cache}"
        );
    }

    // AC3: state <-> status mappings, and loading a closed milestone materializes
    // a closed-equivalent status in the cache.
    #[test]
    fn milestone_state_status_mappings() {
        // STORY-223 AC1: read maps via the type's lifecycle -- closed -> terminal,
        // open -> first active. With the default lifecycle that is complete/draft.
        let lc = crate::engine::config::default_lifecycle();
        assert_eq!(
            milestone_state_to_status("closed", &lc).as_str(),
            "complete"
        );
        assert_eq!(milestone_state_to_status("open", &lc).as_str(), "draft");
        // Write direction shares ITERATION-318's classifier: the default
        // terminal `complete` closes, `draft` stays open.
        assert!(!issue_body::status_maps_to_open(
            "complete",
            lc.terminal_status()
        ));
        assert!(issue_body::status_maps_to_open(
            "draft",
            lc.terminal_status()
        ));
    }

    // STORY-224 AC1: a milestone type with no declared lifecycle derives
    // open/closed from the store's canonical lifecycle -- open milestone -> "open",
    // closed -> "closed" (not the draft/complete empty-lifecycle fallback).
    #[test]
    fn milestone_state_status_undeclared_uses_canonical_open_closed() {
        let td = crate::engine::config::TypeDef::test_fixture(
            "milestone",
            crate::engine::config::StoreBackend::GithubMilestones,
        );
        let lc = td.effective_lifecycle();
        assert_eq!(milestone_state_to_status("open", &lc).as_str(), "open");
        assert_eq!(milestone_state_to_status("closed", &lc).as_str(), "closed");
        // write-through: "open" reopens, "closed" (terminal) closes.
        assert!(issue_body::status_maps_to_open(
            "open",
            lc.terminal_status()
        ));
        assert!(!issue_body::status_maps_to_open(
            "closed",
            lc.terminal_status()
        ));
    }

    // STORY-223 AC1: a custom-lifecycle milestone type inherits its own
    // first-active/terminal states from the remote open/closed state on read.
    #[test]
    fn milestone_state_status_custom_lifecycle() {
        let edge = |from: &str, to: &str| crate::engine::config::Edge {
            from: from.into(),
            to: to.into(),
        };
        let lc = Lifecycle {
            states: vec!["backlog".into(), "doing".into(), "shipped".into()],
            edges: vec![edge("backlog", "doing"), edge("doing", "shipped")],
        };
        assert_eq!(milestone_state_to_status("open", &lc).as_str(), "backlog");
        assert_eq!(milestone_state_to_status("closed", &lc).as_str(), "shipped");
    }

    // STORY-223 AC1/AC3: a closed milestone read via the store materializes the
    // type's terminal status in the cached doc, and an open one the first-active
    // status -- the remote state is inherited without local edits.
    #[test]
    fn milestone_read_inherits_remote_open_closed_into_lifecycle() {
        let root = tmp_root("ms_read_inherits");
        let client = MockGhMilestoneClient::new();
        let store = milestone_store(&root, client);
        let mut td = test_type_def(StoreBackend::GithubMilestones);
        td.lifecycle = crate::engine::config::default_lifecycle();

        let open_ms = crate::engine::gh::GhMilestone {
            number: 1,
            title: "v1".to_string(),
            description: String::new(),
            due_on: None,
            state: "open".to_string(),
            open_issues: 2,
            closed_issues: 0,
            url: String::new(),
        };
        let closed_ms = crate::engine::gh::GhMilestone {
            state: "closed".to_string(),
            number: 2,
            ..open_ms.clone()
        };

        let open_meta = store.meta_from_milestone(&td, "MILESTONE-1", &open_ms, "author");
        let closed_meta = store.meta_from_milestone(&td, "MILESTONE-2", &closed_ms, "author");
        assert_eq!(open_meta.status.as_str(), "draft");
        assert_eq!(closed_meta.status.as_str(), "complete");
    }

    // STORY-224: a milestone type with no declared lifecycle uses the canonical
    // open/closed DAG -- born `open`, transition to the terminal `closed` closes
    // the remote milestone and materializes `closed` in the cache.
    #[test]
    fn milestone_update_status_closed_closes_state_and_cache() {
        let root = tmp_root("ms_close");
        let mut store = milestone_store(&root, MockGhMilestoneClient::new());
        let td = test_type_def(StoreBackend::GithubMilestones);

        let created = store.create(&td, "v1.0", "author", "desc").unwrap();
        let cache = std::fs::read_to_string(root.join(&created.path)).unwrap();
        assert!(cache.contains("status: open"), "born open: {cache}");

        store
            .update(&td, &created.id, &[("status", "closed")])
            .unwrap();

        let edit = store.mock().last_edit.borrow();
        assert_eq!(edit.as_ref().unwrap().state.as_deref(), Some("closed"));

        let cache = std::fs::read_to_string(root.join(&created.path)).unwrap();
        assert!(cache.contains("status: closed"), "cache: {cache}");
    }

    // BUG-008 write half (milestones): transition into a CUSTOM terminal state
    // closes the remote milestone, routed through the shared ITERATION-318
    // classifier. Pre-319 the milestone-only mapping left `shipped` -> "open".
    #[test]
    fn milestone_update_custom_terminal_status_closes() {
        let root = tmp_root("ms_close_custom");
        let mut store = milestone_store(&root, MockGhMilestoneClient::new());
        let mut td = test_type_def(StoreBackend::GithubMilestones);
        td.lifecycle = custom_lifecycle();

        let created = store.create(&td, "v1.0", "author", "desc").unwrap();
        store
            .update(&td, &created.id, &[("status", "shipped")])
            .unwrap();

        let edit = store.mock().last_edit.borrow();
        assert_eq!(edit.as_ref().unwrap().state.as_deref(), Some("closed"));
    }

    // A transition into a custom non-terminal state leaves the milestone open.
    #[test]
    fn milestone_update_non_terminal_status_stays_open() {
        let root = tmp_root("ms_nonterminal_custom");
        let mut store = milestone_store(&root, MockGhMilestoneClient::new());
        let mut td = test_type_def(StoreBackend::GithubMilestones);
        td.lifecycle = custom_lifecycle();

        let created = store.create(&td, "v1.0", "author", "desc").unwrap();
        store
            .update(&td, &created.id, &[("status", "doing")])
            .unwrap();

        let edit = store.mock().last_edit.borrow();
        assert_eq!(edit.as_ref().unwrap().state.as_deref(), Some("open"));
    }

    // AC5: percent_complete is computed, and updating it is rejected (never PATCHed).
    #[test]
    fn percent_complete_computed() {
        assert_eq!(percent_complete(7, 3), Some(30));
        assert_eq!(percent_complete(0, 0), None);
        assert_eq!(percent_complete(0, 5), Some(100));
    }

    #[test]
    fn milestone_update_percent_complete_is_rejected() {
        let root = tmp_root("ms_pct_reject");
        let mut store = milestone_store(&root, MockGhMilestoneClient::new());
        let td = test_type_def(StoreBackend::GithubMilestones);
        let created = store.create(&td, "v1.0", "author", "desc").unwrap();

        let err = store
            .update(&td, &created.id, &[("percent_complete", "50")])
            .unwrap_err();
        assert!(err.to_string().contains("percent_complete"), "{err}");
        // No edit was issued.
        assert!(store.mock().last_edit.borrow().is_none());
    }

    #[test]
    fn milestone_delete_removes_milestone_map_and_cache() {
        let root = tmp_root("ms_delete");
        let mut store = milestone_store(&root, MockGhMilestoneClient::new());
        let td = test_type_def(StoreBackend::GithubMilestones);
        let created = store.create(&td, "v1.0", "author", "desc").unwrap();

        store.delete(&td, &created.id).unwrap();

        assert!(store.mock().milestones.borrow().is_empty());
        assert!(store.issue_map.get(&created.id).is_none());
        assert!(!root.join(&created.path).exists());
    }

    // AC6: dispatch routes a github-milestones type to the milestone store.
    #[test]
    fn dispatch_routes_to_github_milestones() {
        let root = tmp_root("dispatch_ms");
        let fs_store = FilesystemStore {
            root: root.clone(),
            config: Config::default(),
        };
        let ms_store = milestone_store(&root, MockGhMilestoneClient::new());

        let td = test_type_def(StoreBackend::GithubMilestones);
        let mut registry = DocumentStoreRegistry::new();
        registry.register(StoreBackend::Filesystem, Box::new(fs_store));
        registry.register(StoreBackend::GithubMilestones, Box::new(ms_store));
        let store = registry.for_type(&td).unwrap();
        let result = store.create(&td, "dispatched", "author", "");
        assert!(result.is_ok());
    }

    #[test]
    fn dispatch_github_milestones_without_backend_errors() {
        let root = tmp_root("dispatch_no_ms");
        let fs_store = FilesystemStore {
            root: root.clone(),
            config: Config::default(),
        };
        let td = test_type_def(StoreBackend::GithubMilestones);
        let mut registry = DocumentStoreRegistry::new();
        registry.register(StoreBackend::Filesystem, Box::new(fs_store));
        let result = registry.for_type(&td);
        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("github-milestones backend"));
    }

    // A project with no clickup-tasks type must not construct a real
    // ClickupHttpClient (eager reqwest client -> system CA load -> panics in
    // CA-less/hermetic environments). build_registry registers an
    // UnavailableStore instead, which errors loudly only on use.
    #[test]
    fn build_registry_without_clickup_type_registers_unavailable() {
        let root = tmp_root("registry_no_clickup");
        let config = Config::default();
        let mut registry = build_registry(&root, &config);

        let td = test_type_def(StoreBackend::ClickupTasks);
        let store = registry.for_type(&td).unwrap();
        let result = store.create(&td, "x", "author", "");
        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("no clickup-tasks type is configured"));
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
        let mut registry = DocumentStoreRegistry::new();
        registry.register(StoreBackend::Filesystem, Box::new(fs_store));
        let store = registry.for_type(&td).unwrap();

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
            client: Box::new(MockGhClient::new()),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: IssueMap::load(&root).unwrap(),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        let mut registry = DocumentStoreRegistry::new();
        registry.register(StoreBackend::Filesystem, Box::new(fs_store));
        registry.register(StoreBackend::GithubIssues, Box::new(gh_store));
        let store = registry.for_type(&td).unwrap();

        let result = store.create(&td, "dispatched", "author", "");
        assert!(result.is_ok());
    }

    #[test]
    fn github_issues_update_body_success() {
        let root = tmp_root("gh_update_body");
        let issue_body = make_issue_body("agent-7", "2026-03-27", None, "original body");
        let view_issue = GhIssue {
            number: 42,
            id: String::new(),
            url: String::new(),
            title: "My RFC".to_string(),
            body: issue_body,
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store
            .update(&td, "RFC-001", &[("body", "new content")])
            .unwrap();

        let captured = gh_store.mock().last_edit_body.borrow();
        let body_str = captured
            .as_deref()
            .expect("issue_edit should have been called with body");
        assert!(
            body_str.contains("new content"),
            "body should contain 'new content', got: {}",
            body_str
        );
        assert!(
            body_str.contains("<!-- lazyspec"),
            "body should be wrapped in issue_body format"
        );
        assert!(
            !body_str.contains("author:"),
            "re-serialized issue body should not contain author:, got: {}",
            body_str
        );

        // Cache file should still have author in frontmatter
        let cache_path = root.join(".lazyspec/cache/rfc/RFC-001.md");
        let cache_content = std::fs::read_to_string(&cache_path).unwrap();
        assert!(
            cache_content.contains("author:"),
            "cache file should contain author in frontmatter"
        );
    }

    #[test]
    fn github_issues_update_body_with_status() {
        let root = tmp_root("gh_update_body_status");
        let issue_body = make_issue_body("agent-7", "2026-03-27", None, "old body");
        let view_issue = GhIssue {
            number: 42,
            id: String::new(),
            url: String::new(),
            title: "My RFC".to_string(),
            body: issue_body,
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store
            .update(&td, "RFC-001", &[("body", "new"), ("status", "complete")])
            .unwrap();

        let captured = gh_store.mock().last_edit_body.borrow();
        let body_str = captured
            .as_deref()
            .expect("issue_edit should have been called with body");
        assert!(body_str.contains("new"), "body should contain updated text");
        assert!(
            gh_store.mock().closed.get(),
            "issue should be closed for status=complete"
        );
    }

    #[test]
    fn github_issues_update_body_optimistic_lock_failure() {
        let root = tmp_root("gh_update_body_lock");
        let issue_body = make_issue_body("agent-7", "2026-03-27", None, "some body");
        let view_issue = GhIssue {
            number: 42,
            id: String::new(),
            url: String::new(),
            title: "My RFC".to_string(),
            body: issue_body,
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:45:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        let err = gh_store
            .update(&td, "RFC-001", &[("body", "new content")])
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("modified on GitHub"), "got: {}", msg);
    }

    // §1: merge_relation_to_remote merges a single relation into the remote body
    // WITHOUT the optimistic lock -- a stale-then-advanced updated_at must not
    // abort, the remote prose is preserved, and the issue map records the
    // remote's current timestamp (never cleared).
    #[test]
    fn merge_relation_to_remote_no_lock_preserves_prose() {
        let root = tmp_root("merge_rel_no_lock");
        let body = make_issue_body("agent-7", "2026-03-27", None, "REMOTE PROSE LINE");
        let view_issue = GhIssue {
            number: 42,
            id: "I_node42".to_string(),
            url: String::new(),
            title: "My RFC".to_string(),
            body,
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T11:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        // Stale baseline; the merge path must not reject on it.
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store
            .merge_relation_to_remote(&td, "RFC-001", "implements", "STORY-001", true)
            .expect("merge must not reject on a stale updated_at");

        let pushed = gh_store.mock().last_edit_body.borrow();
        let pushed = pushed.as_ref().expect("issue_edit should run");
        assert!(pushed.contains("REMOTE PROSE LINE"), "got:\n{pushed}");
        assert!(pushed.contains("- implements: STORY-001"), "got:\n{pushed}");

        // Records the remote's current updated_at (not cleared like push_cache).
        assert_eq!(
            gh_store.issue_map.get("RFC-001").unwrap().updated_at,
            "2026-03-27T11:00:00Z"
        );
    }

    // §1 (dedup): merging an already-present relation is a no-op -- no issue_edit.
    #[test]
    fn merge_relation_to_remote_dedup_no_edit() {
        use crate::engine::document::{Relation, RelationType};
        let root = tmp_root("merge_rel_dedup");
        let existing = Relation {
            rel_type: RelationType::new("implements"),
            target: "STORY-001".to_string(),
        };
        let mut meta = DocMeta {
            path: PathBuf::new(),
            title: "My RFC".to_string(),
            doc_type: DocType::new("rfc"),
            status: Status::new("draft"),
            author: "agent-7".to_string(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 3, 27).unwrap(),
            tags: vec![],
            provenance: vec![],
            related: vec![existing],
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            attributes: Default::default(),
            id: "RFC-001".to_string(),
        };
        meta.tags = vec![];
        let body = issue_body::serialize(&meta, "prose");
        let view_issue = GhIssue {
            number: 42,
            id: "I_node42".to_string(),
            url: String::new(),
            title: "My RFC".to_string(),
            body,
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T11:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store
            .merge_relation_to_remote(&td, "RFC-001", "implements", "STORY-001", true)
            .unwrap();

        assert!(
            gh_store.mock().last_edit_body.borrow().is_none(),
            "already-present relation must not trigger an issue_edit"
        );
    }

    // BUG-014 / AC1: a remote issue whose body has NO lazyspec comment (empty /
    // null body, e.g. a GitHub-authored issue) must still accept an ordinary
    // relation link -- the merge synthesizes meta from the remote issue fields
    // and pushes a body carrying the lazyspec comment plus the relation.
    #[test]
    fn merge_relation_to_remote_empty_body_gains_comment_and_relation() {
        let root = tmp_root("merge_rel_empty_body");
        let view_issue = GhIssue {
            number: 42,
            id: "I_node42".to_string(),
            url: String::new(),
            title: "My RFC".to_string(),
            body: String::new(),
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T11:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store
            .merge_relation_to_remote(&td, "RFC-001", "related-to", "STORY-001", true)
            .expect("merge must not fail on a body without a lazyspec comment");

        let pushed = gh_store.mock().last_edit_body.borrow();
        let pushed = pushed.as_ref().expect("issue_edit should run");
        assert!(pushed.contains("<!-- lazyspec"), "got:\n{pushed}");
        assert!(pushed.contains("- related-to: STORY-001"), "got:\n{pushed}");
    }

    // BUG-014 / AC3: a remote issue whose body is plain prose with NO lazyspec
    // comment must still accept an ordinary relation link -- the merge pushes a
    // body carrying the lazyspec comment plus the relation, with the original
    // prose preserved verbatim beneath the new comment.
    #[test]
    fn merge_relation_to_remote_prose_only_body_keeps_prose_under_comment() {
        let root = tmp_root("merge_rel_prose_only");
        let prose = "This issue was filed straight on GitHub.\n\nIt has two prose paragraphs and zero lazyspec markers.";
        let view_issue = GhIssue {
            number: 42,
            id: "I_node42".to_string(),
            url: String::new(),
            title: "My RFC".to_string(),
            body: prose.to_string(),
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T11:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store
            .merge_relation_to_remote(&td, "RFC-001", "related-to", "STORY-001", true)
            .expect("merge must not fail on a prose-only body without a lazyspec comment");

        let pushed = gh_store.mock().last_edit_body.borrow();
        let pushed = pushed.as_ref().expect("issue_edit should run");
        assert!(pushed.contains("<!-- lazyspec"), "got:\n{pushed}");
        assert!(pushed.contains("- related-to: STORY-001"), "got:\n{pushed}");

        let comment_end = pushed
            .find("-->")
            .expect("pushed body should carry a closed lazyspec comment");
        let prose_start = pushed
            .find(prose)
            .unwrap_or_else(|| panic!("prose not preserved verbatim, got:\n{pushed}"));
        assert!(
            prose_start > comment_end,
            "prose must sit beneath the lazyspec comment, got:\n{pushed}"
        );
    }

    // BUG-014 / AC2: unlinking a relation on an adopted issue (body already
    // carries the lazyspec comment with the relation) removes the relation from
    // the pushed comment.
    #[test]
    fn merge_relation_to_remote_unlink_removes_relation_from_comment() {
        use crate::engine::document::{Relation, RelationType};
        let root = tmp_root("merge_rel_unlink_adopted");
        let meta = DocMeta {
            path: PathBuf::new(),
            title: "My RFC".to_string(),
            doc_type: DocType::new("rfc"),
            status: Status::new("draft"),
            author: "agent-7".to_string(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 3, 27).unwrap(),
            tags: vec![],
            provenance: vec![],
            related: vec![Relation {
                rel_type: RelationType::new("related-to"),
                target: "STORY-001".to_string(),
            }],
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            attributes: Default::default(),
            id: "RFC-001".to_string(),
        };
        let body = issue_body::serialize(&meta, "ADOPTED PROSE LINE");
        let view_issue = GhIssue {
            number: 42,
            id: "I_node42".to_string(),
            url: String::new(),
            title: "My RFC".to_string(),
            body,
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T11:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store
            .merge_relation_to_remote(&td, "RFC-001", "related-to", "STORY-001", false)
            .expect("unlink on an adopted issue must succeed");

        let pushed = gh_store.mock().last_edit_body.borrow();
        let pushed = pushed.as_ref().expect("issue_edit should run");
        assert!(pushed.contains("<!-- lazyspec"), "got:\n{pushed}");
        assert!(
            !pushed.contains("- related-to: STORY-001"),
            "relation must be removed from the pushed comment, got:\n{pushed}"
        );
        assert!(pushed.contains("ADOPTED PROSE LINE"), "got:\n{pushed}");
    }

    // BUG-014 / AC2 (comment-less remote): unlink rides the same merge path, so
    // a remote body with NO lazyspec comment (relation only in the local cache)
    // must not hard-fail -- the merge synthesizes meta from the remote issue
    // fields and pushes a comment-bearing body without the relation.
    #[test]
    fn merge_relation_to_remote_unlink_comment_less_body_succeeds() {
        let root = tmp_root("merge_rel_unlink_no_comment");
        let view_issue = GhIssue {
            number: 42,
            id: "I_node42".to_string(),
            url: String::new(),
            title: "My RFC".to_string(),
            body: "Filed straight on GitHub, no lazyspec markers.".to_string(),
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T11:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store
            .merge_relation_to_remote(&td, "RFC-001", "related-to", "STORY-001", false)
            .expect("unlink must not fail on a body without a lazyspec comment");

        let pushed = gh_store.mock().last_edit_body.borrow();
        let pushed = pushed.as_ref().expect("issue_edit should run");
        assert!(pushed.contains("<!-- lazyspec"), "got:\n{pushed}");
        assert!(
            !pushed.contains("- related-to: STORY-001"),
            "unlinked relation must not appear in the pushed comment, got:\n{pushed}"
        );
    }

    #[test]
    fn filesystem_update_sets_body() {
        let root = tmp_root("fs_update_body");
        let config = Config::default();

        let mut fs_store = FilesystemStore {
            root: root.clone(),
            config: config.clone(),
        };

        let td = test_type_def(StoreBackend::Filesystem);
        let created = fs_store.create(&td, "test doc", "author", "").unwrap();

        fs_store
            .update(
                &td,
                &created.id,
                &[("status", "review"), ("body", "fresh body")],
            )
            .unwrap();

        let full = root.join(&created.path);
        let content = std::fs::read_to_string(&full).unwrap();
        assert!(
            content.contains("fresh body"),
            "body not written: {}",
            content
        );
        assert!(
            content.contains("status: review"),
            "frontmatter lost: {}",
            content
        );
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
        let mut registry = DocumentStoreRegistry::new();
        registry.register(StoreBackend::Filesystem, Box::new(fs_store));
        let result = registry.for_type(&td);
        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("no github-issues backend"));
    }

    #[test]
    fn dispatch_routes_to_git_ref() {
        let root = tmp_root("dispatch_gitref");
        let config = Config::default();

        let fs_store = FilesystemStore {
            root: root.clone(),
            config,
        };

        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![]))
            .with_create_ref_commit_result(Ok("abc123".into()));
        let git_ref_store = GitRefStore {
            git: Box::new(mock),
            root: root.clone(),
            remote: Config::default().git_ref.remote.clone(),
            config: Config::default(),
            reserved_number: None,
        };

        let td = test_type_def(StoreBackend::GitRef);
        let mut registry = DocumentStoreRegistry::new();
        registry.register(StoreBackend::Filesystem, Box::new(fs_store));
        registry.register(StoreBackend::GitRef, Box::new(git_ref_store));
        let store = registry.for_type(&td).unwrap();

        let result = store.create(&td, "dispatched", "author", "");
        assert!(result.is_ok());
    }

    #[test]
    fn dispatch_filesystem_ignores_git_ref_store() {
        let root = tmp_root("dispatch_fs_ignores_gitref");
        let config = Config::default();

        let fs_store = FilesystemStore {
            root: root.clone(),
            config,
        };

        let mock = MockGitRefClient::new();
        let git_ref_store = GitRefStore {
            git: Box::new(mock),
            root: root.clone(),
            remote: Config::default().git_ref.remote.clone(),
            config: Config::default(),
            reserved_number: None,
        };

        let td = test_type_def(StoreBackend::Filesystem);
        let mut registry = DocumentStoreRegistry::new();
        registry.register(StoreBackend::Filesystem, Box::new(fs_store));
        registry.register(StoreBackend::GitRef, Box::new(git_ref_store));
        let store = registry.for_type(&td).unwrap();

        let result = store.create(&td, "dispatched", "author", "");
        assert!(result.is_ok());
        // The git-ref store is owned by the registry; downcast it back to assert
        // routing to the filesystem type never touched it.
        let git_ref_store = registry
            .get(StoreBackend::GitRef)
            .unwrap()
            .as_any()
            .downcast_ref::<GitRefStore>()
            .unwrap();
        assert!(
            (*git_ref_store.git)
                .as_any()
                .downcast_ref::<MockGitRefClient>()
                .unwrap()
                .calls
                .borrow()
                .is_empty(),
            "GitRefStore should not have been invoked for a Filesystem type"
        );
    }

    #[test]
    fn dispatch_git_ref_without_backend_errors() {
        let root = tmp_root("dispatch_no_gitref");
        let config = Config::default();

        let fs_store = FilesystemStore {
            root: root.clone(),
            config,
        };

        let td = test_type_def(StoreBackend::GitRef);
        let mut registry = DocumentStoreRegistry::new();
        registry.register(StoreBackend::Filesystem, Box::new(fs_store));
        let result = registry.for_type(&td);
        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("no git-ref backend"));
    }

    #[test]
    fn write_cache_file_escapes_special_characters() {
        use crate::engine::document::{Relation, RelationType};
        use chrono::NaiveDate;

        let root = tmp_root("cache_special_chars");
        let td = test_type_def(StoreBackend::GithubIssues);

        let meta = DocMeta {
            path: PathBuf::new(),
            title: "Title with \"quotes\" and: colons".to_string(),
            doc_type: DocType::new("rfc"),
            status: Status::new("draft"),
            author: "O'Brien".to_string(),
            date: NaiveDate::from_ymd_opt(2026, 3, 28).unwrap(),
            tags: vec!["tag:with:colons".to_string(), "tag \"quoted\"".to_string()],
            provenance: vec![],
            related: vec![Relation {
                rel_type: RelationType::new("implements"),
                target: "STORY: special & \"fun\"".to_string(),
            }],
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            attributes: Default::default(),
            id: "RFC-099".to_string(),
        };

        write_cache_file(&root, &td, &meta, "body").unwrap();

        let cache_dir = root.join(".lazyspec/cache/rfc");
        let cache_path = cache_dir.join("RFC-099.md");
        let content = std::fs::read_to_string(&cache_path).unwrap();

        // Verify the file is valid YAML by round-tripping through serde_yaml
        let (yaml, _body) = crate::engine::document::split_frontmatter(&content).unwrap();
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&yaml).expect("frontmatter should be valid YAML");

        assert_eq!(
            parsed["title"].as_str().unwrap(),
            "Title with \"quotes\" and: colons"
        );
        assert_eq!(parsed["author"].as_str().unwrap(), "O'Brien");
        assert_eq!(parsed["tags"][0].as_str().unwrap(), "tag:with:colons");
        assert_eq!(parsed["tags"][1].as_str().unwrap(), "tag \"quoted\"");
        assert_eq!(
            parsed["related"][0]["implements"].as_str().unwrap(),
            "STORY: special & \"fun\""
        );
    }

    fn cache_meta(id: &str) -> DocMeta {
        use chrono::NaiveDate;
        DocMeta {
            path: PathBuf::new(),
            title: format!("Title {id}"),
            doc_type: DocType::new("story"),
            status: Status::new("draft"),
            author: "a".to_string(),
            date: NaiveDate::from_ymd_opt(2026, 6, 26).unwrap(),
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            attributes: Default::default(),
            id: id.to_string(),
        }
    }

    fn story_type_def() -> TypeDef {
        let mut td = test_type_def(StoreBackend::GithubIssues);
        td.name = "story".to_string();
        td.subdirectory = true;
        td
    }

    #[test]
    fn nested_parent_writes_index_md() {
        let root = tmp_root("nested_parent");
        let td = story_type_def();

        write_cache_parent(&root, &td, &cache_meta("STORY-100"), "parent body").unwrap();

        let index = root.join(".lazyspec/cache/story/STORY-100/index.md");
        assert!(index.is_file(), "parent should write to <PARENT>/index.md");
        assert!(std::fs::read_to_string(&index)
            .unwrap()
            .contains("parent body"));
    }

    #[test]
    fn nested_child_writes_padded_order_filename() {
        let root = tmp_root("nested_child");
        let td = story_type_def();

        write_cache_child(&root, &td, "STORY-100", 0, 3, &cache_meta("STORY-12"), "c0").unwrap();
        write_cache_child(&root, &td, "STORY-100", 1, 3, &cache_meta("STORY-13"), "c1").unwrap();

        let folder = root.join(".lazyspec/cache/story/STORY-100");
        assert!(folder.join("00-STORY-12.md").is_file());
        assert!(folder.join("01-STORY-13.md").is_file());
    }

    #[test]
    fn nested_child_uses_three_digit_padding_beyond_99() {
        let root = tmp_root("nested_child_wide");
        let td = story_type_def();

        write_cache_child(
            &root,
            &td,
            "STORY-100",
            5,
            150,
            &cache_meta("STORY-12"),
            "c",
        )
        .unwrap();

        let folder = root.join(".lazyspec/cache/story/STORY-100");
        assert!(folder.join("005-STORY-12.md").is_file());
    }

    #[test]
    fn find_cache_file_resolves_nested_child() {
        let root = tmp_root("find_nested_child");
        let td = story_type_def();
        write_cache_child(&root, &td, "STORY-100", 2, 3, &cache_meta("STORY-12"), "c").unwrap();

        let cache_dir = root.join(".lazyspec/cache/story");
        let found = find_cache_file(&cache_dir, "STORY-12").expect("child must be found");
        assert!(found.ends_with("STORY-100/02-STORY-12.md"));
    }

    #[test]
    fn find_cache_file_resolves_nested_parent_index() {
        let root = tmp_root("find_nested_parent");
        let td = story_type_def();
        write_cache_parent(&root, &td, &cache_meta("STORY-100"), "p").unwrap();

        let cache_dir = root.join(".lazyspec/cache/story");
        let found = find_cache_file(&cache_dir, "STORY-100").expect("parent index must be found");
        assert!(found.ends_with("STORY-100/index.md"));
    }

    #[test]
    fn find_cache_file_still_resolves_flat() {
        let root = tmp_root("find_flat");
        let td = story_type_def();
        write_cache_file(&root, &td, &cache_meta("STORY-7"), "flat").unwrap();

        let cache_dir = root.join(".lazyspec/cache/story");
        let found = find_cache_file(&cache_dir, "STORY-7").expect("flat doc must be found");
        assert!(found.ends_with("STORY-7.md"));
    }

    #[test]
    fn childless_doc_stays_flat() {
        let root = tmp_root("childless_flat");
        let td = story_type_def();

        write_cache_file(&root, &td, &cache_meta("STORY-7"), "body").unwrap();

        let flat = root.join(".lazyspec/cache/story/STORY-7.md");
        assert!(flat.is_file(), "childless doc must stay flat");
        assert!(!root.join(".lazyspec/cache/story/STORY-7").exists());
    }

    // Regression: write_cache_file refuses an empty doc id rather than writing
    // a `cache_dir/.md` empty-stem file.
    #[test]
    fn write_cache_file_rejects_empty_id() {
        use chrono::NaiveDate;

        let root = tmp_root("cache_empty_id");
        let td = test_type_def(StoreBackend::GithubIssues);
        let meta = DocMeta {
            path: PathBuf::new(),
            title: "No id".to_string(),
            doc_type: DocType::new("rfc"),
            status: Status::new("draft"),
            author: "a".to_string(),
            date: NaiveDate::from_ymd_opt(2026, 3, 28).unwrap(),
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            attributes: Default::default(),
            id: String::new(),
        };

        let err = write_cache_file(&root, &td, &meta, "body").unwrap_err();
        assert!(err.to_string().contains("empty doc id"), "got: {err}");
        assert!(
            !root.join(".lazyspec/cache/rfc/.md").exists(),
            "no empty-stem cache file should be written"
        );
    }

    #[test]
    fn push_cache_sends_updated_relationships_to_github() {
        let root = tmp_root("gh_push_cache");

        // Set up a cache file with a relationship (simulating what link() writes)
        let cache_dir = root.join(".lazyspec/cache/rfc");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let cache_content = concat!(
            "---\n",
            "title: My RFC\n",
            "type: rfc\n",
            "status: draft\n",
            "author: agent-7\n",
            "date: 2026-03-27\n",
            "tags: []\n",
            "related:\n",
            "- implements: STORY-001\n",
            "---\n",
            "Some body text.\n",
        );
        std::fs::write(cache_dir.join("RFC-001.md"), cache_content).unwrap();

        // Set up the mock: remote issue has no relationships yet
        let remote_body = make_issue_body("agent-7", "2026-03-27", None, "Some body text.");
        let view_issue = GhIssue {
            number: 42,
            id: String::new(),
            url: String::new(),
            title: "My RFC".to_string(),
            body: remote_body,
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store.push_cache(&td, "RFC-001").unwrap();

        let captured = gh_store.mock().last_edit_body.borrow();
        let body_str = captured
            .as_deref()
            .expect("issue_edit should have been called");
        assert!(
            body_str.contains("implements: STORY-001"),
            "pushed body should contain the relationship, got: {}",
            body_str
        );
        assert!(
            body_str.contains("Some body text."),
            "pushed body should contain markdown body"
        );
        assert!(
            body_str.contains("<!-- lazyspec"),
            "pushed body should be in issue_body format"
        );

        // updated_at should be cleared (we just pushed)
        let entry = gh_store.issue_map.get("RFC-001").unwrap();
        assert_eq!(entry.updated_at, "");
    }

    #[test]
    fn gh_set_provenance_pushes_via_issue_edit() {
        let root = tmp_root("gh_set_prov");
        let issue_body = make_issue_body("agent-7", "2026-03-27", None, "body");
        let view_issue = GhIssue {
            number: 42,
            id: String::new(),
            url: String::new(),
            title: "My RFC".to_string(),
            body: issue_body,
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store
            .set_provenance(&td, "RFC-001", &["A".to_string()])
            .unwrap();

        let captured = gh_store.mock().last_edit_body.borrow();
        let body_str = captured
            .as_deref()
            .expect("issue_edit should have been called");
        assert!(
            body_str.contains("provenance:"),
            "body should contain provenance block, got: {}",
            body_str
        );
        assert!(body_str.contains("- A"));

        // Cache file should reflect the same
        let cache_path = root.join(".lazyspec/cache/rfc/RFC-001.md");
        let cache_content = std::fs::read_to_string(&cache_path).unwrap();
        let (yaml, _) = crate::engine::document::split_frontmatter(&cache_content).unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
        let prov = parsed["provenance"].as_sequence().expect("provenance seq");
        assert_eq!(prov.len(), 1);
        assert_eq!(prov[0].as_str().unwrap(), "A");
    }

    #[test]
    fn gh_set_provenance_clears_when_empty() {
        let root = tmp_root("gh_set_prov_empty");
        let issue_body_str =
            "<!-- lazyspec\n---\ndate: 2026-03-27\nprovenance:\n- old\n---\n-->\n\nbody";
        let view_issue = GhIssue {
            number: 42,
            id: String::new(),
            url: String::new(),
            title: "My RFC".to_string(),
            body: issue_body_str.to_string(),
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store.set_provenance(&td, "RFC-001", &[]).unwrap();

        let captured = gh_store.mock().last_edit_body.borrow();
        let body_str = captured
            .as_deref()
            .expect("issue_edit should have been called");
        assert!(
            !body_str.contains("provenance:"),
            "empty provenance should not emit block, got: {}",
            body_str
        );
    }

    #[test]
    fn cache_frontmatter_round_trips_provenance() {
        use crate::engine::config::{
            Config, DocumentConfig, FilesystemConfig, Naming, Templates, UiConfig,
        };
        use chrono::NaiveDate;

        let root = tmp_root("cache_prov_roundtrip");
        let td = test_type_def(StoreBackend::GithubIssues);

        let meta = DocMeta {
            path: PathBuf::new(),
            title: "Title".to_string(),
            doc_type: DocType::new("rfc"),
            status: Status::new("draft"),
            author: "alice".to_string(),
            date: NaiveDate::from_ymd_opt(2026, 3, 28).unwrap(),
            tags: vec![],
            provenance: vec!["Workshop 2026-04-12".to_string(), "Jane Doe".to_string()],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            attributes: Default::default(),
            id: "RFC-099".to_string(),
        };

        write_cache_file(&root, &td, &meta, "body").unwrap();

        let config = Config {
            documents: DocumentConfig {
                types: vec![td.clone()],
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
        };

        let store = Store::load(&root, &config).unwrap();
        let loaded = store.resolve_shorthand("RFC-099").unwrap();
        assert_eq!(
            loaded.provenance,
            vec!["Workshop 2026-04-12".to_string(), "Jane Doe".to_string()]
        );
    }

    use crate::engine::config::{AttrDef, AttrKind};

    fn type_def_with_attrs(store: StoreBackend, attrs: Vec<AttrDef>) -> TypeDef {
        TypeDef {
            attributes: attrs,
            ..test_type_def(store)
        }
    }

    // AC1: fs --attr owner=jkaloger persists to frontmatter and reads back typed.
    #[test]
    fn fs_update_attr_persists_and_reads_back() {
        let root = tmp_root("fs_attr_persist");
        let config = Config::default();
        let mut fs_store = FilesystemStore {
            root: root.clone(),
            config,
        };

        let td = type_def_with_attrs(
            StoreBackend::Filesystem,
            vec![AttrDef {
                name: "owner".to_string(),
                kind: AttrKind::Str,
                required: false,
                values: vec![],
            }],
        );
        let created = fs_store.create(&td, "doc", "author", "").unwrap();
        fs_store
            .update(&td, &created.id, &[("owner", "jkaloger")])
            .unwrap();

        let content = std::fs::read_to_string(root.join(&created.path)).unwrap();
        assert!(content.contains("owner: jkaloger"), "got: {content}");

        let meta = DocMeta::parse_with_schema(&content, &td.attributes).unwrap();
        assert_eq!(
            meta.attributes["owner"],
            AttrValue::Str("jkaloger".to_string())
        );
    }

    // AC2: bad enum value rejected; file left byte-identical (no write).
    #[test]
    fn fs_update_bad_enum_leaves_file_unchanged() {
        let root = tmp_root("fs_attr_bad_enum");
        let config = Config::default();
        let mut fs_store = FilesystemStore {
            root: root.clone(),
            config,
        };

        let td = type_def_with_attrs(
            StoreBackend::Filesystem,
            vec![AttrDef {
                name: "priority".to_string(),
                kind: AttrKind::Enum,
                required: false,
                values: vec!["low".to_string(), "med".to_string(), "high".to_string()],
            }],
        );
        let created = fs_store.create(&td, "doc", "author", "").unwrap();
        let before = std::fs::read_to_string(root.join(&created.path)).unwrap();

        let err = fs_store
            .update(&td, &created.id, &[("priority", "urgent")])
            .unwrap_err();
        assert!(err.to_string().contains("priority"), "got: {err}");

        let after = std::fs::read_to_string(root.join(&created.path)).unwrap();
        assert_eq!(
            before, after,
            "file must be unchanged on validation failure"
        );
    }

    // AC2: int kind mismatch rejected; file unchanged.
    #[test]
    fn fs_update_bad_int_leaves_file_unchanged() {
        let root = tmp_root("fs_attr_bad_int");
        let config = Config::default();
        let mut fs_store = FilesystemStore {
            root: root.clone(),
            config,
        };

        let td = type_def_with_attrs(
            StoreBackend::Filesystem,
            vec![AttrDef {
                name: "estimate".to_string(),
                kind: AttrKind::Int,
                required: false,
                values: vec![],
            }],
        );
        let created = fs_store.create(&td, "doc", "author", "").unwrap();
        let before = std::fs::read_to_string(root.join(&created.path)).unwrap();

        let err = fs_store
            .update(&td, &created.id, &[("estimate", "notanumber")])
            .unwrap_err();
        assert!(err.to_string().contains("estimate"), "got: {err}");

        let after = std::fs::read_to_string(root.join(&created.path)).unwrap();
        assert_eq!(before, after);
    }

    // AC3 (cache sink) + AC5: github update --attr mirrors typed attrs into the
    // cache .md, where parse_with_schema reads them back as typed values.
    #[test]
    fn github_update_attr_round_trips_through_cache() {
        let root = tmp_root("gh_attr_cache");
        let issue_body = make_issue_body("agent-7", "2026-03-27", None, "body");
        let view_issue = GhIssue {
            number: 42,
            id: String::new(),
            url: String::new(),
            title: "My doc".to_string(),
            body: issue_body,
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = type_def_with_attrs(
            StoreBackend::GithubIssues,
            vec![
                AttrDef {
                    name: "owner".to_string(),
                    kind: AttrKind::Str,
                    required: false,
                    values: vec![],
                },
                AttrDef {
                    name: "estimate".to_string(),
                    kind: AttrKind::Int,
                    required: false,
                    values: vec![],
                },
            ],
        );
        gh_store
            .update(&td, "RFC-001", &[("owner", "jkaloger"), ("estimate", "3")])
            .unwrap();

        // Remote body carries the attributes block (clobber-protection sink).
        let captured = gh_store.mock().last_edit_body.borrow();
        let body_str = captured.as_deref().expect("issue_edit called");
        assert!(body_str.contains("attributes:"), "got: {body_str}");
        assert!(body_str.contains("owner: jkaloger"));

        // Cache .md is the show --json read-back: parse_with_schema -> typed.
        let cache_path = root.join(".lazyspec/cache/rfc/RFC-001.md");
        let cache_content = std::fs::read_to_string(&cache_path).unwrap();
        let meta = DocMeta::parse_with_schema(&cache_content, &td.attributes).unwrap();
        assert_eq!(
            meta.attributes["owner"],
            AttrValue::Str("jkaloger".to_string())
        );
        assert_eq!(meta.attributes["estimate"], AttrValue::Int(3));

        // AC5: doc_to_json emits estimate as a JSON number, not a string.
        let json = crate::engine::document::AttrValue::Int(3);
        assert_eq!(serde_json::to_value(&json).unwrap(), serde_json::json!(3));
        assert_eq!(
            serde_json::to_value(&meta.attributes["estimate"]).unwrap(),
            serde_json::json!(3)
        );
    }

    // AC2 (github atomicity): bad attr bails before any remote issue_edit.
    #[test]
    fn github_update_bad_attr_no_remote_write() {
        let root = tmp_root("gh_attr_bad");
        let issue_body = make_issue_body("agent-7", "2026-03-27", None, "body");
        let view_issue = GhIssue {
            number: 42,
            id: String::new(),
            url: String::new(),
            title: "My doc".to_string(),
            body: issue_body,
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(client),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = type_def_with_attrs(
            StoreBackend::GithubIssues,
            vec![AttrDef {
                name: "estimate".to_string(),
                kind: AttrKind::Int,
                required: false,
                values: vec![],
            }],
        );
        let err = gh_store
            .update(&td, "RFC-001", &[("estimate", "notanumber")])
            .unwrap_err();
        assert!(err.to_string().contains("estimate"), "got: {err}");
        assert!(
            gh_store.mock().last_edit_body.borrow().is_none(),
            "no remote issue_edit should have fired"
        );
    }

    use crate::engine::gh_schema::{GhSchemaSnapshot, IssueTypeId};

    fn write_bug_snapshot(root: &std::path::Path) {
        let snapshot = GhSchemaSnapshot {
            issue_types: vec![IssueTypeId {
                name: "Bug".to_string(),
                id: "IT_bug".to_string(),
            }],
            ..Default::default()
        };
        snapshot.save(root).unwrap();
    }

    fn issue_node_id_response() -> serde_json::Value {
        serde_json::json!({
            "data": { "repository": { "issue": { "id": "I_node1" } } }
        })
    }

    fn gh_view_issue_with_lazyspec_body(number: u64, labels: Vec<&str>) -> GhIssue {
        GhIssue {
            number,
            id: format!("I_node{}", number),
            url: String::new(),
            title: "My doc".to_string(),
            body: make_issue_body("agent-7", "2026-03-27", None, "body"),
            labels: labels
                .into_iter()
                .map(|l| GhLabel {
                    name: l.to_string(),
                    color: String::new(),
                })
                .collect(),
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        }
    }

    fn issue_type_store(
        root: &std::path::Path,
        graphql: Vec<serde_json::Value>,
    ) -> GithubIssuesStore {
        let view_issue = gh_view_issue_with_lazyspec_body(42, vec!["lazyspec:story"]);
        let client = MockGhClient::new()
            .with_view_issue(view_issue)
            .with_graphql_responses(graphql);
        let mut map = IssueMap::load(root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");
        GithubIssuesStore {
            client: Box::new(client),
            root: root.to_path_buf(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(root),
        }
    }

    fn issue_type_attr_td() -> TypeDef {
        type_def_with_attrs(
            StoreBackend::GithubIssues,
            vec![AttrDef {
                name: "issue_type".to_string(),
                kind: AttrKind::Str,
                required: false,
                values: vec![],
            }],
        )
    }

    // AC3: set issue_type=Bug -> exactly ONE updateIssue mutation carrying the
    // resolved id, and NO issue_type in the issue-body HTML comment.
    #[test]
    fn github_update_issue_type_sets_native_field_only() {
        let root = tmp_root("gh_issue_type_set");
        write_bug_snapshot(&root);
        // First graphql response resolves the issue node id; the mutation records.
        let mut gh_store = issue_type_store(
            &root,
            vec![
                issue_node_id_response(),
                serde_json::json!({"data": {"updateIssue": {"issue": {"id": "I_node1"}}}}),
            ],
        );

        let td = issue_type_attr_td();
        gh_store
            .update(&td, "RFC-001", &[("issue_type", "Bug")])
            .unwrap();

        let calls = gh_store.mock().graphql_calls.borrow();
        let mutations: Vec<_> = calls
            .iter()
            .filter(|(q, _)| q.contains("updateIssue"))
            .collect();
        assert_eq!(mutations.len(), 1, "exactly one updateIssue mutation");
        let (_, vars) = mutations[0];
        assert!(
            vars.contains(&("issueTypeId".to_string(), GqlVar::Str("IT_bug".to_string()))),
            "mutation must carry resolved issueTypeId=IT_bug, got: {:?}",
            vars
        );

        // The issue-body HTML comment must NOT carry issue_type.
        let body = gh_store.mock().last_edit_body.borrow();
        let body_str = body.as_deref().expect("issue_edit called");
        assert!(
            !body_str.contains("issue_type"),
            "issue_type must not leak into issue body, got: {body_str}"
        );
    }

    // AC4: clearing issue_type sends issueTypeId: null.
    #[test]
    fn github_update_issue_type_clear_sends_null() {
        let root = tmp_root("gh_issue_type_clear");
        write_bug_snapshot(&root);
        let mut gh_store = issue_type_store(
            &root,
            vec![
                issue_node_id_response(),
                serde_json::json!({"data": {"updateIssue": {"issue": {"id": "I_node1"}}}}),
            ],
        );

        let td = issue_type_attr_td();
        gh_store
            .update(&td, "RFC-001", &[("issue_type", "")])
            .unwrap();

        let calls = gh_store.mock().graphql_calls.borrow();
        let mutation = calls
            .iter()
            .find(|(q, _)| q.contains("updateIssue"))
            .expect("an updateIssue mutation");
        assert!(
            mutation.0.contains("issueTypeId: null"),
            "clear must send issueTypeId: null, got query: {}",
            mutation.0
        );
        // No issueTypeId var on the clear path.
        assert!(
            !mutation.1.iter().any(|(k, _)| k == "issueTypeId"),
            "clear must not pass an issueTypeId value var"
        );
    }

    // AC5: invalid value rejects offline; zero mutations, no issue_edit.
    #[test]
    fn github_update_issue_type_invalid_rejected_offline() {
        let root = tmp_root("gh_issue_type_invalid");
        write_bug_snapshot(&root);
        // No graphql responses at all -> any graphql call would error anyway.
        let mut gh_store = issue_type_store(&root, vec![]);

        let td = issue_type_attr_td();
        let err = gh_store
            .update(&td, "RFC-001", &[("issue_type", "Nonsense")])
            .unwrap_err();
        assert!(
            err.to_string().contains("Nonsense"),
            "error must name the invalid value, got: {err}"
        );
        assert!(
            gh_store.mock().graphql_calls.borrow().is_empty(),
            "zero graphql mutations on invalid issue_type"
        );
        assert!(
            gh_store.mock().last_edit_body.borrow().is_none(),
            "no issue_edit on invalid issue_type"
        );
    }

    // ITERATION-220 AC4: user account (zero issue types) -> issue_type set
    // rejects pre-mutation with an org-only message, no updateIssue mutation.
    #[test]
    fn github_update_issue_type_user_account_org_only_message() {
        let root = tmp_root("gh_issue_type_user_account");
        // Empty snapshot mirrors a user-owned repo: no native issue types at all.
        GhSchemaSnapshot {
            issue_types: vec![],
            ..Default::default()
        }
        .save(&root)
        .unwrap();
        let mut gh_store = issue_type_store(&root, vec![]);

        let td = issue_type_attr_td();
        let err = gh_store
            .update(&td, "RFC-001", &[("issue_type", "Bug")])
            .unwrap_err();
        assert!(
            err.to_string().contains("require an organization"),
            "error must name the org-only constraint, got: {err}"
        );
        let mutations = gh_store
            .mock()
            .graphql_calls
            .borrow()
            .iter()
            .filter(|(q, _)| q.contains("updateIssue"))
            .count();
        assert_eq!(mutations, 0, "no updateIssue mutation on user account");
        assert!(
            gh_store.mock().last_edit_body.borrow().is_none(),
            "no issue_edit on user account"
        );
    }

    // AC6 (write half): setting issue_type does not touch labels or doc_type.
    #[test]
    fn github_update_issue_type_does_not_touch_labels() {
        let root = tmp_root("gh_issue_type_labels");
        write_bug_snapshot(&root);
        let snapshot = GhSchemaSnapshot {
            issue_types: vec![
                IssueTypeId {
                    name: "Bug".to_string(),
                    id: "IT_bug".to_string(),
                },
                IssueTypeId {
                    name: "Task".to_string(),
                    id: "IT_task".to_string(),
                },
            ],
            ..Default::default()
        };
        snapshot.save(&root).unwrap();

        let mut gh_store = issue_type_store(
            &root,
            vec![
                issue_node_id_response(),
                serde_json::json!({"data": {"updateIssue": {"issue": {"id": "I_node1"}}}}),
            ],
        );

        let td = issue_type_attr_td();
        gh_store
            .update(&td, "RFC-001", &[("issue_type", "Task")])
            .unwrap();

        // No label add/remove recorded.
        assert!(
            gh_store.mock().last_edit_labels_remove.borrow().is_empty(),
            "issue_type write must not remove labels"
        );

        // doc_type stays story in the cache file (from the lazyspec:story label).
        let cache_path = root.join(".lazyspec/cache/rfc/RFC-001.md");
        let cache_content = std::fs::read_to_string(&cache_path).unwrap();
        let (yaml, _) = crate::engine::document::split_frontmatter(&cache_content).unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed["type"].as_str().unwrap(), "story");
    }

    // --- STORY-195 / ITERATION-266: push native issue type on create ---

    // AC1: `github_issue_type` configured + resolvable -> create succeeds and
    // pushes exactly one updateIssue mutation carrying the resolved id.
    #[test]
    fn github_create_pushes_configured_issue_type() {
        let root = tmp_root("gh_create_issue_type_push");
        write_bug_snapshot(&root);
        let td = TypeDef {
            github_issue_type: Some("Bug".to_string()),
            ..test_type_def(StoreBackend::GithubIssues)
        };

        let mut gh_store = GithubIssuesStore {
            client: Box::new(MockGhClient::new().with_graphql_responses(vec![
                issue_node_id_response(),
                serde_json::json!({"data": {"updateIssue": {"issue": {"id": "I_node1"}}}}),
            ])),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: IssueMap::load(&root).unwrap(),
            issue_cache: IssueCache::new(&root),
        };

        let result = gh_store
            .create(&td, "my title", "author", "body text")
            .unwrap();
        assert_eq!(result.id, "RFC-1");

        let calls = gh_store.mock().graphql_calls.borrow();
        let mutations: Vec<_> = calls
            .iter()
            .filter(|(q, _)| q.contains("updateIssue"))
            .collect();
        assert_eq!(mutations.len(), 1, "exactly one updateIssue mutation");
        let (_, vars) = mutations[0];
        assert!(
            vars.contains(&("issueTypeId".to_string(), GqlVar::Str("IT_bug".to_string()))),
            "mutation must carry resolved issueTypeId=IT_bug, got: {:?}",
            vars
        );
    }

    // AC2: `github_issue_type` unset -> zero GraphQL calls, no push at all, and
    // plain create behavior is unchanged.
    #[test]
    fn github_create_without_issue_type_makes_no_push_call() {
        let root = tmp_root("gh_create_issue_type_absent");
        let td = test_type_def(StoreBackend::GithubIssues);
        assert_eq!(td.github_issue_type, None);

        let mut gh_store = GithubIssuesStore {
            client: Box::new(MockGhClient::new()),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: IssueMap::load(&root).unwrap(),
            issue_cache: IssueCache::new(&root),
        };

        let result = gh_store
            .create(&td, "my title", "author", "body text")
            .unwrap();
        assert_eq!(result.id, "RFC-1");
        assert_eq!(gh_store.mock().create_titles.borrow().len(), 1);
        assert!(
            gh_store.mock().graphql_calls.borrow().is_empty(),
            "no issue_type configured -> zero GraphQL calls"
        );
    }

    // AC3: `github_issue_type` configured but unresolvable -> create fails
    // before `issue_create` fires, mirroring `update`'s two rejection message
    // shapes (name absent from a populated schema; empty schema on a
    // user-owned repo).
    #[test]
    fn github_create_unresolvable_issue_type_fails_before_create() {
        // Shape 1: populated schema, name not present.
        let root = tmp_root("gh_create_issue_type_invalid");
        write_bug_snapshot(&root);
        let td = TypeDef {
            github_issue_type: Some("Nonsense".to_string()),
            ..test_type_def(StoreBackend::GithubIssues)
        };
        let mut gh_store = GithubIssuesStore {
            client: Box::new(MockGhClient::new()),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: IssueMap::load(&root).unwrap(),
            issue_cache: IssueCache::new(&root),
        };
        let err = gh_store
            .create(&td, "my title", "author", "body text")
            .unwrap_err();
        assert!(
            err.to_string().contains("Nonsense"),
            "error must name the invalid value, got: {err}"
        );
        assert!(
            gh_store.mock().create_titles.borrow().is_empty(),
            "issue_create must never fire when the type is unresolvable"
        );
        assert!(
            gh_store.mock().graphql_calls.borrow().is_empty(),
            "zero GraphQL calls on invalid issue_type"
        );

        // Shape 2: empty schema (user-owned repo) -> org-only message.
        let root2 = tmp_root("gh_create_issue_type_user_account");
        GhSchemaSnapshot {
            issue_types: vec![],
            ..Default::default()
        }
        .save(&root2)
        .unwrap();
        let td2 = TypeDef {
            github_issue_type: Some("Bug".to_string()),
            ..test_type_def(StoreBackend::GithubIssues)
        };
        let mut gh_store2 = GithubIssuesStore {
            client: Box::new(MockGhClient::new()),
            root: root2.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: IssueMap::load(&root2).unwrap(),
            issue_cache: IssueCache::new(&root2),
        };
        let err2 = gh_store2
            .create(&td2, "my title", "author", "body text")
            .unwrap_err();
        assert!(
            err2.to_string().contains("require an organization"),
            "error must name the org-only constraint, got: {err2}"
        );
        assert!(
            gh_store2.mock().create_titles.borrow().is_empty(),
            "issue_create must never fire when the type is unresolvable"
        );
    }

    // AC4: `create_child_subissue` delegates to `create` with zero new logic,
    // so a configured `github_issue_type` on the child's TypeDef is pushed too.
    #[test]
    fn create_child_subissue_pushes_configured_issue_type() {
        let root = tmp_root("create_child_issue_type");
        write_bug_snapshot(&root);
        let td = TypeDef {
            github_issue_type: Some("Bug".to_string()),
            ..test_type_def(StoreBackend::GithubIssues)
        };

        let mut store = GithubIssuesStore {
            client: Box::new(MockGhClient::new().with_graphql_responses(vec![
                issue_node_id_response(),
                serde_json::json!({"data": {"updateIssue": {"issue": {"id": "I_node1"}}}}),
                serde_json::json!({"data": {"node": {"subIssues": {"nodes": []}}}}),
                serde_json::json!({"data": {}}),
            ])),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: IssueMap::load(&root).unwrap(),
            issue_cache: IssueCache::new(&root),
        };
        store.issue_map.insert("RFC-1", 1, "ts", "I_parent");
        store.issue_map.save(&root).unwrap();

        store
            .create_child_subissue(&td, "RFC-1", "Child", "author", "")
            .unwrap();

        let calls = store.mock().graphql_calls.borrow();
        let mutations: Vec<_> = calls
            .iter()
            .filter(|(q, _)| q.contains("updateIssue"))
            .collect();
        assert_eq!(
            mutations.len(),
            1,
            "exactly one updateIssue mutation for the child"
        );
        let (_, vars) = mutations[0];
        assert!(
            vars.contains(&("issueTypeId".to_string(), GqlVar::Str("IT_bug".to_string()))),
            "child updateIssue must carry resolved issueTypeId=IT_bug, got: {:?}",
            vars
        );
    }

    // --- Subdir sub-issue materialization (ITERATION-214) ---

    fn subdir_type_def() -> TypeDef {
        TypeDef {
            subdirectory: true,
            dir: "docs/stories".to_string(),
            ..test_type_def(StoreBackend::GithubIssues)
        }
    }

    fn subdir_config(td: &TypeDef) -> Config {
        let mut config = Config::default();
        config.documents.types = vec![td.clone()];
        config
    }

    fn write_doc(path: &std::path::Path, title: &str, extra: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let content = format!(
            "---\ntitle: {}\ntype: rfc\nstatus: draft\nauthor: a\ndate: 2026-06-25\ntags: []\n{}---\n\nbody for {}\n",
            title, extra, title
        );
        std::fs::write(path, content).unwrap();
    }

    fn subdir_gh_store(root: &std::path::Path, td: &TypeDef) -> GithubIssuesStore {
        // The subIssues read (empty) followed by enough generic mutation-OK
        // responses for the add/reprioritize calls the reconcile may issue.
        let mut responses = vec![serde_json::json!({
            "data": {"node": {"subIssues": {"nodes": []}}}
        })];
        responses.extend(std::iter::repeat_with(|| serde_json::json!({"data": {}})).take(8));
        GithubIssuesStore {
            client: Box::new(MockGhClient::new().with_graphql_responses(responses)),
            root: root.to_path_buf(),
            repo: "owner/repo".to_string(),
            config: subdir_config(td),
            issue_map: IssueMap::load(root).unwrap(),
            issue_cache: IssueCache::new(root),
        }
    }

    // AC1: parent index.md + 2 child .md -> materialize_subdir maps parent +
    // both children AND records 3 issue_create calls (pre-fix: 1).
    #[test]
    fn materialize_subdir_creates_parent_and_children_issues() {
        let root = tmp_root("subdir_materialize");
        let td = subdir_type_def();
        let folder = root.join("docs/stories/STORY-159-shape");
        write_doc(&folder.join("index.md"), "Parent", "");
        write_doc(&folder.join("01-first.md"), "First child", "");
        write_doc(&folder.join("02-second.md"), "Second child", "");

        let mut store = subdir_gh_store(&root, &td);
        let result = store.materialize_subdir(&td, "STORY-159").unwrap();

        assert_eq!(result.children.len(), 2);
        assert!(store.issue_map.get("STORY-159").is_some());
        assert!(store.issue_map.get("01-first").is_some());
        assert!(store.issue_map.get("02-second").is_some());

        assert_eq!(
            store.mock().create_titles.borrow().len(),
            3,
            "parent + 2 children = 3 issue_create calls"
        );
    }

    // AC1 (ordering): children come back in loader (path-sorted) order.
    #[test]
    fn materialize_subdir_children_in_loader_order() {
        let root = tmp_root("subdir_order");
        let td = subdir_type_def();
        let folder = root.join("docs/stories/STORY-200-x");
        write_doc(&folder.join("index.md"), "Parent", "");
        write_doc(&folder.join("02-b.md"), "B", "");
        write_doc(&folder.join("01-a.md"), "A", "");

        let mut store = subdir_gh_store(&root, &td);
        let result = store.materialize_subdir(&td, "STORY-200").unwrap();

        let ids: Vec<&str> = result
            .children
            .iter()
            .map(|c| c.child_id.as_str())
            .collect();
        assert_eq!(ids, vec!["01-a", "02-b"]);
        for (i, c) in result.children.iter().enumerate() {
            assert_eq!(c.order_index, i);
        }
    }

    // AC2 (via wiring): sync_subissues materializes then issues addSubIssue once
    // per child with issueId=parent_node, subIssueId=child_node.
    #[test]
    fn sync_subissues_adds_each_child_as_native_sub_issue() {
        let root = tmp_root("subdir_sync_add");
        let td = subdir_type_def();
        let folder = root.join("docs/stories/STORY-300-y");
        write_doc(&folder.join("index.md"), "Parent", "");
        write_doc(&folder.join("01-a.md"), "A", "");
        write_doc(&folder.join("02-b.md"), "B", "");

        let mut store = subdir_gh_store(&root, &td);
        let result = store.sync_subissues(&td, "STORY-300").unwrap();

        let parent_node = result.parent_node.clone();
        let calls = store.mock().graphql_calls.borrow();
        let adds: Vec<_> = calls
            .iter()
            .filter(|(q, _)| q.contains("addSubIssue"))
            .collect();
        assert_eq!(adds.len(), 2, "one addSubIssue per child");
        for (_, vars) in &adds {
            assert!(vars.contains(&("issueId".to_string(), GqlVar::Str(parent_node.clone()))));
        }
        let child_nodes: Vec<String> = result.children.iter().map(|c| c.node_id.clone()).collect();
        for cn in &child_nodes {
            assert!(
                adds.iter()
                    .any(|(_, vars)| vars
                        .contains(&("subIssueId".to_string(), GqlVar::Str(cn.clone())))),
                "child node {cn} must be added as a sub-issue"
            );
        }
    }

    // AC2 (CLI-authored child -> native sub-issue): author a flat github-issues
    // parent on disk, then drive the CLI create-with-parent path (promote flat
    // parent to index.md + write a sibling child via fs_ops::create_child_in_dir)
    // to produce the subdir source tree. sync_subissues then materializes the
    // child and fires an addSubIssue for it.
    #[test]
    fn cli_authored_child_becomes_native_sub_issue() {
        let root = tmp_root("subdir_cli_child");
        let td = subdir_type_def();
        let config = subdir_config(&td);

        // Flat parent authored on disk in the source dir.
        let stories = root.join("docs/stories");
        std::fs::create_dir_all(&stories).unwrap();
        write_doc(&stories.join("STORY-159-shape.md"), "Parent", "");

        // Emulate create --parent STORY-159: promote the flat parent to a subdir
        // index.md, then write the child as a sibling .md inside it.
        let subdir = stories.join("STORY-159-shape");
        std::fs::create_dir_all(&subdir).unwrap();
        std::fs::rename(stories.join("STORY-159-shape.md"), subdir.join("index.md")).unwrap();
        let child = crate::engine::fs_ops::create_child_in_dir(
            &root, &config, &td, &subdir, "Appendix", "a", None,
        )
        .unwrap();
        assert!(child.starts_with(&subdir));

        let mut store = subdir_gh_store(&root, &td);
        let result = store.sync_subissues(&td, "STORY-159").unwrap();

        assert_eq!(result.children.len(), 1, "one CLI-authored child");
        let child_node = result.children[0].node_id.clone();
        let calls = store.mock().graphql_calls.borrow();
        let adds: Vec<_> = calls
            .iter()
            .filter(|(q, _)| q.contains("addSubIssue"))
            .collect();
        assert_eq!(adds.len(), 1, "child added as a native sub-issue");
        assert!(adds[0]
            .1
            .contains(&("subIssueId".to_string(), GqlVar::Str(child_node))));
    }

    // AC6: a subdir doc with children AND an `implements` relation -> children
    // route to addSubIssue (GraphQL) while the implements relation stays in the
    // issue-body HTML comment and is NOT sent to GraphQL.
    #[test]
    fn structural_children_native_while_implements_stays_in_body() {
        let root = tmp_root("subdir_ac6");
        let td = subdir_type_def();
        let folder = root.join("docs/stories/STORY-400-z");
        write_doc(
            &folder.join("index.md"),
            "Parent",
            "related:\n- implements: RFC-050\n",
        );
        write_doc(&folder.join("01-a.md"), "A", "");

        let mut store = subdir_gh_store(&root, &td);
        let result = store.sync_subissues(&td, "STORY-400").unwrap();

        // Child routed to GraphQL addSubIssue.
        let calls = store.mock().graphql_calls.borrow();
        let adds: Vec<_> = calls
            .iter()
            .filter(|(q, _)| q.contains("addSubIssue"))
            .collect();
        assert_eq!(adds.len(), 1);
        let child_node = result.children[0].node_id.clone();
        assert!(adds[0]
            .1
            .contains(&("subIssueId".to_string(), GqlVar::Str(child_node))));

        // implements never appears in any GraphQL call.
        assert!(
            !calls.iter().any(|(q, vars)| q.contains("implements")
                || vars.iter().any(|(_, v)| matches!(
                    v,
                    GqlVar::Str(s) if s.contains("implements") || s == "RFC-050"
                ))),
            "implements relation must not be sent to GraphQL"
        );

        // implements lives in the parent issue body produced by issue_body::serialize.
        let parent_body = store.mock().last_create_body.borrow();
        // last_create_body holds the most recent create (the child). Verify the
        // parent's body directly via the serializer instead.
        drop(parent_body);
        let parent_meta = crate::engine::document::DocMeta {
            path: PathBuf::new(),
            title: "Parent".to_string(),
            doc_type: DocType::new("rfc"),
            status: Status::new("draft"),
            author: "a".to_string(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 6, 25).unwrap(),
            tags: vec![],
            provenance: vec![],
            related: vec![crate::engine::document::Relation {
                rel_type: crate::engine::document::RelationType::new("implements"),
                target: "RFC-050".to_string(),
            }],
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            attributes: Default::default(),
            id: "STORY-400".to_string(),
        };
        let body = issue_body::serialize(&parent_meta, "body");
        assert!(
            body.contains("implements: RFC-050"),
            "implements must be in the serialized issue body, got: {body}"
        );
    }

    // --- github-native immediate child sub-issue (ITERATION-226) ---

    // AC: creating a child under a github-issues parent fires exactly one
    // addSubIssue whose issueId is the parent node and subIssueId is the
    // newly-created child's node.
    #[test]
    fn create_child_subissue_binds_child_to_parent_node() {
        let root = tmp_root("create_child_subissue");
        let td = test_type_def(StoreBackend::GithubIssues);

        let mut store = subdir_gh_store(&root, &td);
        // Parent already a github issue (it is a github-issues doc).
        store.issue_map.insert("RFC-1", 1, "ts", "I_parent");
        store.issue_map.save(&root).unwrap();

        let created = store
            .create_child_subissue(&td, "RFC-1", "Child", "author", "")
            .unwrap();

        // A real child issue was created.
        assert_eq!(store.mock().create_titles.borrow().len(), 1);
        let child_node = store.issue_map.get(&created.id).unwrap().node_id.clone();
        assert!(!child_node.is_empty());

        let calls = store.mock().graphql_calls.borrow();
        let adds: Vec<_> = calls
            .iter()
            .filter(|(q, _)| q.contains("addSubIssue"))
            .collect();
        assert_eq!(adds.len(), 1, "exactly one addSubIssue edge");
        assert!(adds[0]
            .1
            .contains(&("issueId".to_string(), GqlVar::Str("I_parent".to_string()))));
        assert!(adds[0]
            .1
            .contains(&("subIssueId".to_string(), GqlVar::Str(child_node))));
    }

    // The parent must already be a github issue; absent from the issue map, the
    // child create + bind aborts with a clear error.
    #[test]
    fn create_child_subissue_errors_when_parent_unmapped() {
        let root = tmp_root("create_child_no_parent");
        let td = test_type_def(StoreBackend::GithubIssues);
        let mut store = subdir_gh_store(&root, &td);

        let err = store
            .create_child_subissue(&td, "RFC-99", "Child", "author", "")
            .unwrap_err();
        assert!(
            err.to_string().contains("RFC-99"),
            "names the unmapped parent: {err}"
        );
    }

    // --- github-projects board store (ITERATION-216) ---

    fn projects_type_def() -> TypeDef {
        TypeDef {
            name: "project".to_string(),
            plural: "projects".to_string(),
            dir: "docs/projects".to_string(),
            prefix: "PROJECT".to_string(),
            ..test_type_def(StoreBackend::GithubProjects)
        }
    }

    fn projects_store(
        root: &std::path::Path,
        graphql: Vec<serde_json::Value>,
    ) -> GithubProjectsStore {
        GithubProjectsStore {
            client: Box::new(MockGhClient::new().with_graphql_responses(graphql)),
            root: root.to_path_buf(),
            repo: "my-org/repo".to_string(),
            config: Config::default(),
            issue_map: IssueMap::load(root).unwrap(),
        }
    }

    fn org_board_response(id: &str) -> serde_json::Value {
        serde_json::json!({"data": {"organization": {"projectV2": {"id": id}}}})
    }

    fn org_board_null() -> serde_json::Value {
        serde_json::json!({"data": {"organization": {"projectV2": null}}})
    }

    fn user_board_null() -> serde_json::Value {
        serde_json::json!({"data": {"user": {"projectV2": null}}})
    }

    fn owner_org_response(id: &str) -> serde_json::Value {
        serde_json::json!({"data": {"organization": {"id": id}}})
    }

    fn user_owner_response(id: &str) -> serde_json::Value {
        serde_json::json!({"data": {"user": {"id": id}}})
    }

    fn org_owner_null() -> serde_json::Value {
        serde_json::json!({"data": {"organization": null}})
    }

    fn create_project_response(number: u64, id: &str) -> serde_json::Value {
        serde_json::json!({
            "data": {"createProjectV2": {"projectV2": {"id": id, "number": number}}}
        })
    }

    fn str_var<'a>(vars: &'a [(String, GqlVar)], key: &str) -> Option<&'a str> {
        vars.iter().find_map(|(k, v)| match v {
            GqlVar::Str(s) if k == key => Some(s.as_str()),
            _ => None,
        })
    }

    // AC1: create issues createProjectV2 over the GhGraphql seam, binds the
    // returned board number as the doc id, and persists the node id.
    #[test]
    fn projects_create_authors_board_via_graphql() {
        let root = tmp_root("proj_create_ok");
        let mut store = projects_store(
            &root,
            vec![
                owner_org_response("OWN_org"),
                create_project_response(42, "PVT_42"),
            ],
        );
        let td = projects_type_def();

        let created = store.create(&td, "My Board", "", "").unwrap();
        assert_eq!(created.id, "PROJECT-42");

        let calls = store.mock().graphql_calls.borrow();
        // First call resolves the owner node id via the organization root.
        assert!(
            calls[0].0.contains("organization") && calls[0].0.contains("id"),
            "first call must resolve owner node id via organization, got: {}",
            calls[0].0
        );
        let create_call = calls
            .iter()
            .find(|(q, _)| q.contains("createProjectV2"))
            .expect("a createProjectV2 mutation must be issued");
        assert_eq!(str_var(&create_call.1, "ownerId"), Some("OWN_org"));
        assert_eq!(str_var(&create_call.1, "title"), Some("My Board"));

        let entry = store.issue_map.get("PROJECT-42").unwrap();
        assert_eq!(entry.issue_number, 42);
        assert_eq!(entry.node_id, "PVT_42");
    }

    // AC2: a missing `project` token scope yields an actionable error and
    // persists nothing (no issue-map entry, no cache file).
    #[test]
    fn projects_create_missing_scope_actionable_no_persist() {
        let root = tmp_root("proj_create_scope");
        let mut store = projects_store(
            &root,
            vec![
                owner_org_response("OWN_org"),
                serde_json::json!({"errors": [
                    {"type": "INSUFFICIENT_SCOPES",
                     "message": "Your token has not been granted the required scopes: `project`"}
                ]}),
            ],
        );
        let td = projects_type_def();

        let err = store.create(&td, "My Board", "", "").unwrap_err();
        assert!(
            err.to_string().contains("gh auth refresh -s project"),
            "must name the remedy, got: {err}"
        );

        let reloaded = IssueMap::load(&root).unwrap();
        assert!(
            reloaded.get("PROJECT-42").is_none(),
            "no board binding may be persisted on the scope-missing path"
        );
        let cache_dir = root.join(".lazyspec/cache").join(&td.name);
        assert!(
            find_cache_file(&cache_dir, "PROJECT-42").is_none(),
            "no cache file may be written on the scope-missing path"
        );
    }

    // AC3: the freshly created board binding resolves offline -- a subsequent
    // membership read keys off PROJECT-42 with no further graphql.
    #[test]
    fn projects_create_binding_resolves_offline() {
        let root = tmp_root("proj_create_offline");
        let mut store = projects_store(
            &root,
            vec![
                owner_org_response("OWN_org"),
                create_project_response(42, "PVT_42"),
            ],
        );
        let td = projects_type_def();
        store.create(&td, "My Board", "", "").unwrap();

        let calls_before = store.mock().graphql_calls.borrow().len();
        let entry = store.issue_map.get("PROJECT-42").unwrap();
        assert_eq!(entry.node_id, "PVT_42");
        // The binding came from the create response; the issue map read does not
        // touch graphql.
        assert_eq!(
            store.mock().graphql_calls.borrow().len(),
            calls_before,
            "reading the cached binding must not issue graphql"
        );
    }

    // AC4: owner resolution falls through the organization root (null) to the
    // user root for a user account.
    #[test]
    fn projects_create_owner_is_user_account() {
        let root = tmp_root("proj_create_user");
        let mut store = projects_store(
            &root,
            vec![
                org_owner_null(),
                user_owner_response("OWN_usr"),
                create_project_response(7, "PVT_7"),
            ],
        );
        let td = projects_type_def();

        let created = store.create(&td, "User Board", "", "").unwrap();
        assert_eq!(created.id, "PROJECT-7");

        let calls = store.mock().graphql_calls.borrow();
        assert!(
            calls[0].0.contains("organization"),
            "first owner query is the organization root"
        );
        assert!(
            calls[1].0.contains("user"),
            "owner resolution falls through to the user root, got: {}",
            calls[1].0
        );
        let create_call = calls
            .iter()
            .find(|(q, _)| q.contains("createProjectV2"))
            .expect("createProjectV2 mutation");
        assert_eq!(str_var(&create_call.1, "ownerId"), Some("OWN_usr"));
    }

    // AC1: an existing org board number resolves via the organization root
    // projectV2(number) query, the returned node id binds, and ZERO create
    // mutations are issued.
    #[test]
    fn projects_resolve_board_binds_node_id_no_create() {
        let root = tmp_root("proj_resolve");
        let mut store = projects_store(&root, vec![org_board_response("PVT_board7")]);
        let td = projects_type_def();

        store.set_provenance(&td, "PROJECT-7", &[]).unwrap();

        let calls = store.mock().graphql_calls.borrow();
        assert_eq!(calls.len(), 1, "one org query, nothing else");
        assert!(
            calls[0].0.contains("organization") && calls[0].0.contains("projectV2"),
            "must query organization.projectV2, got: {}",
            calls[0].0
        );
        assert!(
            !calls
                .iter()
                .any(|(q, _)| q.to_lowercase().contains("create")),
            "no create mutation may be issued"
        );

        let entry = store.issue_map.get("PROJECT-7").unwrap();
        assert_eq!(entry.issue_number, 7);
        assert_eq!(entry.node_id, "PVT_board7");
    }

    // AC2: a board number absent under both org and user roots (projectV2 null)
    // bails not-found; zero create mutations.
    #[test]
    fn projects_resolve_board_not_found_no_create() {
        let root = tmp_root("proj_notfound");
        let store = projects_store(&root, vec![org_board_null(), user_board_null()]);

        let err = store.resolve_board("my-org", 99).unwrap_err();
        assert!(err.to_string().contains("not found"), "got: {err}");

        let calls = store.mock().graphql_calls.borrow();
        assert!(
            !calls
                .iter()
                .any(|(q, _)| q.to_lowercase().contains("create")),
            "no create mutation on not-found"
        );
    }

    #[test]
    fn projects_delete_bails() {
        let root = tmp_root("proj_delete_bail");
        let mut store = projects_store(&root, vec![]);
        let td = projects_type_def();
        let err = store.delete(&td, "PROJECT-7").unwrap_err();
        assert!(
            err.to_string().contains("does not delete boards"),
            "got: {err}"
        );
    }

    // AC6: dispatch routes a github-projects type to the projects store.
    #[test]
    fn dispatch_routes_to_github_projects() {
        let root = tmp_root("dispatch_proj");
        let fs_store = FilesystemStore {
            root: root.clone(),
            config: Config::default(),
        };
        let proj_store = projects_store(&root, vec![org_board_response("PVT_x")]);

        let td = projects_type_def();
        let mut registry = DocumentStoreRegistry::new();
        registry.register(StoreBackend::Filesystem, Box::new(fs_store));
        registry.register(StoreBackend::GithubProjects, Box::new(proj_store));
        let store = registry.for_type(&td).unwrap();
        assert!(store.update(&td, "PROJECT-1", &[]).is_ok());
    }

    #[test]
    fn dispatch_github_projects_without_backend_errors() {
        let root = tmp_root("dispatch_no_proj");
        let fs_store = FilesystemStore {
            root: root.clone(),
            config: Config::default(),
        };
        let td = projects_type_def();
        let mut registry = DocumentStoreRegistry::new();
        registry.register(StoreBackend::Filesystem, Box::new(fs_store));
        let result = registry.for_type(&td);
        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("github-projects backend"));
    }

    #[test]
    fn board_number_parses_prefixed_and_bare() {
        assert_eq!(board_number("PROJECT-7").unwrap(), 7);
        assert_eq!(board_number("12").unwrap(), 12);
        assert!(board_number("PROJECT-abc").is_err());
    }

    #[test]
    fn push_cache_missing_cache_file_errors() {
        let root = tmp_root("gh_push_cache_missing");
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client: Box::new(MockGhClient::new()),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        let err = gh_store.push_cache(&td, "RFC-001").unwrap_err();
        assert!(
            err.to_string().contains("cache file not found"),
            "got: {}",
            err
        );
    }

    // --- ITERATION-217: per-board project field attributes ---

    use crate::engine::config::RelationshipDef;
    use crate::engine::document::{Relation, RelationType};
    use crate::engine::gh::{GhFieldKind, GhFieldValueInput, GhFieldValueRepr, ProjectFieldValue};
    use crate::engine::gh_schema::{OptionId, ProjectFieldId};

    fn membership_relationship_config() -> Config {
        Config {
            relationships: vec![RelationshipDef {
                name: "member-of".to_string(),
                inverse: Some("has-member".to_string()),
                github_native: Some("membership".to_string()),
                traversal: None,
            }],
            ..Default::default()
        }
    }

    fn member_meta(id: &str, boards: &[u64]) -> DocMeta {
        let related = boards
            .iter()
            .map(|n| Relation {
                rel_type: RelationType::new("member-of"),
                target: format!("PROJECT-{}", n),
            })
            .collect();
        DocMeta {
            path: PathBuf::new(),
            title: "Story".to_string(),
            doc_type: DocType::new("story"),
            status: Status::new("draft"),
            author: String::new(),
            date: Local::now().date_naive(),
            tags: vec![],
            provenance: vec![],
            related,
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            attributes: Default::default(),
            id: id.to_string(),
        }
    }

    fn issues_store_with(root: &std::path::Path, client: MockGhClient) -> GithubIssuesStore {
        GithubIssuesStore {
            client: Box::new(client),
            root: root.to_path_buf(),
            repo: "my-org/repo".to_string(),
            config: membership_relationship_config(),
            issue_map: IssueMap::load(root).unwrap(),
            issue_cache: IssueCache::new(root),
        }
    }

    // AC1: each field kind surfaces as the right coerced AttrValue under the
    // namespaced key.
    #[test]
    fn inject_project_fields_surfaces_all_kinds() {
        let root = tmp_root("iter217_inject_kinds");
        let values = vec![
            ProjectFieldValue {
                project_number: 1,
                field_name: "Status".into(),
                kind: GhFieldKind::SingleSelect,
                value: GhFieldValueRepr::OptionName("In Progress".into()),
            },
            ProjectFieldValue {
                project_number: 1,
                field_name: "Estimate".into(),
                kind: GhFieldKind::Number,
                value: GhFieldValueRepr::Number(3.0),
            },
            ProjectFieldValue {
                project_number: 1,
                field_name: "Due".into(),
                kind: GhFieldKind::Date,
                value: GhFieldValueRepr::Date(
                    chrono::NaiveDate::from_ymd_opt(2026, 6, 25).unwrap(),
                ),
            },
            ProjectFieldValue {
                project_number: 1,
                field_name: "Notes".into(),
                kind: GhFieldKind::Text,
                value: GhFieldValueRepr::Text("freeform".into()),
            },
            ProjectFieldValue {
                project_number: 1,
                field_name: "Sprint".into(),
                kind: GhFieldKind::Iteration,
                value: GhFieldValueRepr::IterationTitle("Sprint 4".into()),
            },
        ];
        let client = MockGhClient::new().with_project_field_values(values);
        let mut store = issues_store_with(&root, client);
        store.issue_map.insert("STORY-7", 7, "", "I_issue7");

        let mut meta = member_meta("STORY-7", &[1]);
        store.inject_project_fields(&mut meta).unwrap();

        assert_eq!(
            meta.attributes["PROJECT-1.Status"],
            AttrValue::Str("In Progress".into())
        );
        assert_eq!(meta.attributes["PROJECT-1.Estimate"], AttrValue::Int(3));
        assert_eq!(
            meta.attributes["PROJECT-1.Due"],
            AttrValue::Date(chrono::NaiveDate::from_ymd_opt(2026, 6, 25).unwrap())
        );
        assert_eq!(
            meta.attributes["PROJECT-1.Notes"],
            AttrValue::Str("freeform".into())
        );
        assert_eq!(
            meta.attributes["PROJECT-1.Sprint"],
            AttrValue::Str("Sprint 4".into())
        );
    }

    // AC2: same field name on two boards yields two distinct namespaced keys,
    // neither overwriting the other.
    #[test]
    fn inject_project_fields_namespaces_per_board_no_collision() {
        let root = tmp_root("iter217_namespace");
        let values = vec![
            ProjectFieldValue {
                project_number: 1,
                field_name: "Status".into(),
                kind: GhFieldKind::SingleSelect,
                value: GhFieldValueRepr::OptionName("Todo".into()),
            },
            ProjectFieldValue {
                project_number: 2,
                field_name: "Status".into(),
                kind: GhFieldKind::SingleSelect,
                value: GhFieldValueRepr::OptionName("Done".into()),
            },
        ];
        let client = MockGhClient::new().with_project_field_values(values);
        let mut store = issues_store_with(&root, client);
        store.issue_map.insert("STORY-7", 7, "", "I_issue7");

        let mut meta = member_meta("STORY-7", &[1, 2]);
        store.inject_project_fields(&mut meta).unwrap();

        assert_eq!(
            meta.attributes["PROJECT-1.Status"],
            AttrValue::Str("Todo".into())
        );
        assert_eq!(
            meta.attributes["PROJECT-2.Status"],
            AttrValue::Str("Done".into())
        );
    }

    fn write_status_snapshot(root: &std::path::Path) {
        let snapshot = GhSchemaSnapshot {
            project_fields: vec![ProjectFieldId {
                project_number: 1,
                field_name: "Status".into(),
                id: "F_status".into(),
                data_type: "SINGLE_SELECT".into(),
            }],
            single_select_options: vec![OptionId {
                field_id: "F_status".into(),
                name: "In Progress".into(),
                id: "opt_inprog".into(),
            }],
            ..Default::default()
        };
        snapshot.save(root).unwrap();
    }

    fn item_id_response(project_id: &str, item_id: &str) -> serde_json::Value {
        serde_json::json!({"data": {"node": {"projectItems": {"nodes": [
            {"id": item_id, "project": {"id": project_id}}
        ]}}}})
    }

    fn update_doc_with_attr(value: &str, root: &std::path::Path) -> GithubIssuesStore {
        write_status_snapshot(root);
        // Responses: project node-id resolve (org), then item-id lookup.
        let client = MockGhClient::new().with_graphql_responses(vec![
            org_board_response("PVT_board1"),
            item_id_response("PVT_board1", "PVTI_item1"),
        ]);
        let issue_body = make_issue_body("agent", "2026-06-25", None, "");
        let view = GhIssue {
            number: 7,
            id: "I_issue7".into(),
            url: String::new(),
            title: "Story".into(),
            body: issue_body,
            labels: vec![GhLabel {
                name: "lazyspec:story".into(),
                color: String::new(),
            }],
            state: "OPEN".into(),
            updated_at: "2026-06-25T00:00:00Z".into(),
            created_at: "2026-06-25T00:00:00Z".into(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        };
        let client = client.with_view_issue(view);
        let mut store = issues_store_with(root, client);
        store
            .issue_map
            .insert("STORY-7", 7, "2026-06-25T00:00:00Z", "I_issue7");
        store.issue_map.save(root).unwrap();
        let td = TypeDef {
            name: "story".into(),
            ..test_type_def(StoreBackend::GithubIssues)
        };
        store
            .update(&td, "STORY-7", &[("PROJECT-1.Status", value)])
            .unwrap();
        store
    }

    // AC3: single-select write resolves node-id + field/option ids, then records
    // an update whose value object is exactly {singleSelectOptionId}.
    #[test]
    fn write_single_select_three_ids_one_key() {
        let root = tmp_root("iter217_write_select");
        let store = update_doc_with_attr("In Progress", &root);

        let updates = store.mock().field_updates.borrow();
        assert_eq!(updates.len(), 1, "one field update");
        let (project_id, item_id, field_id, value) = &updates[0];
        assert_eq!(project_id, "PVT_board1");
        assert_eq!(item_id, "PVTI_item1");
        assert_eq!(field_id, "F_status");
        assert_eq!(*value, GhFieldValueInput::SingleSelect("opt_inprog".into()));
        let json = serde_json::to_value(value).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"singleSelectOptionId": "opt_inprog"})
        );
        assert!(store.mock().field_clears.borrow().is_empty());
    }

    // AC4: iteration write records exactly {iterationId}.
    #[test]
    fn write_iteration_one_key() {
        let root = tmp_root("iter217_write_iter");
        let snapshot = GhSchemaSnapshot {
            project_fields: vec![ProjectFieldId {
                project_number: 1,
                field_name: "Sprint".into(),
                id: "F_sprint".into(),
                data_type: "ITERATION".into(),
            }],
            iterations: vec![crate::engine::gh_schema::IterationId {
                field_id: "F_sprint".into(),
                title: "Sprint 4".into(),
                id: "iter_4".into(),
            }],
            ..Default::default()
        };
        snapshot.save(&root).unwrap();

        let client = MockGhClient::new()
            .with_graphql_responses(vec![
                org_board_response("PVT_board1"),
                item_id_response("PVT_board1", "PVTI_item1"),
            ])
            .with_view_issue(GhIssue {
                number: 7,
                id: "I_issue7".into(),
                url: String::new(),
                title: "Story".into(),
                body: make_issue_body("agent", "2026-06-25", None, ""),
                labels: vec![GhLabel {
                    name: "lazyspec:story".into(),
                    color: String::new(),
                }],
                state: "OPEN".into(),
                updated_at: "2026-06-25T00:00:00Z".into(),
                created_at: "2026-06-25T00:00:00Z".into(),
                author: None,
                issue_type: None,
                milestone: None,
                assignees: vec![],
            });
        let mut store = issues_store_with(&root, client);
        store
            .issue_map
            .insert("STORY-7", 7, "2026-06-25T00:00:00Z", "I_issue7");
        store.issue_map.save(&root).unwrap();
        let td = TypeDef {
            name: "story".into(),
            ..test_type_def(StoreBackend::GithubIssues)
        };
        store
            .update(&td, "STORY-7", &[("PROJECT-1.Sprint", "Sprint 4")])
            .unwrap();

        let updates = store.mock().field_updates.borrow();
        assert_eq!(updates.len(), 1);
        let json = serde_json::to_value(&updates[0].3).unwrap();
        assert_eq!(json, serde_json::json!({"iterationId": "iter_4"}));
    }

    // AC5: clearing a set single-select uses clearProjectV2ItemFieldValue, NOT an
    // empty-string text update.
    #[test]
    fn clear_field_uses_distinct_mutation() {
        let root = tmp_root("iter217_clear");
        let store = update_doc_with_attr("", &root);

        assert!(
            store.mock().field_updates.borrow().is_empty(),
            "no update mutation on clear"
        );
        let clears = store.mock().field_clears.borrow();
        assert_eq!(clears.len(), 1, "one clear mutation");
        assert_eq!(
            clears[0],
            (
                "PVT_board1".to_string(),
                "PVTI_item1".to_string(),
                "F_status".to_string()
            )
        );
    }

    // AC7: number/date/text writes record exactly {number}/{date}/{text}.
    #[test]
    fn write_number_date_text_single_key() {
        for (data_type, field, id, raw, expected) in [
            (
                "NUMBER",
                "Est",
                "F_num",
                "5",
                serde_json::json!({"number": 5.0}),
            ),
            (
                "DATE",
                "Due",
                "F_date",
                "2026-06-25",
                serde_json::json!({"date": "2026-06-25"}),
            ),
            (
                "TEXT",
                "Notes",
                "F_text",
                "hello",
                serde_json::json!({"text": "hello"}),
            ),
        ] {
            let root = tmp_root(&format!("iter217_write_{}", data_type));
            let snapshot = GhSchemaSnapshot {
                project_fields: vec![ProjectFieldId {
                    project_number: 1,
                    field_name: field.into(),
                    id: id.into(),
                    data_type: data_type.into(),
                }],
                ..Default::default()
            };
            snapshot.save(&root).unwrap();

            let client = MockGhClient::new()
                .with_graphql_responses(vec![
                    org_board_response("PVT_board1"),
                    item_id_response("PVT_board1", "PVTI_item1"),
                ])
                .with_view_issue(GhIssue {
                    number: 7,
                    id: "I_issue7".into(),
                    url: String::new(),
                    title: "Story".into(),
                    body: make_issue_body("agent", "2026-06-25", None, ""),
                    labels: vec![GhLabel {
                        name: "lazyspec:story".into(),
                        color: String::new(),
                    }],
                    state: "OPEN".into(),
                    updated_at: "2026-06-25T00:00:00Z".into(),
                    created_at: "2026-06-25T00:00:00Z".into(),
                    author: None,
                    issue_type: None,
                    milestone: None,
                    assignees: vec![],
                });
            let mut store = issues_store_with(&root, client);
            store
                .issue_map
                .insert("STORY-7", 7, "2026-06-25T00:00:00Z", "I_issue7");
            store.issue_map.save(&root).unwrap();
            let td = TypeDef {
                name: "story".into(),
                ..test_type_def(StoreBackend::GithubIssues)
            };
            let key = format!("PROJECT-1.{}", field);
            store.update(&td, "STORY-7", &[(&key, raw)]).unwrap();

            let updates = store.mock().field_updates.borrow();
            assert_eq!(updates.len(), 1, "{}: one update", data_type);
            let json = serde_json::to_value(&updates[0].3).unwrap();
            assert_eq!(json, expected, "{} value object", data_type);
        }
    }

    // AC6 (write path): an unknown option rejects offline from the snapshot
    // before ANY mutation or even a project/item id lookup is attempted.
    #[test]
    fn write_unknown_option_rejects_zero_mutations() {
        let root = tmp_root("iter217_unknown_option");
        write_status_snapshot(&root);
        let client = MockGhClient::new()
            .with_graphql_responses(vec![])
            .with_view_issue(GhIssue {
                number: 7,
                id: "I_issue7".into(),
                url: String::new(),
                title: "Story".into(),
                body: make_issue_body("agent", "2026-06-25", None, ""),
                labels: vec![GhLabel {
                    name: "lazyspec:story".into(),
                    color: String::new(),
                }],
                state: "OPEN".into(),
                updated_at: "2026-06-25T00:00:00Z".into(),
                created_at: "2026-06-25T00:00:00Z".into(),
                author: None,
                issue_type: None,
                milestone: None,
                assignees: vec![],
            });
        let mut store = issues_store_with(&root, client);
        store
            .issue_map
            .insert("STORY-7", 7, "2026-06-25T00:00:00Z", "I_issue7");
        store.issue_map.save(&root).unwrap();
        let td = TypeDef {
            name: "story".into(),
            ..test_type_def(StoreBackend::GithubIssues)
        };

        let err = store
            .update(&td, "STORY-7", &[("PROJECT-1.Status", "Frozen")])
            .unwrap_err();
        assert!(err.to_string().contains("unknown option"), "got: {}", err);
        assert!(store.mock().field_updates.borrow().is_empty());
        assert!(store.mock().field_clears.borrow().is_empty());
        // Only the check_lock issue_view read happened; no project/item graphql.
        assert!(
            store.mock().graphql_calls.borrow().is_empty(),
            "no project/item graphql lookups before offline reject"
        );
    }

    #[test]
    fn parse_project_field_key_matches_and_rejects() {
        assert_eq!(
            parse_project_field_key("PROJECT-1.Status"),
            Some((1, "Status"))
        );
        assert_eq!(
            parse_project_field_key("PROJECT-12.Due Date"),
            Some((12, "Due Date"))
        );
        assert_eq!(parse_project_field_key("owner"), None);
        assert_eq!(parse_project_field_key("PROJECT-1"), None);
        assert_eq!(parse_project_field_key("PROJECT-x.Status"), None);
    }
}
