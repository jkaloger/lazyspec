mod common;

use common::TestFixture;
use lazyspec::engine::document::DocType;
use lazyspec::engine::store::Filter;

fn write_spec_subdirectory(fixture: &TestFixture, slug: &str, title: &str, status: &str) {
    let content = format!(
        "---\ntitle: \"{}\"\ntype: spec\nstatus: {}\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
        title, status
    );
    fixture.write_subfolder_doc(&format!("docs/specs/{}", slug), &content);
}

#[test]
fn spec_type_constant_exists() {
    assert_eq!(DocType::SPEC, "spec");
    assert_eq!(DocType::new(DocType::SPEC).as_str(), "spec");
}

#[test]
fn store_loads_spec_from_subdirectory() {
    let fixture = TestFixture::new();
    write_spec_subdirectory(&fixture, "SPEC-001-test-spec", "Test Spec", "draft");

    let store = fixture.store();
    let docs = store.all_docs();

    let spec = docs.iter().find(|d| d.doc_type == DocType::new(DocType::SPEC));
    assert!(spec.is_some(), "spec document should be loaded by the store");
    let spec = spec.unwrap();
    assert_eq!(spec.title, "Test Spec");
    assert!(spec.path.ends_with("index.md"));
}

#[test]
fn store_filters_by_spec_type() {
    let fixture = TestFixture::new();
    write_spec_subdirectory(&fixture, "SPEC-001-test-spec", "Test Spec", "draft");
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
    write_spec_subdirectory(&fixture, "SPEC-001-test-spec", "Test Spec", "draft");

    let store = fixture.store();
    let doc = store.resolve_shorthand("SPEC-001");
    assert!(doc.is_ok());
    assert_eq!(doc.unwrap().title, "Test Spec");
}

#[test]
fn spec_id_extracted_correctly() {
    let fixture = TestFixture::new();
    write_spec_subdirectory(&fixture, "SPEC-001-test-spec", "Test Spec", "draft");

    let store = fixture.store();
    let doc = store.resolve_shorthand("SPEC-001").unwrap();
    assert_eq!(doc.id, "SPEC-001");
}

#[test]
fn story_inherits_parent_spec_relations() {
    use std::path::PathBuf;
    use lazyspec::engine::document::RelationType;

    let fixture = TestFixture::new();

    // Write an RFC that the spec will reference
    fixture.write_rfc("RFC-001-some-rfc.md", "Some RFC", "accepted");

    // Write a spec index.md with a relation
    let spec_dir = "docs/specs/SPEC-001-test-spec";
    let index_content = concat!(
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
    fixture.write_subfolder_doc(spec_dir, index_content);

    // Write a story.md child with no relations of its own
    let story_content = concat!(
        "---\n",
        "title: \"Test Story\"\n",
        "type: spec\n",
        "status: draft\n",
        "author: \"test\"\n",
        "date: 2026-01-01\n",
        "tags: []\n",
        "---\n",
    );
    fixture.write_child_doc(
        &format!("{}", fixture.root().join(spec_dir).to_str().unwrap()),
        "story.md",
        story_content,
    );

    let store = fixture.store();

    let story_path = PathBuf::from(format!("{}/story.md", spec_dir));
    let index_path = PathBuf::from(format!("{}/index.md", spec_dir));
    let rfc_path = PathBuf::from("docs/rfcs/RFC-001-some-rfc.md");

    // story.md should have inherited the parent's forward link
    let story_links = store.forward_links_for(&story_path);
    assert!(
        story_links.iter().any(|(rel, target)| *rel == RelationType::Implements && *target == rfc_path),
        "story.md should inherit the 'implements' link to the RFC, got: {:?}",
        story_links
    );

    // The RFC should see story.md in its reverse links
    let rfc_links = store.reverse_links_for(&rfc_path);
    assert!(
        rfc_links.iter().any(|(rel, src)| *rel == RelationType::Implements && *src == story_path),
        "RFC reverse links should include story.md, got: {:?}",
        rfc_links
    );

    // Parent index.md should still have its own forward link
    let index_links = store.forward_links_for(&index_path);
    assert!(
        index_links.iter().any(|(rel, target)| *rel == RelationType::Implements && *target == rfc_path),
        "index.md should retain its own forward link"
    );
}
