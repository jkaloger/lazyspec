use lazyspec::engine::config::Config;
use lazyspec::engine::document::{DocType, Status};
use lazyspec::engine::store::{Filter, Store};
use std::fs;
use tempfile::TempDir;

fn setup_test_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("docs/rfcs")).unwrap();
    fs::create_dir_all(root.join("docs/adrs")).unwrap();
    fs::create_dir_all(root.join("docs/specs")).unwrap();
    fs::create_dir_all(root.join("docs/plans")).unwrap();

    fs::write(
        root.join("docs/rfcs/RFC-001-event-sourcing.md"),
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
    )
    .unwrap();

    fs::write(
        root.join("docs/adrs/ADR-001-adopt-es.md"),
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
    )
    .unwrap();

    dir
}

#[test]
fn store_loads_all_docs() {
    let dir = setup_test_dir();
    let config = Config::default();
    let store = Store::load(dir.path(), &config).unwrap();
    assert_eq!(store.all_docs().len(), 2);
}

#[test]
fn store_filters_by_type() {
    let dir = setup_test_dir();
    let config = Config::default();
    let store = Store::load(dir.path(), &config).unwrap();

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
    let dir = setup_test_dir();
    let config = Config::default();
    let store = Store::load(dir.path(), &config).unwrap();

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
    let dir = setup_test_dir();
    let config = Config::default();
    let store = Store::load(dir.path(), &config).unwrap();

    let docs = store.all_docs();
    let rfc = docs.iter().find(|d| d.doc_type == DocType::Rfc).unwrap();
    let body = store.get_body(&rfc.path).unwrap();
    assert!(body.contains("Event sourcing proposal."));
}

#[test]
fn store_resolves_related_docs() {
    let dir = setup_test_dir();
    let config = Config::default();
    let store = Store::load(dir.path(), &config).unwrap();

    let docs = store.all_docs();
    let adr = docs.iter().find(|d| d.doc_type == DocType::Adr).unwrap();
    let related = store.related_to(&adr.path);
    assert_eq!(related.len(), 1);
}

#[test]
fn store_resolves_shorthand_id() {
    let dir = setup_test_dir();
    let config = Config::default();
    let store = Store::load(dir.path(), &config).unwrap();

    let doc = store.resolve_shorthand("RFC-001");
    assert!(doc.is_some());
    assert_eq!(doc.unwrap().title, "Event Sourcing");
}

#[test]
fn store_filters_by_tag() {
    let dir = setup_test_dir();
    let config = Config::default();
    let store = Store::load(dir.path(), &config).unwrap();

    let filter = Filter {
        tag: Some("events".to_string()),
        ..Default::default()
    };
    let results = store.list(&filter);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Adopt Event Sourcing");
}
