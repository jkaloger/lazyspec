use lazyspec::cli::json::doc_to_json;

fn setup() -> (crate::common::TestFixture, lazyspec::engine::store::Store) {
    let fixture = crate::common::TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-auth.md",
        "---\ntitle: \"Auth Redesign\"\ntype: rfc\nstatus: accepted\nauthor: jkaloger\ndate: 2026-03-01\ntags: [security, auth]\nrelated: []\n---\n\nAuth body content.\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-001-auth-impl.md",
        "---\ntitle: \"Auth Implementation\"\ntype: story\nstatus: draft\nauthor: jkaloger\ndate: 2026-03-02\ntags: [security]\nrelated:\n- implements: RFC-001\n---\n\nStory body.\n",
    );
    let store = fixture.store();
    (fixture, store)
}

#[test]
fn doc_to_json_includes_full_schema() {
    let (_fixture, store) = setup();
    let doc = store.resolve_shorthand("RFC-001").expect("should resolve");
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
    let doc = store
        .resolve_shorthand("STORY-001")
        .expect("should resolve");
    let json = doc_to_json(doc);

    assert_eq!(json["related"][0]["type"], "implements");
    assert_eq!(json["related"][0]["target"], "RFC-001");
}

#[test]
fn show_json_includes_body() {
    let (_fixture, store) = setup();
    let doc = store.resolve_shorthand("RFC-001").expect("should resolve");
    let body = store
        .get_body(&doc.path, &lazyspec::engine::fs::RealFileSystem)
        .unwrap();
    let mut json = doc_to_json(doc);
    json["body"] = serde_json::Value::String(body);

    assert!(json["body"]
        .as_str()
        .unwrap()
        .contains("Auth body content."));
    assert_eq!(json["title"], "Auth Redesign");
}

#[test]
fn show_json_output() {
    let (fixture, store) = setup();
    let output = lazyspec::cli::show::run_json(
        &store,
        "RFC-001",
        false,
        25,
        &lazyspec::engine::fs::RealFileSystem,
        &fixture.config(),
        fixture.root(),
        &crate::common::NoopGh,
    )
    .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(parsed["title"], "Auth Redesign");
    assert!(parsed["body"]
        .as_str()
        .unwrap()
        .contains("Auth body content."));
    assert_eq!(parsed["author"], "jkaloger");
    assert_eq!(parsed["date"], "2026-03-01");
    assert!(parsed["tags"].is_array());
    assert!(parsed["related"].is_array());
}

#[test]
fn create_json_output() {
    let fixture = crate::common::TestFixture::new();

    let config = fixture.config();
    let output = lazyspec::cli::create::run_json(
        fixture.root(),
        &config,
        &fixture.store(),
        "rfc",
        "New Feature",
        "jkaloger",
        |_| {},
    )
    .unwrap();
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

    let rfc = parsed
        .iter()
        .find(|d| d["title"] == "Auth Redesign")
        .unwrap();
    assert_eq!(rfc["author"], "jkaloger");
    assert_eq!(rfc["date"], "2026-03-01");
    assert!(rfc["tags"].is_array());
    assert!(rfc["related"].is_array());
}

#[test]
fn doc_to_json_includes_validate_ignore() {
    let fixture = crate::common::TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-legacy.md",
        "---\ntitle: \"Legacy Doc\"\ntype: rfc\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\nvalidate-ignore: true\n---\n",
    );
    let store = fixture.store();
    let doc = store.resolve_shorthand("RFC-001").expect("should resolve");
    let json = doc_to_json(doc);

    assert_eq!(json["validate_ignore"], true);
}

#[test]
fn doc_to_json_validate_ignore_defaults_false() {
    let (_fixture, store) = setup();
    let doc = store.resolve_shorthand("RFC-001").expect("should resolve");
    let json = doc_to_json(doc);

    assert_eq!(json["validate_ignore"], false);
}

#[test]
fn search_json_includes_full_schema() {
    let (_fixture, store) = setup();
    let output = lazyspec::cli::search::run_json(
        &store,
        "Auth",
        None,
        &lazyspec::engine::fs::RealFileSystem,
    );
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

#[test]
fn show_json_ambiguous_id_returns_error() {
    let fixture = crate::common::TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-020-first.md",
        "---\ntitle: \"First\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n\nFirst body.\n",
    );
    fixture.write_doc(
        "docs/adrs/RFC-020-second.md",
        "---\ntitle: \"Second\"\ntype: adr\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n\nSecond body.\n",
    );
    let store = fixture.store();
    let output = lazyspec::cli::show::run_json(
        &store,
        "RFC-020",
        false,
        25,
        &lazyspec::engine::fs::RealFileSystem,
        &fixture.config(),
        fixture.root(),
        &crate::common::NoopGh,
    )
    .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(parsed["error"], "ambiguous_id");
    assert_eq!(parsed["id"], "RFC-020");
    assert!(parsed["ambiguous_matches"].is_array());
    assert_eq!(parsed["ambiguous_matches"].as_array().unwrap().len(), 2);
}

