use crate::cli::resolve::resolve_shorthand_or_path;
use crate::engine::config::{Config, StoreBackend};
use crate::engine::gh::GhCli;
use crate::engine::git_ref::GitCli;
use crate::engine::git_ref_store::GitRefStore;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::store::Store;
use crate::engine::store_dispatch::{DocumentStore, FilesystemStore, GithubIssuesStore};
use anyhow::{anyhow, bail, Result};
use clap::Subcommand;
use clap_complete::engine::ArgValueCompleter;
use serde::Serialize;
use std::io::Write;
use std::path::Path;

#[derive(Subcommand)]
pub enum ProvenanceCommand {
    /// Add a citation to a document's provenance
    Add {
        /// Document path or shorthand ID
        #[arg(add = ArgValueCompleter::new(crate::cli::completions::complete_doc_id))]
        id: String,
        /// Citation text (any non-empty string)
        citation: String,
        #[arg(long)]
        json: bool,
    },
    /// Remove a citation from a document's provenance (exact match)
    Remove {
        #[arg(add = ArgValueCompleter::new(crate::cli::completions::complete_doc_id))]
        id: String,
        citation: String,
        #[arg(long)]
        json: bool,
    },
    /// List provenance citations
    List {
        /// Optional document path or shorthand ID; when omitted, lists all docs
        #[arg(add = ArgValueCompleter::new(crate::cli::completions::complete_doc_id))]
        id: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Serialize)]
struct AddOutput {
    doc: String,
    added: String,
    provenance: Vec<String>,
}

#[derive(Serialize)]
struct RemoveOutput {
    doc: String,
    removed: String,
    provenance: Vec<String>,
}

#[derive(Serialize)]
struct ListSingleOutput {
    doc: String,
    provenance: Vec<String>,
}

#[derive(Serialize)]
struct ListGlobalOutput {
    documents: Vec<ListGlobalEntry>,
}

#[derive(Serialize)]
struct ListGlobalEntry {
    id: String,
    path: String,
    provenance: Vec<String>,
}

fn dispatch_set_provenance(
    root: &Path,
    config: &Config,
    type_name: &str,
    doc_id: &str,
    new_list: &[String],
) -> Result<()> {
    let type_def = config
        .type_by_name(type_name)
        .ok_or_else(|| anyhow!("unknown document type: {}", type_name))?;

    match type_def.store {
        StoreBackend::Filesystem => {
            let mut fs_store = FilesystemStore {
                root: root.to_path_buf(),
                config: config.clone(),
            };
            fs_store.set_provenance(type_def, doc_id, new_list)
        }
        StoreBackend::GithubIssues => {
            let gh_config = config.documents.github.as_ref().ok_or_else(|| {
                anyhow!(
                    "type '{}' uses github-issues store but no [github] config found",
                    type_name
                )
            })?;
            let repo = gh_config.repo.as_ref().ok_or_else(|| {
                anyhow!(
                    "type '{}' uses github-issues store but no github.repo configured",
                    type_name
                )
            })?;
            let mut gh_store = GithubIssuesStore {
                client: GhCli::new(),
                root: root.to_path_buf(),
                repo: repo.clone(),
                config: config.clone(),
                issue_map: IssueMap::load(root)?,
                issue_cache: IssueCache::new(root),
            };
            gh_store.set_provenance(type_def, doc_id, new_list)
        }
        StoreBackend::GitRef => {
            let mut git_store = GitRefStore {
                git: GitCli,
                root: root.to_path_buf(),
                config: config.clone(),
                reserved_number: None,
            };
            git_store.set_provenance(type_def, doc_id, new_list)
        }
    }
}

pub fn run_add(
    root: &Path,
    store: &Store,
    config: &Config,
    id: &str,
    citation: &str,
    json: bool,
    writer: &mut dyn Write,
) -> Result<()> {
    if citation.is_empty() {
        bail!("citation must not be empty");
    }

    let doc = resolve_shorthand_or_path(store, id)?;
    let doc_id = doc.id.clone();
    let type_name = doc.doc_type.as_str().to_string();
    let mut new_list = doc.provenance.clone();
    new_list.push(citation.to_string());

    dispatch_set_provenance(root, config, &type_name, &doc_id, &new_list)?;

    let store = Store::load(root, config)?;
    let reloaded = resolve_shorthand_or_path(&store, &doc_id)?;
    let provenance = reloaded.provenance.clone();

    if json {
        let output = AddOutput {
            doc: doc_id,
            added: citation.to_string(),
            provenance,
        };
        writeln!(writer, "{}", serde_json::to_string_pretty(&output)?)?;
    } else {
        for entry in &provenance {
            writeln!(writer, "{}", entry)?;
        }
    }
    Ok(())
}

pub fn run_remove(
    root: &Path,
    store: &Store,
    config: &Config,
    id: &str,
    citation: &str,
    json: bool,
    writer: &mut dyn Write,
) -> Result<()> {
    let doc = resolve_shorthand_or_path(store, id)?;
    let doc_id = doc.id.clone();
    let type_name = doc.doc_type.as_str().to_string();

    let idx = doc
        .provenance
        .iter()
        .position(|c| c == citation)
        .ok_or_else(|| anyhow!("citation not found: {}", citation))?;

    let mut new_list = doc.provenance.clone();
    new_list.remove(idx);

    dispatch_set_provenance(root, config, &type_name, &doc_id, &new_list)?;

    let store = Store::load(root, config)?;
    let reloaded = resolve_shorthand_or_path(&store, &doc_id)?;
    let provenance = reloaded.provenance.clone();

    if json {
        let output = RemoveOutput {
            doc: doc_id,
            removed: citation.to_string(),
            provenance,
        };
        writeln!(writer, "{}", serde_json::to_string_pretty(&output)?)?;
    } else {
        for entry in &provenance {
            writeln!(writer, "{}", entry)?;
        }
    }
    Ok(())
}

pub fn run_list(
    store: &Store,
    id: Option<&str>,
    json: bool,
    writer: &mut dyn Write,
) -> Result<()> {
    match id {
        Some(id) => {
            let doc = resolve_shorthand_or_path(store, id)?;
            let provenance = doc.provenance.clone();
            if json {
                let output = ListSingleOutput {
                    doc: doc.id.clone(),
                    provenance,
                };
                writeln!(writer, "{}", serde_json::to_string_pretty(&output)?)?;
            } else {
                for entry in &provenance {
                    writeln!(writer, "{}", entry)?;
                }
            }
        }
        None => {
            let mut docs = store.all_docs();
            docs.sort_by(|a, b| a.id.cmp(&b.id));
            if json {
                let documents: Vec<ListGlobalEntry> = docs
                    .iter()
                    .filter(|d| !d.provenance.is_empty())
                    .map(|d| ListGlobalEntry {
                        id: d.id.clone(),
                        path: d.path.to_string_lossy().to_string(),
                        provenance: d.provenance.clone(),
                    })
                    .collect();
                let output = ListGlobalOutput { documents };
                writeln!(writer, "{}", serde_json::to_string_pretty(&output)?)?;
            } else {
                for d in &docs {
                    if d.provenance.is_empty() {
                        continue;
                    }
                    for entry in &d.provenance {
                        writeln!(writer, "{}\t{}", d.id, entry)?;
                    }
                }
            }
        }
    }
    Ok(())
}
