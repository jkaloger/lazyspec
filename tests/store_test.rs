mod common;

use common::TestFixture;
use lazyspec::engine::document::{DocType, Status};
use lazyspec::engine::store::Filter;
use std::path::PathBuf;

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

#[test]
fn store_discovers_subfolder_index_md() {
    let fixture = TestFixture::new();
    fixture.write_subfolder_doc(
        "docs/rfcs/RFC-002-folder-feature",
        r#"---
title: "Folder Feature"
type: rfc
status: draft
author: "test"
date: 2026-01-01
tags: []
---
"#,
    );

    let store = fixture.store();
    let docs = store.all_docs();
    assert_eq!(docs.len(), 1);
    assert!(docs[0].path.ends_with("index.md"));
}

#[test]
fn store_discovers_both_flat_and_subfolder_docs() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-flat.md", "Flat RFC", "draft");
    fixture.write_subfolder_doc(
        "docs/rfcs/RFC-002-folder-feature",
        r#"---
title: "Folder Feature"
type: rfc
status: draft
author: "test"
date: 2026-01-01
tags: []
---
"#,
    );

    let store = fixture.store();
    let docs = store.all_docs();
    assert_eq!(docs.len(), 2);

    let titles: Vec<&str> = docs.iter().map(|d| d.title.as_str()).collect();
    assert!(titles.contains(&"Flat RFC"));
    assert!(titles.contains(&"Folder Feature"));
}

#[test]
fn store_ignores_subfolder_without_index_md() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-flat.md", "Flat RFC", "draft");

    let empty_dir = fixture.root().join("docs/rfcs/RFC-002-no-index");
    std::fs::create_dir_all(&empty_dir).unwrap();

    let store = fixture.store();
    let docs = store.all_docs();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].title, "Flat RFC");
}

#[test]
fn store_resolves_shorthand_for_subfolder_doc() {
    let fixture = TestFixture::new();
    fixture.write_subfolder_doc(
        "docs/rfcs/RFC-002-folder-feature",
        r#"---
title: "Folder Feature"
type: rfc
status: draft
author: "test"
date: 2026-01-01
tags: []
---
"#,
    );

    let store = fixture.store();
    let doc = store.resolve_shorthand("RFC-002");
    assert!(doc.is_some());
    assert_eq!(doc.unwrap().title, "Folder Feature");
}

#[test]
fn store_subfolder_doc_relationships_resolve() {
    let fixture = TestFixture::new();
    fixture.write_subfolder_doc(
        "docs/rfcs/RFC-002-folder-feature",
        r#"---
title: "Folder Feature"
type: rfc
status: draft
author: "test"
date: 2026-01-01
tags: []
---
"#,
    );
    fixture.write_story(
        "STORY-001-impl.md",
        "Implement Folder Feature",
        "draft",
        Some("docs/rfcs/RFC-002-folder-feature/index.md"),
    );

    let store = fixture.store();
    let docs = store.all_docs();
    let story = docs
        .iter()
        .find(|d| d.doc_type == DocType::Story)
        .unwrap();
    let related = store.related_to(&story.path);
    assert_eq!(related.len(), 1);
    assert_eq!(
        related[0].1,
        &PathBuf::from("docs/rfcs/RFC-002-folder-feature/index.md")
    );
}

#[test]
fn store_search_finds_subfolder_doc() {
    let fixture = TestFixture::new();
    fixture.write_subfolder_doc(
        "docs/rfcs/RFC-002-folder-feature",
        r#"---
title: "Unique Folder Feature"
type: rfc
status: draft
author: "test"
date: 2026-01-01
tags: []
---
"#,
    );

    let store = fixture.store();
    let results = store.search("Unique Folder");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].doc.title, "Unique Folder Feature");
}
