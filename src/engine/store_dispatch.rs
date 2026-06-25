use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{bail, Result};
use chrono::Local;
use serde::Serialize;

use crate::engine::config::{Config, StoreBackend, TypeDef};
use crate::engine::document::{compose_frontmatter, AttrValue, DocMeta, DocType, Status};
use crate::engine::gh::{self, GhGraphql, GhIssueReader, GhIssueWriter, GqlVar};
use crate::engine::gh_schema::GhSchemaSnapshot;
use crate::engine::git_ref::GitRefOps;
use crate::engine::git_ref_store::GitRefStore;
use crate::engine::issue_body;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::store::{self, Store};
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
    provenance: Vec<String>,
    related: Vec<BTreeMap<String, String>>,
    /// Custom attributes are flattened to top-level frontmatter keys so the
    /// cache loader's `parse_with_schema` (which reads undeclared top-level keys)
    /// coerces them back to typed values on read-back.
    #[serde(flatten)]
    attributes: BTreeMap<String, AttrValue>,
}

#[derive(Debug)]
pub struct CreatedDoc {
    pub path: PathBuf,
    pub id: String,
}

pub trait DocumentStore {
    fn create(
        &mut self,
        type_def: &TypeDef,
        title: &str,
        author: &str,
        body: &str,
    ) -> Result<CreatedDoc>;

    fn update(&mut self, type_def: &TypeDef, doc_id: &str, updates: &[(&str, &str)]) -> Result<()>;

    fn delete(&mut self, type_def: &TypeDef, doc_id: &str) -> Result<()>;

    fn set_provenance(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        provenance: &[String],
    ) -> Result<()>;
}

pub struct FilesystemStore {
    pub root: PathBuf,
    pub config: Config,
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

        Ok(CreatedDoc { path: relative, id })
    }

    fn update(&mut self, type_def: &TypeDef, doc_id: &str, updates: &[(&str, &str)]) -> Result<()> {
        let store = Store::load(&self.root, &self.config)?;
        crate::engine::fs_ops::update_document_with_type(
            &self.root,
            &store,
            doc_id,
            updates,
            Some(type_def),
        )
    }

    fn delete(&mut self, _type_def: &TypeDef, doc_id: &str) -> Result<()> {
        let store = Store::load(&self.root, &self.config)?;
        crate::engine::fs_ops::delete_document(&self.root, &store, doc_id)
    }

    fn set_provenance(
        &mut self,
        _type_def: &TypeDef,
        doc_id: &str,
        provenance: &[String],
    ) -> Result<()> {
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
    }
}

pub struct GithubIssuesStore<G: GhIssueReader + GhIssueWriter + GhGraphql> {
    pub client: G,
    pub root: PathBuf,
    pub repo: String,
    pub config: Config,
    pub issue_map: IssueMap,
    pub issue_cache: IssueCache,
}

impl<G: GhIssueReader + GhIssueWriter + GhGraphql> GithubIssuesStore<G> {
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
        self.issue_cache.touch_lock(doc_id);

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
        let label = gh::type_label(&type_def.name);
        let color = gh::deterministic_color(&type_def.name);
        let description = format!("lazyspec document type: {}", type_def.name);
        self.client
            .label_ensure(&self.repo, &label, &description, &color)?;
        let issue = self
            .client
            .issue_create(&self.repo, &meta.title, &issue_body, &[label])?;

        let materialized_meta = DocMeta {
            id: doc_id.to_string(),
            ..meta.clone()
        };
        self.issue_map
            .insert(doc_id, issue.number, &issue.updated_at, &issue.id);
        self.issue_map.save(&self.root)?;
        write_cache_file(&self.root, type_def, &materialized_meta, &body)?;
        self.issue_cache.touch_lock(doc_id);

