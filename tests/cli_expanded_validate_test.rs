mod common;

use common::TestFixture;
use lazyspec::engine::config::{Config, Severity, ValidationRule};
use lazyspec::engine::validation::ValidationIssue;

fn setup_with_chain(rfc_status: &str, story_status: &str, iter_status: &str) -> TestFixture {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-feature.md",
        &format!(
            "---\ntitle: \"Feature\"\ntype: rfc\nstatus: {}\nauthor: a\ndate: 2026-01-01\ntags: []\nrelated: []\n---\n",
            rfc_status
        ),
    );
    fixture.write_story(
        "STORY-001-impl.md",
        "Impl",
        story_status,
        Some("docs/rfcs/RFC-001-feature.md"),
    );
    fixture.write_iteration(
        "ITERATION-001-sprint.md",
        "Sprint",
        iter_status,
        Some("docs/stories/STORY-001-impl.md"),
    );
    fixture
}

fn setup_with_two_stories(rfc_status: &str, story1_status: &str, story2_status: &str) -> TestFixture {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-feature.md",
        &format!(
            "---\ntitle: \"Feature\"\ntype: rfc\nstatus: {}\nauthor: a\ndate: 2026-01-01\ntags: []\nrelated: []\n---\n",
            rfc_status
        ),
    );
    fixture.write_story(
        "STORY-001-impl.md",
        "Impl",
        story1_status,
        Some("docs/rfcs/RFC-001-feature.md"),
    );
    fixture.write_story(
        "STORY-002-impl.md",
        "Impl2",
        story2_status,
        Some("docs/rfcs/RFC-001-feature.md"),
    );
    fixture
}

#[test]
fn superseded_parent_warning() {
    let fixture = setup_with_chain("superseded", "accepted", "accepted");
    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    assert!(result.warnings.iter().any(|w| matches!(w, ValidationIssue::SupersededParent { .. })));
    assert!(result.errors.is_empty());
}

#[test]
fn rejected_parent_error() {
    let fixture = setup_with_chain("rejected", "draft", "draft");
    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    assert!(result.errors.iter().any(|e| matches!(e, ValidationIssue::RejectedParent { .. })));
}

#[test]
fn orphaned_acceptance_warning() {
    let fixture = setup_with_chain("accepted", "draft", "accepted");
    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    assert!(result.warnings.iter().any(|w| matches!(w, ValidationIssue::OrphanedAcceptance { .. })));
}

#[test]
fn warnings_dont_affect_exit_code() {
    let fixture = setup_with_chain("superseded", "accepted", "accepted");
    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    assert!(!result.warnings.is_empty());
    assert!(result.errors.is_empty());
    // Exit code should be 0 when only warnings
}

#[test]
fn validate_json_has_separate_arrays() {
    let fixture = setup_with_chain("superseded", "accepted", "accepted");
    let store = fixture.store();
    let output = lazyspec::cli::validate::run_json(&store, &fixture.config());
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert!(parsed["errors"].is_array());
    assert!(parsed["warnings"].is_array());
    assert!(!parsed["warnings"].as_array().unwrap().is_empty());
}

#[test]
fn validate_without_warnings_flag_hides_warnings() {
    let fixture = setup_with_chain("superseded", "accepted", "accepted");
    let store = fixture.store();
    let output = lazyspec::cli::validate::run_human(&store, &fixture.config(), false);

    assert!(!output.contains("superseded"));
}

#[test]
fn validate_with_warnings_flag_shows_warnings() {
    let fixture = setup_with_chain("superseded", "accepted", "accepted");
    let store = fixture.store();
    let output = lazyspec::cli::validate::run_human(&store, &fixture.config(), true);

    assert!(output.contains("superseded"));
}

#[test]
fn all_stories_accepted_warns_draft_rfc() {
    let fixture = setup_with_chain("draft", "accepted", "accepted");
    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    assert!(
        result.warnings.iter().any(|w| matches!(
            w,
            ValidationIssue::AllChildrenAccepted { parent, .. }
                if parent.ends_with("RFC-001-feature.md")
        )),
        "expected AllChildrenAccepted warning with RFC as parent, got: {:?}",
        result.warnings
    );
}

