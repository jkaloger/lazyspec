use crate::engine::clickup::ClickupHttpClient;
use crate::engine::config::{validate_status, Config, EdgeDef, StoreBackend, ValidationRule};
use crate::engine::credentials::{CredentialStore, LayeredCredentialStore};
use crate::engine::document::DocType;
use crate::engine::document::Status;
use crate::engine::fs_ops;
use crate::engine::gh::GhCli;
use crate::engine::git_ref::GitCli;
use crate::engine::git_ref_store::GitRefStore;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::reservation;
use crate::engine::store::{Filter, Store};
use crate::engine::store_dispatch::{
    DocumentStore, GithubIssuesStore, GithubMilestonesStore, PushOutcome,
};
use anyhow::{anyhow, bail, Result};
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// One target type of an edge whose `require_to_status` gate nothing satisfies.
///
/// `current_statuses` holds the distinct statuses documents of that type sit at
/// today, and is empty when the project holds none: the gate asks whether *any*
/// document of the type has reached the status, so there is no single current
/// status to name and nothing to invent when there are no documents at all.
#[derive(Debug, Clone, PartialEq)]
pub struct UnsatisfiedTargetGate {
    pub target_type: String,
    pub required_status: String,
    pub current_statuses: Vec<String>,
}

/// A `create` refused because no target of `edge` has reached the status its
/// `require_to_status` map demands (RFC-067, STORY-255).
///
/// Carried as a typed error so the CLI can render the fields as JSON without
/// re-parsing the message.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeStatusRefusal {
    pub doc_type: String,
    pub edge: String,
    pub unsatisfied: Vec<UnsatisfiedTargetGate>,
}

impl fmt::Display for EdgeStatusRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cannot create {}: edge \"{}\" requires ",
            self.doc_type, self.edge
        )?;
        for (i, gate) in self.unsatisfied.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{} at \"{}\" ", gate.target_type, gate.required_status)?;
            if gate.current_statuses.is_empty() {
                write!(f, "(none exists)")?;
            } else {
                write!(f, "(found {})", gate.current_statuses.join(", "))?;
            }
        }
        Ok(())
    }
}

impl std::error::Error for EdgeStatusRefusal {}

pub fn run(
    root: &Path,
    config: &Config,
    store: &Store,
    doc_type: &str,
    title: &str,
    author: &str,
    on_progress: impl Fn(reservation::ReservationProgress),
) -> Result<PathBuf> {
    run_with_body(
        root,
        config,
        store,
        doc_type,
        title,
        author,
        None,
        None,
        on_progress,
    )
    .map(|(path, _)| path)
}

