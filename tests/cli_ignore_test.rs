mod common;

use common::TestFixture;
use lazyspec::engine::document::DocMeta;
use std::fs;

#[test]
fn ignore_adds_validate_ignore_field() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-auth.md", "Auth", "draft");
    let store = fixture.store();

    lazyspec::cli::ignore::ignore(fixture.root(), &store, "docs/rfcs/RFC-001-auth.md").unwrap();

    let content = fs::read_to_string(fixture.root().join("docs/rfcs/RFC-001-auth.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert!(meta.validate_ignore);
}

#[test]
fn unignore_removes_validate_ignore_field() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-auth.md",
        "---\ntitle: \"Auth\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\nvalidate-ignore: true\n---\n",
    );
    let store = fixture.store();

    lazyspec::cli::ignore::unignore(fixture.root(), &store, "docs/rfcs/RFC-001-auth.md").unwrap();

    let content = fs::read_to_string(fixture.root().join("docs/rfcs/RFC-001-auth.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert!(!meta.validate_ignore);
}

#[test]
fn ignore_is_idempotent() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-auth.md", "Auth", "draft");
    let store = fixture.store();

    lazyspec::cli::ignore::ignore(fixture.root(), &store, "docs/rfcs/RFC-001-auth.md").unwrap();
    // Reload store after mutation
    let store = fixture.store();
    lazyspec::cli::ignore::ignore(fixture.root(), &store, "docs/rfcs/RFC-001-auth.md").unwrap();

    let content = fs::read_to_string(fixture.root().join("docs/rfcs/RFC-001-auth.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert!(meta.validate_ignore);
}

#[test]
fn unignore_on_document_without_field_succeeds() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-auth.md", "Auth", "draft");
    let store = fixture.store();

    lazyspec::cli::ignore::unignore(fixture.root(), &store, "docs/rfcs/RFC-001-auth.md").unwrap();

    let content = fs::read_to_string(fixture.root().join("docs/rfcs/RFC-001-auth.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert!(!meta.validate_ignore);
}

#[test]
fn ignore_then_validate_skips_document() {
    let fixture = TestFixture::new();
    // Iteration without a story link triggers UnlinkedIteration error
    fixture.write_iteration("ITERATION-001-sprint.md", "Sprint 1", "draft", None);

    // Verify the error exists before ignoring
    let store = fixture.store();
    let result = store.validate_full(&fixture.config());
    assert!(
        result.errors.iter().any(|e| {
            let msg = format!("{:?}", e);
            msg.contains("ITERATION-001-sprint.md")
        }),
        "expected validation error for unlinked iteration before ignore"
    );

    // Ignore the document
    let store = fixture.store();
    lazyspec::cli::ignore::ignore(fixture.root(), &store, "docs/iterations/ITERATION-001-sprint.md")
        .unwrap();

    // Reload store and validate again
    let store = fixture.store();
    let result = store.validate_full(&fixture.config());
    assert!(
        !result.errors.iter().any(|e| {
            let msg = format!("{:?}", e);
            msg.contains("ITERATION-001-sprint.md")
        }),
        "expected no validation error for ignored iteration, got: {:?}",
        result.errors
    );
}

#[test]
fn ignore_with_shorthand_id() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-auth.md", "Auth", "draft");
    let store = fixture.store();

    lazyspec::cli::ignore::ignore(fixture.root(), &store, "RFC-001").unwrap();

    let content = fs::read_to_string(fixture.root().join("docs/rfcs/RFC-001-auth.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert!(meta.validate_ignore);
}

#[test]
fn unignore_with_shorthand_id() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-auth.md", "Auth", "draft");
    let store = fixture.store();

    lazyspec::cli::ignore::ignore(fixture.root(), &store, "docs/rfcs/RFC-001-auth.md").unwrap();

    let store = fixture.store();
    lazyspec::cli::ignore::unignore(fixture.root(), &store, "RFC-001").unwrap();

    let content = fs::read_to_string(fixture.root().join("docs/rfcs/RFC-001-auth.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert!(!meta.validate_ignore);
}
