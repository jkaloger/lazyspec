use lazyspec::cli::show;
use lazyspec::engine::git_ref::test_support::MockGitRefClient;
use std::fs;
use std::process::Command;

// RFC-074 AC3/AC6: a bundle folder (an `index.md` beside frontmatter-less
// `.md` parts and a non-`.md` sidecar) loads with no parse errors, and
// `show`'s human output lists a `Parts:` block after `Children:`, one line
// per part and sidecar.
fn write_bundle(fixture: &crate::common::TestFixture) {
    let dir = fixture.root().join("docs/rfcs/RFC-001-change");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("index.md"),
        "---\ntitle: \"Change\"\ntype: rfc\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\n---\n\nParent body.\n",
    )
    .unwrap();
    fs::write(dir.join("design.md"), "Design content.\n").unwrap();
    fs::write(dir.join("tasks.md"), "Tasks content.\n").unwrap();
    fs::write(dir.join("notes.yaml"), "note: sidecar\n").unwrap();
    // A `.md` beside `index.md` that carries frontmatter is a child document
    // (AC3), not a part -- present so `show`'s `Children:` block is not empty
    // and `show_human_output_lists_a_parts_block_after_children` actually
    // exercises the "Parts: follows Children:" ordering it asserts, rather
    // than skipping the check because there was nothing to find.
    fs::write(
        dir.join("follow-up.md"),
        "---\ntitle: \"Follow up\"\ntype: rfc\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\n---\n\nFollow-up body.\n",
    )
    .unwrap();
}

#[test]
fn bundle_loads_with_no_parse_errors_and_carries_parts_and_sidecars() {
    let fixture = crate::common::TestFixture::new();
    write_bundle(&fixture);

    let store = fixture.store();
    assert!(
        store.parse_errors().is_empty(),
        "got: {:?}",
        store.parse_errors()
    );

    let doc = store.resolve_shorthand("RFC-001").unwrap();
    assert_eq!(
        doc.parts
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        vec!["design", "tasks"]
    );
    assert_eq!(doc.sidecars.len(), 1);
}

#[test]
fn show_human_output_lists_a_parts_block_after_children() {
    let fixture = crate::common::TestFixture::new();
    write_bundle(&fixture);
    let store = fixture.store();

    let mut out = Vec::new();
    show::run(
        &mut out,
        &store,
        "RFC-001",
        show::ShowArgs {
            expand: false,
            max_ref_lines: 25,
            fs: &lazyspec::engine::fs::RealFileSystem,
            config: &fixture.config(),
            git: &MockGitRefClient::new(),
            parts: false,
        },
    )
    .unwrap();
    let text = String::from_utf8(out).unwrap();

    let parts_pos = text.find("Parts:").expect("Parts: block present");
    let children_pos = text.find("Children:").expect("Children: block present");
    assert!(parts_pos > children_pos, "Parts: must follow Children:");
    assert!(text.contains("design"), "got: {text}");
    assert!(text.contains("tasks"), "got: {text}");
    assert!(text.contains("notes.yaml"), "got: {text}");
}

#[test]
fn show_parts_flag_concatenates_index_and_part_bodies_in_order() {
    let fixture = crate::common::TestFixture::new();
    write_bundle(&fixture);
    let store = fixture.store();

    let mut out = Vec::new();
    show::run(
        &mut out,
        &store,
        "RFC-001",
        show::ShowArgs {
            expand: false,
            max_ref_lines: 25,
            fs: &lazyspec::engine::fs::RealFileSystem,
            config: &fixture.config(),
            git: &MockGitRefClient::new(),
            parts: true,
        },
    )
    .unwrap();
    let text = String::from_utf8(out).unwrap();

    let parent_pos = text.find("Parent body.").expect("parent body present");
    let design_heading = text.find("## design").expect("design heading present");
    let design_body = text.find("Design content.").expect("design body present");
    let tasks_heading = text.find("## tasks").expect("tasks heading present");
    let tasks_body = text.find("Tasks content.").expect("tasks body present");

    assert!(parent_pos < design_heading, "parent body comes first");
    assert!(design_heading < design_body, "heading precedes its body");
    assert!(
        design_body < tasks_heading,
        "design part precedes tasks part"
    );
    assert!(tasks_heading < tasks_body);
}