#[test]
fn all_iterations_accepted_warns_draft_story() {
    let fixture = setup_with_chain("accepted", "draft", "accepted");
    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    assert!(
        result.warnings.iter().any(|w| matches!(
            w,
            ValidationIssue::AllChildrenAccepted { parent, .. }
                if parent.ends_with("STORY-001-impl.md")
        )),
        "expected AllChildrenAccepted warning with Story as parent, got: {:?}",
        result.warnings
    );
}

#[test]
fn partial_children_no_all_accepted_warning() {
    let fixture = setup_with_two_stories("draft", "accepted", "draft");
    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    assert!(
        !result.warnings.iter().any(|w| matches!(
            w,
            ValidationIssue::AllChildrenAccepted { parent, .. }
                if parent.ends_with("RFC-001-feature.md")
        )),
        "expected no AllChildrenAccepted warning for RFC, got: {:?}",
        result.warnings
    );
}

#[test]
fn accepted_story_draft_rfc_orphaned() {
    let fixture = setup_with_two_stories("draft", "accepted", "draft");
    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    assert!(
        result.warnings.iter().any(|w| matches!(
            w,
            ValidationIssue::UpwardOrphanedAcceptance { path, parent }
                if path.ends_with("STORY-001-impl.md") && parent.ends_with("RFC-001-feature.md")
        )),
        "expected UpwardOrphanedAcceptance for accepted story with draft RFC parent, got: {:?}",
        result.warnings
    );
}

#[test]
fn all_children_accepted_json_output() {
    let fixture = setup_with_chain("draft", "accepted", "accepted");
    let store = fixture.store();
    let output = lazyspec::cli::validate::run_json(&store, &fixture.config());
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    let warnings = parsed["warnings"].as_array().expect("warnings should be an array");
    assert!(
        warnings.iter().any(|w| {
            w.as_str()
                .map(|s| s.contains("all children accepted"))
                .unwrap_or(false)
        }),
        "expected JSON warnings to contain 'all children accepted', got: {:?}",
        warnings
    );
}

// --- Custom rule tests ---

fn config_with_rules(rules: Vec<ValidationRule>) -> Config {
    let mut config = Config::default();
    config.rules = rules;
    config
}

#[test]
fn custom_parent_child_rule_fires_when_story_lacks_rfc_link() {
    let fixture = TestFixture::new();
    fixture.write_story("STORY-001.md", "Orphan Story", "draft", None);

    let config = config_with_rules(vec![ValidationRule::ParentChild {
        name: "stories-must-implement-rfcs".to_string(),
        child: "story".to_string(),
        parent: "rfc".to_string(),
        link: "implements".to_string(),
        severity: Severity::Error,
    }]);

    let store = fixture.store();
    let result = store.validate_full(&config);

    assert!(
        result.errors.iter().any(|e| matches!(
            e,
            ValidationIssue::MissingParentLink { rule_name, child_type, parent_type, .. }
                if rule_name == "stories-must-implement-rfcs"
                && child_type == "story"
                && parent_type == "rfc"
        )),
        "expected MissingParentLink error for story without RFC, got: {:?}",
        result.errors
    );
}

#[test]
fn custom_relation_existence_rule_fires_for_type_with_no_relations() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001.md", "Lonely RFC", "draft");

    let config = config_with_rules(vec![ValidationRule::RelationExistence {
        name: "rfcs-need-relations".to_string(),
        doc_type: "rfc".to_string(),
        require: "any-relation".to_string(),
        severity: Severity::Error,
    }]);

    let store = fixture.store();
    let result = store.validate_full(&config);

    assert!(
        result.errors.iter().any(|e| matches!(
            e,
            ValidationIssue::MissingRelation { rule_name, doc_type, .. }
                if rule_name == "rfcs-need-relations"
                && doc_type == "rfc"
        )),
        "expected MissingRelation error for RFC without relations, got: {:?}",
        result.errors
    );
}

