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

#[test]
fn show_parent_json_includes_children() {
    let fixture = setup_parent_with_children();
    let store = fixture.store();

    let output = lazyspec::cli::show::run_json(&store, "RFC-003").unwrap();
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();

    let children = json["children"].as_array().expect("children array missing");
    assert_eq!(children.len(), 2);

    let titles: Vec<&str> = children.iter().map(|c| c["title"].as_str().unwrap()).collect();
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

    let output = lazyspec::cli::show::run_json(&store, "RFC-003/appendix").unwrap();
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

#[test]
fn list_json_includes_family_metadata() {
    let fixture = setup_parent_with_children();
    let store = fixture.store();

    let output = lazyspec::cli::list::run_json(&store, None, None);
    let json: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap();

    let parent = json.iter().find(|d| d["title"] == "Multi Doc").unwrap();
    let children = parent["children"].as_array().expect("parent should have children array");
    assert_eq!(children.len(), 2);

    let appendix = json.iter().find(|d| d["title"] == "Appendix").unwrap();
    let parent_ref = appendix["parent"].as_object().expect("child should have parent object");
    assert_eq!(parent_ref["title"].as_str().unwrap(), "Multi Doc");
}

#[test]
fn show_parent_json_no_children_field_when_none() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-010-flat.md", "Flat RFC", "draft");
    let store = fixture.store();

    let output = lazyspec::cli::show::run_json(&store, "RFC-010").unwrap();
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert!(json.get("children").is_none(), "children field should be absent for docs without children");
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

    let virtual_parent = json.iter().find(|d| d["title"] == "Virtual").expect("virtual parent should appear in list");
    assert_eq!(virtual_parent["virtual_doc"].as_bool(), Some(true), "virtual_doc should be true");
    let children = virtual_parent["children"].as_array().expect("virtual parent should have children");
    assert_eq!(children.len(), 2);
}
