use crate::common::TestFixture;
use lazyspec::engine::config::{
    starter_hierarchy_edges, Config, EdgeDef, RelSelector, Severity, Traversal, TypeSelector,
};
use lazyspec::engine::validation::ValidationIssue;

/// The starter config with the chain stated in `[[edges]]`. The two findings
/// that iterate `(parent type, child type)` pairs -- `AllChildrenAccepted` and
/// `UpwardOrphanedAcceptance` -- enumerate the child types declared beneath a
/// parent type, and only a row naming concrete endpoints declares one: a global
/// traversal marker names a relationship and no types at all (ITERATION-380).
fn config_with_hierarchy_edges() -> Config {
    Config {
        edges: starter_hierarchy_edges(),
        ..Config::default()
    }
}

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

fn setup_with_two_stories(
    rfc_status: &str,
    story1_status: &str,
    story2_status: &str,
) -> TestFixture {
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

    assert!(result
        .warnings
        .iter()
        .any(|w| matches!(w, ValidationIssue::SupersededParent { .. })));
    assert!(result.errors.is_empty());
}

#[test]
fn rejected_parent_error() {
    let fixture = setup_with_chain("rejected", "draft", "draft");
    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    assert!(result
        .errors
        .iter()
        .any(|e| matches!(e, ValidationIssue::RejectedParent { .. })));
}

#[test]
fn orphaned_acceptance_warning() {
    let fixture = setup_with_chain("accepted", "draft", "accepted");
    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    assert!(result
        .warnings
        .iter()
        .any(|w| matches!(w, ValidationIssue::OrphanedAcceptance { .. })));
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
    let output = lazyspec::cli::validate::run_json(&store, &fixture.config(), &[]);
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert!(parsed["errors"].is_array());
    assert!(parsed["warnings"].is_array());
    assert!(!parsed["warnings"].as_array().unwrap().is_empty());
}

#[test]
fn validate_without_warnings_flag_hides_warnings() {
    let fixture = setup_with_chain("superseded", "accepted", "accepted");
    let store = fixture.store();
    let output = lazyspec::cli::validate::run_human(&store, &fixture.config(), false, &[]);

    assert!(!output.contains("superseded"));
}

#[test]
fn validate_with_warnings_flag_shows_warnings() {
    let fixture = setup_with_chain("superseded", "accepted", "accepted");
    let store = fixture.store();
    let output = lazyspec::cli::validate::run_human(&store, &fixture.config(), true, &[]);

    assert!(output.contains("superseded"));
}

#[test]
fn all_stories_accepted_warns_draft_rfc() {
    let fixture = setup_with_chain("draft", "accepted", "accepted");
    let config = config_with_hierarchy_edges();
    let store = fixture.store_with(&config);
    let result = store.validate_full(&config);

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
    let config = config_with_hierarchy_edges();
    let store = fixture.store_with(&config);
    let result = store.validate_full(&config);

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
    let config = config_with_hierarchy_edges();
    let store = fixture.store_with(&config);
    let result = store.validate_full(&config);

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
    let config = config_with_hierarchy_edges();
    let store = fixture.store_with(&config);
    let result = store.validate_full(&config);

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
    let config = config_with_hierarchy_edges();
    let store = fixture.store_with(&config);
    let output = lazyspec::cli::validate::run_json(&store, &config, &[]);
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    let warnings = parsed["warnings"]
        .as_array()
        .expect("warnings should be an array");
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

// --- Custom constraint tests ---
//
// Each of these declared its constraint as a `[[rules]]` block until
// STORY-259 made that shape unloadable. The constraint is the same; only its
// spelling changed, so each is now the `[[edges]]` row the migration
// translates its rule to (ADR-032) and each asserts the same finding under the
// same name.

fn config_with_edges(edges: Vec<EdgeDef>) -> Config {
    Config {
        edges,
        ..Config::default()
    }
}

/// The row a `parent-child` rule translates to: from the child type, to the
/// parent type, via the relationship the config marks chain.
fn parent_child_row(name: &str, child: &str, parent: &str, severity: Severity) -> EdgeDef {
    EdgeDef {
        name: name.to_string(),
        from: TypeSelector::Types(vec![child.to_string()]),
        to: TypeSelector::Types(vec![parent.to_string()]),
        via: RelSelector::Named(vec!["implements".to_string()]),
        required: Some(severity),
        traversal: Some(Traversal::Chain),
    }
}

/// The row a `relation-existence` rule translates to: a relationship of any
/// kind, to a document of any type (RFC-067 §Design).
fn relation_existence_row(name: &str, doc_type: &str, severity: Severity) -> EdgeDef {
    EdgeDef {
        name: name.to_string(),
        from: TypeSelector::Types(vec![doc_type.to_string()]),
        to: TypeSelector::Any,
        via: RelSelector::Any,
        required: Some(severity),
        traversal: None,
    }
}

#[test]
fn custom_parent_child_row_fires_when_story_lacks_rfc_link() {
    let fixture = TestFixture::new();
    fixture.write_story("STORY-001.md", "Orphan Story", "draft", None);

    let config = config_with_edges(vec![parent_child_row(
        "stories-must-implement-rfcs",
        "story",
        "rfc",
        Severity::Error,
    )]);

    let result = fixture.store_with(&config).validate_full(&config);

    assert!(
        result.errors.iter().any(|e| matches!(
            e,
            ValidationIssue::UnsatisfiedEdge { edge_name, from_type, to, .. }
                if edge_name == "stories-must-implement-rfcs"
                && from_type == "story"
                && to.names() == ["rfc"]
        )),
        "expected an unsatisfied-edge error for story without RFC, got: {:?}",
        result.errors
    );
}

#[test]
fn custom_relation_existence_row_fires_for_type_with_no_relations() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001.md", "Lonely RFC", "draft");

    let config = config_with_edges(vec![relation_existence_row(
        "rfcs-need-relations",
        "rfc",
        Severity::Error,
    )]);

    let result = fixture.store_with(&config).validate_full(&config);

    assert!(
        result.errors.iter().any(|e| matches!(
            e,
            ValidationIssue::UnsatisfiedEdge { edge_name, from_type, .. }
                if edge_name == "rfcs-need-relations" && from_type == "rfc"
        )),
        "expected an unsatisfied-edge error for RFC without relations, got: {:?}",
        result.errors
    );
}

