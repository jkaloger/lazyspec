mod common;

use lazyspec::engine::config::{Config, TypeDef, NumberingStrategy};
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

fn config_with_extra_types(extra: Vec<TypeDef>) -> Config {
    let mut config = Config::default();
    let extra_names: Vec<&str> = extra.iter().map(|t| t.name.as_str()).collect();
    config.documents.types.retain(|t| !extra_names.contains(&t.name.as_str()));
    config.documents.types.extend(extra);
    config
}

fn singleton_type(name: &str, dir: &str, prefix: &str) -> TypeDef {
    TypeDef {
        name: name.to_string(),
        plural: format!("{}s", name),
        dir: dir.to_string(),
        prefix: prefix.to_string(),
        icon: None,
        numbering: NumberingStrategy::default(),
        subdirectory: false,
        store: Default::default(),
        singleton: true,
        parent_type: None,
    }
}

fn child_type(name: &str, dir: &str, prefix: &str, parent: &str) -> TypeDef {
    TypeDef {
        name: name.to_string(),
        plural: format!("{}s", name),
        dir: dir.to_string(),
        prefix: prefix.to_string(),
        icon: None,
        numbering: NumberingStrategy::default(),
        subdirectory: false,
        store: Default::default(),
        singleton: false,
        parent_type: Some(parent.to_string()),
    }
}

#[test]
fn singleton_violation_detected() {
    let fixture = common::TestFixture::new();
    let config = config_with_extra_types(vec![
        singleton_type("convention", "docs/convention", "CONV"),
    ]);

    std::fs::create_dir_all(fixture.root().join("docs/convention")).unwrap();
    fixture.write_doc(
        "docs/convention/CONV-001-first.md",
        "---\ntitle: \"First\"\ntype: convention\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\n---\n",
    );
    fixture.write_doc(
        "docs/convention/CONV-002-second.md",
        "---\ntitle: \"Second\"\ntype: convention\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\n---\n",
    );

    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();
    let result = store.validate_full(&config);

    let violations: Vec<_> = result.errors.iter().filter(|e| matches!(e, ValidationIssue::SingletonViolation { .. })).collect();
    assert_eq!(violations.len(), 1);
    match &violations[0] {
        ValidationIssue::SingletonViolation { type_name, paths } => {
            assert_eq!(type_name, "convention");
            assert_eq!(paths.len(), 2);
        }
        _ => unreachable!(),
    }
}

#[test]
fn singleton_single_doc_no_error() {
    let fixture = common::TestFixture::new();
    let config = config_with_extra_types(vec![
        singleton_type("convention", "docs/convention", "CONV"),
    ]);

    std::fs::create_dir_all(fixture.root().join("docs/convention")).unwrap();
    fixture.write_doc(
        "docs/convention/CONV-001-only.md",
        "---\ntitle: \"Only\"\ntype: convention\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\n---\n",
    );

    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();
    let result = store.validate_full(&config);

    let violations: Vec<_> = result.errors.iter().filter(|e| matches!(e, ValidationIssue::SingletonViolation { .. })).collect();
    assert!(violations.is_empty());
}

#[test]
fn parent_type_inside_dir_no_error() {
    let fixture = common::TestFixture::new();
    let config = config_with_extra_types(vec![
        singleton_type("convention", "docs/convention", "CONV"),
        child_type("dictum", "docs/convention", "DICT", "convention"),
    ]);

    std::fs::create_dir_all(fixture.root().join("docs/convention")).unwrap();
    fixture.write_doc(
        "docs/convention/CONV-001-main.md",
        "---\ntitle: \"Main Convention\"\ntype: convention\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\n---\n",
    );
    fixture.write_doc(
        "docs/convention/DICT-001-child.md",
        "---\ntitle: \"A Dictum\"\ntype: dictum\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\n---\n",
    );

    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();
    let result = store.validate_full(&config);

    let violations: Vec<_> = result.errors.iter().filter(|e| matches!(e, ValidationIssue::ParentTypeViolation { .. })).collect();
    assert!(violations.is_empty());
}

#[test]
fn parent_type_outside_dir_error() {
    let fixture = common::TestFixture::new();
    let config = config_with_extra_types(vec![
        singleton_type("convention", "docs/convention", "CONV"),
        child_type("dictum", "docs/dictums", "DICT", "convention"),
    ]);

    std::fs::create_dir_all(fixture.root().join("docs/convention")).unwrap();
    std::fs::create_dir_all(fixture.root().join("docs/dictums")).unwrap();
    fixture.write_doc(
        "docs/convention/CONV-001-main.md",
        "---\ntitle: \"Main Convention\"\ntype: convention\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\n---\n",
    );
    fixture.write_doc(
        "docs/dictums/DICT-001-stray.md",
        "---\ntitle: \"Stray Dictum\"\ntype: dictum\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\n---\n",
    );

    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();
    let result = store.validate_full(&config);

    let violations: Vec<_> = result.errors.iter().filter(|e| matches!(e, ValidationIssue::ParentTypeViolation { .. })).collect();
    assert_eq!(violations.len(), 1);
    match &violations[0] {
        ValidationIssue::ParentTypeViolation { path, type_name, expected_dir } => {
            assert_eq!(type_name, "dictum");
            assert_eq!(expected_dir, "docs/convention");
            assert!(path.to_string_lossy().contains("DICT-001"));
        }
        _ => unreachable!(),
    }
}

#[test]
fn parent_type_references_non_singleton_error() {
    let fixture = common::TestFixture::new();

    let non_singleton_parent = TypeDef {
        name: "guideline".to_string(),
        plural: "guidelines".to_string(),
        dir: "docs/guidelines".to_string(),
        prefix: "GUIDE".to_string(),
        icon: None,
        numbering: NumberingStrategy::default(),
        subdirectory: false,
        store: Default::default(),
        singleton: false,
        parent_type: None,
    };
    let config = config_with_extra_types(vec![
        non_singleton_parent,
        child_type("dictum", "docs/guidelines", "DICT", "guideline"),
    ]);

    std::fs::create_dir_all(fixture.root().join("docs/guidelines")).unwrap();

    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();
    let result = store.validate_full(&config);

    let violations: Vec<_> = result.errors.iter().filter(|e| matches!(e, ValidationIssue::ParentTypeNotSingleton { .. })).collect();
    assert_eq!(violations.len(), 1);
    match &violations[0] {
        ValidationIssue::ParentTypeNotSingleton { type_name, parent_type } => {
            assert_eq!(type_name, "dictum");
            assert_eq!(parent_type, "guideline");
        }
        _ => unreachable!(),
    }
}
