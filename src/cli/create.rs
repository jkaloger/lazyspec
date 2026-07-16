use crate::cli::json::doc_to_json;
use crate::engine::clickup::ClickupHttpClient;
use crate::engine::config::{validate_status, Config, StoreBackend, ValidationRule};
use crate::engine::credentials::{CredentialStore, LayeredCredentialStore};
use crate::engine::document::Status;
use crate::engine::document::{split_frontmatter, DocMeta, DocType};
use crate::engine::fs_ops;
use crate::engine::gh::GhCli;
use crate::engine::git_ref::GitCli;
use crate::engine::git_ref_store::GitRefStore;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::reservation;
use crate::engine::store::{Filter, Store};
use crate::engine::store_dispatch::{
    ClickupTasksStore, DocumentStore, GithubIssuesStore, GithubMilestonesStore,
};
use anyhow::{anyhow, bail, Result};
use std::fs;
use std::path::{Path, PathBuf};

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
}

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
) -> Result<PathBuf> {
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

    if let Some(parent_id) = parent {
        return create_with_parent(
            root, config, store, type_def, title, author, body, parent_id,
        );
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
        return Ok(root.join(&created.path));
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
        return Ok(root.join(&created.path));
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
        return Ok(root.join(&created.path));
    }

    if type_def.store == StoreBackend::GitRef {
        let mut store = GitRefStore {
            git: Box::new(GitCli),
            root: root.to_path_buf(),
            config: config.clone(),
            reserved_number: None,
        };
        let created = store.create(type_def, title, author, body.unwrap_or(""))?;
        return Ok(root.join(&created.path));
    }

    if type_def.store == StoreBackend::ClickupTasks {
        // The registry leaves the ClickUp store's token unloaded to keep
        // registry construction free of keychain I/O; the write path loads it
        // here, from the global credential store (keychain-first, file
        // fallback), never a repo-local file.
        let token = LayeredCredentialStore::global()
            .load_clickup_token()?
            .ok_or_else(|| {
                anyhow!(
                    "no ClickUp token found; run `lazyspec setup clickup` before creating \
                     clickup-tasks documents"
                )
            })?;
        let mut store = ClickupTasksStore {
            client: Box::new(ClickupHttpClient::new()),
            root: root.to_path_buf(),
            config: config.clone(),
            token: Some(token),
        };
        let created = store.create(type_def, title, author, body.unwrap_or(""))?;
        return Ok(root.join(&created.path));
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
        let content = fs::read_to_string(&path)?;
        let (yaml, _) = split_frontmatter(&content)?;
        let new_content = format!("---\n{}\n---\n\n{}\n", yaml.trim(), body_text);
        fs::write(&path, new_content)?;
    }

    Ok(path)
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

pub fn run_json(
    root: &Path,
    config: &Config,
    store: &Store,
    doc_type: &str,
    title: &str,
    author: &str,
    on_progress: impl Fn(reservation::ReservationProgress),
) -> Result<String> {
    run_json_with_body(
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
}

#[allow(clippy::too_many_arguments)]
pub fn run_json_with_body(
    root: &Path,
    config: &Config,
    store: &Store,
    doc_type: &str,
    title: &str,
    author: &str,
    parent: Option<&str>,
    body: Option<&str>,
    on_progress: impl Fn(reservation::ReservationProgress),
) -> Result<String> {
    let path = run_with_body(
        root,
        config,
        store,
        doc_type,
        title,
        author,
        parent,
        body,
        on_progress,
    )?;
    let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();

    let content = fs::read_to_string(&path)?;
    let mut meta = DocMeta::parse(&content)?;
    meta.path = relative;

    let json = doc_to_json(&meta);
    Ok(serde_json::to_string_pretty(&json)?)
}
