use lazyspec::engine::config::Config;
use lazyspec::engine::store::Store;
use std::fs;
use tempfile::TempDir;

#[test]
fn validate_catches_broken_link() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("docs/adrs")).unwrap();
    fs::write(
        root.join("docs/adrs/ADR-001.md"),
        "---\ntitle: \"Bad Link\"\ntype: adr\nstatus: draft\nauthor: a\ndate: 2026-01-01\ntags: []\nrelated:\n  - implements: docs/rfcs/DOES-NOT-EXIST.md\n---\n",
    ).unwrap();

    let config = Config::default();
    let store = Store::load(root, &config).unwrap();
    let errors = store.validate();

    assert!(!errors.is_empty());
}

#[test]
fn validate_passes_clean_repo() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("docs/rfcs")).unwrap();
    fs::write(
        root.join("docs/rfcs/RFC-001.md"),
        "---\ntitle: \"Good\"\ntype: rfc\nstatus: draft\nauthor: a\ndate: 2026-01-01\ntags: []\n---\n",
    ).unwrap();

    let config = Config::default();
    let store = Store::load(root, &config).unwrap();
    let errors = store.validate();

    assert!(errors.is_empty());
}

#[test]
fn validate_catches_unlinked_iteration() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("docs/iterations")).unwrap();
    fs::write(
        root.join("docs/iterations/ITERATION-001.md"),
        "---\ntitle: \"Orphan Iteration\"\ntype: iteration\nstatus: draft\nauthor: a\ndate: 2026-01-01\ntags: []\n---\n",
    ).unwrap();

    let config = Config::default();
    let store = Store::load(root, &config).unwrap();
    let errors = store.validate();

    assert!(!errors.is_empty());
    let has_unlinked = errors.iter().any(|e| matches!(e, lazyspec::engine::store::ValidationError::UnlinkedIteration { .. }));
    assert!(has_unlinked);
}

#[test]
fn validate_catches_unlinked_adr() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("docs/adrs")).unwrap();
    fs::write(
        root.join("docs/adrs/ADR-001.md"),
        "---\ntitle: \"Orphan ADR\"\ntype: adr\nstatus: draft\nauthor: a\ndate: 2026-01-01\ntags: []\n---\n",
    ).unwrap();

    let config = Config::default();
    let store = Store::load(root, &config).unwrap();
    let errors = store.validate();

    assert!(!errors.is_empty());
    let has_unlinked = errors.iter().any(|e| matches!(e, lazyspec::engine::store::ValidationError::UnlinkedAdr { .. }));
    assert!(has_unlinked);
}

#[test]
fn validate_passes_linked_iteration() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("docs/stories")).unwrap();
    fs::create_dir_all(root.join("docs/iterations")).unwrap();

    fs::write(
        root.join("docs/stories/STORY-001.md"),
        "---\ntitle: \"A Story\"\ntype: story\nstatus: draft\nauthor: a\ndate: 2026-01-01\ntags: []\n---\n",
    ).unwrap();

    fs::write(
        root.join("docs/iterations/ITERATION-001.md"),
        "---\ntitle: \"Impl\"\ntype: iteration\nstatus: draft\nauthor: a\ndate: 2026-01-01\ntags: []\nrelated:\n  - implements: docs/stories/STORY-001.md\n---\n",
    ).unwrap();

    let config = Config::default();
    let store = Store::load(root, &config).unwrap();
    let errors = store.validate();

    assert!(errors.is_empty());
}
