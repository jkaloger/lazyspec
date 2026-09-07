//! STORY-265 AC1-AC3: `why <path>` lists the documents governing a file.

use crate::common::TestFixture;
use lazyspec::cli::why;
use lazyspec::engine::store::Store;
use std::path::Path;

fn spec(governs: &str, reviewed: &str) -> String {
    format!(
        "---\ntitle: \"Context\"\ntype: spec\nstatus: accepted\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\ngoverns:\n  - {governs}\nreviewed: \"{reviewed}\"\n---\n\nBody.\n"
    )
}

fn results(store: &Store, path: &str) -> Vec<serde_json::Value> {
    serde_json::from_str(&why::run_json(store, Path::new(path))).unwrap()
}

// AC1: a pinned document is reported with its id, type, title, status,
// `reviewed`, and the glob that matched.
#[test]
fn json_reports_the_governing_document_and_its_glob() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/specs/SPEC-001-context.md",
        &spec("src/engine/context/**", "0123456789abcdef"),
    );
    let store = fixture.store();

    let found = results(&store, "src/engine/context/resolve.rs");

    assert_eq!(found.len(), 1, "got: {found:?}");
    assert_eq!(found[0]["id"], "SPEC-001");
    assert_eq!(found[0]["type"], "spec");
    assert_eq!(found[0]["title"], "Context");
    assert_eq!(found[0]["status"], "accepted");
    assert_eq!(found[0]["reviewed"], "0123456789abcdef");
    assert_eq!(found[0]["glob"], "src/engine/context/**");
}

// AC2: two documents whose globs both match one path both appear, each
// carrying the glob that matched for it.
#[test]
fn two_matching_documents_each_report_their_own_glob() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/specs/SPEC-001-context.md",
        &spec("src/engine/context/**", "0123456789abcdef"),
    );
    fixture.write_doc(
        "docs/specs/SPEC-002-engine.md",
        &spec("src/engine/**/*.rs", "fedcba9876543210"),
    );
    let store = fixture.store();

    let found = results(&store, "src/engine/context/resolve.rs");

    assert_eq!(found.len(), 2, "got: {found:?}");
    let mut pairs: Vec<(&str, &str)> = found
        .iter()
        .map(|r| (r["id"].as_str().unwrap(), r["glob"].as_str().unwrap()))
        .collect();
    pairs.sort();
    assert_eq!(
        pairs,
        vec![
            ("SPEC-001", "src/engine/context/**"),
            ("SPEC-002", "src/engine/**/*.rs"),
        ]
    );
}

// AC3: an unmatched path is an empty list, not an error. `run` returns nothing
// fallible, so the process cannot exit non-zero on this path.
#[test]
fn unmatched_path_is_an_empty_list() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/specs/SPEC-001-context.md",
        &spec("src/engine/context/**", "0123456789abcdef"),
    );
    let store = fixture.store();

    assert_eq!(
        why::run_json(&store, Path::new("src/cli/show.rs")).trim(),
        "[]"
    );
    assert!(results(&store, "src/cli/show.rs").is_empty());
}

// A document with no `reviewed` still reports the key, as null, so a consumer
// need not check for its absence.
#[test]
fn reviewed_is_null_when_unset() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/specs/SPEC-001-context.md",
        "---\ntitle: \"Context\"\ntype: spec\nstatus: accepted\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\ngoverns:\n  - src/engine/context/**\n---\n\nBody.\n",
    );
    let store = fixture.store();

    let found = results(&store, "src/engine/context/resolve.rs");
    assert_eq!(found.len(), 1, "got: {found:?}");
    assert!(found[0]["reviewed"].is_null());
}
