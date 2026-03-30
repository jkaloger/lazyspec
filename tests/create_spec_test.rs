mod common;

use lazyspec::engine::document::{DocMeta, Status};
use std::fs;

#[test]
fn create_spec_produces_flat_file() {
    let fixture = common::TestFixture::new();
    let config = fixture.config();

    let path = lazyspec::cli::create::run(
        fixture.root(),
        &config,
        &fixture.store(),
        "spec",
        "Auth Flow",
        "jkaloger",
        |_| {},
    )
    .unwrap();

    // Returned path should be a flat .md file
    assert!(
        path.to_str().unwrap().ends_with(".md"),
        "expected .md file, got: {:?}",
        path
    );
    assert!(path.exists(), "spec file should exist");

    let filename = path.file_name().unwrap().to_str().unwrap();
    assert!(filename.starts_with("SPEC-001-"), "got: {}", filename);
    assert!(filename.ends_with("auth-flow.md"), "got: {}", filename);
}

#[test]
fn created_spec_has_correct_frontmatter() {
    let fixture = common::TestFixture::new();
    let config = fixture.config();

    let path = lazyspec::cli::create::run(
        fixture.root(),
        &config,
        &fixture.store(),
        "spec",
        "Payment Gateway",
        "alice",
        |_| {},
    )
    .unwrap();

    let content = fs::read_to_string(&path).unwrap();
    let meta = DocMeta::parse(&content).unwrap();

    assert_eq!(meta.title, "Payment Gateway");
    assert_eq!(meta.doc_type.as_str(), "spec");
    assert_eq!(meta.status, Status::Draft);
    assert_eq!(meta.author, "alice");
}

#[test]
fn created_spec_loads_in_store() {
    let fixture = common::TestFixture::new();
    let config = fixture.config();

    let path = lazyspec::cli::create::run(
        fixture.root(),
        &config,
        &fixture.store(),
        "spec",
        "Search Index",
        "carol",
        |_| {},
    )
    .unwrap();

    let store = fixture.store();
    let rel_path = path.strip_prefix(fixture.root()).unwrap();

    let all = store.all_docs();
    let doc = all.iter().find(|d| d.path == rel_path);
    assert!(doc.is_some(), "spec should appear in store");
    assert_eq!(doc.unwrap().doc_type.as_str(), "spec");
}

#[test]
fn non_subdirectory_types_still_produce_flat_files() {
    let fixture = common::TestFixture::new();
    let config = fixture.config();

    let path = lazyspec::cli::create::run(
        fixture.root(),
        &config,
        &fixture.store(),
        "rfc",
        "Flat File Test",
        "dave",
        |_| {},
    )
    .unwrap();

    assert!(path.exists());
    let filename = path.file_name().unwrap().to_str().unwrap();
    assert!(filename.ends_with(".md"), "rfc should be a flat .md file");
    assert!(
        !path
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("RFC-"),
        "rfc should not be inside a type-prefixed directory"
    );
}
