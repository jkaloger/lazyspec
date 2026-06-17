use crate::common::TestFixture;
use lazyspec::cli::provenance::{run_add, run_list, run_remove};
use lazyspec::engine::document::DocMeta;
use lazyspec::engine::store::Store;
use std::path::PathBuf;

fn write_rfc_with_provenance(fixture: &TestFixture, filename: &str, entries: &[&str]) -> PathBuf {
    let provenance_block = if entries.is_empty() {
        String::new()
    } else {
        let mut s = String::from("provenance:\n");
        for e in entries {
            s.push_str(&format!("  - \"{}\"\n", e));
        }
        s
    };
    let content = format!(
        "---\ntitle: \"Test\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n{}---\n",
        provenance_block,
    );
    fixture.write_doc(&format!("docs/rfcs/{}", filename), &content)
}

fn provenance_of(fixture: &TestFixture, rel: &str) -> Vec<String> {
    let content = std::fs::read_to_string(fixture.root().join(rel)).unwrap();
    DocMeta::parse(&content).unwrap().provenance
}

fn reload(fixture: &TestFixture) -> Store {
    let config = fixture.config();
    Store::load(fixture.root(), &config).unwrap()
}

fn captured() -> Vec<u8> {
    Vec::new()
}

fn into_string(buf: Vec<u8>) -> String {
    String::from_utf8(buf).expect("stdout is utf8")
}

#[test]
fn add_appends_to_empty() {
    let fixture = TestFixture::new();
    write_rfc_with_provenance(&fixture, "RFC-001-test.md", &[]);

    let store = fixture.store();
    let config = fixture.config();
    let mut buf = captured();
    run_add(
        fixture.root(),
        &store,
        &config,
        "RFC-001",
        "X",
        false,
        &mut buf,
    )
    .unwrap();

    assert_eq!(
        provenance_of(&fixture, "docs/rfcs/RFC-001-test.md"),
        vec!["X".to_string()]
    );
}

#[test]
fn add_appends_to_existing() {
    let fixture = TestFixture::new();
    write_rfc_with_provenance(&fixture, "RFC-001-test.md", &["A", "B"]);

    let store = fixture.store();
    let config = fixture.config();
    let mut buf = captured();
    run_add(
        fixture.root(),
        &store,
        &config,
        "RFC-001",
        "C",
        false,
        &mut buf,
    )
    .unwrap();

    assert_eq!(
        provenance_of(&fixture, "docs/rfcs/RFC-001-test.md"),
        vec!["A".to_string(), "B".to_string(), "C".to_string()]
    );
}

#[test]
fn add_empty_citation_errors() {
    let fixture = TestFixture::new();
    write_rfc_with_provenance(&fixture, "RFC-001-test.md", &["A"]);

    let store = fixture.store();
    let config = fixture.config();
    let mut buf = captured();
    let result = run_add(
        fixture.root(),
        &store,
        &config,
        "RFC-001",
        "",
        false,
        &mut buf,
    );
    let err = result.unwrap_err();
    assert!(
        err.to_string().to_lowercase().contains("empty"),
        "got: {}",
        err
    );

    assert_eq!(
        provenance_of(&fixture, "docs/rfcs/RFC-001-test.md"),
        vec!["A".to_string()]
    );
}

#[test]
fn add_unresolved_doc_errors() {
    let fixture = TestFixture::new();
    let store = fixture.store();
    let config = fixture.config();

    let mut buf = captured();
    let err = run_add(
        fixture.root(),
        &store,
        &config,
        "BOGUS-999",
        "X",
        false,
        &mut buf,
    )
    .unwrap_err();
    assert!(err.to_string().contains("not found"), "got: {}", err);
}

#[test]
fn remove_exact_match() {
    let fixture = TestFixture::new();
    write_rfc_with_provenance(&fixture, "RFC-001-test.md", &["A", "B", "C"]);

    let store = fixture.store();
    let config = fixture.config();
    let mut buf = captured();
    run_remove(
        fixture.root(),
        &store,
        &config,
        "RFC-001",
        "B",
        false,
        &mut buf,
    )
    .unwrap();

    assert_eq!(
        provenance_of(&fixture, "docs/rfcs/RFC-001-test.md"),
        vec!["A".to_string(), "C".to_string()]
    );
}

#[test]
fn remove_first_match_only_when_duplicates() {
    let fixture = TestFixture::new();
    write_rfc_with_provenance(&fixture, "RFC-001-test.md", &["A", "A", "B"]);

    let store = fixture.store();
    let config = fixture.config();
    let mut buf = captured();
    run_remove(
        fixture.root(),
        &store,
        &config,
        "RFC-001",
        "A",
        false,
        &mut buf,
    )
    .unwrap();

    assert_eq!(
        provenance_of(&fixture, "docs/rfcs/RFC-001-test.md"),
        vec!["A".to_string(), "B".to_string()]
    );
}

#[test]
fn remove_missing_errors() {
    let fixture = TestFixture::new();
    write_rfc_with_provenance(&fixture, "RFC-001-test.md", &["A"]);

    let store = fixture.store();
    let config = fixture.config();
    let mut buf = captured();
    let err = run_remove(
        fixture.root(),
        &store,
        &config,
        "RFC-001",
        "Z",
        false,
        &mut buf,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("citation not found"),
        "got: {}",
        err
    );

    assert_eq!(
        provenance_of(&fixture, "docs/rfcs/RFC-001-test.md"),
        vec!["A".to_string()]
    );
}

