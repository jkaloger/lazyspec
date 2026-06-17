use crate::common::TestFixture;
use lazyspec::engine::document::{DocMeta, RelationType};
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
    )
    .unwrap();

    let content =
        fs::read_to_string(fixture.root().join("docs/adrs/ADR-001-adopt-auth.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert_eq!(meta.related.len(), 1);
    assert_eq!(meta.related[0].target, "RFC-001");
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
    )
    .unwrap();

    lazyspec::cli::link::unlink(
        fixture.root(),
        &store,
        "docs/adrs/ADR-001-adopt-auth.md",
        "implements",
        "docs/rfcs/RFC-001-auth.md",
        &fs,
    )
    .unwrap();

    let content =
        fs::read_to_string(fixture.root().join("docs/adrs/ADR-001-adopt-auth.md")).unwrap();
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
    assert_eq!(meta.related[0].target, "RFC-001");
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

#[test]
fn link_blocked_by_flips_and_stores_canonical_on_target() {
    let fixture = setup_two_docs();
    let store = fixture.store();
    let fs = lazyspec::engine::fs::RealFileSystem;

    let adr_before =
        fs::read_to_string(fixture.root().join("docs/adrs/ADR-001-adopt-auth.md")).unwrap();

    // ADR blocked-by RFC -> the canonical "blocks: ADR-001" lands on the RFC.
    lazyspec::cli::link::link(
        fixture.root(),
        &store,
        "ADR-001",
        "blocked-by",
        "RFC-001",
        &fs,
    )
    .unwrap();

    let rfc_content = fs::read_to_string(fixture.root().join("docs/rfcs/RFC-001-auth.md")).unwrap();
    let rfc_meta = DocMeta::parse(&rfc_content).unwrap();
    assert_eq!(rfc_meta.related.len(), 1);
    assert_eq!(rfc_meta.related[0].rel_type, RelationType::Blocks);
    assert_eq!(rfc_meta.related[0].target, "ADR-001");

    // The ADR (the "from" doc) is untouched.
    let adr_after =
        fs::read_to_string(fixture.root().join("docs/adrs/ADR-001-adopt-auth.md")).unwrap();
    assert_eq!(adr_after, adr_before, "from-doc must be byte-identical");

    // No inverse keyword leaks into either frontmatter.
    assert!(!rfc_content.contains("blocked-by"));
    assert!(!adr_after.contains("blocked-by"));
}

#[test]
fn link_implemented_by_flips_to_implements_on_target() {
    let fixture = setup_two_docs();
    let store = fixture.store();
    let fs = lazyspec::engine::fs::RealFileSystem;

    // RFC implemented-by ADR -> "implements: RFC-001" lands on the ADR.
    lazyspec::cli::link::link(
        fixture.root(),
        &store,
        "RFC-001",
        "implemented-by",
        "ADR-001",
        &fs,
    )
    .unwrap();

    let adr_content =
        fs::read_to_string(fixture.root().join("docs/adrs/ADR-001-adopt-auth.md")).unwrap();
    let adr_meta = DocMeta::parse(&adr_content).unwrap();
    assert_eq!(adr_meta.related.len(), 1);
    assert_eq!(adr_meta.related[0].rel_type, RelationType::Implements);
    assert_eq!(adr_meta.related[0].target, "RFC-001");
    assert!(!adr_content.contains("implemented-by"));
}

#[test]
fn link_superseded_by_flips_to_supersedes_on_target() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-old.md", "Old", "accepted");
    fixture.write_rfc("RFC-002-new.md", "New", "draft");
    let store = fixture.store();
    let fs = lazyspec::engine::fs::RealFileSystem;

    // RFC-001 superseded-by RFC-002 -> "supersedes: RFC-001" lands on RFC-002.
    lazyspec::cli::link::link(
        fixture.root(),
        &store,
        "RFC-001",
        "superseded-by",
        "RFC-002",
        &fs,
    )
    .unwrap();

    let new_content = fs::read_to_string(fixture.root().join("docs/rfcs/RFC-002-new.md")).unwrap();
    let new_meta = DocMeta::parse(&new_content).unwrap();
    assert_eq!(new_meta.related.len(), 1);
    assert_eq!(new_meta.related[0].rel_type, RelationType::Supersedes);
    assert_eq!(new_meta.related[0].target, "RFC-001");
    assert!(!new_content.contains("superseded-by"));
}