        Ok((issue.number, issue.id))
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
        crate::engine::gh_subissue::reconcile_subissues(&self.client, &self.repo, &plan)?;
        Ok(result)
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

impl<G: GhIssueReader + GhIssueWriter + GhGraphql> DocumentStore for GithubIssuesStore<G> {
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
            status: Status::new("draft"),
            author: author.to_string(),
            date,
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            attributes: Default::default(),
            id: String::new(),
        };

        let issue_body = issue_body::serialize(&placeholder_meta, body);
        let label = gh::type_label(&type_def.name);
        let color = gh::deterministic_color(&type_def.name);
        let description = format!("lazyspec document type: {}", type_def.name);
        self.client
            .label_ensure(&self.repo, &label, &description, &color)?;
        let issue = self
            .client
            .issue_create(&self.repo, title, &issue_body, &[label])?;

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
        self.issue_cache.touch_lock(&id);

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
        Ok(CreatedDoc { path: relative, id })
    }

    fn update(&mut self, type_def: &TypeDef, doc_id: &str, updates: &[(&str, &str)]) -> Result<()> {
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
                .map(|t| t.name.clone())
                .collect(),
            default_type: type_def.name.clone(),
            attr_defs: type_def.attributes.clone(),
        };
        let (mut meta, mut body) = issue_body::deserialize(&remote_issue.body, &ctx)?;

        let mut new_status: Option<Status> = None;
        let mut attr_updates: Vec<(&str, &str)> = Vec::new();
        let mut issue_type_update: Option<&str> = None;
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
                // The native issue-type lives in GitHub's `issueType` field, not
                // the issue-body HTML comment, so it is kept out of attr_updates.
                "issue_type" => issue_type_update = Some(value),
                _ => attr_updates.push((key, value)),
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
                    anyhow::anyhow!(
                        "invalid issue_type '{}': not a known GitHub issue type",
                        name
                    )
                })?;
                Some(Some(id.to_string()))
            }
            None => None,
        };

        let new_body = issue_body::serialize(&meta, &body);
        self.client
            .issue_edit(&self.repo, issue_number, None, Some(&new_body), &[], &[])?;

        if let Some(status) = new_status {
            let should_be_open = matches!(
                status.as_str(),
                "draft" | "review" | "accepted" | "in-progress"
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
        self.issue_cache.touch_lock(doc_id);

        Ok(())
    }

    fn set_provenance(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        provenance: &[String],
    ) -> Result<()> {
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
                .map(|t| t.name.clone())
                .collect(),
            default_type: type_def.name.clone(),
            attr_defs: type_def.attributes.clone(),
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
        self.issue_cache.touch_lock(doc_id);

        Ok(())
    }

    fn delete(&mut self, type_def: &TypeDef, doc_id: &str) -> Result<()> {
        let (issue_number, remote_issue) = self.check_lock(doc_id)?;

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

        self.issue_map.remove(doc_id);
        self.issue_map.save(&self.root)?;

        self.issue_cache.remove(doc_id, &type_def.name);

        Ok(())
    }
}

/// Map a lifecycle status to a milestone REST `state`. Closed-equivalent
/// statuses (`complete`, `rejected`, `superseded`) map to `"closed"`; everything
/// else (draft/review/accepted/in-progress) maps to `"open"`. Mirrors the
/// open/closed set used for issues.
pub fn status_to_milestone_state(status: &Status) -> &'static str {
    match status.as_str() {
        "complete" | "rejected" | "superseded" => "closed",
        _ => "open",
    }
}