#[test]
fn show_json_always_carries_parts_and_sidecars_without_body() {
    let fixture = crate::common::TestFixture::new();
    write_bundle(&fixture);
    let store = fixture.store();

    let output = show::run_json(
        &store,
        "RFC-001",
        false,
        25,
        &lazyspec::engine::fs::RealFileSystem,
        &fixture.config(),
        fixture.root(),
        &crate::common::NoopGh,
        &MockGitRefClient::new(),
        false,
    )
    .unwrap();
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();

    let parts = json["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0]["name"], "design");
    assert!(
        parts[0].get("body").is_none(),
        "no --parts flag: no body field, got {parts:?}"
    );
    assert_eq!(json["sidecars"].as_array().unwrap().len(), 1);
}

#[test]
fn show_json_parts_flag_adds_body_to_each_part() {
    let fixture = crate::common::TestFixture::new();
    write_bundle(&fixture);
    let store = fixture.store();

    let output = show::run_json(
        &store,
        "RFC-001",
        false,
        25,
        &lazyspec::engine::fs::RealFileSystem,
        &fixture.config(),
        fixture.root(),
        &crate::common::NoopGh,
        &MockGitRefClient::new(),
        true,
    )
    .unwrap();
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();

    let parts = json["parts"].as_array().unwrap();
    assert_eq!(parts[0]["name"], "design");
    assert_eq!(parts[0]["body"], "Design content.\n");
    assert_eq!(parts[1]["name"], "tasks");
    assert_eq!(parts[1]["body"], "Tasks content.\n");
}

// RFC-074 AC7: `update --part <name> --body` overwrites that part's file,
// leaving the parent's frontmatter and other parts untouched.
#[test]
fn update_part_overwrites_an_existing_part() {
    let fixture = crate::common::TestFixture::new();
    write_bundle(&fixture);
    let store = fixture.store();
    let config = fixture.config();

    lazyspec::cli::update::run_part(
        fixture.root(),
        &config,
        &store,
        "RFC-001",
        "design",
        "Updated design.",
        &MockGitRefClient::new(),
    )
    .unwrap();

    let design_path = fixture.root().join("docs/rfcs/RFC-001-change/design.md");
    assert_eq!(
        fs::read_to_string(&design_path).unwrap(),
        "Updated design.\n"
    );
    let index_path = fixture.root().join("docs/rfcs/RFC-001-change/index.md");
    assert!(fs::read_to_string(&index_path)
        .unwrap()
        .contains("title: \"Change\""));
}

// RFC-074 AC6: `list --json` carries `parts`/`sidecars` per document, the
// same shape `show --json` reports -- both route through
// `doc_to_json_with_family`, which wraps `doc_to_json`.
#[test]
fn list_json_carries_parts_and_sidecars() {
    let fixture = crate::common::TestFixture::new();
    write_bundle(&fixture);
    let store = fixture.store();

    let output = lazyspec::cli::list::run_json(&store, None, None);
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();
    let items = json.as_array().unwrap();
    let doc = items
        .iter()
        .find(|d| d["id"] == "RFC-001")
        .expect("RFC-001 present");

    let parts = doc["parts"].as_array().unwrap();
    assert_eq!(
        parts
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["design", "tasks"]
    );
    assert_eq!(doc["sidecars"].as_array().unwrap().len(), 1);
}

// RFC-074 AC6: `context --json` (single-id chain form) carries `parts`/
// `sidecars` on its chain entries too.
#[test]
fn context_json_carries_parts_and_sidecars() {
    let fixture = crate::common::TestFixture::new();
    write_bundle(&fixture);
    let store = fixture.store();

    let output = lazyspec::cli::context::run_json(&store, "RFC-001", 1).unwrap();
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();
    let chain = json["chain"].as_array().unwrap();
    let doc = chain
        .iter()
        .find(|d| d["id"] == "RFC-001")
        .expect("RFC-001 present in chain");

    let parts = doc["parts"].as_array().unwrap();
    assert_eq!(
        parts
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["design", "tasks"]
    );
    assert_eq!(doc["sidecars"].as_array().unwrap().len(), 1);
}

// RFC-074 AC7: `update --part` creates the part when the document has none by
// that name yet.
#[test]
fn update_part_creates_an_absent_part() {
    let fixture = crate::common::TestFixture::new();
    write_bundle(&fixture);
    let store = fixture.store();
    let config = fixture.config();

    lazyspec::cli::update::run_part(
        fixture.root(),
        &config,
        &store,
        "RFC-001",
        "risks",
        "New risks section.",
        &MockGitRefClient::new(),
    )
    .unwrap();

    let risks_path = fixture.root().join("docs/rfcs/RFC-001-change/risks.md");
    assert_eq!(
        fs::read_to_string(&risks_path).unwrap(),
        "New risks section.\n"
    );
}

// STORY-291 AC6: `-e` expands `@ref` inside a part's body too, not just the
// index's -- `show --parts -e` (human) and `show --json --parts -e`
// (`expand=true`) both read a part through `Store::get_part_body_expanded`.
fn write_bundle_with_a_ref_in_a_part(fixture: &crate::common::TestFixture) {
    Command::new("git")
        .args(["init", "-q"])
        .current_dir(fixture.root())
        .status()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(fixture.root())
        .status()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(fixture.root())
        .status()
        .unwrap();

    let dir = fixture.root().join("docs/rfcs/RFC-001-change");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("index.md"),
        "---\ntitle: \"Change\"\ntype: rfc\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\n---\n\nParent body.\n",
    )
    .unwrap();
    fs::write(
        dir.join("design.md"),
        "See the code:\n\n@ref referenced.txt\n",
    )
    .unwrap();
    fs::write(fixture.root().join("referenced.txt"), "referenced content").unwrap();

    Command::new("git")
        .args(["add", "-A"])
        .current_dir(fixture.root())
        .status()
        .unwrap();
    Command::new("git")
        .args(["commit", "-q", "-m", "bundle with a ref"])
        .current_dir(fixture.root())
        .status()
        .unwrap();
}

