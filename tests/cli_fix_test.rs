mod common;

use lazyspec::engine::document::{split_frontmatter, DocMeta};
use lazyspec::engine::fs::RealFileSystem;

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
        &["docs/rfcs/RFC-broken.md".to_string()],
        false,
        &RealFileSystem,
    );

    let content = std::fs::read_to_string(fixture.root().join("docs/rfcs/RFC-broken.md")).unwrap();
    let (yaml_str, _) = split_frontmatter(&content).unwrap();
    let value: serde_yaml::Value = serde_yaml::from_str(&yaml_str).unwrap();
    let map = value.as_mapping().unwrap();

    assert_eq!(
        map.get(serde_yaml::Value::String("title".into())).unwrap(),
        &serde_yaml::Value::String("Broken".into()),
    );
    assert_eq!(
        map.get(serde_yaml::Value::String("type".into())).unwrap(),
        &serde_yaml::Value::String("rfc".into()),
    );
    assert_eq!(
        map.get(serde_yaml::Value::String("author".into())).unwrap(),
        &serde_yaml::Value::String("test".into()),
    );
    assert_eq!(
        map.get(serde_yaml::Value::String("date".into())).unwrap(),
        &serde_yaml::Value::String("2026-01-01".into()),
    );
    assert_eq!(
        map.get(serde_yaml::Value::String("status".into())).unwrap(),
        &serde_yaml::Value::String("draft".into()),
    );
    let tags = map.get(serde_yaml::Value::String("tags".into())).unwrap();
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
        &["docs/rfcs/RFC-body.md".to_string()],
        false,
        &RealFileSystem,
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
        &["docs/rfcs/RFC-dry.md".to_string()],
        true,
        &RealFileSystem,
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
        &[],
        false,
        &RealFileSystem,
    );

    let content_a = std::fs::read_to_string(fixture.root().join("docs/rfcs/RFC-a.md")).unwrap();
    let content_b =
        std::fs::read_to_string(fixture.root().join("docs/stories/STORY-b.md")).unwrap();
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
        &["docs/rfcs/RFC-json.md".to_string()],
        false,
        &RealFileSystem,
    );

    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    let obj = parsed.as_object().unwrap();
    assert!(obj.contains_key("field_fixes"));
    assert!(obj.contains_key("conflict_fixes"));
    let arr = obj["field_fixes"].as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert!(arr[0]["path"].is_string());
    assert!(!arr[0]["fields_added"].as_array().unwrap().is_empty());
    assert!(arr[0]["written"].is_boolean());
}

#[test]
fn fix_conflict_older_wins() {
    let fixture = common::TestFixture::new();
    // Two docs with same ID (RFC-001), different dates
    fixture.write_doc(
        "docs/rfcs/RFC-001-older.md",
        "---\ntitle: \"RFC-001 Older\"\ntype: rfc\nstatus: draft\nauthor: test\ndate: 2025-01-01\ntags: []\n---\n",
    );
    fixture.write_doc(
        "docs/rfcs/RFC-001-newer.md",
        "---\ntitle: \"RFC-001 Newer\"\ntype: rfc\nstatus: draft\nauthor: test\ndate: 2026-06-01\ntags: []\n---\n",
    );

    let store = fixture.store();
    let output = lazyspec::cli::fix::run_json(
        fixture.root(),
        &store,
        &fixture.config(),
        &[],
        false,
        &RealFileSystem,
    );

    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    let conflicts = parsed["conflict_fixes"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1);

    let fix = &conflicts[0];
    // The newer doc should be renamed (older wins)
    assert!(fix["old_id"].as_str().unwrap() == "RFC-001");
    assert!(fix["new_id"].as_str().unwrap() != "RFC-001");
    assert!(fix["written"].as_bool().unwrap());

    // The older doc should still exist at its original path
    assert!(fixture.root().join("docs/rfcs/RFC-001-older.md").exists());
    // The newer doc should have been renamed
    assert!(!fixture.root().join("docs/rfcs/RFC-001-newer.md").exists());
}

#[test]
fn fix_conflict_dry_run_no_side_effects() {
    let fixture = common::TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-alpha.md",
        "---\ntitle: \"RFC-001 Alpha\"\ntype: rfc\nstatus: draft\nauthor: test\ndate: 2025-01-01\ntags: []\n---\n",
    );
    fixture.write_doc(
        "docs/rfcs/RFC-001-beta.md",
        "---\ntitle: \"RFC-001 Beta\"\ntype: rfc\nstatus: draft\nauthor: test\ndate: 2026-06-01\ntags: []\n---\n",
    );

    let store = fixture.store();
    let output = lazyspec::cli::fix::run_json(
        fixture.root(),
        &store,
        &fixture.config(),
        &[],
        true,
        &RealFileSystem,
    );

    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    let conflicts = parsed["conflict_fixes"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1);
    assert!(!conflicts[0]["written"].as_bool().unwrap());

    // Both files should still exist
    assert!(fixture.root().join("docs/rfcs/RFC-001-alpha.md").exists());
    assert!(fixture.root().join("docs/rfcs/RFC-001-beta.md").exists());
}

