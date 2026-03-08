mod common;

use lazyspec::engine::document::{DocMeta, split_frontmatter};

#[test]
fn fix_fills_missing_fields() {
    let fixture = common::TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-broken.md",
        "---\ntitle: \"Broken\"\ntype: rfc\nauthor: test\ndate: 2026-01-01\n---\n",
    );

    let store = fixture.store();
    lazyspec::cli::fix::run_human(
        fixture.root(),
        &store,
        &fixture.config(),
        &vec!["docs/rfcs/RFC-broken.md".to_string()],
        false,
    );

    let content = std::fs::read_to_string(fixture.root().join("docs/rfcs/RFC-broken.md")).unwrap();
    let (yaml_str, _) = split_frontmatter(&content).unwrap();
    let value: serde_yaml::Value = serde_yaml::from_str(&yaml_str).unwrap();
    let map = value.as_mapping().unwrap();

    assert_eq!(
        map.get(&serde_yaml::Value::String("title".into())).unwrap(),
        &serde_yaml::Value::String("Broken".into()),
    );
    assert_eq!(
        map.get(&serde_yaml::Value::String("type".into())).unwrap(),
        &serde_yaml::Value::String("rfc".into()),
    );
    assert_eq!(
        map.get(&serde_yaml::Value::String("author".into())).unwrap(),
        &serde_yaml::Value::String("test".into()),
    );
    assert_eq!(
        map.get(&serde_yaml::Value::String("date".into())).unwrap(),
        &serde_yaml::Value::String("2026-01-01".into()),
    );
    assert_eq!(
        map.get(&serde_yaml::Value::String("status".into())).unwrap(),
        &serde_yaml::Value::String("draft".into()),
    );
    let tags = map.get(&serde_yaml::Value::String("tags".into())).unwrap();
    assert_eq!(tags.as_sequence().unwrap().len(), 0);
}

#[test]
fn fix_preserves_body() {
    let fixture = common::TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-body.md",
        "---\ntitle: \"Body Test\"\ntype: rfc\nauthor: test\ndate: 2026-01-01\ntags: []\n---\n## Hello\n\nWorld\n",
    );

    let store = fixture.store();
    lazyspec::cli::fix::run_human(
        fixture.root(),
        &store,
        &fixture.config(),
        &vec!["docs/rfcs/RFC-body.md".to_string()],
        false,
    );

    let content = std::fs::read_to_string(fixture.root().join("docs/rfcs/RFC-body.md")).unwrap();
    let (_, body) = split_frontmatter(&content).unwrap();
    assert!(body.contains("## Hello"));
    assert!(body.contains("World"));
}

#[test]
fn fix_dry_run_does_not_write() {
    let fixture = common::TestFixture::new();
    let original = "---\ntitle: \"Dry\"\ntype: rfc\nauthor: test\ndate: 2026-01-01\n---\n";
    fixture.write_doc("docs/rfcs/RFC-dry.md", original);

    let store = fixture.store();
    let output = lazyspec::cli::fix::run_human(
        fixture.root(),
        &store,
        &fixture.config(),
        &vec!["docs/rfcs/RFC-dry.md".to_string()],
        true,
    );

    assert!(output.contains("Would fix"));
    let content = std::fs::read_to_string(fixture.root().join("docs/rfcs/RFC-dry.md")).unwrap();
    assert_eq!(content, original);
}

#[test]
fn fix_all_broken_docs() {
    let fixture = common::TestFixture::new();
    // RFC missing status
    fixture.write_doc(
        "docs/rfcs/RFC-a.md",
        "---\ntitle: \"A\"\ntype: rfc\nauthor: test\ndate: 2026-01-01\ntags: []\n---\n",
    );
    // Story missing date
    fixture.write_doc(
        "docs/stories/STORY-b.md",
        "---\ntitle: \"B\"\ntype: story\nstatus: draft\nauthor: test\ntags: []\n---\n",
    );

    let store = fixture.store();
    lazyspec::cli::fix::run_human(
        fixture.root(),
        &store,
        &fixture.config(),
        &vec![],
        false,
    );

    let content_a = std::fs::read_to_string(fixture.root().join("docs/rfcs/RFC-a.md")).unwrap();
    let content_b = std::fs::read_to_string(fixture.root().join("docs/stories/STORY-b.md")).unwrap();
    assert!(DocMeta::parse(&content_a).is_ok());
    assert!(DocMeta::parse(&content_b).is_ok());
}

#[test]
fn fix_json_output() {
    let fixture = common::TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-json.md",
        "---\ntitle: \"JSON\"\ntype: rfc\nauthor: test\ndate: 2026-01-01\n---\n",
    );

    let store = fixture.store();
    let output = lazyspec::cli::fix::run_json(
        fixture.root(),
        &store,
        &fixture.config(),
        &vec!["docs/rfcs/RFC-json.md".to_string()],
        false,
    );

    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    let arr = parsed.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert!(arr[0]["path"].is_string());
    assert!(arr[0]["fields_added"].as_array().unwrap().len() > 0);
    assert!(arr[0]["written"].is_boolean());
}

#[test]
fn fix_infers_type_from_directory() {
    let fixture = common::TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-notype.md",
        "---\ntitle: \"No Type\"\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\n---\n",
    );

    let store = fixture.store();
    lazyspec::cli::fix::run_human(
        fixture.root(),
        &store,
        &fixture.config(),
        &vec!["docs/rfcs/RFC-notype.md".to_string()],
        false,
    );

    let content = std::fs::read_to_string(fixture.root().join("docs/rfcs/RFC-notype.md")).unwrap();
    let (yaml_str, _) = split_frontmatter(&content).unwrap();
    let value: serde_yaml::Value = serde_yaml::from_str(&yaml_str).unwrap();
    let map = value.as_mapping().unwrap();
    assert_eq!(
        map.get(&serde_yaml::Value::String("type".into())).unwrap(),
        &serde_yaml::Value::String("rfc".into()),
    );
}
