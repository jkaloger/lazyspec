use crate::common::TestFixture;
use lazyspec::engine::config::{Config, StoreBackend};
use lazyspec::engine::store::{Filter, Store};
use std::path::PathBuf;

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
fn list_returns_docs_from_filesystem_and_git_ref_backends() {
    let fixture = TestFixture::new();

    fixture.write_rfc("RFC-001-cross-backend.md", "Cross-backend RFC", "draft");

    let cache_dir = fixture.root().join(".lazyspec/cache/iteration");
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::write(
        cache_dir.join("ITERATION-001-slice-one.md"),
        concat!(
            "---\n",
            "title: \"Slice one\"\n",
            "type: iteration\n",
            "status: draft\n",
            "author: \"test\"\n",
            "date: 2026-04-01\n",
            "tags: []\n",
            "---\n",
            "First iteration from git-ref cache.\n",
        ),
    )
    .unwrap();

    let config = config_with_git_ref_iteration();
    let store = Store::load(fixture.root(), &config).unwrap();
    let all = store.all_docs();

    let titles: Vec<&str> = all.iter().map(|d| d.title.as_str()).collect();
    assert!(
        titles.contains(&"Cross-backend RFC"),
        "filesystem RFC missing from all_docs; got: {:?}",
        titles
    );
    assert!(
        titles.contains(&"Slice one"),
        "git-ref iteration missing from all_docs; got: {:?}",
        titles
    );
}

#[test]
fn list_filter_by_type_returns_only_matching_backend() {
    let fixture = TestFixture::new();

    fixture.write_rfc("RFC-002-filter.md", "Filtered RFC", "accepted");

    let cache_dir = fixture.root().join(".lazyspec/cache/iteration");
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::write(
        cache_dir.join("ITERATION-010-cached.md"),
        concat!(
            "---\n",
            "title: \"Cached iteration\"\n",
            "type: iteration\n",
            "status: in-progress\n",
            "author: \"test\"\n",
            "date: 2026-04-02\n",
            "tags: []\n",
            "---\n",
            "Body.\n",
        ),
    )
    .unwrap();

    let config = config_with_git_ref_iteration();
    let store = Store::load(fixture.root(), &config).unwrap();

    let rfc_filter = Filter {
        doc_type: Some(lazyspec::engine::document::DocType::new("rfc")),
        ..Default::default()
    };
    let rfcs = store.list(&rfc_filter);
    assert_eq!(rfcs.len(), 1);
    assert_eq!(rfcs[0].title, "Filtered RFC");

    let iter_filter = Filter {
        doc_type: Some(lazyspec::engine::document::DocType::new("iteration")),
        ..Default::default()
    };
    let iters = store.list(&iter_filter);
    assert_eq!(iters.len(), 1);
    assert_eq!(iters[0].title, "Cached iteration");
}

#[test]
fn git_ref_cached_doc_has_correct_relative_path() {
    let fixture = TestFixture::new();

    let cache_dir = fixture.root().join(".lazyspec/cache/iteration");
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::write(
        cache_dir.join("ITERATION-005-path-check.md"),
        concat!(
            "---\n",
            "title: \"Path check\"\n",
            "type: iteration\n",
            "status: draft\n",
            "author: \"test\"\n",
            "date: 2026-04-01\n",
            "tags: []\n",
            "---\n",
            "Body.\n",
        ),
    )
    .unwrap();

    let config = config_with_git_ref_iteration();
    let store = Store::load(fixture.root(), &config).unwrap();

    let expected = PathBuf::from(".lazyspec/cache/iteration/ITERATION-005-path-check.md");
    let doc = store.get(&expected);
    assert!(
        doc.is_some(),
        "should retrieve git-ref doc by its cache-relative path"
    );
    assert_eq!(doc.unwrap().id, "ITERATION-005");
}
