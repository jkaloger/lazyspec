mod common;

use lazyspec::engine::document::DocType;
use lazyspec::engine::store::Filter;

fn setup() -> (common::TestFixture, lazyspec::engine::store::Store) {
    let fixture = common::TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-auth.md",
        "---\ntitle: \"Auth Redesign\"\ntype: rfc\nstatus: review\nauthor: jkaloger\ndate: 2026-03-01\ntags: [security, auth]\n---\n\nAuth body.\n",
    );
    fixture.write_doc(
        "docs/rfcs/RFC-002-api.md",
        "---\ntitle: \"API Versioning\"\ntype: rfc\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-02\ntags: [api]\n---\n\nAPI body.\n",
    );
    let store = fixture.store();
    (fixture, store)
}

#[test]
fn list_all_rfcs() {
    let (_fixture, store) = setup();
    let filter = Filter {
        doc_type: Some(DocType::new(DocType::RFC)),
        ..Default::default()
    };
    let results = store.list(&filter);
    assert_eq!(results.len(), 2);
}

#[test]
fn filter_by_tag() {
    let (_fixture, store) = setup();
    let filter = Filter {
        tag: Some("security".to_string()),
        ..Default::default()
    };
    let results = store.list(&filter);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Auth Redesign");
}

#[test]
fn resolve_shorthand_id() {
    let (_fixture, store) = setup();
    let doc = store.resolve_shorthand("RFC-001");
    assert!(doc.is_ok());
    assert_eq!(doc.unwrap().title, "Auth Redesign");
}
