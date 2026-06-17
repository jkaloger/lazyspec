use crate::common::TestFixture;
use lazyspec::cli::json::doc_to_json;
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

#[test]
fn update_with_json_flag() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test", "draft");
    let store = fixture.store();

    lazyspec::cli::update::run(fixture.root(), &store, "RFC-001", &[("status", "review")]).unwrap();

    // Reload store to pick up changes (mirrors what main.rs does for --json)
    let config = fixture.config();
    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();
    let doc = lazyspec::cli::resolve::resolve_shorthand_or_path(&store, "RFC-001").unwrap();
    let json_val = doc_to_json(doc);
    let output = serde_json::to_string_pretty(&json_val).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["status"], "review");
    assert_eq!(parsed["title"], "Test");
    assert_eq!(parsed["type"], "rfc");
    assert!(parsed["path"].as_str().unwrap().contains("RFC-001"));
}
