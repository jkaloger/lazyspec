mod common;

use common::TestFixture;
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
    let result = store.validate_full();

    assert!(result.warnings.iter().any(|w| matches!(w, ValidationIssue::SupersededParent { .. })));
    assert!(result.errors.is_empty());
}

#[test]
fn rejected_parent_error() {
    let fixture = setup_with_chain("rejected", "draft", "draft");
    let store = fixture.store();
    let result = store.validate_full();

    assert!(result.errors.iter().any(|e| matches!(e, ValidationIssue::RejectedParent { .. })));
}

#[test]
fn orphaned_acceptance_warning() {
    let fixture = setup_with_chain("accepted", "draft", "accepted");
    let store = fixture.store();
    let result = store.validate_full();

    assert!(result.warnings.iter().any(|w| matches!(w, ValidationIssue::OrphanedAcceptance { .. })));
}

#[test]
fn warnings_dont_affect_exit_code() {
    let fixture = setup_with_chain("superseded", "accepted", "accepted");
    let store = fixture.store();
    let result = store.validate_full();

    assert!(!result.warnings.is_empty());
    assert!(result.errors.is_empty());
    // Exit code should be 0 when only warnings
}

#[test]
fn validate_json_has_separate_arrays() {
    let fixture = setup_with_chain("superseded", "accepted", "accepted");
    let store = fixture.store();
    let output = lazyspec::cli::validate::run_json(&store);
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert!(parsed["errors"].is_array());
    assert!(parsed["warnings"].is_array());
    assert!(!parsed["warnings"].as_array().unwrap().is_empty());
}

#[test]
fn validate_without_warnings_flag_hides_warnings() {
    let fixture = setup_with_chain("superseded", "accepted", "accepted");
    let store = fixture.store();
    let output = lazyspec::cli::validate::run_human(&store, false);

    assert!(!output.contains("superseded"));
}

#[test]
fn validate_with_warnings_flag_shows_warnings() {
    let fixture = setup_with_chain("superseded", "accepted", "accepted");
    let store = fixture.store();
    let output = lazyspec::cli::validate::run_human(&store, true);

    assert!(output.contains("superseded"));
}

#[test]
fn all_stories_accepted_warns_draft_rfc() {
    let fixture = setup_with_chain("draft", "accepted", "accepted");
    let store = fixture.store();
    let result = store.validate_full();

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
    let result = store.validate_full();

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
    let result = store.validate_full();

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
    let result = store.validate_full();

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
    let output = lazyspec::cli::validate::run_json(&store);
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
