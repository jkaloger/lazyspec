use crate::common::TestFixture;
use lazyspec::engine::config::{NumberingStrategy, TypeDef};

const PARENT_CONTENT: &str = "\
---
title: \"Multi Doc\"
type: rfc
status: draft
author: \"test\"
date: 2026-01-01
tags: []
---
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
";

fn setup_parent_with_children() -> TestFixture {
    let fixture = TestFixture::new();
    fixture.write_subfolder_doc("docs/rfcs/RFC-003-multi", PARENT_CONTENT);
    fixture.write_child_doc("docs/rfcs/RFC-003-multi", "appendix.md", APPENDIX_CONTENT);
    fixture.write_child_doc("docs/rfcs/RFC-003-multi", "glossary.md", GLOSSARY_CONTENT);
    fixture
}

const FLAT_PARENT: &str = "\
---
title: \"Multi Doc\"
type: rfc
status: draft
author: \"test\"
date: 2026-01-01
tags: []
---
";

// AC1: `create rfc "Appendix" --parent RFC-003` against a flat parent promotes
// it to `RFC-003-multi/index.md`, removes the flat file, lands the child as a
// sibling `.md`, and the reloaded store tracks the parent/child edges.
#[test]
fn create_with_parent_promotes_flat_parent_and_tracks_child() {
    let fixture = TestFixture::new();
    fixture.write_doc("docs/rfcs/RFC-003-multi.md", FLAT_PARENT);
    let config = fixture.config();
    let store = fixture.store();

    let child_path = lazyspec::cli::create::run_with_body(
        fixture.root(),
        &config,
        &store,
        "rfc",
        "Appendix",
        "test",
        Some("RFC-003"),
        None,
        |_| {},
    )
    .unwrap();

    let index_path = fixture.root().join("docs/rfcs/RFC-003-multi/index.md");
    assert!(index_path.exists(), "parent should be promoted to index.md");
    assert!(
        !fixture.root().join("docs/rfcs/RFC-003-multi.md").exists(),
        "flat parent file should be gone after promotion"
    );
    assert!(
        child_path.starts_with(fixture.root().join("docs/rfcs/RFC-003-multi")),
        "child should live inside the parent subdir, got {}",
        child_path.display()
    );
    assert_eq!(child_path.file_name().unwrap(), "RFC-001-appendix.md");

    let store = fixture.store();
    let rel_index = index_path.strip_prefix(fixture.root()).unwrap();
    let rel_child = child_path.strip_prefix(fixture.root()).unwrap();
    let children = store.children_of(rel_index);
    assert!(
        children.iter().any(|c| c == rel_child),
        "parent should list the new child, got {:?}",
        children
    );
    assert_eq!(
        store.parent_of(rel_child).map(|p| p.as_path()),
        Some(rel_index)
    );
}

// AC1 (idempotent): a second child against an already-promoted (index.md)
// parent skips promotion and lands a second sibling.
#[test]
fn create_with_parent_idempotent_on_promoted_parent() {
    let fixture = TestFixture::new();
    fixture.write_subfolder_doc("docs/rfcs/RFC-003-multi", FLAT_PARENT);
    let config = fixture.config();

    let first = lazyspec::cli::create::run_with_body(
        fixture.root(),
        &config,
        &fixture.store(),
        "rfc",
        "Appendix",
        "test",
        Some("RFC-003"),
        None,
        |_| {},
    )
    .unwrap();
    let second = lazyspec::cli::create::run_with_body(
        fixture.root(),
        &config,
        &fixture.store(),
        "rfc",
        "Glossary",
        "test",
        Some("RFC-003"),
        None,
        |_| {},
    )
    .unwrap();

    assert_ne!(first, second);
    for p in [&first, &second] {
        assert!(p.starts_with(fixture.root().join("docs/rfcs/RFC-003-multi")));
    }
    assert!(fixture
        .root()
        .join("docs/rfcs/RFC-003-multi/index.md")
        .exists());
}

// AC3: a child type whose store differs from the parent's store is rejected
// before any file mutation; the error names the store mismatch and no child
// file is written.
#[test]
fn create_with_parent_cross_store_rejected_before_mutation() {
    use lazyspec::engine::config::StoreBackend;

    let fixture = TestFixture::new();
    fixture.write_doc("docs/rfcs/RFC-003-multi.md", FLAT_PARENT);

    // Mark the `rfc` child type as github-issues while the parent (also rfc on
    // disk) is resolved against a filesystem type -- here we instead use two
    // types: keep rfc filesystem (parent) and add a github-issues `issue` type
    // as the child.
    let mut config = fixture.config();
    let issue_type = TypeDef {
        name: "issue".to_string(),
        plural: "issues".to_string(),
        dir: "docs/issues".to_string(),
        prefix: "ISSUE".to_string(),
        icon: None,
        numbering: NumberingStrategy::Incremental,
        subdirectory: false,
        store: StoreBackend::GithubIssues,
        singleton: false,
        parent_type: None,
        agents: Vec::new(),
        intent: None,
        authorship: Default::default(),
        lifecycle: Default::default(),
        attributes: Default::default(),
        label_override: None,
        github_issue_tag: None,
        github_issue_type: None,
        clickup_list_id: None,
        clickup_custom_field_map: None,
    };
    config.documents.types.push(issue_type);
    let store = fixture.store();

    let err = lazyspec::cli::create::run_with_body(
        fixture.root(),
        &config,
        &store,
        "issue",
        "Cross",
        "test",
        Some("RFC-003"),
        None,
        |_| {},
    )
    .unwrap_err();

    let msg = err.to_string();
    assert!(msg.contains("different stores"), "explains: {msg}");
    assert!(
        fixture.root().join("docs/rfcs/RFC-003-multi.md").exists(),
        "flat parent must not be promoted on reject"
    );
    assert!(
        !fixture.root().join("docs/rfcs/RFC-003-multi").exists(),
        "no subdir should be created on reject"
    );
}