#[test]
fn show_parts_expand_resolves_a_ref_inside_a_part_body() {
    let fixture = crate::common::TestFixture::new();
    write_bundle_with_a_ref_in_a_part(&fixture);
    let store = fixture.store();

    let mut out = Vec::new();
    show::run(
        &mut out,
        &store,
        "RFC-001",
        show::ShowArgs {
            expand: true,
            max_ref_lines: 25,
            fs: &lazyspec::engine::fs::RealFileSystem,
            config: &fixture.config(),
            git: &MockGitRefClient::new(),
            parts: true,
        },
    )
    .unwrap();
    let text = String::from_utf8(out).unwrap();

    assert!(
        text.contains("referenced content"),
        "the part's @ref must expand to the referenced file's content, got: {text}"
    );
    assert!(
        !text.contains("@ref referenced.txt"),
        "the raw @ref directive must not survive expansion, got: {text}"
    );
}

#[test]
fn show_json_parts_expand_resolves_a_ref_inside_a_part_body() {
    let fixture = crate::common::TestFixture::new();
    write_bundle_with_a_ref_in_a_part(&fixture);
    let store = fixture.store();

    let output = show::run_json(
        &store,
        "RFC-001",
        true,
        25,
        &lazyspec::engine::fs::RealFileSystem,
        &fixture.config(),
        fixture.root(),
        &crate::common::NoopGh,
        &MockGitRefClient::new(),
        true,
    )
    .unwrap();
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();

    let design = json["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "design")
        .expect("design part present");
    let body = design["body"].as_str().unwrap();
    assert!(body.contains("referenced content"), "got: {body}");
    assert!(!body.contains("@ref referenced.txt"), "got: {body}");
}