#[test]
fn fix_no_conflicts_empty_array() {
    let fixture = common::TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-unique.md",
        "---\ntitle: \"RFC-001 Unique\"\ntype: rfc\nstatus: draft\nauthor: test\ndate: 2025-01-01\ntags: []\n---\n",
    );
    fixture.write_doc(
        "docs/rfcs/RFC-002-other.md",
        "---\ntitle: \"RFC-002 Other\"\ntype: rfc\nstatus: draft\nauthor: test\ndate: 2025-01-01\ntags: []\n---\n",
    );

    let store = fixture.store();
    let output = lazyspec::cli::fix::run_json(
        fixture.root(),
        &store,
        &fixture.config(),
        &[],
        false,
        &RealFileSystem,
    );

    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    let conflicts = parsed["conflict_fixes"].as_array().unwrap();
    assert!(conflicts.is_empty());
}

#[test]
fn fix_conflict_subfolder_rename() {
    let fixture = common::TestFixture::new();
    // First RFC-001 as flat file (older)
    fixture.write_doc(
        "docs/rfcs/RFC-001-flat.md",
        "---\ntitle: \"RFC-001 Flat\"\ntype: rfc\nstatus: draft\nauthor: test\ndate: 2025-01-01\ntags: []\n---\n",
    );
    // Second RFC-001 as subfolder (newer)
    fixture.write_subfolder_doc(
        "docs/rfcs/RFC-001-folder",
        "---\ntitle: \"RFC-001 Folder\"\ntype: rfc\nstatus: draft\nauthor: test\ndate: 2026-06-01\ntags: []\n---\n",
    );

    let store = fixture.store();
    let output = lazyspec::cli::fix::run_json(
        fixture.root(),
        &store,
        &fixture.config(),
        &[],
        false,
        &RealFileSystem,
    );

    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    let conflicts = parsed["conflict_fixes"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1);
    assert!(conflicts[0]["written"].as_bool().unwrap());

    // Flat file should remain
    assert!(fixture.root().join("docs/rfcs/RFC-001-flat.md").exists());
    // Original subfolder should be gone
    assert!(!fixture.root().join("docs/rfcs/RFC-001-folder").exists());
}

#[test]
fn fix_conflict_human_output_rename_message() {
    let fixture = common::TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-first.md",
        "---\ntitle: \"RFC-001 First\"\ntype: rfc\nstatus: draft\nauthor: test\ndate: 2025-01-01\ntags: []\n---\n",
    );
    fixture.write_doc(
        "docs/rfcs/RFC-001-second.md",
        "---\ntitle: \"RFC-001 Second\"\ntype: rfc\nstatus: draft\nauthor: test\ndate: 2026-06-01\ntags: []\n---\n",
    );

    let store = fixture.store();
    let output = lazyspec::cli::fix::run_human(
        fixture.root(),
        &store,
        &fixture.config(),
        &[],
        false,
        &RealFileSystem,
    );

    assert!(output.contains("Renamed"));
}

#[test]
fn fix_conflict_human_output_dry_run_would_rename() {
    let fixture = common::TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-first.md",
        "---\ntitle: \"RFC-001 First\"\ntype: rfc\nstatus: draft\nauthor: test\ndate: 2025-01-01\ntags: []\n---\n",
    );
    fixture.write_doc(
        "docs/rfcs/RFC-001-second.md",
        "---\ntitle: \"RFC-001 Second\"\ntype: rfc\nstatus: draft\nauthor: test\ndate: 2026-06-01\ntags: []\n---\n",
    );

    let store = fixture.store();
    let output = lazyspec::cli::fix::run_human(
        fixture.root(),
        &store,
        &fixture.config(),
        &[],
        true,
        &RealFileSystem,
    );

    assert!(output.contains("Would rename"));
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
        &["docs/rfcs/RFC-notype.md".to_string()],
        false,
        &RealFileSystem,
    );

    let content = std::fs::read_to_string(fixture.root().join("docs/rfcs/RFC-notype.md")).unwrap();
    let (yaml_str, _) = split_frontmatter(&content).unwrap();
    let value: serde_yaml::Value = serde_yaml::from_str(&yaml_str).unwrap();
    let map = value.as_mapping().unwrap();
    assert_eq!(
        map.get(serde_yaml::Value::String("type".into())).unwrap(),
        &serde_yaml::Value::String("rfc".into()),
    );
}

