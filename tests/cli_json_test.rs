mod common;

use lazyspec::cli::json::doc_to_json;

fn setup() -> (common::TestFixture, lazyspec::engine::store::Store) {
    let fixture = common::TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-auth.md",
        "---\ntitle: \"Auth Redesign\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: [security, auth]\nrelated: []\n---\n\nAuth body content.\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-001-auth-impl.md",
        "---\ntitle: \"Auth Implementation\"\ntype: story\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-02\ntags: [security]\nrelated:\n- implements: docs/rfcs/RFC-001-auth.md\n---\n\nStory body.\n",
    );
    let store = fixture.store();
    (fixture, store)
}

#[test]
fn doc_to_json_includes_full_schema() {
    let (_fixture, store) = setup();
    let doc = store.resolve_shorthand("RFC-001").unwrap();
    let json = doc_to_json(doc);

    assert_eq!(json["path"], "docs/rfcs/RFC-001-auth.md");
    assert_eq!(json["title"], "Auth Redesign");
    assert_eq!(json["type"], "rfc");
    assert_eq!(json["status"], "accepted");
    assert_eq!(json["author"], "jkaloger");
    assert_eq!(json["date"], "2026-03-01");
    assert!(json["tags"].is_array());
    assert_eq!(json["tags"][0], "security");
    assert_eq!(json["tags"][1], "auth");
    assert!(json["related"].is_array());
}

#[test]
fn doc_to_json_includes_related() {
    let (_fixture, store) = setup();
    let doc = store.resolve_shorthand("STORY-001").unwrap();
    let json = doc_to_json(doc);

    assert_eq!(json["related"][0]["type"], "implements");
    assert_eq!(
        json["related"][0]["target"],
        "docs/rfcs/RFC-001-auth.md"
    );
}

#[test]
fn show_json_includes_body() {
    let (_fixture, store) = setup();
    let doc = store.resolve_shorthand("RFC-001").unwrap();
    let body = store.get_body(&doc.path).unwrap();
    let mut json = doc_to_json(doc);
    json["body"] = serde_json::Value::String(body);

    assert!(json["body"].as_str().unwrap().contains("Auth body content."));
    assert_eq!(json["title"], "Auth Redesign");
}

#[test]
fn show_json_output() {
    let (_fixture, store) = setup();
    let output = lazyspec::cli::show::run_json(&store, "RFC-001").unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(parsed["title"], "Auth Redesign");
    assert!(parsed["body"].as_str().unwrap().contains("Auth body content."));
    assert_eq!(parsed["author"], "jkaloger");
    assert_eq!(parsed["date"], "2026-03-01");
    assert!(parsed["tags"].is_array());
    assert!(parsed["related"].is_array());
}

#[test]
fn create_json_output() {
    let fixture = common::TestFixture::new();

    let config = fixture.config();
    let output =
        lazyspec::cli::create::run_json(fixture.root(), &config, "rfc", "New Feature", "jkaloger").unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(parsed["title"], "New Feature");
    assert_eq!(parsed["type"], "rfc");
    assert_eq!(parsed["status"], "draft");
    assert_eq!(parsed["author"], "jkaloger");
    assert!(parsed["path"].as_str().unwrap().contains("RFC-001"));
}

#[test]
fn list_json_includes_full_schema() {
    let (_fixture, store) = setup();
    let output = lazyspec::cli::list::run_json(&store, None, None);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap();

    let rfc = parsed.iter().find(|d| d["title"] == "Auth Redesign").unwrap();
    assert_eq!(rfc["author"], "jkaloger");
    assert_eq!(rfc["date"], "2026-03-01");
    assert!(rfc["tags"].is_array());
    assert!(rfc["related"].is_array());
}

#[test]
fn doc_to_json_includes_validate_ignore() {
    let fixture = common::TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-legacy.md",
        "---\ntitle: \"Legacy Doc\"\ntype: rfc\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\nvalidate-ignore: true\n---\n",
    );
    let store = fixture.store();
    let doc = store.resolve_shorthand("RFC-001").unwrap();
    let json = doc_to_json(doc);

    assert_eq!(json["validate_ignore"], true);
}

#[test]
fn doc_to_json_validate_ignore_defaults_false() {
    let (_fixture, store) = setup();
    let doc = store.resolve_shorthand("RFC-001").unwrap();
    let json = doc_to_json(doc);

    assert_eq!(json["validate_ignore"], false);
}

#[test]
fn search_json_includes_full_schema() {
    let (_fixture, store) = setup();
    let output = lazyspec::cli::search::run_json(&store, "Auth", None);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap();

    assert!(!parsed.is_empty());
    let first = &parsed[0];
    assert!(first["author"].is_string());
    assert!(first["date"].is_string());
    assert!(first["tags"].is_array());
    assert!(first["related"].is_array());
    assert!(first["match_field"].is_string());
    assert!(first["snippet"].is_string());
}
