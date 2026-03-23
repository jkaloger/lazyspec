use std::ffi::OsStr;
use std::path::Path;

use clap_complete::engine::CompletionCandidate;

use crate::engine::config::Config;
use crate::engine::document::RelationType;
use crate::engine::store::Store;

pub fn complete_doc_id(current: &OsStr) -> Vec<CompletionCandidate> {
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(_) => return vec![],
    };
    complete_doc_id_in(&cwd, current)
}

pub fn complete_doc_id_in(root: &Path, current: &OsStr) -> Vec<CompletionCandidate> {
    let current_str = current.to_str().unwrap_or("");
    let config = match Config::load(root) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let store = match Store::load(root, &config) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    store
        .all_docs()
        .into_iter()
        .filter(|doc| doc.id.starts_with(current_str))
        .map(|doc| CompletionCandidate::new(&doc.id))
        .collect()
}

pub fn complete_rel_type(current: &OsStr) -> Vec<CompletionCandidate> {
    let current_str = current.to_str().unwrap_or("");
    RelationType::ALL_STRS
        .into_iter()
        .filter(|rt| rt.starts_with(current_str))
        .map(CompletionCandidate::new)
        .collect()
}
