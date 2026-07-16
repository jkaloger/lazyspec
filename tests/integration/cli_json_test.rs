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
    // AUDIT-018 F5 / STORY-210 AC4: the created doc's real assigned id, not "".
    assert_eq!(parsed["id"], "RFC-001");
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

/// Run the lazyspec binary in `root`, asserting success, returning stdout.
fn run_lazyspec(root: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lazyspec"))
        .args(args)
        .current_dir(root)
        .output()
        .expect("failed to run lazyspec");
    assert!(
        output.status.success(),
        "lazyspec {:?} should succeed, stderr: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

// ITERATION-299 / STORY-211 AC1: delete --json emits the structured outcome.
#[test]
fn delete_json_output() {
    let fixture = crate::common::TestFixture::new();
    fixture.write_rfc("RFC-001-auth.md", "Auth", "draft");

    let stdout = run_lazyspec(fixture.root(), &["delete", "RFC-001", "--json"]);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(parsed["action"], "deleted");
    assert_eq!(parsed["id"], "RFC-001");
    assert_eq!(parsed["path"], "docs/rfcs/RFC-001-auth.md");
    assert!(!fixture.root().join("docs/rfcs/RFC-001-auth.md").exists());
}

// ITERATION-299 / STORY-211 AC1: link --json emits the written edge.
#[test]
fn link_json_output() {
    let fixture = crate::common::TestFixture::new();
    fixture.write_rfc("RFC-001-auth.md", "Auth", "draft");
    fixture.write_story("STORY-001-impl.md", "Impl", "draft", None);

    let stdout = run_lazyspec(
        fixture.root(),
        &["link", "STORY-001", "implements", "RFC-001", "--json"],
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(parsed["action"], "linked");
    assert_eq!(parsed["source"], "docs/stories/STORY-001-impl.md");
    assert_eq!(parsed["rel_type"], "implements");
    assert_eq!(parsed["target"], "RFC-001");
}

// ITERATION-299 / STORY-211 AC1: unlink --json emits the removed edge.
#[test]
fn unlink_json_output() {
    let fixture = crate::common::TestFixture::new();
    fixture.write_rfc("RFC-001-auth.md", "Auth", "draft");
    fixture.write_story("STORY-001-impl.md", "Impl", "draft", Some("RFC-001"));

    let stdout = run_lazyspec(
        fixture.root(),
        &["unlink", "STORY-001", "implements", "RFC-001", "--json"],
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(parsed["action"], "unlinked");
    assert_eq!(parsed["source"], "docs/stories/STORY-001-impl.md");
    assert_eq!(parsed["rel_type"], "implements");
    assert_eq!(parsed["target"], "RFC-001");
    let content =
        std::fs::read_to_string(fixture.root().join("docs/stories/STORY-001-impl.md")).unwrap();
    assert!(!content.contains("implements: RFC-001"));
}

// ITERATION-299 / STORY-211 AC1: ignore --json emits the outcome and new state.
#[test]
fn ignore_json_output() {
    let fixture = crate::common::TestFixture::new();
    fixture.write_rfc("RFC-001-auth.md", "Auth", "draft");

    let stdout = run_lazyspec(fixture.root(), &["ignore", "RFC-001", "--json"]);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(parsed["action"], "ignored");
    assert_eq!(parsed["id"], "RFC-001");
    assert_eq!(parsed["path"], "docs/rfcs/RFC-001-auth.md");
    assert_eq!(parsed["validate_ignore"], true);
    let content =
        std::fs::read_to_string(fixture.root().join("docs/rfcs/RFC-001-auth.md")).unwrap();
    assert!(content.contains("validate-ignore: true"));
}

// ITERATION-299 / STORY-211 AC1: unignore --json emits the outcome and new state.
#[test]
fn unignore_json_output() {
    let fixture = crate::common::TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-auth.md",
        "---\ntitle: \"Auth\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\nvalidate-ignore: true\n---\n",
    );

    let stdout = run_lazyspec(fixture.root(), &["unignore", "RFC-001", "--json"]);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(parsed["action"], "unignored");
    assert_eq!(parsed["id"], "RFC-001");
    assert_eq!(parsed["path"], "docs/rfcs/RFC-001-auth.md");
    assert_eq!(parsed["validate_ignore"], false);
    let content =
        std::fs::read_to_string(fixture.root().join("docs/rfcs/RFC-001-auth.md")).unwrap();
    assert!(!content.contains("validate-ignore"));
}

// ITERATION-299 / STORY-211 AC2: without --json the human output is unchanged.
#[test]
fn mutating_commands_human_output_unchanged() {
    let fixture = crate::common::TestFixture::new();
    fixture.write_rfc("RFC-001-auth.md", "Auth", "draft");
    fixture.write_story("STORY-001-impl.md", "Impl", "draft", None);
    let root = fixture.root();

    assert_eq!(
        run_lazyspec(root, &["link", "STORY-001", "implements", "RFC-001"]),
        "Linked docs/stories/STORY-001-impl.md --implements--> RFC-001\n"
    );
    assert_eq!(
        run_lazyspec(root, &["unlink", "STORY-001", "implements", "RFC-001"]),
        "Unlinked docs/stories/STORY-001-impl.md --implements--> RFC-001\n"
    );
    assert_eq!(
        run_lazyspec(root, &["ignore", "RFC-001"]),
        "Ignoring docs/rfcs/RFC-001-auth.md\n"
    );
    assert_eq!(
        run_lazyspec(root, &["unignore", "RFC-001"]),
        "Unignoring docs/rfcs/RFC-001-auth.md\n"
    );
    assert_eq!(
        run_lazyspec(root, &["delete", "RFC-001"]),
        "Deleted docs/rfcs/RFC-001-auth.md\n"
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
