mod common;

use common::TestFixture;
use lazyspec::engine::document::{DocType, Status};
use lazyspec::engine::store::{Filter, ResolveError};
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
        doc_type: Some(DocType::new(DocType::RFC)),
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
    let rfc = docs.iter().find(|d| d.doc_type == DocType::new(DocType::RFC)).unwrap();
    let body = store.get_body(&rfc.path).unwrap();
    assert!(body.contains("Event sourcing proposal."));
}

#[test]
fn store_get_body_raw_returns_unexpanded_refs() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-010-refs.md",
        r#"---
title: "Ref Test"
type: rfc
status: draft
author: test
date: 2026-03-01
tags: []
---

See code:

@ref src/main.rs#MyStruct
"#,
    );

    let store = fixture.store();
    let doc = store.resolve_shorthand("RFC-010").expect("should resolve");
    let body = store.get_body_raw(&doc.path).unwrap();
    assert!(body.contains("@ref src/main.rs#MyStruct"), "raw body should preserve @ref directives");
}

#[test]
fn store_get_body_defaults_to_raw() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-011-alias.md",
        r#"---
title: "Alias Test"
type: rfc
status: draft
author: test
date: 2026-03-01
tags: []
---

@ref some/file.ts
"#,
    );

    let store = fixture.store();
    let doc = store.resolve_shorthand("RFC-011").expect("should resolve");
    let raw = store.get_body_raw(&doc.path).unwrap();
    let default = store.get_body(&doc.path).unwrap();
    assert_eq!(raw, default, "get_body should return the same result as get_body_raw");
}

#[test]
fn store_get_body_expanded_processes_refs() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-012-expand.md",
        r#"---
title: "Expand Test"
type: rfc
status: draft
author: test
date: 2026-03-01
tags: []
---

@ref nonexistent/file.rs
"#,
    );

    let store = fixture.store();
    let doc = store.resolve_shorthand("RFC-012").expect("should resolve");
    let expanded = store.get_body_expanded(&doc.path, 25).unwrap();
    assert!(!expanded.contains("@ref nonexistent/file.rs"), "expanded body should not contain raw @ref");
    assert!(expanded.contains("> [unresolved:"), "expanded body should contain unresolved marker");
}

#[test]
fn store_resolves_related_docs() {
    let fixture = setup_fixture();
    let store = fixture.store();

    let docs = store.all_docs();
    let adr = docs.iter().find(|d| d.doc_type == DocType::new(DocType::ADR)).unwrap();
    let related = store.related_to(&adr.path);
    assert_eq!(related.len(), 1);
}