#[test]
fn custom_rule_with_warning_severity_produces_warning_not_error() {
    let fixture = TestFixture::new();
    fixture.write_story("STORY-001.md", "Orphan Story", "draft", None);

    let config = config_with_rules(vec![ValidationRule::ParentChild {
        name: "soft-story-check".to_string(),
        child: "story".to_string(),
        parent: "rfc".to_string(),
        link: "implements".to_string(),
        severity: Severity::Warning,
    }]);

    let store = fixture.store();
    let result = store.validate_full(&config);

    assert!(
        result.warnings.iter().any(|w| matches!(
            w,
            ValidationIssue::MissingParentLink { rule_name, .. }
                if rule_name == "soft-story-check"
        )),
        "expected MissingParentLink warning, got warnings: {:?}",
        result.warnings
    );
    assert!(
        !result.errors.iter().any(|e| matches!(e, ValidationIssue::MissingParentLink { .. })),
        "expected no MissingParentLink errors when severity is warning, got: {:?}",
        result.errors
    );
}

#[test]
fn custom_rules_replace_defaults_so_default_checks_do_not_fire() {
    let fixture = TestFixture::new();
    // Iteration without a story link would fail with default rules
    fixture.write_iteration("ITERATION-001.md", "Orphan", "draft", None);
    // ADR without relations would fail with default rules
    fixture.write_adr("ADR-001.md", "Orphan ADR", "draft", None);

    // Only define an unrelated rule
    let config = config_with_rules(vec![ValidationRule::RelationExistence {
        name: "rfcs-need-relations".to_string(),
        doc_type: "rfc".to_string(),
        require: "any-relation".to_string(),
        severity: Severity::Error,
    }]);

    let store = fixture.store();
    let result = store.validate_full(&config);

    assert!(
        !result.errors.iter().any(|e| matches!(e, ValidationIssue::MissingParentLink { .. })),
        "expected no MissingParentLink since default iteration rule was replaced, got: {:?}",
        result.errors
    );
    assert!(
        !result.errors.iter().any(|e| matches!(
            e,
            ValidationIssue::MissingRelation { doc_type, .. }
                if doc_type == "adr"
        )),
        "expected no MissingRelation for ADR since default rule was replaced, got: {:?}",
        result.errors
    );
}

#[test]
fn status_based_checks_work_with_custom_hierarchy() {
    let fixture = TestFixture::new();
    // Set up a chain: RFC (rejected) <- Story (implements RFC)
    // With a custom rule defining story->rfc hierarchy
    fixture.write_doc(
        "docs/rfcs/RFC-001.md",
        "---\ntitle: \"Rejected\"\ntype: rfc\nstatus: rejected\nauthor: a\ndate: 2026-01-01\ntags: []\nrelated: []\n---\n",
    );
    fixture.write_story(
        "STORY-001.md",
        "Impl",
        "draft",
        Some("docs/rfcs/RFC-001.md"),
    );

    let config = config_with_rules(vec![ValidationRule::ParentChild {
        name: "stories-need-rfcs".to_string(),
        child: "story".to_string(),
        parent: "rfc".to_string(),
        link: "implements".to_string(),
        severity: Severity::Warning,
    }]);

    let store = fixture.store();
    let result = store.validate_full(&config);

    // RejectedParent should fire from status-based check inferred from custom hierarchy
    assert!(
        result.errors.iter().any(|e| matches!(e, ValidationIssue::RejectedParent { .. })),
        "expected RejectedParent error from custom hierarchy, got errors: {:?}, warnings: {:?}",
        result.errors,
        result.warnings
    );
}

#[test]
fn all_children_accepted_fires_with_custom_hierarchy() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001.md",
        "---\ntitle: \"Feature\"\ntype: rfc\nstatus: draft\nauthor: a\ndate: 2026-01-01\ntags: []\nrelated: []\n---\n",
    );
    fixture.write_story(
        "STORY-001.md",
        "Impl",
        "accepted",
        Some("docs/rfcs/RFC-001.md"),
    );

    // Only custom rule, no defaults
    let config = config_with_rules(vec![ValidationRule::ParentChild {
        name: "stories-need-rfcs".to_string(),
        child: "story".to_string(),
        parent: "rfc".to_string(),
        link: "implements".to_string(),
        severity: Severity::Warning,
    }]);

    let store = fixture.store();
    let result = store.validate_full(&config);

    assert!(
        result.warnings.iter().any(|w| matches!(
            w,
            ValidationIssue::AllChildrenAccepted { parent, .. }
                if parent.ends_with("RFC-001.md")
        )),
        "expected AllChildrenAccepted warning from custom hierarchy, got: {:?}",
        result.warnings
    );
}
