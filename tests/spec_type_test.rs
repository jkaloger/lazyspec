mod common;

use common::TestFixture;
use lazyspec::engine::document::DocType;
use lazyspec::engine::store::Filter;

fn write_spec_flat(fixture: &TestFixture, slug: &str, title: &str, status: &str) {
    let content = format!(
        "---\ntitle: \"{}\"\ntype: spec\nstatus: {}\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
        title, status
    );
    fixture.write_doc(&format!("docs/specs/{}.md", slug), &content);
}

#[test]
fn spec_type_constant_exists() {
    assert_eq!(DocType::SPEC, "spec");
    assert_eq!(DocType::new(DocType::SPEC).as_str(), "spec");
}

#[test]
fn store_loads_spec_from_subdirectory() {
    let fixture = TestFixture::new();
    write_spec_flat(&fixture, "SPEC-001-test-spec", "Test Spec", "draft");

    let store = fixture.store();
    let docs = store.all_docs();

    let spec = docs
        .iter()
        .find(|d| d.doc_type == DocType::new(DocType::SPEC));
    assert!(
        spec.is_some(),
        "spec document should be loaded by the store"
    );
    let spec = spec.unwrap();
    assert_eq!(spec.title, "Test Spec");
    assert!(spec.path.ends_with("SPEC-001-test-spec.md"));
}

#[test]
fn store_filters_by_spec_type() {
    let fixture = TestFixture::new();
    write_spec_flat(&fixture, "SPEC-001-test-spec", "Test Spec", "draft");
    fixture.write_rfc("RFC-001-other.md", "Other Doc", "draft");

    let store = fixture.store();
    let filter = Filter {
        doc_type: Some(DocType::new(DocType::SPEC)),
        ..Default::default()
    };
    let results = store.list(&filter);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Test Spec");
}

#[test]
fn store_resolves_spec_shorthand() {
    let fixture = TestFixture::new();
    write_spec_flat(&fixture, "SPEC-001-test-spec", "Test Spec", "draft");

    let store = fixture.store();
    let doc = store.resolve_shorthand("SPEC-001");
    assert!(doc.is_ok());
    assert_eq!(doc.unwrap().title, "Test Spec");
}

#[test]
fn spec_id_extracted_correctly() {
    let fixture = TestFixture::new();
    write_spec_flat(&fixture, "SPEC-001-test-spec", "Test Spec", "draft");

    let store = fixture.store();
    let doc = store.resolve_shorthand("SPEC-001").unwrap();
    assert_eq!(doc.id, "SPEC-001");
}

#[test]
fn story_inherits_parent_spec_relations() {
    use lazyspec::engine::document::RelationType;
    use std::path::PathBuf;

    let fixture = TestFixture::new();

    // Write an RFC that the spec will reference
    fixture.write_rfc("RFC-001-some-rfc.md", "Some RFC", "accepted");

    // Write a spec flat file with a relation
    let spec_path = "docs/specs/SPEC-001-test-spec.md";
    let spec_content = concat!(
        "---\n",
        "title: \"Test Spec\"\n",
        "type: spec\n",
        "status: draft\n",
        "author: \"test\"\n",
        "date: 2026-01-01\n",
        "tags: []\n",
        "related:\n",
        "- implements: docs/rfcs/RFC-001-some-rfc.md\n",
        "---\n",
    );
    fixture.write_doc(spec_path, spec_content);

    let store = fixture.store();

    let spec_file_path = PathBuf::from(spec_path);
    let rfc_path = PathBuf::from("docs/rfcs/RFC-001-some-rfc.md");

    // Spec should have its forward link to the RFC
    let spec_links = store.forward_links_for(&spec_file_path);
    assert!(
        spec_links
            .iter()
            .any(|(rel, target)| *rel == RelationType::Implements && *target == rfc_path),
        "spec should have the 'implements' link to the RFC, got: {:?}",
        spec_links
    );

    // The RFC should see the spec in its reverse links
    let rfc_links = store.reverse_links_for(&rfc_path);
    assert!(
        rfc_links
            .iter()
            .any(|(rel, src)| *rel == RelationType::Implements && *src == spec_file_path),
        "RFC reverse links should include the spec, got: {:?}",
        rfc_links
    );
}