#[test]
fn unlink_blocked_by_removes_canonical_from_target() {
    let fixture = setup_two_docs();
    let store = fixture.store();
    let fs = lazyspec::engine::fs::RealFileSystem;

    // Seed RFC with "blocks: ADR-001".
    lazyspec::cli::link::link(fixture.root(), &store, "RFC-001", "blocks", "ADR-001", &fs).unwrap();

    // Unlink via the inverse keyword from the ADR's perspective.
    let store = fixture.store();
    lazyspec::cli::link::unlink(
        fixture.root(),
        &store,
        "ADR-001",
        "blocked-by",
        "RFC-001",
        &fs,
    )
    .unwrap();

    let rfc_content = fs::read_to_string(fixture.root().join("docs/rfcs/RFC-001-auth.md")).unwrap();
    let rfc_meta = DocMeta::parse(&rfc_content).unwrap();
    assert!(
        rfc_meta.related.is_empty(),
        "entry should be removed from RFC"
    );
}

#[test]
fn link_unknown_keyword_rejected_and_writes_nothing() {
    let fixture = setup_two_docs();
    let store = fixture.store();
    let fs = lazyspec::engine::fs::RealFileSystem;

    let adr_before =
        fs::read_to_string(fixture.root().join("docs/adrs/ADR-001-adopt-auth.md")).unwrap();
    let rfc_before = fs::read_to_string(fixture.root().join("docs/rfcs/RFC-001-auth.md")).unwrap();

    let result =
        lazyspec::cli::link::link(fixture.root(), &store, "ADR-001", "frobs", "RFC-001", &fs);

    assert!(result.is_err());
    assert!(
        result.unwrap_err().to_string().contains("frobs"),
        "error should name the unknown keyword"
    );

    let adr_after =
        fs::read_to_string(fixture.root().join("docs/adrs/ADR-001-adopt-auth.md")).unwrap();
    let rfc_after = fs::read_to_string(fixture.root().join("docs/rfcs/RFC-001-auth.md")).unwrap();
    assert_eq!(adr_after, adr_before, "from-doc must be unmodified");
    assert_eq!(rfc_after, rfc_before, "to-doc must be unmodified");
}

#[test]
fn unlink_unknown_keyword_rejected_and_writes_nothing() {
    let fixture = setup_two_docs();
    let store = fixture.store();
    let fs = lazyspec::engine::fs::RealFileSystem;

    let adr_before =
        fs::read_to_string(fixture.root().join("docs/adrs/ADR-001-adopt-auth.md")).unwrap();
    let rfc_before = fs::read_to_string(fixture.root().join("docs/rfcs/RFC-001-auth.md")).unwrap();

    let result =
        lazyspec::cli::link::unlink(fixture.root(), &store, "ADR-001", "frobs", "RFC-001", &fs);

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("frobs"));

    let adr_after =
        fs::read_to_string(fixture.root().join("docs/adrs/ADR-001-adopt-auth.md")).unwrap();
    let rfc_after = fs::read_to_string(fixture.root().join("docs/rfcs/RFC-001-auth.md")).unwrap();
    assert_eq!(adr_after, adr_before);
    assert_eq!(rfc_after, rfc_before);
}

#[test]
fn link_outcome_reflects_flip() {
    let fixture = setup_two_docs();
    let store = fixture.store();
    let fs = lazyspec::engine::fs::RealFileSystem;

    let outcome = lazyspec::cli::link::link(
        fixture.root(),
        &store,
        "ADR-001",
        "blocked-by",
        "RFC-001",
        &fs,
    )
    .unwrap();

    assert_eq!(
        outcome.source,
        std::path::PathBuf::from("docs/rfcs/RFC-001-auth.md")
    );
    assert_eq!(outcome.rel_type, RelationType::Blocks);
    assert_eq!(outcome.target, "ADR-001");
}