#[test]
fn store_resolves_shorthand_id() {
    let fixture = setup_fixture();
    let store = fixture.store();

    let doc = store.resolve_shorthand("RFC-001");
    assert!(doc.is_ok());
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
    assert!(doc.is_ok());
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
        .find(|d| d.doc_type == DocType::new(DocType::STORY))
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

#[test]
fn store_discovers_child_md_files() {
    let fixture = TestFixture::new();
    fixture.write_subfolder_doc(
        "docs/rfcs/RFC-003-multi",
        "---\ntitle: \"Multi Doc\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );
    fixture.write_child_doc(
        "docs/rfcs/RFC-003-multi",
        "appendix.md",
        "---\ntitle: \"Appendix\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );

    let store = fixture.store();
    assert_eq!(store.all_docs().len(), 2);
    let child = store.all_docs().into_iter().find(|d| d.title == "Appendix").unwrap();
    assert!(child.path.to_string_lossy().contains("appendix.md"));
}

#[test]
fn store_tracks_parent_child_relationship() {
    let fixture = TestFixture::new();
    fixture.write_subfolder_doc(
        "docs/rfcs/RFC-003-multi",
        "---\ntitle: \"Multi Doc\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );
    fixture.write_child_doc(
        "docs/rfcs/RFC-003-multi",
        "appendix.md",
        "---\ntitle: \"Appendix\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );

    let store = fixture.store();
    let parent_path = PathBuf::from("docs/rfcs/RFC-003-multi/index.md");
    let child_path = PathBuf::from("docs/rfcs/RFC-003-multi/appendix.md");

    let children = store.children_of(&parent_path);
    assert!(children.contains(&child_path));

    let parent = store.parent_of(&child_path);
    assert_eq!(parent, Some(&parent_path));
}

#[test]
fn store_synthesises_virtual_parent() {
    let fixture = TestFixture::new();
    fixture.write_child_doc(
        "docs/rfcs/RFC-004-virtual",
        "notes.md",
        "---\ntitle: \"Notes\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );
    fixture.write_child_doc(
        "docs/rfcs/RFC-004-virtual",
        "design.md",
        "---\ntitle: \"Design\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );

    let store = fixture.store();
    let virtual_parent = store.all_docs().into_iter().find(|d| d.virtual_doc).unwrap();
    assert_eq!(virtual_parent.title, "Virtual");
    assert_eq!(virtual_parent.status, Status::Draft);
}

#[test]
fn store_virtual_parent_accepted_when_all_children_accepted() {
    let fixture = TestFixture::new();
    fixture.write_child_doc(
        "docs/rfcs/RFC-004-virtual",
        "notes.md",
        "---\ntitle: \"Notes\"\ntype: rfc\nstatus: accepted\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );
    fixture.write_child_doc(
        "docs/rfcs/RFC-004-virtual",
        "design.md",
        "---\ntitle: \"Design\"\ntype: rfc\nstatus: accepted\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );

    let store = fixture.store();
    let virtual_parent = store.all_docs().into_iter().find(|d| d.virtual_doc).unwrap();
    assert_eq!(virtual_parent.status, Status::Accepted);
}

#[test]
fn store_virtual_parent_not_on_disk() {
    let fixture = TestFixture::new();
    fixture.write_child_doc(
        "docs/rfcs/RFC-004-virtual",
        "notes.md",
        "---\ntitle: \"Notes\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );
    fixture.write_child_doc(
        "docs/rfcs/RFC-004-virtual",
        "design.md",
        "---\ntitle: \"Design\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );

    let store = fixture.store();
    assert!(store.all_docs().iter().any(|d| d.virtual_doc));
    assert!(!fixture.root().join("docs/rfcs/RFC-004-virtual/index.md").exists());
    assert!(!fixture.root().join("docs/rfcs/RFC-004-virtual/.virtual").exists());
}

#[test]
fn store_qualified_shorthand_resolves_child() {
    let fixture = TestFixture::new();
    fixture.write_subfolder_doc(
        "docs/rfcs/RFC-003-multi",
        "---\ntitle: \"Multi Doc\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );
    fixture.write_child_doc(
        "docs/rfcs/RFC-003-multi",
        "appendix.md",
        "---\ntitle: \"Appendix\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );

    let store = fixture.store();
    let doc = store.resolve_shorthand("RFC-003/appendix");
    assert!(doc.is_ok());
    assert_eq!(doc.unwrap().title, "Appendix");
}

#[test]
fn store_unqualified_shorthand_skips_children() {
    let fixture = TestFixture::new();
    fixture.write_subfolder_doc(
        "docs/rfcs/RFC-003-multi",
        "---\ntitle: \"Multi Doc\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );
    fixture.write_child_doc(
        "docs/rfcs/RFC-003-multi",
        "notes.md",
        "---\ntitle: \"Notes A\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );
    fixture.write_subfolder_doc(
        "docs/rfcs/RFC-005-other",
        "---\ntitle: \"Other Doc\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );
    fixture.write_child_doc(
        "docs/rfcs/RFC-005-other",
        "notes.md",
        "---\ntitle: \"Notes B\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );

    let store = fixture.store();
    let doc = store.resolve_shorthand("notes");
    assert!(doc.is_err());
}

#[test]
fn store_child_relationships_resolve() {
    let fixture = TestFixture::new();
    fixture.write_subfolder_doc(
        "docs/rfcs/RFC-003-multi",
        "---\ntitle: \"Multi Doc\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );
    fixture.write_child_doc(
        "docs/rfcs/RFC-003-multi",
        "appendix.md",
        "---\ntitle: \"Appendix\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\nrelated:\n  - implements: docs/stories/STORY-001-impl.md\n---\n",
    );
    fixture.write_story("STORY-001-impl.md", "Implement Feature", "draft", None);

    let store = fixture.store();
    let child_path = PathBuf::from("docs/rfcs/RFC-003-multi/appendix.md");
    let story_path = PathBuf::from("docs/stories/STORY-001-impl.md");

    let related = store.related_to(&child_path);
    assert!(related.iter().any(|(_, p)| **p == story_path));

    let refs = store.referenced_by(&story_path);
    assert!(refs.iter().any(|(_, p)| **p == child_path));
}

#[test]
fn store_collects_parse_errors() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-broken.md",
        "---\ntitle: \"Broken\"\ntype: rfc\nauthor: test\ndate: 2026-01-01\ntags: []\n---\n",
    );

    let store = fixture.store();
    assert_eq!(store.parse_errors().len(), 1);
    assert!(store.parse_errors()[0].path.to_string_lossy().contains("RFC-broken.md"));
    assert!(store.all_docs().is_empty());
}

#[test]
fn store_loads_valid_alongside_invalid() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-good.md", "Good RFC", "draft");
    fixture.write_doc(
        "docs/rfcs/RFC-002-bad.md",
        "---\ntitle: \"Bad\"\ntype: rfc\nauthor: test\ntags: []\n---\n",
    );

    let store = fixture.store();
    assert_eq!(store.all_docs().len(), 1);
    assert_eq!(store.all_docs()[0].title, "Good RFC");
    assert_eq!(store.parse_errors().len(), 1);
    assert!(store.parse_errors()[0].path.to_string_lossy().contains("RFC-002-bad.md"));
}

#[test]
fn store_ignores_nested_subdirectories() {
    let fixture = TestFixture::new();
    fixture.write_subfolder_doc(
        "docs/rfcs/RFC-003-multi",
        "---\ntitle: \"Multi Doc\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );
    let deep_dir = fixture.root().join("docs/rfcs/RFC-003-multi/deep");
    std::fs::create_dir_all(&deep_dir).unwrap();
    std::fs::write(
        deep_dir.join("hidden.md"),
        "---\ntitle: \"Hidden\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    ).unwrap();

    let store = fixture.store();
    assert_eq!(store.all_docs().len(), 1);
    assert!(store.all_docs().iter().all(|d| d.title != "Hidden"));
}

#[test]
fn resolve_shorthand_ambiguous_returns_error() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-020-first.md",
        "---\ntitle: \"First\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );
    fixture.write_doc(
        "docs/adrs/RFC-020-second.md",
        "---\ntitle: \"Second\"\ntype: adr\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );

    let store = fixture.store();
    let result = store.resolve_shorthand("RFC-020");
    assert!(result.is_err());
    match result.unwrap_err() {
        ResolveError::Ambiguous { id, matches } => {
            assert_eq!(id, "RFC-020");
            assert_eq!(matches.len(), 2);
        }
        ResolveError::NotFound(_) => panic!("expected Ambiguous, got NotFound"),
    }
}

#[test]
fn resolve_shorthand_unique_id_still_works() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-030-unique.md", "Unique Doc", "draft");

    let store = fixture.store();
    let doc = store.resolve_shorthand("RFC-030");
    assert!(doc.is_ok());
    assert_eq!(doc.unwrap().title, "Unique Doc");
}

#[test]
fn resolve_shorthand_not_found() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-exists.md", "Exists", "draft");

    let store = fixture.store();
    let result = store.resolve_shorthand("RFC-999");
    assert!(result.is_err());
    match result.unwrap_err() {
        ResolveError::NotFound(id) => assert_eq!(id, "RFC-999"),
        ResolveError::Ambiguous { .. } => panic!("expected NotFound, got Ambiguous"),
    }
}

#[test]
fn list_includes_both_duplicate_docs() {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-020-first.md",
        "---\ntitle: \"First\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );
    fixture.write_doc(
        "docs/adrs/RFC-020-second.md",
        "---\ntitle: \"Second\"\ntype: adr\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );

    let store = fixture.store();
    let all = store.all_docs();
    assert_eq!(all.len(), 2);
    let titles: Vec<&str> = all.iter().map(|d| d.title.as_str()).collect();
    assert!(titles.contains(&"First"));
    assert!(titles.contains(&"Second"));
}
