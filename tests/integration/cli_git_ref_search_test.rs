use crate::common::TestFixture;
use lazyspec::engine::config::{Config, StoreBackend};
use lazyspec::engine::fs::RealFileSystem;
use lazyspec::engine::store::Store;

fn config_with_git_ref_iteration() -> Config {
    let mut config = Config::default();
    for t in &mut config.documents.types {
        if t.name == "iteration" {
            t.store = StoreBackend::GitRef;
        }
    }
    config
}

#[test]
fn search_finds_git_ref_doc_by_title() {
    let fixture = TestFixture::new();

    let cache_dir = fixture.root().join(".lazyspec/cache/iteration");
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::write(
        cache_dir.join("ITERATION-001-auth-migration.md"),
        concat!(
            "---\n",
            "title: \"Auth migration\"\n",
            "type: iteration\n",
            "status: draft\n",
            "author: \"test\"\n",
            "date: 2026-04-01\n",
            "tags: []\n",
            "---\n",
            "Some unrelated body content.\n",
        ),
    )
    .unwrap();

    let config = config_with_git_ref_iteration();
    let store = Store::load(fixture.root(), &config).unwrap();
    let results = store.search("migration", &RealFileSystem);

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].doc.title, "Auth migration");
    assert_eq!(results[0].match_field, "title");
}

#[test]
fn search_finds_git_ref_doc_by_body() {
    let fixture = TestFixture::new();

    let cache_dir = fixture.root().join(".lazyspec/cache/iteration");
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::write(
        cache_dir.join("ITERATION-002-performance-tuning.md"),
        concat!(
            "---\n",
            "title: \"Performance tuning\"\n",
            "type: iteration\n",
            "status: draft\n",
            "author: \"test\"\n",
            "date: 2026-04-01\n",
            "tags: []\n",
            "---\n",
            "This iteration introduces a throughput benchmark for the ingest pipeline.\n",
        ),
    )
    .unwrap();

    let config = config_with_git_ref_iteration();
    let store = Store::load(fixture.root(), &config).unwrap();
    let results = store.search("throughput", &RealFileSystem);

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].doc.title, "Performance tuning");
    assert_eq!(results[0].match_field, "body");
    assert!(results[0].snippet.contains("throughput"));
}

#[test]
fn search_returns_results_from_both_backends() {
    let fixture = TestFixture::new();

    fixture.write_rfc(
        "RFC-010-unified-search.md",
        "Unified search across backends",
        "draft",
    );

    let cache_dir = fixture.root().join(".lazyspec/cache/iteration");
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::write(
        cache_dir.join("ITERATION-010-search-slice.md"),
        concat!(
            "---\n",
            "title: \"Search slice iteration\"\n",
            "type: iteration\n",
            "status: draft\n",
            "author: \"test\"\n",
            "date: 2026-04-01\n",
            "tags: []\n",
            "---\n",
            "Body of the search iteration.\n",
        ),
    )
    .unwrap();

    let config = config_with_git_ref_iteration();
    let store = Store::load(fixture.root(), &config).unwrap();
    let results = store.search("search", &RealFileSystem);

    let titles: Vec<&str> = results.iter().map(|r| r.doc.title.as_str()).collect();
    assert!(
        titles.contains(&"Unified search across backends"),
        "filesystem RFC should appear in search results; got: {:?}",
        titles
    );
    assert!(
        titles.contains(&"Search slice iteration"),
        "git-ref iteration should appear in search results; got: {:?}",
        titles
    );
    assert_eq!(results.len(), 2);
}