#[test]
fn list_single_doc_empty_stdout_is_empty() {
    let fixture = TestFixture::new();
    write_rfc_with_provenance(&fixture, "RFC-001-test.md", &[]);

    let store = fixture.store();
    let mut buf = captured();
    run_list(&store, Some("RFC-001"), false, &mut buf).unwrap();

    assert_eq!(into_string(buf), "");
}

#[test]
fn list_single_doc_plain_prints_each_citation() {
    let fixture = TestFixture::new();
    write_rfc_with_provenance(&fixture, "RFC-001-test.md", &["A", "B"]);

    let store = fixture.store();
    let mut buf = captured();
    run_list(&store, Some("RFC-001"), false, &mut buf).unwrap();

    let stdout = into_string(buf);
    assert_eq!(stdout, "A\nB\n");
}

#[test]
fn list_global_plain_groups_by_doc_skips_empty() {
    let fixture = TestFixture::new();
    write_rfc_with_provenance(&fixture, "RFC-001-test.md", &["A"]);
    write_rfc_with_provenance(&fixture, "RFC-002-test.md", &["B1", "B2"]);
    write_rfc_with_provenance(&fixture, "RFC-003-test.md", &[]);

    let store = fixture.store();
    let mut buf = captured();
    run_list(&store, None, false, &mut buf).unwrap();

    let stdout = into_string(buf);
    assert_eq!(stdout, "RFC-001\tA\nRFC-002\tB1\nRFC-002\tB2\n");
    assert!(!stdout.contains("RFC-003"));
}

#[test]
fn add_json_shape() {
    let fixture = TestFixture::new();
    write_rfc_with_provenance(&fixture, "RFC-001-test.md", &["A"]);

    let store = fixture.store();
    let config = fixture.config();
    let mut buf = captured();
    run_add(
        fixture.root(),
        &store,
        &config,
        "RFC-001",
        "B",
        true,
        &mut buf,
    )
    .unwrap();

    let stdout = into_string(buf);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(parsed["doc"], "RFC-001");
    assert_eq!(parsed["added"], "B");
    assert_eq!(
        parsed["provenance"],
        serde_json::json!(["A".to_string(), "B".to_string()])
    );

    let store = reload(&fixture);
    let doc = store.resolve_shorthand("RFC-001").unwrap();
    assert_eq!(doc.provenance, vec!["A".to_string(), "B".to_string()]);
}

#[test]
fn remove_json_shape() {
    let fixture = TestFixture::new();
    write_rfc_with_provenance(&fixture, "RFC-001-test.md", &["A", "B"]);

    let store = fixture.store();
    let config = fixture.config();
    let mut buf = captured();
    run_remove(
        fixture.root(),
        &store,
        &config,
        "RFC-001",
        "A",
        true,
        &mut buf,
    )
    .unwrap();

    let stdout = into_string(buf);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(parsed["doc"], "RFC-001");
    assert_eq!(parsed["removed"], "A");
    assert_eq!(parsed["provenance"], serde_json::json!(["B".to_string()]));

    let store = reload(&fixture);
    let doc = store.resolve_shorthand("RFC-001").unwrap();
    assert_eq!(doc.provenance, vec!["B".to_string()]);
}

#[test]
fn list_json_with_id_shape() {
    let fixture = TestFixture::new();
    write_rfc_with_provenance(&fixture, "RFC-001-test.md", &["A"]);

    let store = fixture.store();
    let mut buf = captured();
    run_list(&store, Some("RFC-001"), true, &mut buf).unwrap();

    let stdout = into_string(buf);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(parsed["doc"], "RFC-001");
    assert_eq!(parsed["provenance"], serde_json::json!(["A".to_string()]));
}

#[test]
fn list_json_global_shape() {
    let fixture = TestFixture::new();
    write_rfc_with_provenance(&fixture, "RFC-001-test.md", &["A"]);
    write_rfc_with_provenance(&fixture, "RFC-002-test.md", &[]);

    let store = fixture.store();
    let mut buf = captured();
    run_list(&store, None, true, &mut buf).unwrap();

    let stdout = into_string(buf);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let documents = parsed["documents"].as_array().expect("documents is array");
    assert_eq!(documents.len(), 1);
    let entry = &documents[0];
    assert_eq!(entry["id"], "RFC-001");
    assert!(entry["path"].is_string());
    assert!(entry["path"].as_str().unwrap().contains("RFC-001-test.md"));
    assert_eq!(entry["provenance"], serde_json::json!(["A".to_string()]));
}

#[test]
fn shorthand_id_resolves() {
    let fixture = TestFixture::new();
    write_rfc_with_provenance(&fixture, "RFC-001-test.md", &[]);

    let store = fixture.store();
    let config = fixture.config();
    let mut buf = captured();
    run_add(
        fixture.root(),
        &store,
        &config,
        "RFC-001",
        "Source",
        false,
        &mut buf,
    )
    .unwrap();

    assert_eq!(
        provenance_of(&fixture, "docs/rfcs/RFC-001-test.md"),
        vec!["Source".to_string()]
    );
}