#[test]
fn show_parent_json_includes_children() {
    let fixture = setup_parent_with_children();
    let store = fixture.store();

    let output = lazyspec::cli::show::run_json(
        &store,
        "RFC-003",
        false,
        25,
        &lazyspec::engine::fs::RealFileSystem,
        &fixture.config(),
        fixture.root(),
        &crate::common::NoopGh,
    )
    .unwrap();
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();

    let children = json["children"].as_array().expect("children array missing");
    assert_eq!(children.len(), 2);

    let titles: Vec<&str> = children
        .iter()
        .map(|c| c["title"].as_str().unwrap())
        .collect();
    assert!(titles.contains(&"Appendix"));
    assert!(titles.contains(&"Glossary"));

    for child in children {
        assert!(child["path"].as_str().unwrap().contains("RFC-003-multi"));
    }
}

#[test]
fn show_child_json_includes_parent() {
    let fixture = setup_parent_with_children();
    let store = fixture.store();

    let output = lazyspec::cli::show::run_json(
        &store,
        "RFC-003/appendix",
        false,
        25,
        &lazyspec::engine::fs::RealFileSystem,
        &fixture.config(),
        fixture.root(),
        &crate::common::NoopGh,
    )
    .unwrap();
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();

    let parent = json["parent"].as_object().expect("parent object missing");
    assert_eq!(parent["title"].as_str().unwrap(), "Multi Doc");
    assert!(parent["path"].as_str().unwrap().contains("index.md"));
}

#[test]
fn list_includes_child_documents() {
    let fixture = setup_parent_with_children();
    let store = fixture.store();

    let output = lazyspec::cli::list::run_json(&store, None, None);
    let json: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap();

    let titles: Vec<&str> = json.iter().map(|d| d["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"Multi Doc"));
    assert!(titles.contains(&"Appendix"));
    assert!(titles.contains(&"Glossary"));
}

// AC4: every record in `list --json` carries a non-null id.
#[test]
fn list_json_records_carry_id() {
    let fixture = setup_parent_with_children();
    let store = fixture.store();

    let output = lazyspec::cli::list::run_json(&store, None, None);
    let json: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap();

    assert!(!json.is_empty());
    for record in &json {
        assert!(
            record.get("id").is_some_and(|v| !v.is_null()),
            "every record must carry a non-null id, got {record}"
        );
    }
}

#[test]
fn list_json_includes_family_metadata() {
    let fixture = setup_parent_with_children();
    let store = fixture.store();

    let output = lazyspec::cli::list::run_json(&store, None, None);
    let json: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap();

    let parent = json.iter().find(|d| d["title"] == "Multi Doc").unwrap();
    let children = parent["children"]
        .as_array()
        .expect("parent should have children array");
    assert_eq!(children.len(), 2);

    let appendix = json.iter().find(|d| d["title"] == "Appendix").unwrap();
    let parent_ref = appendix["parent"]
        .as_object()
        .expect("child should have parent object");
    assert_eq!(parent_ref["title"].as_str().unwrap(), "Multi Doc");
}

#[test]
fn show_parent_json_no_children_field_when_none() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-010-flat.md", "Flat RFC", "draft");
    let store = fixture.store();

    let output = lazyspec::cli::show::run_json(
        &store,
        "RFC-010",
        false,
        25,
        &lazyspec::engine::fs::RealFileSystem,
        &fixture.config(),
        fixture.root(),
        &crate::common::NoopGh,
    )
    .unwrap();
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert!(
        json.get("children").is_none(),
        "children field should be absent for docs without children"
    );
}

#[test]
fn list_json_virtual_doc_flag() {
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

    let output = lazyspec::cli::list::run_json(&store, None, None);
    let json: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap();

    let virtual_parent = json
        .iter()
        .find(|d| d["title"] == "Virtual")
        .expect("virtual parent should appear in list");
    assert_eq!(
        virtual_parent["virtual_doc"].as_bool(),
        Some(true),
        "virtual_doc should be true"
    );
    let children = virtual_parent["children"]
        .as_array()
        .expect("virtual parent should have children");
    assert_eq!(children.len(), 2);
}
