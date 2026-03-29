mod common;

use common::TestFixture;
use lazyspec::engine::document::DocMeta;
use std::fs;

#[test]
fn update_status_in_frontmatter() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test", "draft");
    let store = fixture.store();

    lazyspec::cli::update::run(
        fixture.root(),
        &store,
        "docs/rfcs/RFC-001-test.md",
        &[("status", "review")],
    )
    .unwrap();

    let content = fs::read_to_string(fixture.root().join("docs/rfcs/RFC-001-test.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert_eq!(format!("{}", meta.status), "review");
}

#[test]
fn delete_removes_file() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test", "draft");
    let store = fixture.store();

    let path = fixture.root().join("docs/rfcs/RFC-001-test.md");
    assert!(path.exists());

    lazyspec::cli::delete::run(fixture.root(), &store, "docs/rfcs/RFC-001-test.md").unwrap();
    assert!(!path.exists());
}

#[test]
fn update_with_shorthand_id() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test", "draft");
    let store = fixture.store();

    lazyspec::cli::update::run(fixture.root(), &store, "RFC-001", &[("status", "review")]).unwrap();

    let content = fs::read_to_string(fixture.root().join("docs/rfcs/RFC-001-test.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert_eq!(format!("{}", meta.status), "review");
}

#[test]
fn delete_with_shorthand_id() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test", "draft");
    let store = fixture.store();

    let path = fixture.root().join("docs/rfcs/RFC-001-test.md");
    assert!(path.exists());

    lazyspec::cli::delete::run(fixture.root(), &store, "RFC-001").unwrap();
    assert!(!path.exists());
}