/// Map a milestone REST `state` back to a lifecycle status. `"closed"` -> the
/// closed-equivalent `complete`; anything else -> the open-equivalent
/// `in-progress`.
pub fn milestone_state_to_status(state: &str) -> Status {
    if state.eq_ignore_ascii_case("closed") {
        Status::new("complete")
    } else {
        Status::new("in-progress")
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
pub struct GithubMilestonesStore<M: gh::GhMilestoneApi> {
    pub client: M,
    pub root: PathBuf,
    pub repo: String,
    pub config: Config,
    pub issue_map: IssueMap,
}

impl<M: gh::GhMilestoneApi> GithubMilestonesStore<M> {
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
        DocMeta {
            path: PathBuf::new(),
            title: milestone.title.clone(),
            doc_type: DocType::new(&type_def.name),
            status: milestone_state_to_status(&milestone.state),
            author: author.to_string(),
            date: Local::now().date_naive(),
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            attributes,
            id: id.to_string(),
        }
    }
}

impl<M: gh::GhMilestoneApi> DocumentStore for GithubMilestonesStore<M> {
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
        self.issue_map.insert(&id, milestone.number, "", "");
        self.issue_map.save(&self.root)?;

        let meta = self.meta_from_milestone(type_def, &id, &milestone, author);
        write_cache_file(&self.root, type_def, &meta, body)?;

        let cache_path = cache_dir.join(format!("{}.md", id));
        let relative = cache_path
            .strip_prefix(&self.root)
            .unwrap_or(&cache_path)
            .to_path_buf();
        Ok(CreatedDoc { path: relative, id })
    }

    fn update(&mut self, type_def: &TypeDef, doc_id: &str, updates: &[(&str, &str)]) -> Result<()> {
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
                    state = Some(status_to_milestone_state(&s).to_string());
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

        Ok(())
    }

    fn delete(&mut self, type_def: &TypeDef, doc_id: &str) -> Result<()> {
        let number = self.resolve_number(doc_id)?;
        self.client.milestone_delete(&self.repo, number)?;

        self.issue_map.remove(doc_id);
        self.issue_map.save(&self.root)?;

        let cache_dir = self.root.join(".lazyspec/cache").join(&type_def.name);
        if let Some(path) = find_cache_file(&cache_dir, doc_id) {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }

    fn set_provenance(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        provenance: &[String],
    ) -> Result<()> {
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
    }
}

const PROJECT_NODE_ID_ORG_QUERY: &str = "query($owner: String!, $number: Int!) { organization(login: $owner) { projectV2(number: $number) { id } } }";

const PROJECT_NODE_ID_USER_QUERY: &str = "query($owner: String!, $number: Int!) { user(login: $owner) { projectV2(number: $number) { id } } }";

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
/// READ/ASSOCIATE only: boards are authored on GitHub, never created or deleted
/// from lazyspec (RFC-050 non-goal). `create`/`delete` bail; `update`/
/// `set_provenance` resolve the board node id without mutating the board. The
/// owner type (org vs user) is auto-detected by trying the organization root
/// first, then falling back to the user root.
pub struct GithubProjectsStore<G: GhGraphql> {
    pub client: G,
    pub root: PathBuf,
    pub repo: String,
    pub config: Config,
    pub issue_map: IssueMap,
}

impl<G: GhGraphql> GithubProjectsStore<G> {
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

        self.issue_map.insert(doc_id, number, "", node_id.clone());
        self.issue_map.save(&self.root)?;

        let meta = DocMeta {
            path: PathBuf::new(),
            title: doc_id.to_string(),
            doc_type: DocType::new(&type_def.name),
            status: Status::new("draft"),
            author: String::new(),
            date: Local::now().date_naive(),
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            attributes: Default::default(),
            id: doc_id.to_string(),
        };
        write_cache_file(&self.root, type_def, &meta, &node_id)?;

        Ok(node_id)
    }
}

impl<G: GhGraphql> DocumentStore for GithubProjectsStore<G> {
    fn create(
        &mut self,
        _type_def: &TypeDef,
        _title: &str,
        _author: &str,
        _body: &str,
    ) -> Result<CreatedDoc> {
        bail!("github-projects backend does not author boards; bind to existing Projects v2 board number")
    }

