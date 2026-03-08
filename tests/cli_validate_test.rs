mod common;

use lazyspec::engine::validation::ValidationIssue;

#[test]
fn validate_catches_broken_link() {
    let fixture = common::TestFixture::new();
    fixture.write_doc(
        "docs/adrs/ADR-001.md",
        "---\ntitle: \"Bad Link\"\ntype: adr\nstatus: draft\nauthor: a\ndate: 2026-01-01\ntags: []\nrelated:\n  - implements: docs/rfcs/DOES-NOT-EXIST.md\n---\n",
    );

    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    assert!(!result.errors.is_empty());
}

#[test]
fn validate_passes_clean_repo() {
    let fixture = common::TestFixture::new();
    fixture.write_rfc("RFC-001.md", "Good", "draft");

    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    assert!(result.errors.is_empty());
}

#[test]
fn validate_catches_unlinked_iteration() {
    let fixture = common::TestFixture::new();
    fixture.write_iteration("ITERATION-001.md", "Orphan Iteration", "draft", None);

    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    assert!(!result.errors.is_empty());
    let has_unlinked = result.errors.iter().any(|e| matches!(e, ValidationIssue::MissingParentLink { .. }));
    assert!(has_unlinked);
}

#[test]
fn validate_catches_unlinked_adr() {
    let fixture = common::TestFixture::new();
    fixture.write_adr("ADR-001.md", "Orphan ADR", "draft", None);

    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    assert!(!result.errors.is_empty());
    let has_unlinked = result.errors.iter().any(|e| matches!(e, ValidationIssue::MissingRelation { .. }));
    assert!(has_unlinked);
}

#[test]
fn validate_json_includes_parse_errors() {
    let fixture = common::TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-broken.md",
        "---\ntitle: \"Broken\"\ntype: rfc\nauthor: test\ndate: 2026-01-01\ntags: []\n---\n",
    );

    let store = fixture.store();
    let output = lazyspec::cli::validate::run_json(&store, &fixture.config());
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    let errors = parsed["parse_errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert!(errors[0]["path"].is_string());
    assert!(errors[0]["error"].is_string());
}

#[test]
fn validate_passes_linked_iteration() {
    let fixture = common::TestFixture::new();
    fixture.write_story("STORY-001.md", "A Story", "draft", None);
    fixture.write_iteration(
        "ITERATION-001.md",
        "Impl",
        "draft",
        Some("docs/stories/STORY-001.md"),
    );

    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    assert!(result.errors.is_empty());
}
