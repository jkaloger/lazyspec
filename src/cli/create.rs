use crate::cli::json::doc_to_json;
use crate::engine::config::{Config, StoreBackend};
use crate::engine::document::{DocMeta, DocType};
use crate::engine::fs_ops;
use crate::engine::gh::GhCli;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::reservation;
use crate::engine::store::{Filter, Store};
use crate::engine::store_dispatch::{DocumentStore, GithubIssuesStore};
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
            client: GhCli::new(),
            root: root.to_path_buf(),
            repo: repo.clone(),
            config: config.clone(),
            issue_map: IssueMap::load(root)?,
            issue_cache: IssueCache::new(root),
        };
        let created = store.create(type_def, title, author, "")?;
        return Ok(root.join(&created.path));
    }

    fs_ops::create_document(
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
    let path = run(root, config, store, doc_type, title, author, on_progress)?;
    let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();

    let content = fs::read_to_string(&path)?;
    let mut meta = DocMeta::parse(&content)?;
    meta.path = relative;

    let json = doc_to_json(&meta);
    Ok(serde_json::to_string_pretty(&json)?)
}
