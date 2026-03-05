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
        "---\ntitle: \"Auth Redesign\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: [security]\nrelated: []\n---\n\nRFC body.\n",
    ).unwrap();

    fs::write(
        root.join("docs/stories/STORY-001-auth-impl.md"),
        "---\ntitle: \"Auth Implementation\"\ntype: story\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-02\ntags: [security]\nrelated:\n- implements: docs/rfcs/RFC-001-auth.md\n---\n\nStory body.\n",
    ).unwrap();

    fs::write(
        root.join("docs/iterations/ITERATION-001-auth-sprint.md"),
        "---\ntitle: \"Auth Sprint 1\"\ntype: iteration\nstatus: draft\nauthor: agent\ndate: 2026-03-03\ntags: []\nrelated:\n- implements: docs/stories/STORY-001-auth-impl.md\n---\n\nIteration body.\n",
    ).unwrap();

    let config = Config::default();
    let store = Store::load(root, &config).unwrap();
    (dir, store)
}

#[test]
fn context_walks_full_chain() {
    let (_dir, store) = setup();
    let chain = lazyspec::cli::context::resolve_chain(&store, "ITERATION-001").unwrap();

    assert_eq!(chain.len(), 3);
    assert_eq!(chain[0].title, "Auth Redesign");
    assert_eq!(chain[1].title, "Auth Implementation");
    assert_eq!(chain[2].title, "Auth Sprint 1");
}

#[test]
fn context_standalone_document() {
    let (_dir, store) = setup();
    let chain = lazyspec::cli::context::resolve_chain(&store, "RFC-001").unwrap();

    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].title, "Auth Redesign");
}

#[test]
fn context_json_output() {
    let (_dir, store) = setup();
    let output = lazyspec::cli::context::run_json(&store, "ITERATION-001").unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    let chain = parsed["chain"].as_array().unwrap();
    assert_eq!(chain.len(), 3);
    assert_eq!(chain[0]["type"], "rfc");
    assert_eq!(chain[1]["type"], "story");
    assert_eq!(chain[2]["type"], "iteration");
    assert_eq!(chain[0]["title"], "Auth Redesign");
}

#[test]
fn context_human_output() {
    let (_dir, store) = setup();
    let output = lazyspec::cli::context::run_human(&store, "ITERATION-001").unwrap();

    assert!(output.contains("Auth Redesign"));
    assert!(output.contains("Auth Implementation"));
    assert!(output.contains("Auth Sprint 1"));
    assert!(output.contains("rfc"));
    assert!(output.contains("story"));
    assert!(output.contains("iteration"));
}

#[test]
fn context_not_found() {
    let (_dir, store) = setup();
    let result = lazyspec::cli::context::resolve_chain(&store, "NONEXISTENT-999");

    assert!(result.is_err());
}
