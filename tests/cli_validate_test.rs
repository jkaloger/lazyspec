mod common;

use lazyspec::engine::validation::ValidationIssue;
use std::path::PathBuf;

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
    let output = lazyspec::cli::validate::run_json(&store, &fixture.config(), &[]);
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

#[test]
fn validate_catches_duplicate_ids() {
    let fixture = common::TestFixture::new();
    fixture.write_rfc("RFC-020-foo.md", "Foo RFC", "draft");
    fixture.write_rfc("RFC-020-bar.md", "Bar RFC", "draft");

    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    let dups: Vec<_> = result
        .errors
        .iter()
        .filter(|e| matches!(e, ValidationIssue::DuplicateId { .. }))
        .collect();
    assert_eq!(dups.len(), 1);
    match &dups[0] {
        ValidationIssue::DuplicateId { id, paths } => {
            assert_eq!(id, "RFC-020");
            assert_eq!(paths.len(), 2);
            assert!(paths.contains(&PathBuf::from("docs/rfcs/RFC-020-foo.md")));
            assert!(paths.contains(&PathBuf::from("docs/rfcs/RFC-020-bar.md")));
        }
        _ => unreachable!(),
    }
}

#[test]
fn validate_duplicate_id_json_output() {
    let fixture = common::TestFixture::new();
    fixture.write_rfc("RFC-030-alpha.md", "Alpha", "draft");
    fixture.write_rfc("RFC-030-beta.md", "Beta", "draft");

    let store = fixture.store();
    let output = lazyspec::cli::validate::run_json(&store, &fixture.config(), &[]);
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    let errors = parsed["errors"].as_array().unwrap();
    let has_dup = errors.iter().any(|e| {
        e.as_str()
            .map(|s| s.starts_with("duplicate id: RFC-030"))
            .unwrap_or(false)
    });
    assert!(has_dup, "JSON output should contain duplicate id error");
}

#[test]
fn validate_duplicate_id_human_output() {
    let fixture = common::TestFixture::new();
    fixture.write_rfc("RFC-040-one.md", "One", "draft");
    fixture.write_rfc("RFC-040-two.md", "Two", "draft");

    let store = fixture.store();
    let output = lazyspec::cli::validate::run_human(&store, &fixture.config(), true, &[]);

    assert!(
        output.contains("duplicate id: RFC-040"),
        "Human output should contain duplicate id line, got: {}",
        output
    );
}

#[test]
fn validate_no_duplicate_ids_when_unique() {
    let fixture = common::TestFixture::new();
    fixture.write_rfc("RFC-050-first.md", "First", "draft");
    fixture.write_rfc("RFC-051-second.md", "Second", "draft");

    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    let dups: Vec<_> = result
        .errors
        .iter()
        .filter(|e| matches!(e, ValidationIssue::DuplicateId { .. }))
        .collect();
    assert!(dups.is_empty());
}

#[test]
fn validate_ignore_excludes_from_duplicate_check() {
    let fixture = common::TestFixture::new();
    fixture.write_rfc("RFC-060-real.md", "Real", "draft");
    fixture.write_doc(
        "docs/rfcs/RFC-060-ignored.md",
        "---\ntitle: \"Ignored\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\nvalidate-ignore: true\n---\n",
    );

    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    let dups: Vec<_> = result
        .errors
        .iter()
        .filter(|e| matches!(e, ValidationIssue::DuplicateId { .. }))
        .collect();
    assert!(dups.is_empty(), "validate_ignore docs should be excluded from duplicate ID check");
}

#[test]
fn validate_broken_link_with_nonexistent_id() {
    let fixture = common::TestFixture::new();
    fixture.write_doc(
        "docs/adrs/ADR-001-bad-id.md",
        "---\ntitle: \"Bad ID Link\"\ntype: adr\nstatus: draft\nauthor: a\ndate: 2026-01-01\ntags: []\nrelated:\n  - implements: RFC-999\n---\n",
    );

    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    let broken: Vec<_> = result
        .errors
        .iter()
        .filter(|e| matches!(e, ValidationIssue::BrokenLink { .. }))
        .collect();
    assert_eq!(broken.len(), 1, "expected exactly one BrokenLink error");
    match &broken[0] {
        ValidationIssue::BrokenLink { source, target } => {
            assert!(source.ends_with("ADR-001-bad-id.md"));
            assert_eq!(target, "RFC-999", "broken link target should be the unresolved ID");
        }
        _ => unreachable!(),
    }
}

#[test]
fn validate_valid_id_link_is_not_broken() {
    let fixture = common::TestFixture::new();
    fixture.write_rfc("RFC-001-feature.md", "Feature", "draft");
    fixture.write_doc(
        "docs/adrs/ADR-001-linked.md",
        "---\ntitle: \"Linked ADR\"\ntype: adr\nstatus: draft\nauthor: a\ndate: 2026-01-01\ntags: []\nrelated:\n  - implements: RFC-001\n---\n",
    );

    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    let broken: Vec<_> = result
        .errors
        .iter()
        .filter(|e| matches!(e, ValidationIssue::BrokenLink { .. }))
        .collect();
    assert!(broken.is_empty(), "valid ID link should not produce BrokenLink error, got: {:?}", broken);
}

#[test]
fn validate_broken_link_with_nonexistent_id_in_json_output() {
    let fixture = common::TestFixture::new();
    fixture.write_doc(
        "docs/adrs/ADR-001-bad-id.md",
        "---\ntitle: \"Bad ID Link\"\ntype: adr\nstatus: draft\nauthor: a\ndate: 2026-01-01\ntags: []\nrelated:\n  - implements: RFC-999\n---\n",
    );

    let store = fixture.store();
    let output = lazyspec::cli::validate::run_json(&store, &fixture.config(), &[]);
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    let errors = parsed["errors"].as_array().unwrap();
    let has_broken = errors.iter().any(|e| {
        e.as_str()
            .map(|s| s.contains("RFC-999"))
            .unwrap_or(false)
    });
    assert!(has_broken, "JSON output should contain broken link error with unresolved ID RFC-999, got: {:?}", errors);
}
