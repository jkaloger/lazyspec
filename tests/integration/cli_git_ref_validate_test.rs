use lazyspec::engine::config::{Config, StoreBackend};
use lazyspec::engine::store::Store;
use lazyspec::engine::validation::ValidationIssue;

/// The starter project with `iteration` moved onto a git ref, and the standard
/// constraints stated as `[[edges]]` — the one declaration strict load accepts
/// (STORY-259). `iterations-need-stories` is the demand these tests are about;
/// it is a row now, so the finding is an `UnsatisfiedEdge`.
fn config_with_git_ref_iterations() -> Config {
    let mut config = Config {
        edges: lazyspec::engine::config::starter_edges(),
        ..Config::default()
    };
    for t in &mut config.documents.types {
        if t.name == "iteration" {
            t.store = StoreBackend::GitRef;
        }
    }
    config
}

#[test]
fn validate_catches_issues_in_git_ref_docs() {
    let fixture = crate::common::TestFixture::new();
    let config = config_with_git_ref_iterations();

    let cache_dir = fixture.root().join(".lazyspec/cache/iteration");
    std::fs::create_dir_all(&cache_dir).unwrap();

    // Iteration with no implements link -- leaves `iterations-need-stories`
    // unsatisfied.
    std::fs::write(
        cache_dir.join("ITERATION-001-orphan.md"),
        "---\ntitle: \"Orphan Iteration\"\ntype: iteration\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    )
    .unwrap();

    let store = Store::load(fixture.root(), &config).unwrap();
    let result = store.validate_full(&config);

    assert!(
        result.errors.iter().any(|e| matches!(
            e,
            ValidationIssue::UnsatisfiedEdge { edge_name, .. }
                if edge_name == "iterations-need-stories"
        )),
        "git-ref iteration without parent link should be an unsatisfied-edge error, got: {:?}",
        result.errors
    );
}

#[test]
fn validate_passes_for_valid_git_ref_docs() {
    let fixture = crate::common::TestFixture::new();
    let config = config_with_git_ref_iterations();

    // Story lives on filesystem as usual
    fixture.write_story("STORY-001.md", "A Story", "draft", None);

    let cache_dir = fixture.root().join(".lazyspec/cache/iteration");
    std::fs::create_dir_all(&cache_dir).unwrap();

    // Iteration in git-ref cache with valid implements link to the story
    std::fs::write(
        cache_dir.join("ITERATION-001-valid.md"),
        "---\ntitle: \"Valid Iteration\"\ntype: iteration\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: STORY-001\n---\n",
    )
    .unwrap();

    let store = Store::load(fixture.root(), &config).unwrap();
    let result = store.validate_full(&config);

    let iteration_errors: Vec<_> = result
        .errors
        .iter()
        .filter(|e| match e {
            ValidationIssue::UnsatisfiedEdge { from_type, .. } => from_type == "iteration",
            ValidationIssue::BrokenLink { source, .. } => {
                source.to_string_lossy().contains("ITERATION")
            }
            _ => false,
        })
        .collect();

    assert!(
        iteration_errors.is_empty(),
        "valid git-ref iteration should not produce iteration-specific errors, got: {:?}",
        iteration_errors
    );
}
