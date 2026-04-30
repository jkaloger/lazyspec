use crate::cli::resolve::resolve_shorthand_or_path;
use crate::engine::config::Config;
use crate::engine::store::Store;
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

pub fn run_add(
    root: &Path,
    store: &Store,
    config: &Config,
    id: &str,
    citation: &str,
    json: bool,
    writer: &mut dyn Write,
) -> Result<()> {
    let citation = match crate::engine::provenance::validate_citation(citation) {
        Ok(c) => c,
        Err(e) => bail!("{}", e),
    };

    let doc = resolve_shorthand_or_path(store, id)?;
    let doc_id = doc.id.clone();
    let type_name = doc.doc_type.as_str().to_string();
    let mut new_list = doc.provenance.clone();
    new_list.push(citation.to_string());

    crate::engine::provenance::set_provenance(root, config, &type_name, &doc_id, &new_list)?;

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

    crate::engine::provenance::set_provenance(root, config, &type_name, &doc_id, &new_list)?;

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

pub fn run_list(store: &Store, id: Option<&str>, json: bool, writer: &mut dyn Write) -> Result<()> {
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
