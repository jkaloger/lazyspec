use lazyspec::engine::config::Config;
use lazyspec::engine::document::DocType;
use lazyspec::engine::store::{Filter, Store};
use std::fs;
use tempfile::TempDir;

fn setup() -> (TempDir, Store) {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("docs/rfcs")).unwrap();
    fs::create_dir_all(root.join("docs/adrs")).unwrap();

    fs::write(
        root.join("docs/rfcs/RFC-001-auth.md"),
        "---\ntitle: \"Auth Redesign\"\ntype: rfc\nstatus: review\nauthor: jkaloger\ndate: 2026-03-01\ntags: [security, auth]\n---\n\nAuth body.\n",
    )
    .unwrap();

    fs::write(
        root.join("docs/rfcs/RFC-002-api.md"),
        "---\ntitle: \"API Versioning\"\ntype: rfc\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-02\ntags: [api]\n---\n\nAPI body.\n",
    )
    .unwrap();

    let config = Config::default();
    let store = Store::load(root, &config).unwrap();
    (dir, store)
}

#[test]
fn list_all_rfcs() {
    let (_dir, store) = setup();
    let filter = Filter {
        doc_type: Some(DocType::Rfc),
        ..Default::default()
    };
    let results = store.list(&filter);
    assert_eq!(results.len(), 2);
}

#[test]
fn filter_by_tag() {
    let (_dir, store) = setup();
    let filter = Filter {
        tag: Some("security".to_string()),
        ..Default::default()
    };
    let results = store.list(&filter);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Auth Redesign");
}

#[test]
fn resolve_shorthand_id() {
    let (_dir, store) = setup();
    let doc = store.resolve_shorthand("RFC-001");
    assert!(doc.is_some());
    assert_eq!(doc.unwrap().title, "Auth Redesign");
}
