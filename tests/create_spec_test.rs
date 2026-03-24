mod common;

use lazyspec::engine::document::{DocMeta, Status};
use std::fs;

#[test]
fn create_spec_produces_directory_with_index_and_story() {
    let fixture = common::TestFixture::new();
    let config = fixture.config();

    let path = lazyspec::cli::create::run(
        fixture.root(), &config, "spec", "Auth Flow", "jkaloger", |_| {},
    ).unwrap();

    // Returned path should point to index.md
    assert!(path.to_str().unwrap().ends_with("index.md"), "expected index.md, got: {:?}", path);
    assert!(path.exists(), "index.md should exist");

    // The parent directory should be SPEC-001-auth-flow
    let spec_dir = path.parent().unwrap();
    let dir_name = spec_dir.file_name().unwrap().to_str().unwrap();
    assert!(dir_name.starts_with("SPEC-001-"), "got: {}", dir_name);
    assert!(dir_name.ends_with("auth-flow"), "got: {}", dir_name);

    // story.md should also exist in the same directory
    let story_path = spec_dir.join("story.md");
    assert!(story_path.exists(), "story.md should exist in spec directory");
}

#[test]
fn created_index_has_correct_frontmatter() {
    let fixture = common::TestFixture::new();
    let config = fixture.config();

    let path = lazyspec::cli::create::run(
        fixture.root(), &config, "spec", "Payment Gateway", "alice", |_| {},
    ).unwrap();

    let content = fs::read_to_string(&path).unwrap();
    let meta = DocMeta::parse(&content).unwrap();

    assert_eq!(meta.title, "Payment Gateway");
    assert_eq!(meta.doc_type.as_str(), "spec");
    assert_eq!(meta.status, Status::Draft);
    assert_eq!(meta.author, "alice");
}

#[test]
fn created_story_has_correct_frontmatter_and_ac_template() {
    let fixture = common::TestFixture::new();
    let config = fixture.config();

    let path = lazyspec::cli::create::run(
        fixture.root(), &config, "spec", "Notifications", "bob", |_| {},
    ).unwrap();

    let story_path = path.parent().unwrap().join("story.md");
    let content = fs::read_to_string(&story_path).unwrap();

    assert!(content.contains("type: spec"), "story.md should have type: spec");
    assert!(content.contains("### AC:"), "story.md should contain AC template");
    assert!(content.contains("## Acceptance Criteria"), "story.md should contain AC heading");

    let meta = DocMeta::parse(&content).unwrap();
    assert_eq!(meta.doc_type.as_str(), "spec");
    assert_eq!(meta.status, Status::Draft);
}

#[test]
fn created_spec_loads_in_store() {
    let fixture = common::TestFixture::new();
    let config = fixture.config();

    let path = lazyspec::cli::create::run(
        fixture.root(), &config, "spec", "Search Index", "carol", |_| {},
    ).unwrap();

    let store = fixture.store();
    let rel_path = path.strip_prefix(fixture.root()).unwrap();

    // The index.md should be in the store
    let all = store.all_docs();
    let doc = all.iter().find(|d| d.path == rel_path);
    assert!(doc.is_some(), "spec index.md should appear in store");
    assert_eq!(doc.unwrap().doc_type.as_str(), "spec");

    // story.md should be a child of index.md
    let children = store.children_of(rel_path);
    assert!(!children.is_empty(), "spec should have child documents");
    assert!(
        children.iter().any(|c| c.to_str().unwrap().ends_with("story.md")),
        "story.md should be a child of the spec"
    );
}

#[test]
fn non_subdirectory_types_still_produce_flat_files() {
    let fixture = common::TestFixture::new();
    let config = fixture.config();

    let path = lazyspec::cli::create::run(
        fixture.root(), &config, "rfc", "Flat File Test", "dave", |_| {},
    ).unwrap();

    assert!(path.exists());
    let filename = path.file_name().unwrap().to_str().unwrap();
    assert!(filename.ends_with(".md"), "rfc should be a flat .md file");
    assert!(!path.parent().unwrap().file_name().unwrap().to_str().unwrap().starts_with("RFC-"),
        "rfc should not be inside a type-prefixed directory");
}
