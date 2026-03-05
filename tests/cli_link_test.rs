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

    lazyspec::cli::link::link(
        fixture.root(),
        "docs/adrs/ADR-001-adopt-auth.md",
        "implements",
        "docs/rfcs/RFC-001-auth.md",
    ).unwrap();

    let content = fs::read_to_string(fixture.root().join("docs/adrs/ADR-001-adopt-auth.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert_eq!(meta.related.len(), 1);
    assert_eq!(meta.related[0].target, "docs/rfcs/RFC-001-auth.md");
}

#[test]
fn unlink_removes_relationship() {
    let fixture = setup_two_docs();

    lazyspec::cli::link::link(
        fixture.root(),
        "docs/adrs/ADR-001-adopt-auth.md",
        "implements",
        "docs/rfcs/RFC-001-auth.md",
    ).unwrap();

    lazyspec::cli::link::unlink(
        fixture.root(),
        "docs/adrs/ADR-001-adopt-auth.md",
        "implements",
        "docs/rfcs/RFC-001-auth.md",
    ).unwrap();

    let content = fs::read_to_string(fixture.root().join("docs/adrs/ADR-001-adopt-auth.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert!(meta.related.is_empty());
}