#[test]
fn a_row_required_at_warning_severity_produces_warning_not_error() {
    let fixture = TestFixture::new();
    fixture.write_story("STORY-001.md", "Orphan Story", "draft", None);

    let config = config_with_edges(vec![parent_child_row(
        "soft-story-check",
        "story",
        "rfc",
        Severity::Warning,
    )]);

    let result = fixture.store_with(&config).validate_full(&config);

    assert!(
        result.warnings.iter().any(|w| matches!(
            w,
            ValidationIssue::UnsatisfiedEdge { edge_name, .. }
                if edge_name == "soft-story-check"
        )),
        "expected an unsatisfied-edge warning, got warnings: {:?}",
        result.warnings
    );
    assert!(
        !result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationIssue::UnsatisfiedEdge { .. })),
        "expected no unsatisfied-edge errors when required is warning, got: {:?}",
        result.errors
    );
}

#[test]
fn declared_rows_are_the_only_demands_so_the_standard_ones_do_not_fire() {
    let fixture = TestFixture::new();
    // Without a story link, `iterations-need-stories` would fail this document.
    fixture.write_iteration("ITERATION-001.md", "Orphan", "draft", None);
    // Without relations, `adrs-need-relations` would fail this one.
    fixture.write_adr("ADR-001.md", "Orphan ADR", "draft", None);

    // One unrelated row, and no standard set behind it.
    let config = config_with_edges(vec![relation_existence_row(
        "rfcs-need-relations",
        "rfc",
        Severity::Error,
    )]);

    let result = fixture.store_with(&config).validate_full(&config);

    assert!(
        !result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationIssue::UnsatisfiedEdge { .. })),
        "a config declares its whole DAG, so no unnamed demand may fire, got: {:?}",
        result.errors
    );
}

/// A config whose only declaration of hierarchy is one row: `story
/// -implements-> rfc`, with no `[[rules]]` block behind it. The row states a
/// traversal for `implements`, which suppresses the starter global marker
/// (ADR-035), so the row is the whole story.
fn config_with_one_chain_row() -> Config {
    Config {
        edges: vec![EdgeDef {
            name: "stories-implement-rfcs".to_string(),
            from: TypeSelector::Types(vec!["story".to_string()]),
            to: TypeSelector::Types(vec!["rfc".to_string()]),
            via: RelSelector::Named(vec!["implements".to_string()]),
            required: None,
            traversal: Some(Traversal::Chain),
        }],
        ..Config::default()
    }
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

    let config = config_with_one_chain_row();
    let store = fixture.store_with(&config);
    let result = store.validate_full(&config);

    // RejectedParent should fire from status-based check inferred from custom hierarchy
    assert!(
        result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationIssue::RejectedParent { .. })),
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

    // Only the custom row, no rules and no global marker in play
    let config = config_with_one_chain_row();
    let store = fixture.store_with(&config);
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
