mod common;

use common::TestFixture;
use lazyspec::engine::config::{Config, NumberingStrategy, TypeDef};
use lazyspec::engine::fs::RealFileSystem;

fn convention_config(fixture: &TestFixture) -> Config {
    let mut config = fixture.config();
    config
        .documents
        .types
        .retain(|t| t.name != "convention" && t.name != "dictum");
    config.documents.types.push(TypeDef {
        name: "convention".to_string(),
        plural: "conventions".to_string(),
        dir: "docs/convention".to_string(),
        prefix: "CONV".to_string(),
        icon: None,
        numbering: NumberingStrategy::default(),
        subdirectory: true,
        store: Default::default(),
        singleton: true,
        parent_type: None,
    });
    config.documents.types.push(TypeDef {
        name: "dictum".to_string(),
        plural: "dicta".to_string(),
        dir: "docs/convention".to_string(),
        prefix: "DICT".to_string(),
        icon: None,
        numbering: NumberingStrategy::default(),
        subdirectory: false,
        store: Default::default(),
        singleton: false,
        parent_type: Some("convention".to_string()),
    });
    config
}

fn write_convention(fixture: &TestFixture) {
    std::fs::create_dir_all(fixture.root().join("docs/convention")).unwrap();
    fixture.write_subfolder_doc(
        "docs/convention/CONV-001-project-conventions",
        "---\ntitle: \"Project Conventions\"\ntype: convention\nstatus: accepted\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n\nThese are the project conventions.\n",
    );
}

fn write_dictum(fixture: &TestFixture, name: &str, title: &str, tags: &[&str], body: &str) {
    let tags_str = tags
        .iter()
        .map(|t| format!("\"{}\"", t))
        .collect::<Vec<_>>()
        .join(", ");
    let content = format!(
        "---\ntitle: \"{}\"\ntype: dictum\nstatus: accepted\nauthor: \"test\"\ndate: 2026-01-01\ntags: [{}]\n---\n\n{}\n",
        title, tags_str, body
    );
    fixture.write_child_doc(
        "docs/convention/CONV-001-project-conventions",
        name,
        &content,
    );
}

#[test]
fn default_invocation_returns_preamble_and_all_dictum() {
    let fixture = TestFixture::new();
    write_convention(&fixture);
    write_dictum(
        &fixture,
        "DICT-001-naming.md",
        "Naming Rules",
        &[],
        "Use snake_case.",
    );
    write_dictum(
        &fixture,
        "DICT-002-testing.md",
        "Testing Rules",
        &[],
        "Write tests first.",
    );

    let config = convention_config(&fixture);
    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();
    let fs = RealFileSystem;

    let output = lazyspec::cli::convention::run_human(&store, &config, false, None, &fs).unwrap();

    assert!(
        output.contains("These are the project conventions."),
        "should contain convention body"
    );
    assert!(
        output.contains("Naming Rules"),
        "should contain first dictum title"
    );
    assert!(
        output.contains("Use snake_case."),
        "should contain first dictum body"
    );
    assert!(
        output.contains("Testing Rules"),
        "should contain second dictum title"
    );
    assert!(
        output.contains("Write tests first."),
        "should contain second dictum body"
    );
}

#[test]
fn preamble_only_returns_convention_index() {
    let fixture = TestFixture::new();
    write_convention(&fixture);
    write_dictum(
        &fixture,
        "DICT-001-naming.md",
        "Naming Rules",
        &[],
        "Use snake_case.",
    );

    let config = convention_config(&fixture);
    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();
    let fs = RealFileSystem;

    let output = lazyspec::cli::convention::run_human(&store, &config, true, None, &fs).unwrap();

    assert!(
        output.contains("These are the project conventions."),
        "should contain convention body"
    );
    assert!(
        !output.contains("Naming Rules"),
        "should not contain dictum title"
    );
}

#[test]
fn tags_single_filters_dictum() {
    let fixture = TestFixture::new();
    write_convention(&fixture);
    write_dictum(
        &fixture,
        "DICT-001-naming.md",
        "Naming Rules",
        &["testing"],
        "Use snake_case.",
    );
    write_dictum(
        &fixture,
        "DICT-002-arch.md",
        "Architecture Rules",
        &["architecture"],
        "Use hexagonal.",
    );

    let config = convention_config(&fixture);
    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();
    let fs = RealFileSystem;

    let output =
        lazyspec::cli::convention::run_human(&store, &config, false, Some("testing"), &fs).unwrap();

    assert!(
        output.contains("Naming Rules"),
        "should contain testing dictum"
    );
    assert!(
        !output.contains("Architecture Rules"),
        "should not contain architecture dictum"
    );
}

