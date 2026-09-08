use crate::cli::style::{dim, doc_card};
use crate::engine::document::DocMeta;
use crate::engine::git_ref::GitRefOps;
use crate::engine::staleness::drifted;
use crate::engine::staleness_cache::StalenessCache;
use crate::engine::status_colors::StatusColors;
use crate::engine::store::Store;
use serde_json::Value;
use std::path::Path;

/// One result: the document and the glob of its that matched, in the shape
/// RFC-068 specifies. Deliberately narrower than [`crate::cli::json::doc_to_json`]
/// -- `why` answers "which spec applies here", so a caller wants the id to read
/// next and the pin that put it in the list, not the whole frontmatter.
///
/// `drifted` (RFC-069) is the one staleness fact this shape carries: a reader
/// asking which document governs a path also learns whether that document has
/// been reviewed since anything under it last moved. Not the band -- a `why`
/// record is an answer about a path, not a document summary.
fn entry(doc: &DocMeta, glob: &str, drifted: bool) -> Value {
    serde_json::json!({
        "id": doc.id,
        "type": format!("{}", doc.doc_type).to_lowercase(),
        "title": doc.title,
        "status": format!("{}", doc.status),
        "reviewed": doc.reviewed,
        "glob": glob,
        "drifted": drifted,
    })
}

pub fn run_json(store: &Store, path: &Path, git: &dyn GitRefOps) -> String {
    let cache = StalenessCache::load(store.root());
    let items: Vec<Value> = store
        .governing(path)
        .into_iter()
        .map(|(doc, glob)| {
            // Per record, not per store: `governing` already narrowed the set to
            // the documents that answer for this path, so nothing else is
            // diffed. One subprocess per record and not two, because `drifted`
            // does not read the anchor commit a band would need and this shape
            // would discard (STORY-276). A document with no `reviewed` has
            // nothing to diff and reads as undrifted, which is what an unpinned
            // document honestly is.
            let drifted = drifted(store.governs_root(), doc, git, &cache);
            entry(doc, glob, drifted)
        })
        .collect();
    serde_json::to_string_pretty(&items).unwrap()
}

fn human_output(store: &Store, path: &Path) -> String {
    let matches = store.governing(path);
    if matches.is_empty() {
        return format!("No documents govern {}\n", path.display());
    }
    let colors = StatusColors::load(store.root()).unwrap_or_default();
    matches
        .into_iter()
        .map(|(doc, glob)| {
            format!(
                "{} {}\n",
                doc_card(
                    &colors,
                    &doc.title,
                    &doc.doc_type,
                    &doc.status,
                    doc.assignee.as_deref(),
                    &doc.path
                ),
                dim(&format!("[{}]", glob)),
            )
        })
        .collect()
}

pub fn run(store: &Store, path: &Path, json: bool, git: &dyn GitRefOps) {
    if json {
        println!("{}", run_json(store, path, git));
    } else {
        print!("{}", human_output(store, path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::Config;
    use crate::engine::store::test_support::store_from_with_config;

    /// The human surface is out of RFC-069's scope here: `drifted` is a
    /// `--json`-only field, so the card list reads exactly as it did. It stays a
    /// unit test because `human_output` is private -- the `drifted` behaviour
    /// itself is an integration test, in `tests/integration/cli_why_test.rs`.
    #[test]
    fn the_human_output_says_nothing_about_drift() {
        let doc = "---\ntitle: \"Context\"\ntype: spec\nstatus: accepted\nauthor: t\ndate: 2026-01-01\ntags: []\ngoverns:\n  - \"src/engine/context/**\"\nreviewed: 0123456789abcdef\nrelated: []\n---\n\nBody.\n";
        let (_tmp, store) = store_from_with_config(
            &[("docs/specs/SPEC-001-context.md", doc)],
            &Config::default(),
        );

        let out = human_output(&store, Path::new("src/engine/context/resolve.rs"));

        assert!(out.contains("Context"), "got: {out}");
        assert!(!out.to_lowercase().contains("drift"), "got: {out}");
    }
}