#[test]
fn show_json_full_path_works_when_shorthand_ambiguous() {
    let fixture = crate::common::TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-020-first.md",
        "---\ntitle: \"First\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n\nFirst body.\n",
    );
    fixture.write_doc(
        "docs/adrs/RFC-020-second.md",
        "---\ntitle: \"Second\"\ntype: adr\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n\nSecond body.\n",
    );
    let store = fixture.store();
    let output = lazyspec::cli::show::run_json(
        &store,
        "docs/rfcs/RFC-020-first.md",
        false,
        25,
        &lazyspec::engine::fs::RealFileSystem,
        &fixture.config(),
        fixture.root(),
        &crate::common::NoopGh,
    )
    .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(parsed["title"], "First");
    assert!(parsed["body"].as_str().unwrap().contains("First body."));
}

#[test]
fn doc_to_json_link_command_produces_id_targets() {
    let fixture = crate::common::TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-feature.md",
        "---\ntitle: \"Feature\"\ntype: rfc\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\nrelated: []\n---\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-001-impl.md",
        "---\ntitle: \"Impl\"\ntype: story\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\nrelated: []\n---\n",
    );

    let store = fixture.store();
    let fs = lazyspec::engine::fs::RealFileSystem;
    lazyspec::cli::link::link_with_config(
        fixture.root(),
        &store,
        "STORY-001",
        "implements",
        "RFC-001",
        &fs,
        Some(&fixture.config()),
    )
    .unwrap();

    // Reload store after link wrote the file
    let store = fixture.store();
    let doc = store
        .resolve_shorthand("STORY-001")
        .expect("should resolve");
    let json = doc_to_json(doc);

    assert_eq!(json["related"][0]["type"], "implements");
    assert_eq!(json["related"][0]["target"], "RFC-001");
}

// ITERATION-207 AC1: show --json exposes typed attributes (int -> number,
// enum/string -> string, date -> "YYYY-MM-DD" string).
#[test]
fn show_json_exposes_typed_attributes() {
    use std::process::Command;
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    let config = r#"
[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"

[[types.attributes]]
name = "estimate"
kind = "int"

[[types.attributes]]
name = "priority"
kind = "enum"
values = ["low", "high"]

[[types.attributes]]
name = "due"
kind = "date"

[[relationships]]
name = "related-to"

[naming]
pattern = "{type}-{n:03}-{title}.md"

[templates]
dir = ".lazyspec/templates"
"#;
    std::fs::write(root.join(".lazyspec.toml"), config).unwrap();
    std::fs::create_dir_all(root.join("docs/stories")).unwrap();
    std::fs::write(
        root.join("docs/stories/STORY-001-a.md"),
        "---\ntitle: \"A\"\ntype: story\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\nestimate: 5\npriority: high\ndue: 2026-03-15\n---\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_lazyspec"))
        .args(["show", "STORY-001", "--json"])
        .current_dir(root)
        .output()
        .expect("failed to run lazyspec show");
    assert!(
        output.status.success(),
        "show should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let parsed: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    let attrs = &parsed["attributes"];
    assert!(attrs.is_object(), "attributes must be an object");
    assert_eq!(attrs["estimate"], serde_json::json!(5));
    assert_eq!(attrs["priority"], serde_json::json!("high"));
    assert_eq!(attrs["due"], serde_json::json!("2026-03-15"));
}

// ITERATION-207 AC3: a doc with no attributes still has a stable `{}` shape.
#[test]
fn show_json_attributes_empty_object_when_none() {
    let (fixture, store) = setup();
    let output = lazyspec::cli::show::run_json(
        &store,
        "RFC-001",
        false,
        25,
        &lazyspec::engine::fs::RealFileSystem,
        &fixture.config(),
        fixture.root(),
        &crate::common::NoopGh,
    )
    .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert!(
        parsed["attributes"].is_object(),
        "attributes must be present as an object"
    );
    assert_eq!(
        parsed["attributes"].as_object().unwrap().len(),
        0,
        "attributes must be an empty object when no attrs"
    );
}

// AC7: --json serializes a relationship under its configured name, end-to-end.
#[test]
fn json_output_serializes_relationship_by_configured_name() {
    use std::process::Command;
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    // A project declaring a custom `tracks` relationship.
    let config = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[relationships]]
name = "tracks"

[[relationships]]
name = "related-to"

[naming]
pattern = "{type}-{n:03}-{title}.md"

[templates]
dir = ".lazyspec/templates"
"#;
    std::fs::write(root.join(".lazyspec.toml"), config).unwrap();
    std::fs::create_dir_all(root.join("docs/rfcs")).unwrap();
    std::fs::write(
        root.join("docs/rfcs/RFC-001-a.md"),
        "---\ntitle: \"A\"\ntype: rfc\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\nrelated:\n- tracks: RFC-002\n---\n",
    )
    .unwrap();
    std::fs::write(
        root.join("docs/rfcs/RFC-002-b.md"),
        "---\ntitle: \"B\"\ntype: rfc\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\n---\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_lazyspec"))
        .args(["show", "RFC-001", "--json"])
        .current_dir(root)
        .output()
        .expect("failed to run lazyspec show");
    assert!(
        output.status.success(),
        "show should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let parsed: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    assert_eq!(
        parsed["related"][0]["type"], "tracks",
        "relationship should serialize under its configured name"
    );
}