/// Author a document, returning its path alongside the backend push outcome.
///
/// The outcome is `Synced` for every synchronous backend (filesystem, and the
/// REST/GraphQL stores whose create either lands remotely or errors); only a
/// git-ref-backed create can report `LocalOnly` when the deferred push cannot
/// reach the remote, carrying the warning the CLI surfaces in its JSON.
#[allow(clippy::too_many_arguments)]
pub fn run_with_body(
    root: &Path,
    config: &Config,
    store: &Store,
    doc_type: &str,
    title: &str,
    author: &str,
    parent: Option<&str>,
    body: Option<&str>,
    on_progress: impl Fn(reservation::ReservationProgress),
) -> Result<(PathBuf, PushOutcome)> {
    let type_def = config.type_by_name(doc_type).ok_or_else(|| {
        anyhow!(
            "unknown doc type: '{}'. valid types: {}",
            doc_type,
            config
                .documents
                .types
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;

    if type_def.singleton {
        let existing: Vec<_> = store.list(&Filter {
            doc_type: Some(DocType::new(doc_type)),
            ..Default::default()
        });
        if let Some(doc) = existing.first() {
            bail!("{} already exists at {}", doc_type, doc.path.display());
        }
    }

    for rule in &config.rules {
        let ValidationRule::ParentChild {
            child,
            parent,
            require_parent_status: Some(required),
            ..
        } = rule
        else {
            continue;
        };
        if child != doc_type {
            continue;
        }

        let required_status = Status::new(required);
        if let Some(parent_def) = config.type_by_name(parent) {
            validate_status(parent_def, &required_status)?;
        }

        let satisfied = store
            .docs
            .values()
            .any(|d| d.doc_type == DocType::new(parent) && d.status == required_status);
        if !satisfied {
            bail!(
                "cannot create {} until a {} reaches status \"{}\"",
                doc_type,
                parent,
                required
            );
        }
    }

    // The edge table's gate runs alongside the scalar one above; STORY-259
    // retires `[[rules]]` and with it the scalar.
    for edge in &config.edges {
        if edge.from != doc_type || edge.require_to_status.is_empty() {
            continue;
        }
        if let Some(refusal) = edge_status_refusal(store, doc_type, edge) {
            return Err(anyhow!(refusal));
        }
    }

    if let Some(parent_id) = parent {
        return create_with_parent(
            root, config, store, type_def, title, author, body, parent_id,
        )
        .map(|path| (path, PushOutcome::Synced));
    }

    if type_def.store == StoreBackend::GithubIssues {
        let gh_config = config.documents.github.as_ref().ok_or_else(|| {
            anyhow!(
                "type '{}' uses github-issues store but no [github] config found",
                doc_type
            )
        })?;
        let repo = gh_config.repo.as_ref().ok_or_else(|| {
            anyhow!(
                "type '{}' uses github-issues store but no github.repo configured",
                doc_type
            )
        })?;
        let mut store = GithubIssuesStore {
            client: Box::new(GhCli::new()),
            root: root.to_path_buf(),
            repo: repo.clone(),
            config: config.clone(),
            issue_map: IssueMap::load(root)?,
            issue_cache: IssueCache::new(root),
        };
        let created = store.create(type_def, title, author, body.unwrap_or(""))?;
        return Ok((root.join(&created.path), created.push_outcome));
    }

    if type_def.store == StoreBackend::GithubMilestones {
        let gh_config = config.documents.github.as_ref().ok_or_else(|| {
            anyhow!(
                "type '{}' uses github-milestones store but no [github] config found",
                doc_type
            )
        })?;
        let repo = gh_config.repo.as_ref().ok_or_else(|| {
            anyhow!(
                "type '{}' uses github-milestones store but no github.repo configured",
                doc_type
            )
        })?;
        let mut store = GithubMilestonesStore {
            client: Box::new(GhCli::new()),
            root: root.to_path_buf(),
            repo: repo.clone(),
            config: config.clone(),
            issue_map: IssueMap::load(root)?,
        };
        let created = store.create(type_def, title, author, body.unwrap_or(""))?;
        return Ok((root.join(&created.path), created.push_outcome));
    }

    if type_def.store == StoreBackend::GithubProjects {
        let gh_config = config.documents.github.as_ref().ok_or_else(|| {
            anyhow!(
                "type '{}' uses github-projects store but no [github] config found",
                doc_type
            )
        })?;
        let repo = gh_config.repo.as_ref().ok_or_else(|| {
            anyhow!(
                "type '{}' uses github-projects store but no github.repo configured",
                doc_type
            )
        })?;
        let mut store = crate::engine::store_dispatch::GithubProjectsStore {
            client: Box::new(GhCli::new()),
            root: root.to_path_buf(),
            repo: repo.clone(),
            config: config.clone(),
            issue_map: IssueMap::load(root)?,
        };
        let created = store.create(type_def, title, author, body.unwrap_or(""))?;
        return Ok((root.join(&created.path), created.push_outcome));
    }

    if type_def.store == StoreBackend::GitRef {
        let mut store = GitRefStore {
            git: Box::new(GitCli),
            root: root.to_path_buf(),
            config: config.clone(),
            remote: config.git_ref.remote.clone(),
            reserved_number: None,
        };
        let created = store.create(type_def, title, author, body.unwrap_or(""))?;
        return Ok((root.join(&created.path), created.push_outcome));
    }

    if type_def.store == StoreBackend::ClickupTasks {
        // The registry leaves the ClickUp store's token unloaded to keep
        // registry construction free of keychain I/O; the shared write-store
        // helper loads it (from the global credential store: keychain-first,
        // file fallback, never a repo-local file) and binds a token-bearing
        // store.
        let mut store = crate::engine::store_dispatch::clickup_write_store(
            root,
            config,
            "creating",
            ClickupHttpClient::new,
            || LayeredCredentialStore::global().load_clickup_token(),
        )?;
        let created = store.create(type_def, title, author, body.unwrap_or(""))?;
        return Ok((root.join(&created.path), created.push_outcome));
    }

    let path = fs_ops::create_document(
        root,
        config,
        doc_type,
        &type_def.dir,
        &type_def.prefix,
        title,
        author,
        &type_def.numbering,
        type_def.subdirectory,
        on_progress,
    )?;

    if let Some(body_text) = body {
        fs_ops::replace_body(&path, body_text)?;
    }

    Ok((path, PushOutcome::Synced))
}

/// Whether `edge` withholds creation, and if so which of its target types are
/// unsatisfied.
///
/// An edge over a target set is satisfied by any one member, mirroring
/// `required` (RFC-067 §Design). A gated member is satisfied when some document
/// of that type has reached its required status; an ungated member — one absent
/// from `require_to_status` — by the existence of any document of that type.
fn edge_status_refusal(store: &Store, doc_type: &str, edge: &EdgeDef) -> Option<EdgeStatusRefusal> {
    let mut unsatisfied = Vec::new();

    for target in &edge.to {
        let target_type = DocType::new(target);
        let current: BTreeSet<&str> = store
            .docs
            .values()
            .filter(|d| d.doc_type == target_type)
            .map(|d| d.status.as_str())
            .collect();

        let Some(required) = edge.require_to_status.get(target) else {
            if current.is_empty() {
                continue;
            }
            return None;
        };

        if current.contains(Status::new(required).as_str()) {
            return None;
        }
        unsatisfied.push(UnsatisfiedTargetGate {
            target_type: target.clone(),
            required_status: required.clone(),
            current_statuses: current.into_iter().map(str::to_string).collect(),
        });
    }

    Some(EdgeStatusRefusal {
        doc_type: doc_type.to_string(),
        edge: edge.name.clone(),
        unsatisfied,
    })
}

/// Author a child of `parent_id`, branching on the child type's store.
///
/// For github-issues children the child becomes a REAL GitHub issue bound as a
/// native sub-issue of the parent immediately at create time (via
/// [`GithubIssuesStore::create_child_subissue`]) -- no local-only `.md` is left
/// behind. For filesystem (and any other) store the child is written as a
/// sibling `.md` inside the parent's subdirectory, promoting a flat parent to
/// `TYPE-n-slug/index.md` on the first child; the loader tracks the new
/// parent/child edges directly.
///
/// Both branches enforce the same-store guard: parent and child must share a
/// [`StoreBackend`].
#[allow(clippy::too_many_arguments)]
fn create_with_parent(
    root: &Path,
    config: &Config,
    store: &Store,
    child_type_def: &crate::engine::config::TypeDef,
    title: &str,
    author: &str,
    body: Option<&str>,
    parent_id: &str,
) -> Result<PathBuf> {
    let parent_meta = store
        .resolve_shorthand(parent_id)
        .map_err(|_| anyhow!("could not resolve parent document: {}", parent_id))?;

    let parent_type_def = config
        .type_by_name(parent_meta.doc_type.as_str())
        .ok_or_else(|| {
            anyhow!(
                "parent {} has unknown type '{}'",
                parent_id,
                parent_meta.doc_type
            )
        })?;

    if child_type_def.store != parent_type_def.store {
        bail!(
            "sub-issue link rejected: parent {} (store {}) and child type {} (store {}) \
             are in different stores; lazyspec sub-issues are same-store only",
            parent_id,
            parent_type_def.store,
            child_type_def.name,
            child_type_def.store
        );
    }

    if child_type_def.store == StoreBackend::GithubIssues {
        let gh_config = config.documents.github.as_ref().ok_or_else(|| {
            anyhow!(
                "type '{}' uses github-issues store but no [github] config found",
                child_type_def.name
            )
        })?;
        let repo = gh_config.repo.as_ref().ok_or_else(|| {
            anyhow!(
                "type '{}' uses github-issues store but no github.repo configured",
                child_type_def.name
            )
        })?;
        let mut gh_store = GithubIssuesStore {
            client: Box::new(GhCli::new()),
            root: root.to_path_buf(),
            repo: repo.clone(),
            config: config.clone(),
            issue_map: IssueMap::load(root)?,
            issue_cache: IssueCache::new(root),
        };
        let created = gh_store.create_child_subissue(
            child_type_def,
            parent_id,
            title,
            author,
            body.unwrap_or(""),
        )?;
        return Ok(root.join(&created.path));
    }

    let parent_path = root.join(&parent_meta.path);
    let is_index = parent_path
        .file_name()
        .and_then(|f| f.to_str())
        .map(|f| f == "index.md")
        .unwrap_or(false);

    let parent_subdir = if is_index {
        parent_path
            .parent()
            .ok_or_else(|| {
                anyhow!(
                    "parent index.md has no directory: {}",
                    parent_path.display()
                )
            })?
            .to_path_buf()
    } else {
        let parent_dir = parent_path
            .parent()
            .ok_or_else(|| anyhow!("parent doc has no directory: {}", parent_path.display()))?;
        let stem = parent_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("parent doc has no file stem: {}", parent_path.display()))?;
        let new_dir = parent_dir.join(stem);
        let new_index = new_dir.join("index.md");
        fs::create_dir_all(&new_dir)?;
        fs::rename(&parent_path, &new_index)?;
        new_dir
    };

    fs_ops::create_child_in_dir(
        root,
        config,
        child_type_def,
        &parent_subdir,
        title,
        author,
        body,
    )
}
