mod common;

use common::TestFixture;
use lazyspec::engine::document::DocMeta;
use std::fs;

fn setup_two_docs() -> TestFixture {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-auth.md", "Auth", "accepted");
    fixture.write_adr("ADR-001-adopt-auth.md", "Adopt Auth", "draft", None);
    fixture
}

#[test]
fn link_adds_relationship_to_frontmatter() {
    let fixture = setup_two_docs();
    let store = fixture.store();
    let fs = lazyspec::engine::fs::RealFileSystem;

    lazyspec::cli::link::link(
        fixture.root(),
        &store,
        "docs/adrs/ADR-001-adopt-auth.md",
        "implements",
        "docs/rfcs/RFC-001-auth.md",
        &fs,
    ).unwrap();

    let content = fs::read_to_string(fixture.root().join("docs/adrs/ADR-001-adopt-auth.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert_eq!(meta.related.len(), 1);
    assert_eq!(meta.related[0].target, "docs/rfcs/RFC-001-auth.md");
}

#[test]
fn unlink_removes_relationship() {
    let fixture = setup_two_docs();
    let store = fixture.store();
    let fs = lazyspec::engine::fs::RealFileSystem;

    lazyspec::cli::link::link(
        fixture.root(),
        &store,
        "docs/adrs/ADR-001-adopt-auth.md",
        "implements",
        "docs/rfcs/RFC-001-auth.md",
        &fs,
    ).unwrap();

    lazyspec::cli::link::unlink(
        fixture.root(),
        &store,
        "docs/adrs/ADR-001-adopt-auth.md",
        "implements",
        "docs/rfcs/RFC-001-auth.md",
        &fs,
    ).unwrap();

    let content = fs::read_to_string(fixture.root().join("docs/adrs/ADR-001-adopt-auth.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert!(meta.related.is_empty());
}

#[test]
fn link_with_shorthand_ids() {
    let fixture = setup_two_docs();
    let store = fixture.store();
    let fs = lazyspec::engine::fs::RealFileSystem;

    lazyspec::cli::link::link(
        fixture.root(),
        &store,
        "ADR-001",
        "implements",
        "RFC-001",
        &fs,
    )
    .unwrap();

    let content =
        std::fs::read_to_string(fixture.root().join("docs/adrs/ADR-001-adopt-auth.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert_eq!(meta.related.len(), 1);
    assert_eq!(meta.related[0].target, "docs/rfcs/RFC-001-auth.md");
}

#[test]
fn unlink_with_shorthand_ids() {
    let fixture = setup_two_docs();
    let store = fixture.store();
    let fs = lazyspec::engine::fs::RealFileSystem;

    lazyspec::cli::link::link(
        fixture.root(),
        &store,
        "ADR-001",
        "implements",
        "RFC-001",
        &fs,
    )
    .unwrap();

    let store = fixture.store();
    lazyspec::cli::link::unlink(
        fixture.root(),
        &store,
        "ADR-001",
        "implements",
        "RFC-001",
        &fs,
    )
    .unwrap();

    let content =
        std::fs::read_to_string(fixture.root().join("docs/adrs/ADR-001-adopt-auth.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert!(meta.related.is_empty());
}

#[test]
fn link_ambiguous_id_returns_error() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-alpha.md", "Alpha", "draft");
    fixture.write_rfc("RFC-001-beta.md", "Beta", "draft");
    fixture.write_adr("ADR-001-test.md", "Test", "draft", None);
    let store = fixture.store();
    let fs = lazyspec::engine::fs::RealFileSystem;

    let result = lazyspec::cli::link::link(
        fixture.root(),
        &store,
        "ADR-001",
        "implements",
        "RFC-001",
        &fs,
    );

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Ambiguous"),
        "expected ambiguous error, got: {}",
        err_msg
    );
}

#[test]
fn link_not_found_id_returns_error() {
    let fixture = TestFixture::new();
    fixture.write_adr("ADR-001-test.md", "Test", "draft", None);
    let store = fixture.store();
    let fs = lazyspec::engine::fs::RealFileSystem;

    let result = lazyspec::cli::link::link(
        fixture.root(),
        &store,
        "ADR-001",
        "implements",
        "RFC-999",
        &fs,
    );

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not found"),
        "expected not-found error, got: {}",
        err_msg
    );
}
