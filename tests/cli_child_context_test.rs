mod common;

use common::TestFixture;

const PARENT_CONTENT: &str = "\
---
title: \"Multi Doc\"
type: rfc
status: draft
author: \"test\"
date: 2026-01-01
tags: []
---
Parent overview stuff
";

const APPENDIX_CONTENT: &str = "\
---
title: \"Appendix\"
type: rfc
status: draft
author: \"test\"
date: 2026-01-01
tags: []
---
Child appendix content with unique-child-term-xyz
";

const GLOSSARY_CONTENT: &str = "\
---
title: \"Glossary\"
type: rfc
status: draft
author: \"test\"
date: 2026-01-01
tags: []
---
Glossary definitions
";

fn setup_parent_with_children_and_story() -> TestFixture {
    let fixture = TestFixture::new();
    fixture.write_subfolder_doc("docs/rfcs/RFC-003-multi", PARENT_CONTENT);
    fixture.write_child_doc("docs/rfcs/RFC-003-multi", "appendix.md", APPENDIX_CONTENT);
    fixture.write_child_doc("docs/rfcs/RFC-003-multi", "glossary.md", GLOSSARY_CONTENT);
    fixture.write_story(
        "STORY-001-impl.md",
        "Impl Story",
        "draft",
        Some("docs/rfcs/RFC-003-multi/index.md"),
    );
    fixture
}

// AC3: context JSON includes children as relationships
#[test]
fn context_includes_children_json() {
    let fixture = setup_parent_with_children_and_story();
    let store = fixture.store();

    let output = lazyspec::cli::context::run_json(&store, "STORY-001").unwrap();
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();

    let chain = json["chain"].as_array().expect("chain array missing");
    let rfc_entry = chain
        .iter()
        .find(|item| item["title"].as_str() == Some("Multi Doc"))
        .expect("RFC should be in chain");

    let children = rfc_entry["children"]
        .as_array()
        .expect("children array missing on RFC in chain");
    assert_eq!(children.len(), 2);

    let titles: Vec<&str> = children
        .iter()
        .map(|c| c["title"].as_str().unwrap())
        .collect();
    assert!(titles.contains(&"Appendix"));
    assert!(titles.contains(&"Glossary"));
}

// AC3: context human output includes children
#[test]
fn context_human_includes_children() {
    let fixture = setup_parent_with_children_and_story();
    let store = fixture.store();

    let output = lazyspec::cli::context::run_human(&store, "STORY-001").unwrap();
    assert!(
        output.contains("Appendix"),
        "human context should mention child title 'Appendix'"
    );
    assert!(
        output.contains("Glossary"),
        "human context should mention child title 'Glossary'"
    );
}

// AC5: search matches child content independently
#[test]
fn search_matches_child_independently() {
    let fixture = setup_parent_with_children_and_story();
    let store = fixture.store();

    let output = lazyspec::cli::search::run_json(&store, "unique-child-term-xyz", None);
    let results: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap();

    assert!(
        !results.is_empty(),
        "search should find the child with unique term"
    );

    let titles: Vec<&str> = results
        .iter()
        .map(|r| r["title"].as_str().unwrap())
        .collect();
    assert!(
        titles.contains(&"Appendix"),
        "search should return the matching child"
    );
}

// AC5: search does not include parent for child-only match
#[test]
fn search_does_not_include_parent_for_child_match() {
    let fixture = setup_parent_with_children_and_story();
    let store = fixture.store();

    let output = lazyspec::cli::search::run_json(&store, "unique-child-term-xyz", None);
    let results: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap();

    let titles: Vec<&str> = results
        .iter()
        .map(|r| r["title"].as_str().unwrap())
        .collect();
    assert!(
        !titles.contains(&"Multi Doc"),
        "parent should not appear when only child matches"
    );
    assert!(
        !titles.contains(&"Glossary"),
        "sibling should not appear when only one child matches"
    );
}

// AC6: validate reports child errors specifically
#[test]
fn validate_reports_child_errors_specifically() {
    let fixture = TestFixture::new();
    fixture.write_subfolder_doc("docs/rfcs/RFC-005-val", PARENT_CONTENT);
    fixture.write_child_doc(
        "docs/rfcs/RFC-005-val",
        "appendix.md",
        "---\ntitle: \"Bad Appendix\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: docs/nonexistent.md\n---\n",
    );
    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    let child_errors: Vec<_> = result
        .errors
        .iter()
        .filter(|e| format!("{:?}", e).contains("appendix.md"))
        .collect();
    assert!(
        !child_errors.is_empty(),
        "validation should report errors referencing the child document (appendix.md)"
    );
}

// AC6: validate parent unaffected by child error
#[test]
fn validate_parent_unaffected_by_child_error() {
    let fixture = TestFixture::new();
    fixture.write_subfolder_doc("docs/rfcs/RFC-005-val", PARENT_CONTENT);
    fixture.write_child_doc(
        "docs/rfcs/RFC-005-val",
        "appendix.md",
        "---\ntitle: \"Bad Appendix\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: docs/nonexistent.md\n---\n",
    );
    let store = fixture.store();
    let result = store.validate_full(&fixture.config());

    let parent_errors: Vec<_> = result
        .errors
        .iter()
        .filter(|e| format!("{:?}", e).contains("index.md"))
        .collect();
    assert!(
        parent_errors.is_empty(),
        "parent (index.md) should not have errors caused by child's broken link"
    );
}