#[test]
fn tags_comma_or_logic() {
    let fixture = TestFixture::new();
    write_convention(&fixture);
    write_dictum(
        &fixture,
        "DICT-001-naming.md",
        "Naming Rules",
        &["testing"],
        "Use snake_case.",
    );
    write_dictum(
        &fixture,
        "DICT-002-arch.md",
        "Architecture Rules",
        &["architecture"],
        "Use hexagonal.",
    );
    write_dictum(
        &fixture,
        "DICT-003-errors.md",
        "Error Rules",
        &["errors"],
        "Handle errors.",
    );

    let config = convention_config(&fixture);
    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();
    let fs = RealFileSystem;

    let output = lazyspec::cli::convention::run_human(
        &store,
        &config,
        false,
        Some("testing,architecture"),
        &fs,
    )
    .unwrap();

    assert!(
        output.contains("Naming Rules"),
        "should contain testing dictum"
    );
    assert!(
        output.contains("Architecture Rules"),
        "should contain architecture dictum"
    );
    assert!(
        !output.contains("Error Rules"),
        "should not contain errors dictum"
    );
}

#[test]
fn json_output_structured() {
    let fixture = TestFixture::new();
    write_convention(&fixture);
    write_dictum(
        &fixture,
        "DICT-001-naming.md",
        "Naming Rules",
        &[],
        "Use snake_case.",
    );

    let config = convention_config(&fixture);
    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();
    let fs = RealFileSystem;

    let output = lazyspec::cli::convention::run_json(&store, &config, false, None, &fs).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(parsed["convention"]["title"], "Project Conventions");
    assert!(parsed["convention"]["body"]
        .as_str()
        .unwrap()
        .contains("These are the project conventions."));
    let dicta = parsed["dicta"].as_array().unwrap();
    assert_eq!(dicta.len(), 1);
    assert_eq!(dicta[0]["title"], "Naming Rules");
    assert!(dicta[0]["body"]
        .as_str()
        .unwrap()
        .contains("Use snake_case."));
}

#[test]
fn preamble_takes_precedence_over_tags() {
    let fixture = TestFixture::new();
    write_convention(&fixture);
    write_dictum(
        &fixture,
        "DICT-001-naming.md",
        "Naming Rules",
        &["testing"],
        "Use snake_case.",
    );

    let config = convention_config(&fixture);
    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();
    let fs = RealFileSystem;

    let output =
        lazyspec::cli::convention::run_human(&store, &config, true, Some("testing"), &fs).unwrap();

    assert!(
        output.contains("These are the project conventions."),
        "should contain convention body"
    );
    assert!(
        !output.contains("Naming Rules"),
        "should not contain dictum when preamble takes precedence"
    );
}

#[test]
fn no_convention_returns_empty_output() {
    let fixture = TestFixture::new();
    let config = convention_config(&fixture);
    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();
    let fs = RealFileSystem;

    let output = lazyspec::cli::convention::run_human(&store, &config, false, None, &fs).unwrap();
    assert!(
        output.is_empty(),
        "should return empty string when no convention exists"
    );
}

#[test]
fn no_convention_json_returns_null_convention() {
    let fixture = TestFixture::new();
    let config = convention_config(&fixture);
    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();
    let fs = RealFileSystem;

    let output = lazyspec::cli::convention::run_json(&store, &config, false, None, &fs).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert!(parsed["convention"].is_null(), "convention should be null");
    assert_eq!(
        parsed["dicta"].as_array().unwrap().len(),
        0,
        "dicta should be empty"
    );
}

#[test]
fn convention_with_no_dictum_returns_preamble() {
    let fixture = TestFixture::new();
    write_convention(&fixture);

    let config = convention_config(&fixture);
    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();
    let fs = RealFileSystem;

    let output = lazyspec::cli::convention::run_human(&store, &config, false, None, &fs).unwrap();

    assert!(
        output.contains("These are the project conventions."),
        "should contain convention body"
    );
    assert!(
        !output.contains("## "),
        "should not contain dictum heading separators"
    );
}
