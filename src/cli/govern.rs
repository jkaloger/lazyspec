use crate::cli::resolve::resolve_shorthand_or_path;
use crate::engine::config::Config;
use crate::engine::document::rewrite_frontmatter;
use crate::engine::fs::FileSystem;
use crate::engine::git_ref::GitRefOps;
use crate::engine::git_store::commit_if_git_backed;
use crate::engine::store::{compile_governs, Store};
use anyhow::{anyhow, Result};
use clap::Subcommand;
use clap_complete::engine::ArgValueCompleter;

#[derive(Subcommand)]
pub enum GovernCommand {
    /// Add globs to a document's `governs` list; each must compile
    Add {
        /// Document path or shorthand ID (e.g. RFC-001)
        #[arg(add = ArgValueCompleter::new(crate::cli::completions::complete_doc_id))]
        id: String,
        /// Globs, relative to the code root (e.g. 'src/engine/**')
        #[arg(required = true, num_args = 1..)]
        globs: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    /// Remove globs from a document's `governs` list (exact match)
    Remove {
        #[arg(add = ArgValueCompleter::new(crate::cli::completions::complete_doc_id))]
        id: String,
        #[arg(required = true, num_args = 1..)]
        globs: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    /// List a document's `governs` globs
    List {
        #[arg(add = ArgValueCompleter::new(crate::cli::completions::complete_doc_id))]
        id: String,
        #[arg(long)]
        json: bool,
    },
}

pub fn run_add(
    store: &Store,
    config: &Config,
    git: &dyn GitRefOps,
    fs: &dyn FileSystem,
    id: &str,
    globs: &[String],
) -> Result<Vec<String>> {
    let doc = resolve_shorthand_or_path(store, id)?;
    let mut next = doc.clone();
    for glob in globs {
        if !next.governs.contains(glob) {
            next.governs.push(glob.clone());
        }
    }
    compile_governs(&next).map_err(|e| anyhow!("{}", e.error))?;
    write_governs(store, config, git, fs, &next)
}

pub fn run_remove(
    store: &Store,
    config: &Config,
    git: &dyn GitRefOps,
    fs: &dyn FileSystem,
    id: &str,
    globs: &[String],
) -> Result<Vec<String>> {
    let doc = resolve_shorthand_or_path(store, id)?;
    let mut next = doc.clone();
    next.governs.retain(|g| !globs.contains(g));
    write_governs(store, config, git, fs, &next)
}

pub fn run_list(store: &Store, id: &str) -> Result<Vec<String>> {
    Ok(resolve_shorthand_or_path(store, id)?.governs.clone())
}

fn write_governs(
    store: &Store,
    config: &Config,
    git: &dyn GitRefOps,
    fs: &dyn FileSystem,
    next: &crate::engine::document::DocMeta,
) -> Result<Vec<String>> {
    let full_path = store.root().join(&next.path);
    rewrite_frontmatter(&full_path, fs, |value| {
        let map = value
            .as_mapping_mut()
            .ok_or_else(|| anyhow!("frontmatter root must be a mapping"))?;
        let key = serde_yaml::Value::String("governs".to_string());
        if next.governs.is_empty() {
            map.remove(&key);
        } else {
            let seq = next
                .governs
                .iter()
                .map(|g| serde_yaml::Value::String(g.clone()))
                .collect();
            map.insert(key, serde_yaml::Value::Sequence(seq));
        }
        Ok(())
    })?;
    commit_if_git_backed(
        store.root(),
        config,
        &next.path,
        git,
        &format!("govern {}", next.id),
    )?;
    Ok(next.governs.clone())
}

pub fn print(id: &str, governs: &[String], json: bool) -> Result<()> {
    if json {
        let out = serde_json::json!({ "doc": id, "governs": governs });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    for glob in governs {
        println!("{glob}");
    }
    Ok(())
}