#[test]
fn fix_migrates_path_targets_to_ids() {
    let fixture = common::TestFixture::new();

    // Create an RFC that will be the target
    fixture.write_doc(
        "docs/rfcs/RFC-001-target.md",
        "---\ntitle: \"RFC-001 Target\"\ntype: rfc\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\n---\n",
    );

    // Create a story that references the RFC by path
    fixture.write_doc(
        "docs/stories/STORY-001-impl.md",
        "---\ntitle: \"STORY-001 Impl\"\ntype: story\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: docs/rfcs/RFC-001-target.md\n---\n",
    );

    let store = fixture.store();
    let output = lazyspec::cli::fix::run_json(
        fixture.root(),
        &store,
        &fixture.config(),
        &[],
        false,
        &RealFileSystem,
    );

    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    let relation_fixes = parsed["relation_fixes"].as_array().unwrap();
    assert_eq!(relation_fixes.len(), 1);
    assert_eq!(
        relation_fixes[0]["path"].as_str().unwrap(),
        "docs/stories/STORY-001-impl.md"
    );
    assert!(relation_fixes[0]["written"].as_bool().unwrap());

    // Verify the file was actually updated
    let content =
        std::fs::read_to_string(fixture.root().join("docs/stories/STORY-001-impl.md")).unwrap();
    let (yaml_str, _) = split_frontmatter(&content).unwrap();
    // The path target should now be an ID
    assert!(!yaml_str.contains("docs/rfcs/RFC-001-target.md"));
    assert!(yaml_str.contains("RFC-001"));
}

#[test]
fn fix_migrates_path_targets_dry_run() {
    let fixture = common::TestFixture::new();

    fixture.write_doc(
        "docs/rfcs/RFC-001-target.md",
        "---\ntitle: \"RFC-001 Target\"\ntype: rfc\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\n---\n",
    );

    fixture.write_doc(
        "docs/stories/STORY-001-ref.md",
        "---\ntitle: \"STORY-001 Ref\"\ntype: story\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: docs/rfcs/RFC-001-target.md\n---\n",
    );

    let store = fixture.store();
    let output = lazyspec::cli::fix::run_json(
        fixture.root(),
        &store,
        &fixture.config(),
        &[],
        true,
        &RealFileSystem,
    );

    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    let relation_fixes = parsed["relation_fixes"].as_array().unwrap();
    assert_eq!(relation_fixes.len(), 1);
    assert!(!relation_fixes[0]["written"].as_bool().unwrap());

    // File should still have path target
    let content =
        std::fs::read_to_string(fixture.root().join("docs/stories/STORY-001-ref.md")).unwrap();
    assert!(content.contains("docs/rfcs/RFC-001-target.md"));
}

#[test]
fn fix_no_relation_fixes_when_already_ids() {
    let fixture = common::TestFixture::new();

    fixture.write_doc(
        "docs/rfcs/RFC-001-target.md",
        "---\ntitle: \"RFC-001 Target\"\ntype: rfc\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\n---\n",
    );

    // Story already references by ID
    fixture.write_doc(
        "docs/stories/STORY-001-good.md",
        "---\ntitle: \"STORY-001 Good\"\ntype: story\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: RFC-001\n---\n",
    );

    let store = fixture.store();
    let output = lazyspec::cli::fix::run_json(
        fixture.root(),
        &store,
        &fixture.config(),
        &[],
        false,
        &RealFileSystem,
    );

    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    let relation_fixes = parsed["relation_fixes"].as_array().unwrap();
    assert!(relation_fixes.is_empty());
}

#[test]
fn fix_human_output_relation_migration() {
    let fixture = common::TestFixture::new();

    fixture.write_doc(
        "docs/rfcs/RFC-001-target.md",
        "---\ntitle: \"RFC-001 Target\"\ntype: rfc\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\n---\n",
    );

    fixture.write_doc(
        "docs/stories/STORY-001-human.md",
        "---\ntitle: \"STORY-001 Human\"\ntype: story\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: docs/rfcs/RFC-001-target.md\n---\n",
    );

    let store = fixture.store();
    let output = lazyspec::cli::fix::run_human(
        fixture.root(),
        &store,
        &fixture.config(),
        &[],
        false,
        &RealFileSystem,
    );

    assert!(output.contains("Migrated relation"));
    assert!(output.contains("docs/rfcs/RFC-001-target.md"));
    assert!(output.contains("RFC-001"));
}
