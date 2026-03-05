mod common;

use common::TestFixture;
use lazyspec::engine::document::{DocType, Status};
use lazyspec::engine::store::Filter;

fn setup_fixture() -> TestFixture {
    let fixture = TestFixture::new();

    fixture.write_doc(
        "docs/rfcs/RFC-001-event-sourcing.md",
        r#"---
title: "Event Sourcing"
type: rfc
status: accepted
author: jkaloger
date: 2026-03-01
tags: [architecture]
---

## Summary
Event sourcing proposal.
"#,
    );

    fixture.write_doc(
        "docs/adrs/ADR-001-adopt-es.md",
        r#"---
title: "Adopt Event Sourcing"
type: adr
status: draft
author: jkaloger
date: 2026-03-04
tags: [architecture, events]
related:
  - implements: docs/rfcs/RFC-001-event-sourcing.md
---

## Decision
We adopt event sourcing.
"#,
    );

    fixture
}

#[test]
fn store_loads_all_docs() {
    let fixture = setup_fixture();
    let store = fixture.store();
    assert_eq!(store.all_docs().len(), 2);
}

#[test]
fn store_filters_by_type() {
    let fixture = setup_fixture();
    let store = fixture.store();

    let filter = Filter {
        doc_type: Some(DocType::Rfc),
        ..Default::default()
    };
    let results = store.list(&filter);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Event Sourcing");
}

#[test]
fn store_filters_by_status() {
    let fixture = setup_fixture();
    let store = fixture.store();

    let filter = Filter {
        status: Some(Status::Draft),
        ..Default::default()
    };
    let results = store.list(&filter);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Adopt Event Sourcing");
}

#[test]
fn store_gets_body_lazily() {
    let fixture = setup_fixture();
    let store = fixture.store();

    let docs = store.all_docs();
    let rfc = docs.iter().find(|d| d.doc_type == DocType::Rfc).unwrap();
    let body = store.get_body(&rfc.path).unwrap();
    assert!(body.contains("Event sourcing proposal."));
}

#[test]
fn store_resolves_related_docs() {
    let fixture = setup_fixture();
    let store = fixture.store();

    let docs = store.all_docs();
    let adr = docs.iter().find(|d| d.doc_type == DocType::Adr).unwrap();
    let related = store.related_to(&adr.path);
    assert_eq!(related.len(), 1);
}

#[test]
fn store_resolves_shorthand_id() {
    let fixture = setup_fixture();
    let store = fixture.store();

    let doc = store.resolve_shorthand("RFC-001");
    assert!(doc.is_some());
    assert_eq!(doc.unwrap().title, "Event Sourcing");
}

#[test]
fn store_filters_by_tag() {
    let fixture = setup_fixture();
    let store = fixture.store();

    let filter = Filter {
        tag: Some("events".to_string()),
        ..Default::default()
    };
    let results = store.list(&filter);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Adopt Event Sourcing");
}

#[test]
fn store_search_matches_title() {
    let fixture = setup_fixture();
    let store = fixture.store();

    let results = store.search("Event");
    assert_eq!(results.len(), 2);
    assert!(results.iter().any(|r| r.doc.title == "Event Sourcing"));
    assert!(results.iter().any(|r| r.doc.title == "Adopt Event Sourcing"));
}

#[test]
fn store_search_matches_body() {
    let fixture = setup_fixture();
    let store = fixture.store();

    let results = store.search("proposal");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].doc.title, "Event Sourcing");
}

#[test]
fn store_search_matches_tags() {
    let fixture = setup_fixture();
    let store = fixture.store();

    let results = store.search("events");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].doc.title, "Adopt Event Sourcing");
}

#[test]
fn store_search_is_case_insensitive() {
    let fixture = setup_fixture();
    let store = fixture.store();

    let results = store.search("event sourcing");
    assert!(!results.is_empty());
}

#[test]
fn store_search_no_results() {
    let fixture = setup_fixture();
    let store = fixture.store();

    let results = store.search("nonexistent_xyz");
    assert!(results.is_empty());
}
