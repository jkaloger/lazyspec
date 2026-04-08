mod common;

use lazyspec::engine::config::{Config, StoreBackend};
use lazyspec::engine::store::Store;

fn config_with_git_ref() -> Config {
    let mut config = Config::default();
    for t in &mut config.documents.types {
        if t.name == "iteration" {
            t.store = StoreBackend::GitRef;
        }
    }
    config
}

fn setup() -> (common::TestFixture, Store, Config) {
    let fixture = common::TestFixture::new();

    fixture.write_doc(
        "docs/rfcs/RFC-001-auth.md",
        "---\ntitle: \"Auth Redesign\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: [security]\nrelated: []\n---\n\nBody.\n",
    );

    std::fs::create_dir_all(fixture.root().join(".lazyspec/cache/iteration")).unwrap();
    fixture.write_doc(
        ".lazyspec/cache/iteration/ITERATION-001-impl.md",
        "---\ntitle: \"Impl Sprint\"\ntype: iteration\nstatus: draft\nauthor: agent\ndate: 2026-03-03\ntags: []\nrelated: []\n---\n\nIteration body.\n",
    );

    let config = config_with_git_ref();
    let store = Store::load(fixture.root(), &config).unwrap();
    (fixture, store, config)
}

#[test]
fn status_json_includes_git_ref_documents() {
    let (_fixture, store, config) = setup();
    let output = lazyspec::cli::status::run_json(&store, &config);
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    let docs = parsed["documents"].as_array().unwrap();
    assert_eq!(docs.len(), 2, "expected both filesystem and git-ref docs");

    let titles: Vec<&str> = docs.iter().map(|d| d["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"Auth Redesign"), "missing filesystem RFC");
    assert!(titles.contains(&"Impl Sprint"), "missing git-ref iteration");
}

#[test]
fn status_human_includes_git_ref_documents() {
    let (_fixture, store, _config) = setup();
    let output = lazyspec::cli::status::run_human(&store);

    assert!(
        output.contains("RFC"),
        "human output should contain RFC header"
    );
    assert!(
        output.contains("ITERATION"),
        "human output should contain ITERATION header"
    );
    assert!(
        output.contains("Auth Redesign"),
        "human output should contain filesystem doc title"
    );
    assert!(
        output.contains("Impl Sprint"),
        "human output should contain git-ref doc title"
    );
}