    fn update(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        _updates: &[(&str, &str)],
    ) -> Result<()> {
        // Boards are not mutated from lazyspec; resolving the node id is the only
        // side effect, so an out-of-date binding refreshes.
        self.bind_board(type_def, doc_id)?;
        Ok(())
    }

    fn delete(&mut self, _type_def: &TypeDef, _doc_id: &str) -> Result<()> {
        bail!("github-projects backend does not delete boards; boards are managed on GitHub")
    }

    fn set_provenance(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        _provenance: &[String],
    ) -> Result<()> {
        self.bind_board(type_def, doc_id)?;
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

    let frontmatter = CacheFrontmatter {
        title: meta.title.clone(),
        doc_type: meta.doc_type.as_str().to_string(),
        status: meta.status.to_string(),
        author: meta.author.clone(),
        date: meta.date.to_string(),
        tags: meta.tags.clone(),
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
    let cache_content = compose_frontmatter(&yaml, &body_section);
    std::fs::write(&cache_path, &cache_content)?;
    Ok(())
}

pub(crate) fn find_cache_file(cache_dir: &std::path::Path, doc_id: &str) -> Option<PathBuf> {
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

#[allow(clippy::too_many_arguments)]
pub fn dispatch_for_type<
    'a,
    G: GhIssueReader + GhIssueWriter + GhGraphql,
    R: GitRefOps,
    M: gh::GhMilestoneApi,
    P: GhGraphql,
>(
    type_def: &TypeDef,
    fs_store: &'a mut FilesystemStore,
    gh_store: Option<&'a mut GithubIssuesStore<G>>,
    git_ref_store: Option<&'a mut GitRefStore<R>>,
    milestone_store: Option<&'a mut GithubMilestonesStore<M>>,
    projects_store: Option<&'a mut GithubProjectsStore<P>>,
) -> Result<&'a mut dyn DocumentStore> {
    match type_def.store {
        StoreBackend::Filesystem => Ok(fs_store as &mut dyn DocumentStore),
        StoreBackend::GithubIssues => match gh_store {
            Some(s) => Ok(s as &mut dyn DocumentStore),
            None => bail!(
                "type '{}' uses {} store but no GitHub backend is configured",
                type_def.name,
                type_def.store
            ),
        },
        StoreBackend::GithubMilestones => match milestone_store {
            Some(s) => Ok(s as &mut dyn DocumentStore),
            None => bail!(
                "type '{}' uses {} store but no GitHub milestones backend is configured",
                type_def.name,
                type_def.store
            ),
        },
        StoreBackend::GithubProjects => match projects_store {
            Some(s) => Ok(s as &mut dyn DocumentStore),
            None => bail!(
                "type '{}' uses {} store but no GitHub projects backend is configured",
                type_def.name,
                type_def.store
            ),
        },
        StoreBackend::GitRef => match git_ref_store {
            Some(s) => Ok(s as &mut dyn DocumentStore),
            None => bail!(
                "type '{}' uses git-ref store but no git-ref backend is configured",
                type_def.name,
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{Config, NumberingStrategy, StoreBackend, TypeDef};
    use crate::engine::gh::{
        test_support::{MockGhClient, MockGhMilestoneClient},
        GhIssue, GhLabel, GhMilestoneApi,
    };
    use crate::engine::git_ref::test_support::MockGitRefClient;
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
            client: MockGhClient::new(),
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
        let create_body = gh_store.client.last_create_body.borrow();
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
        assert_eq!(parsed["status"].as_str().unwrap(), "draft");
        assert_eq!(parsed["author"].as_str().unwrap(), "author");
        assert!(content.contains("body text"));
    }

    #[test]
    fn github_issues_create_updates_issue_map() {
        let root = tmp_root("gh_create_map");
        let mut gh_store = GithubIssuesStore {
            client: MockGhClient::new(),
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
    fn github_issues_create_persists_issue_map() {
        let root = tmp_root("gh_create_persist");
        let mut gh_store = GithubIssuesStore {
            client: MockGhClient::new(),
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
            client: MockGhClient::new(),
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
            client: MockGhClient::new(),
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
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client,
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store
            .update(&td, "RFC-001", &[("status", "accepted")])
            .unwrap();

        // Re-serialized body sent to GH should not contain author:
        let captured = gh_store.client.last_edit_body.borrow();
        let body_str = captured
            .as_deref()
            .expect("issue_edit should have been called with body");
        assert!(
            !body_str.contains("author:"),
            "re-serialized issue body should not contain author:, got: {}",
            body_str
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
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client,
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
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client,
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
        assert!(gh_store.client.closed.get());
        assert!(!gh_store.client.reopened.get());
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
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client,
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
        assert!(gh_store.client.reopened.get());
        assert!(!gh_store.client.closed.get());
    }

    #[test]
    fn github_issues_update_not_in_map() {
        let root = tmp_root("gh_update_nomap");
        let mut gh_store = GithubIssuesStore {
            client: MockGhClient::new(),
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
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client,
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store.delete(&td, "RFC-001").unwrap();

        assert!(gh_store.client.closed.get());
        let title = gh_store.client.last_edit_title.borrow();
        assert_eq!(title.as_deref(), Some("[DELETED] My RFC"));
        let labels_remove = gh_store.client.last_edit_labels_remove.borrow();
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
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client,
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
        assert!(!gh_store.client.closed.get());
    }

    #[test]
    fn github_issues_delete_not_in_map() {
        let root = tmp_root("gh_delete_nomap");
        let mut gh_store = GithubIssuesStore {
            client: MockGhClient::new(),
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
            client,
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
    ) -> GithubMilestonesStore<MockGhMilestoneClient> {
        GithubMilestonesStore {
            client,
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
        assert_eq!(store.client.create_calls.get(), 1);
        let ms = &store.client.milestones.borrow()[0];
        assert_eq!(ms.title, "v1.0");
        assert_eq!(ms.description, "first release");

        let entry = store.issue_map.get(&created.id).unwrap();
        assert_eq!(entry.issue_number, 1);

        assert!(root.join(&created.path).exists());
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

        let edit = store.client.last_edit.borrow();
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
        assert_eq!(milestone_state_to_status("closed").as_str(), "complete");
        assert_eq!(milestone_state_to_status("open").as_str(), "in-progress");
        assert_eq!(
            status_to_milestone_state(&Status::new("complete")),
            "closed"
        );
        assert_eq!(status_to_milestone_state(&Status::new("draft")), "open");
    }

    #[test]
    fn milestone_update_status_complete_closes_state_and_cache() {
        let root = tmp_root("ms_close");
        let mut store = milestone_store(&root, MockGhMilestoneClient::new());
        let td = test_type_def(StoreBackend::GithubMilestones);

        let created = store.create(&td, "v1.0", "author", "desc").unwrap();
        store
            .update(&td, &created.id, &[("status", "complete")])
            .unwrap();

        let edit = store.client.last_edit.borrow();
        assert_eq!(edit.as_ref().unwrap().state.as_deref(), Some("closed"));

        let cache = std::fs::read_to_string(root.join(&created.path)).unwrap();
        assert!(cache.contains("status: complete"), "cache: {cache}");
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
        assert!(store.client.last_edit.borrow().is_none());
    }

    #[test]
    fn milestone_delete_removes_milestone_map_and_cache() {
        let root = tmp_root("ms_delete");
        let mut store = milestone_store(&root, MockGhMilestoneClient::new());
        let td = test_type_def(StoreBackend::GithubMilestones);
        let created = store.create(&td, "v1.0", "author", "desc").unwrap();

        store.delete(&td, &created.id).unwrap();

        assert!(store.client.milestones.borrow().is_empty());
        assert!(store.issue_map.get(&created.id).is_none());
        assert!(!root.join(&created.path).exists());
    }

    // AC6: dispatch routes a github-milestones type to the milestone store.
    #[test]
    fn dispatch_routes_to_github_milestones() {
        let root = tmp_root("dispatch_ms");
        let mut fs_store = FilesystemStore {
            root: root.clone(),
            config: Config::default(),
        };
        let mut ms_store = milestone_store(&root, MockGhMilestoneClient::new());

        let td = test_type_def(StoreBackend::GithubMilestones);
        let store = dispatch_for_type::<MockGhClient, MockGitRefClient, _, MockGhClient>(
            &td,
            &mut fs_store,
            None,
            None,
            Some(&mut ms_store),
            None,
        )
        .unwrap();
        let result = store.create(&td, "dispatched", "author", "");
        assert!(result.is_ok());
    }

    #[test]
    fn dispatch_github_milestones_without_backend_errors() {
        let root = tmp_root("dispatch_no_ms");
        let mut fs_store = FilesystemStore {
            root: root.clone(),
            config: Config::default(),
        };
        let td = test_type_def(StoreBackend::GithubMilestones);
        let result = dispatch_for_type::<
            MockGhClient,
            MockGitRefClient,
            MockGhMilestoneClient,
            MockGhClient,
        >(&td, &mut fs_store, None, None, None, None);
        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("GitHub milestones backend"));
    }

    #[test]
    fn dispatch_routes_to_filesystem() {
        let root = tmp_root("dispatch_fs");
        let config = Config::default();

        let mut fs_store = FilesystemStore {
            root: root.clone(),
            config,
        };

        let td = test_type_def(StoreBackend::Filesystem);
        let store = dispatch_for_type::<
            MockGhClient,
            MockGitRefClient,
            MockGhMilestoneClient,
            MockGhClient,
        >(&td, &mut fs_store, None, None, None, None)
        .unwrap();

        // Should succeed (routed to filesystem)
        let result = store.create(&td, "dispatched", "author", "");
        assert!(result.is_ok());
    }

    #[test]
    fn dispatch_routes_to_github() {
        let root = tmp_root("dispatch_gh");
        let config = Config::default();

        let mut fs_store = FilesystemStore {
            root: root.clone(),
            config,
        };

        let mut gh_store = GithubIssuesStore {
            client: MockGhClient::new(),
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: IssueMap::load(&root).unwrap(),
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        let store = dispatch_for_type::<_, MockGitRefClient, MockGhMilestoneClient, MockGhClient>(
            &td,
            &mut fs_store,
            Some(&mut gh_store),
            None,
            None,
            None,
        )
        .unwrap();

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
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client,
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

        let captured = gh_store.client.last_edit_body.borrow();
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
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client,
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

        let captured = gh_store.client.last_edit_body.borrow();
        let body_str = captured
            .as_deref()
            .expect("issue_edit should have been called with body");
        assert!(body_str.contains("new"), "body should contain updated text");
        assert!(
            gh_store.client.closed.get(),
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
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client,
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

        let mut fs_store = FilesystemStore {
            root: root.clone(),
            config,
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        let result = dispatch_for_type::<
            MockGhClient,
            MockGitRefClient,
            MockGhMilestoneClient,
            MockGhClient,
        >(&td, &mut fs_store, None, None, None, None);
        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("no GitHub backend"));
    }

    #[test]
    fn dispatch_routes_to_git_ref() {
        let root = tmp_root("dispatch_gitref");
        let config = Config::default();

        let mut fs_store = FilesystemStore {
            root: root.clone(),
            config,
        };

        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![]))
            .with_create_ref_commit_result(Ok("abc123".into()));
        let mut git_ref_store = GitRefStore {
            git: mock,
            root: root.clone(),
            config: Config::default(),
            reserved_number: None,
        };

        let td = test_type_def(StoreBackend::GitRef);
        let store = dispatch_for_type::<MockGhClient, _, MockGhMilestoneClient, MockGhClient>(
            &td,
            &mut fs_store,
            None,
            Some(&mut git_ref_store),
            None,
            None,
        )
        .unwrap();

        let result = store.create(&td, "dispatched", "author", "");
        assert!(result.is_ok());
    }

    #[test]
    fn dispatch_filesystem_ignores_git_ref_store() {
        let root = tmp_root("dispatch_fs_ignores_gitref");
        let config = Config::default();

        let mut fs_store = FilesystemStore {
            root: root.clone(),
            config,
        };

        let mock = MockGitRefClient::new();
        let mut git_ref_store = GitRefStore {
            git: mock,
            root: root.clone(),
            config: Config::default(),
            reserved_number: None,
        };

        let td = test_type_def(StoreBackend::Filesystem);
        let store = dispatch_for_type::<MockGhClient, _, MockGhMilestoneClient, MockGhClient>(
            &td,
            &mut fs_store,
            None,
            Some(&mut git_ref_store),
            None,
            None,
        )
        .unwrap();

        let result = store.create(&td, "dispatched", "author", "");
        assert!(result.is_ok());
        assert!(
            git_ref_store.git.calls.borrow().is_empty(),
            "GitRefStore should not have been invoked for a Filesystem type"
        );
    }

    #[test]
    fn dispatch_git_ref_without_backend_errors() {
        let root = tmp_root("dispatch_no_gitref");
        let config = Config::default();

        let mut fs_store = FilesystemStore {
            root: root.clone(),
            config,
        };

        let td = test_type_def(StoreBackend::GitRef);
        let result = dispatch_for_type::<
            MockGhClient,
            MockGitRefClient,
            MockGhMilestoneClient,
            MockGhClient,
        >(&td, &mut fs_store, None, None, None, None);
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
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client,
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store.push_cache(&td, "RFC-001").unwrap();

        let captured = gh_store.client.last_edit_body.borrow();
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
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client,
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

        let captured = gh_store.client.last_edit_body.borrow();
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
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client,
            root: root.clone(),
            repo: "owner/repo".to_string(),
            config: Config::default(),
            issue_map: map,
            issue_cache: IssueCache::new(&root),
        };

        let td = test_type_def(StoreBackend::GithubIssues);
        gh_store.set_provenance(&td, "RFC-001", &[]).unwrap();

        let captured = gh_store.client.last_edit_body.borrow();
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
            coordination: None,
            agents: Default::default(),
            skills: Default::default(),
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
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client,
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
        let captured = gh_store.client.last_edit_body.borrow();
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
        };

        let client = MockGhClient::new().with_view_issue(view_issue);
        let mut map = IssueMap::load(&root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");

        let mut gh_store = GithubIssuesStore {
            client,
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
            gh_store.client.last_edit_body.borrow().is_none(),
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
        }
    }

    fn issue_type_store(
        root: &std::path::Path,
        graphql: Vec<serde_json::Value>,
    ) -> GithubIssuesStore<MockGhClient> {
        let view_issue = gh_view_issue_with_lazyspec_body(42, vec!["lazyspec:story"]);
        let client = MockGhClient::new()
            .with_view_issue(view_issue)
            .with_graphql_responses(graphql);
        let mut map = IssueMap::load(root).unwrap();
        map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");
        GithubIssuesStore {
            client,
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

        let calls = gh_store.client.graphql_calls.borrow();
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
        let body = gh_store.client.last_edit_body.borrow();
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

        let calls = gh_store.client.graphql_calls.borrow();
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
            gh_store.client.graphql_calls.borrow().is_empty(),
            "zero graphql mutations on invalid issue_type"
        );
        assert!(
            gh_store.client.last_edit_body.borrow().is_none(),
            "no issue_edit on invalid issue_type"
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
            gh_store.client.last_edit_labels_remove.borrow().is_empty(),
            "issue_type write must not remove labels"
        );

        // doc_type stays story in the cache file (from the lazyspec:story label).
        let cache_path = root.join(".lazyspec/cache/rfc/RFC-001.md");
        let cache_content = std::fs::read_to_string(&cache_path).unwrap();
        let (yaml, _) = crate::engine::document::split_frontmatter(&cache_content).unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed["type"].as_str().unwrap(), "story");
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

    fn subdir_gh_store(root: &std::path::Path, td: &TypeDef) -> GithubIssuesStore<MockGhClient> {
        // The subIssues read (empty) followed by enough generic mutation-OK
        // responses for the add/reprioritize calls the reconcile may issue.
        let mut responses = vec![serde_json::json!({
            "data": {"node": {"subIssues": {"nodes": []}}}
        })];
        responses.extend(std::iter::repeat_with(|| serde_json::json!({"data": {}})).take(8));
        GithubIssuesStore {
            client: MockGhClient::new().with_graphql_responses(responses),
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
            store.client.create_titles.borrow().len(),
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
        let calls = store.client.graphql_calls.borrow();
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
        let calls = store.client.graphql_calls.borrow();
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
        let parent_body = store.client.last_create_body.borrow();
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
            attributes: Default::default(),
            id: "STORY-400".to_string(),
        };
        let body = issue_body::serialize(&parent_meta, "body");
        assert!(
            body.contains("implements: RFC-050"),
            "implements must be in the serialized issue body, got: {body}"
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
    ) -> GithubProjectsStore<MockGhClient> {
        GithubProjectsStore {
            client: MockGhClient::new().with_graphql_responses(graphql),
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

    // AC1: an existing org board number resolves via the organization root
    // projectV2(number) query, the returned node id binds, and ZERO create
    // mutations are issued.
    #[test]
    fn projects_resolve_board_binds_node_id_no_create() {
        let root = tmp_root("proj_resolve");
        let mut store = projects_store(&root, vec![org_board_response("PVT_board7")]);
        let td = projects_type_def();

        store.set_provenance(&td, "PROJECT-7", &[]).unwrap();

        let calls = store.client.graphql_calls.borrow();
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

        let calls = store.client.graphql_calls.borrow();
        assert!(
            !calls
                .iter()
                .any(|(q, _)| q.to_lowercase().contains("create")),
            "no create mutation on not-found"
        );
    }

    // AC2 corollary: create() never authors a board.
    #[test]
    fn projects_create_does_not_author_boards() {
        let root = tmp_root("proj_create_bail");
        let mut store = projects_store(&root, vec![]);
        let td = projects_type_def();

        let err = store.create(&td, "title", "author", "").unwrap_err();
        assert!(
            err.to_string().contains("does not author boards"),
            "got: {err}"
        );
        assert!(
            store.client.graphql_calls.borrow().is_empty(),
            "no graphql mutation on create"
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
        let mut fs_store = FilesystemStore {
            root: root.clone(),
            config: Config::default(),
        };
        let mut proj_store = projects_store(&root, vec![org_board_response("PVT_x")]);

        let td = projects_type_def();
        let store = dispatch_for_type::<MockGhClient, MockGitRefClient, MockGhMilestoneClient, _>(
            &td,
            &mut fs_store,
            None,
            None,
            None,
            Some(&mut proj_store),
        )
        .unwrap();
        assert!(store.update(&td, "PROJECT-1", &[]).is_ok());
    }

    #[test]
    fn dispatch_github_projects_without_backend_errors() {
        let root = tmp_root("dispatch_no_proj");
        let mut fs_store = FilesystemStore {
            root: root.clone(),
            config: Config::default(),
        };
        let td = projects_type_def();
        let result = dispatch_for_type::<
            MockGhClient,
            MockGitRefClient,
            MockGhMilestoneClient,
            MockGhClient,
        >(&td, &mut fs_store, None, None, None, None);
        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("GitHub projects backend"));
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
            client: MockGhClient::new(),
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
}
