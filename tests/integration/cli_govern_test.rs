//! `govern add|remove|list`: the one write path for a document's `governs`
//! globs (BUG-029). Globs are refused before they are stored, so a document
//! never loads with a pin that would not compile.

use crate::common::TestFixture;
use lazyspec::cli::govern::{run_add, run_list, run_remove};
use lazyspec::engine::document::DocMeta;
use lazyspec::engine::fs::RealFileSystem;
use lazyspec::engine::git_ref::test_support::MockGitRefClient;

const DOC: &str = "docs/rfcs/RFC-001-engine.md";

fn write_rfc(fixture: &TestFixture, governs: &[&str]) {
    let block = if governs.is_empty() {
        String::new()
    } else {
        let mut s = String::from("governs:\n");
        for g in governs {
            s.push_str(&format!("  - \"{g}\"\n"));
        }
        s
    };
    fixture.write_doc(
        DOC,
        &format!(
            "---\ntitle: Engine\ntype: rfc\nstatus: draft\nauthor: a\ndate: 2026-01-01\ntags: []\n{block}---\nBody.\n"
        ),
    );
}

fn governs_on_disk(fixture: &TestFixture) -> Vec<String> {
    let content = std::fs::read_to_string(fixture.root().join(DOC)).unwrap();
    DocMeta::parse(&content).unwrap().governs
}

fn globs(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn add_writes_globs_to_a_document_without_any() {
    let fixture = TestFixture::new();
    write_rfc(&fixture, &[]);
    let store = fixture.store();
    let config = fixture.config();

    let result = run_add(
        &store,
        &config,
        &MockGitRefClient::new(),
        &RealFileSystem,
        "RFC-001",
        &globs(&["src/engine/**", "src/cli/*.rs"]),
    )
    .unwrap();

    assert_eq!(result, globs(&["src/engine/**", "src/cli/*.rs"]));
    assert_eq!(governs_on_disk(&fixture), result);
}

#[test]
fn add_is_idempotent_and_keeps_existing_globs() {
    let fixture = TestFixture::new();
    write_rfc(&fixture, &["src/engine/**"]);
    let store = fixture.store();
    let config = fixture.config();

    let result = run_add(
        &store,
        &config,
        &MockGitRefClient::new(),
        &RealFileSystem,
        "RFC-001",
        &globs(&["src/engine/**", "src/tui/**"]),
    )
    .unwrap();

    assert_eq!(result, globs(&["src/engine/**", "src/tui/**"]));
}

#[test]
fn add_refuses_a_glob_that_does_not_compile_and_writes_nothing() {
    let fixture = TestFixture::new();
    write_rfc(&fixture, &["src/engine/**"]);
    let store = fixture.store();
    let config = fixture.config();

    let err = run_add(
        &store,
        &config,
        &MockGitRefClient::new(),
        &RealFileSystem,
        "RFC-001",
        &globs(&["src/[", "src/tui/**"]),
    )
    .unwrap_err();

    assert!(err.to_string().contains("src/["), "got: {err}");
    assert_eq!(governs_on_disk(&fixture), globs(&["src/engine/**"]));
}

#[test]
fn remove_drops_the_named_glob_and_keeps_the_rest() {
    let fixture = TestFixture::new();
    write_rfc(&fixture, &["src/engine/**", "src/tui/**"]);
    let store = fixture.store();
    let config = fixture.config();

    let result = run_remove(
        &store,
        &config,
        &MockGitRefClient::new(),
        &RealFileSystem,
        "RFC-001",
        &globs(&["src/engine/**"]),
    )
    .unwrap();

    assert_eq!(result, globs(&["src/tui/**"]));
    assert_eq!(governs_on_disk(&fixture), result);
}

#[test]
fn list_reports_the_document_globs_in_order() {
    let fixture = TestFixture::new();
    write_rfc(&fixture, &["src/tui/**", "src/engine/**"]);
    let store = fixture.store();

    let result = run_list(&store, "RFC-001").unwrap();

    assert_eq!(result, globs(&["src/tui/**", "src/engine/**"]));
}
