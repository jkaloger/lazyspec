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
    let fs = crate::engine::fs::RealFileSystem;
    let config = match Config::load(root, &fs) {
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
        .chain(RelationType::INVERSE_STRS)
        .filter(|rt| rt.starts_with(current_str))
        .map(CompletionCandidate::new)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(current: &str) -> Vec<String> {
        complete_rel_type(OsStr::new(current))
            .into_iter()
            .map(|c| c.get_value().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn empty_prefix_offers_all_canonical_and_inverse_keywords() {
        let got = values("");
        for expected in [
            "implements",
            "supersedes",
            "blocks",
            "related-to",
            "implemented-by",
            "superseded-by",
            "blocked-by",
        ] {
            assert!(got.contains(&expected.to_string()), "missing {expected}");
        }
        assert_eq!(got.len(), 7);
    }

    #[test]
    fn block_prefix_offers_canonical_and_inverse() {
        let mut got = values("block");
        got.sort();
        assert_eq!(got, vec!["blocked-by", "blocks"]);
    }

    #[test]
    fn inverse_only_prefix_offers_just_the_inverse() {
        assert_eq!(values("implemented"), vec!["implemented-by"]);
    }
}
