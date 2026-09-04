use crate::common::TestFixture;
use lazyspec::engine::validation::ValidationIssue;

#[test]
fn ignored_document_with_broken_link_produces_no_error() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/adrs/ADR-001-ignored.md",
        "---\ntitle: \"Ignored ADR\"\ntype: adr\nstatus: accepted\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\nvalidate-ignore: true\nrelated:\n- implements: docs/rfcs/NONEXISTENT.md\n---\n",
    );
    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    assert!(
        !result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationIssue::BrokenLink { .. })),
        "expected no BrokenLink error for ignored document, got: {:?}",
        result.errors
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
    let result = store.validate_full(&fixture.config());

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
