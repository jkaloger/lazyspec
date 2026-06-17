use lazyspec::cli::show;
use lazyspec::engine::config::{Config, StoreBackend};
use lazyspec::engine::fs::RealFileSystem;
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

fn setup() -> (crate::common::TestFixture, Store) {
    let fixture = crate::common::TestFixture::new();
    let root = fixture.root();

    std::fs::create_dir_all(root.join(".lazyspec/cache/iteration")).unwrap();
    std::fs::write(
        root.join(".lazyspec/cache/iteration/ITERATION-001-feature.md"),
        "---\ntitle: \"Feature Work\"\ntype: iteration\nstatus: draft\nauthor: \"agent\"\ndate: 2026-04-01\ntags: []\nrelated: []\n---\n\nImplementation details here.\n",
    )
    .unwrap();

    let config = config_with_git_ref();
    let store = Store::load_with_fs(root, &config, &RealFileSystem, None).unwrap();
    (fixture, store)
}

#[test]
fn resolve_shorthand_finds_git_ref_iteration() {
    let (_fixture, store) = setup();
    let doc = store
        .resolve_shorthand("ITERATION-001")
        .expect("should resolve git-ref iteration");
    assert_eq!(doc.title, "Feature Work");
    assert_eq!(doc.id, "ITERATION-001");
}

#[test]
fn show_json_displays_git_ref_iteration() {
    let (_fixture, store) = setup();
    let output = show::run_json(&store, "ITERATION-001", false, 25, &RealFileSystem).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(parsed["title"], "Feature Work");
    assert_eq!(parsed["type"], "iteration");
    assert_eq!(parsed["status"], "draft");
    assert_eq!(parsed["author"], "agent");
    assert_eq!(parsed["date"], "2026-04-01");
    assert!(parsed["body"]
        .as_str()
        .unwrap()
        .contains("Implementation details here."));
}

#[test]
fn show_json_not_found_for_missing_git_ref_doc() {
    let (_fixture, store) = setup();
    let result = show::run_json(&store, "ITERATION-999", false, 25, &RealFileSystem);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("document not found"));
}
