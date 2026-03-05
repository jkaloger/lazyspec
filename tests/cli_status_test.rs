use lazyspec::engine::config::Config;
use lazyspec::engine::store::Store;
use std::fs;
use tempfile::TempDir;

fn setup() -> (TempDir, Store) {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("docs/rfcs")).unwrap();
    fs::create_dir_all(root.join("docs/stories")).unwrap();
    fs::create_dir_all(root.join("docs/iterations")).unwrap();

    fs::write(
        root.join("docs/rfcs/RFC-001-auth.md"),
        "---\ntitle: \"Auth Redesign\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: [security]\nrelated: []\n---\n\nBody.\n",
    ).unwrap();

    fs::write(
        root.join("docs/stories/STORY-001-auth.md"),
        "---\ntitle: \"Auth Story\"\ntype: story\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-02\ntags: []\nrelated:\n- implements: docs/rfcs/RFC-001-auth.md\n---\n\nBody.\n",
    ).unwrap();

    fs::write(
        root.join("docs/iterations/ITERATION-001-sprint.md"),
        "---\ntitle: \"Sprint 1\"\ntype: iteration\nstatus: draft\nauthor: agent\ndate: 2026-03-03\ntags: []\nrelated:\n- implements: docs/stories/STORY-001-auth.md\n---\n\nBody.\n",
    ).unwrap();

    let config = Config::default();
    let store = Store::load(root, &config).unwrap();
    (dir, store)
}

#[test]
fn status_json_has_documents_and_validation() {
    let (_dir, store) = setup();
    let output = lazyspec::cli::status::run_json(&store);
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert!(parsed["documents"].is_array());
    assert!(parsed["validation"].is_object());
    assert!(parsed["validation"]["errors"].is_array());
    assert!(parsed["validation"]["warnings"].is_array());
}

#[test]
fn status_json_includes_all_documents() {
    let (_dir, store) = setup();
    let output = lazyspec::cli::status::run_json(&store);
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    let docs = parsed["documents"].as_array().unwrap();
    assert_eq!(docs.len(), 3);

    let titles: Vec<&str> = docs.iter().map(|d| d["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"Auth Redesign"));
    assert!(titles.contains(&"Auth Story"));
    assert!(titles.contains(&"Sprint 1"));
}

#[test]
fn status_json_documents_use_full_schema() {
    let (_dir, store) = setup();
    let output = lazyspec::cli::status::run_json(&store);
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    let doc = &parsed["documents"][0];
    assert!(doc["path"].is_string());
    assert!(doc["title"].is_string());
    assert!(doc["type"].is_string());
    assert!(doc["status"].is_string());
    assert!(doc["author"].is_string());
    assert!(doc["date"].is_string());
    assert!(doc["tags"].is_array());
    assert!(doc["related"].is_array());
}

#[test]
fn status_human_grouped_by_type() {
    let (_dir, store) = setup();
    let output = lazyspec::cli::status::run_human(&store);

    assert!(output.contains("RFC"));
    assert!(output.contains("STORY"));
    assert!(output.contains("ITERATION"));
    assert!(output.contains("Auth Redesign"));
    assert!(output.contains("Auth Story"));
    assert!(output.contains("Sprint 1"));
}

#[test]
fn status_empty_project() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("docs/rfcs")).unwrap();

    let config = Config::default();
    let store = Store::load(root, &config).unwrap();

    let json_output = lazyspec::cli::status::run_json(&store);
    let parsed: serde_json::Value = serde_json::from_str(&json_output).unwrap();
    assert_eq!(parsed["documents"].as_array().unwrap().len(), 0);

    let human_output = lazyspec::cli::status::run_human(&store);
    assert!(human_output.is_empty() || human_output.trim().is_empty());
}
