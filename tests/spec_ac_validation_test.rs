mod common;

use common::TestFixture;

fn write_spec_with_ac(fixture: &TestFixture, slug: &str, ac_body: &str) {
    let content = format!(
        "---\ntitle: \"Test Spec\"\ntype: spec\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n{}",
        ac_body
    );
    fixture.write_doc(&format!("docs/specs/{}.md", slug), &content);
}

fn warning_messages(fixture: &TestFixture) -> Vec<String> {
    let store = fixture.store();
    let result = store.validate_full(&fixture.config());
    result.warnings.iter().map(|w| format!("{}", w)).collect()
}

#[test]
fn valid_ac_slug_passes_validation() {
    let fixture = TestFixture::new();
    write_spec_with_ac(
        &fixture,
        "SPEC-001-test",
        "### AC: valid-slug\nSome criteria\n",
    );

    let warnings = warning_messages(&fixture);
    let ac_warnings: Vec<_> = warnings.iter().filter(|w| w.contains("AC slug")).collect();
    assert!(
        ac_warnings.is_empty(),
        "valid slug should produce no warnings, got: {:?}",
        ac_warnings
    );
}

#[test]
fn empty_ac_slug_produces_warning() {
    let fixture = TestFixture::new();
    write_spec_with_ac(&fixture, "SPEC-002-test", "### AC: \nSome criteria\n");

    let warnings = warning_messages(&fixture);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("AC slug") && w.contains("empty")),
        "empty slug should produce a warning, got: {:?}",
        warnings
    );
}

#[test]
fn camel_case_ac_slug_produces_warning() {
    let fixture = TestFixture::new();
    write_spec_with_ac(
        &fixture,
        "SPEC-003-test",
        "### AC: CamelCase\nSome criteria\n",
    );

    let warnings = warning_messages(&fixture);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("AC slug") && w.contains("CamelCase")),
        "CamelCase slug should produce a warning, got: {:?}",
        warnings
    );
}

#[test]
fn duplicate_ac_slugs_produce_warning() {
    let fixture = TestFixture::new();
    write_spec_with_ac(
        &fixture,
        "SPEC-004-test",
        "### AC: same-slug\nFirst\n\n### AC: same-slug\nSecond\n",
    );

    let warnings = warning_messages(&fixture);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("AC slug") && w.contains("duplicate")),
        "duplicate slugs should produce a warning, got: {:?}",
        warnings
    );
}

#[test]
fn non_spec_subdoc_does_not_trigger_ac_validation() {
    let fixture = TestFixture::new();

    // Write a story (non-spec type) with arbitrary headings
    let content = "---\ntitle: \"A Story\"\ntype: story\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n### AC: BadSlug!!!\nSome content\n";
    fixture.write_doc("docs/stories/STORY-001-test.md", content);

    let warnings = warning_messages(&fixture);
    let ac_warnings: Vec<_> = warnings.iter().filter(|w| w.contains("AC slug")).collect();
    assert!(
        ac_warnings.is_empty(),
        "non-spec doc should not trigger AC validation, got: {:?}",
        ac_warnings
    );
}

#[test]
fn spec_flat_file_triggers_ac_validation() {
    let fixture = TestFixture::new();

    // Write a spec flat file with AC headings - should now be validated
    let content = "---\ntitle: \"Test Spec\"\ntype: spec\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n### AC: BadSlug!!!\n";
    fixture.write_doc("docs/specs/SPEC-005-test.md", content);

    let warnings = warning_messages(&fixture);
    let ac_warnings: Vec<_> = warnings.iter().filter(|w| w.contains("AC slug")).collect();
    assert!(
        !ac_warnings.is_empty(),
        "spec flat file should trigger AC validation for bad slugs, got: {:?}",
        warnings
    );
}

#[test]
fn multiple_valid_ac_slugs_pass() {
    let fixture = TestFixture::new();
    write_spec_with_ac(
        &fixture,
        "SPEC-006-test",
        "### AC: first-slug\nCriteria 1\n\n### AC: second-slug\nCriteria 2\n\n### AC: third-slug\nCriteria 3\n",
    );

    let warnings = warning_messages(&fixture);
    let ac_warnings: Vec<_> = warnings.iter().filter(|w| w.contains("AC slug")).collect();
    assert!(
        ac_warnings.is_empty(),
        "all valid slugs should pass, got: {:?}",
        ac_warnings
    );
}

#[test]
fn slug_with_numbers_passes() {
    let fixture = TestFixture::new();
    write_spec_with_ac(
        &fixture,
        "SPEC-007-test",
        "### AC: step-2-verify\nCriteria\n",
    );

    let warnings = warning_messages(&fixture);
    let ac_warnings: Vec<_> = warnings.iter().filter(|w| w.contains("AC slug")).collect();
    assert!(
        ac_warnings.is_empty(),
        "slug with numbers should pass, got: {:?}",
        ac_warnings
    );
}

#[test]
fn slug_with_underscores_produces_warning() {
    let fixture = TestFixture::new();
    write_spec_with_ac(
        &fixture,
        "SPEC-008-test",
        "### AC: uses_underscores\nCriteria\n",
    );

    let warnings = warning_messages(&fixture);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("AC slug") && w.contains("uses_underscores")),
        "underscore slug should produce a warning, got: {:?}",
        warnings
    );
}
