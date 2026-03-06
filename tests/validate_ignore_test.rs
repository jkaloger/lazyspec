mod common;

use common::TestFixture;
use lazyspec::engine::validation::ValidationIssue;

#[test]
fn ignored_document_with_broken_link_produces_no_error() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/adrs/ADR-001-ignored.md",
        "---\ntitle: \"Ignored ADR\"\ntype: adr\nstatus: accepted\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\nvalidate-ignore: true\nrelated:\n- implements: docs/rfcs/NONEXISTENT.md\n---\n",
    );
    let store = fixture.store();
    let result = store.validate_full();

    assert!(
        !result.errors.iter().any(|e| matches!(e, ValidationIssue::BrokenLink { .. })),
        "expected no BrokenLink error for ignored document, got: {:?}",
        result.errors
    );
}

#[test]
fn ignored_story_skips_upward_orphaned_acceptance() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-feature.md",
        "---\ntitle: \"Feature\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\nrelated: []\n---\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-001-impl.md",
        "---\ntitle: \"Impl\"\ntype: story\nstatus: accepted\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\nvalidate-ignore: true\nrelated:\n- implements: docs/rfcs/RFC-001-feature.md\n---\n",
    );
    let store = fixture.store();
    let result = store.validate_full();

    assert!(
        !result.warnings.iter().any(|w| matches!(w, ValidationIssue::UpwardOrphanedAcceptance { .. })),
        "expected no UpwardOrphanedAcceptance warning for ignored story, got: {:?}",
        result.warnings
    );
}

#[test]
fn non_ignored_documents_still_report_errors() {
    let fixture = TestFixture::new();
    // Ignored doc with broken link
    fixture.write_doc(
        "docs/adrs/ADR-001-ignored.md",
        "---\ntitle: \"Ignored ADR\"\ntype: adr\nstatus: accepted\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\nvalidate-ignore: true\nrelated:\n- implements: docs/rfcs/NONEXISTENT.md\n---\n",
    );
    // Non-ignored doc with broken link
    fixture.write_doc(
        "docs/adrs/ADR-002-normal.md",
        "---\ntitle: \"Normal ADR\"\ntype: adr\nstatus: accepted\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: docs/rfcs/ALSO-NONEXISTENT.md\n---\n",
    );
    let store = fixture.store();
    let result = store.validate_full();

    assert!(
        result.errors.iter().any(|e| matches!(
            e,
            ValidationIssue::BrokenLink { source, .. }
                if source.ends_with("ADR-002-normal.md")
        )),
        "expected BrokenLink error for non-ignored document, got: {:?}",
        result.errors
    );
    assert!(
        !result.errors.iter().any(|e| matches!(
            e,
            ValidationIssue::BrokenLink { source, .. }
                if source.ends_with("ADR-001-ignored.md")
        )),
        "expected no BrokenLink error for ignored document, got: {:?}",
        result.errors
    );
}

#[test]
fn ignored_children_excluded_from_all_children_accepted_check() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-feature.md",
        "---\ntitle: \"Feature\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\nrelated: []\n---\n",
    );
    // Accepted story, ignored
    fixture.write_doc(
        "docs/stories/STORY-001-impl.md",
        "---\ntitle: \"Impl\"\ntype: story\nstatus: accepted\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\nvalidate-ignore: true\nrelated:\n- implements: docs/rfcs/RFC-001-feature.md\n---\n",
    );
    // Accepted story, not ignored
    fixture.write_story(
        "STORY-002-impl.md",
        "Impl2",
        "accepted",
        Some("docs/rfcs/RFC-001-feature.md"),
    );
    let store = fixture.store();
    let result = store.validate_full();

    // The ignored child should be excluded. Only STORY-002 remains, which is accepted,
    // so AllChildrenAccepted should fire (one non-ignored child, all accepted).
    assert!(
        result.warnings.iter().any(|w| matches!(
            w,
            ValidationIssue::AllChildrenAccepted { parent, children }
                if parent.ends_with("RFC-001-feature.md")
                && children.len() == 1
                && children[0].ends_with("STORY-002-impl.md")
        )),
        "expected AllChildrenAccepted warning with only non-ignored children, got: {:?}",
        result.warnings
    );
}
