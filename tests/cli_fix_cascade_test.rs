mod common;

use common::TestFixture;
use lazyspec::cli::fix::renumber::cascade_references;
use lazyspec::engine::document::DocMeta;
use std::fs;

fn fixture_with_related() -> TestFixture {
    let fixture = TestFixture::new();

    fixture.write_rfc("RFC-020-foo.md", "Foo", "accepted");

    fixture.write_story(
        "STORY-042-bar.md",
        "Bar",
        "draft",
        Some("docs/rfcs/RFC-020-foo.md"),
    );

    fixture
}

#[test]
fn cascade_updates_related_frontmatter() {
    let fixture = fixture_with_related();
    let store = fixture.store();

    let updates = cascade_references(
        fixture.root(),
        &store,
        "docs/rfcs/RFC-020-foo.md",
        "docs/rfcs/RFC-021-foo.md",
        false,
    );

    assert!(!updates.is_empty());
    let related_update = updates.iter().find(|u| u.field == "related").unwrap();
    assert_eq!(related_update.old_value, "docs/rfcs/RFC-020-foo.md");
    assert_eq!(related_update.new_value, "docs/rfcs/RFC-021-foo.md");

    let content = fs::read_to_string(
        fixture.root().join("docs/stories/STORY-042-bar.md"),
    )
    .unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert_eq!(meta.related[0].target, "docs/rfcs/RFC-021-foo.md");
}

#[test]
fn cascade_updates_body_ref_directive() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-020-foo.md", "Foo", "accepted");

    let body_content = "---\ntitle: \"Story\"\ntype: story\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\nSee @ref docs/rfcs/RFC-020-foo.md#SomeStruct for details.\n";
    fixture.write_doc("docs/stories/STORY-050-uses-ref.md", body_content);

    let store = fixture.store();

    let updates = cascade_references(
        fixture.root(),
        &store,
        "docs/rfcs/RFC-020-foo.md",
        "docs/rfcs/RFC-021-foo.md",
        false,
    );

    let body_update = updates.iter().find(|u| u.field == "body").unwrap();
    assert!(body_update.old_value.contains("docs/rfcs/RFC-020-foo.md"));
    assert!(body_update.new_value.contains("docs/rfcs/RFC-021-foo.md"));

    let content = fs::read_to_string(
        fixture.root().join("docs/stories/STORY-050-uses-ref.md"),
    )
    .unwrap();
    assert!(content.contains("docs/rfcs/RFC-021-foo.md"));
    assert!(!content.contains("docs/rfcs/RFC-020-foo.md"));
}

#[test]
fn cascade_subfolder_rename_updates_child_references() {
    let fixture = TestFixture::new();

    let parent_content = "---\ntitle: \"Parent RFC\"\ntype: rfc\nstatus: accepted\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n";
    fixture.write_subfolder_doc("docs/rfcs/RFC-020-foo", parent_content);

    fixture.write_child_doc(
        "docs/rfcs/RFC-020-foo",
        "design.md",
        "---\ntitle: \"Design\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
    );

    let story_content = "---\ntitle: \"Referencing story\"\ntype: story\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: docs/rfcs/RFC-020-foo/design.md\n---\nAlso see @ref docs/rfcs/RFC-020-foo/index.md for context.\n";
    fixture.write_doc("docs/stories/STORY-060-refs.md", story_content);

    let store = fixture.store();

    let updates = cascade_references(
        fixture.root(),
        &store,
        "docs/rfcs/RFC-020-foo/",
        "docs/rfcs/RFC-021-foo/",
        false,
    );

    assert!(updates.len() >= 2);

    let related_update = updates.iter().find(|u| u.field == "related").unwrap();
    assert_eq!(related_update.old_value, "docs/rfcs/RFC-020-foo/design.md");
    assert_eq!(related_update.new_value, "docs/rfcs/RFC-021-foo/design.md");

    let body_update = updates.iter().find(|u| u.field == "body").unwrap();
    assert!(body_update.old_value.contains("docs/rfcs/RFC-020-foo/"));
    assert!(body_update.new_value.contains("docs/rfcs/RFC-021-foo/"));
}

#[test]
fn cascade_dry_run_returns_updates_without_modifying_files() {
    let fixture = fixture_with_related();
    let store = fixture.store();

    let content_before = fs::read_to_string(
        fixture.root().join("docs/stories/STORY-042-bar.md"),
    )
    .unwrap();

    let updates = cascade_references(
        fixture.root(),
        &store,
        "docs/rfcs/RFC-020-foo.md",
        "docs/rfcs/RFC-021-foo.md",
        true,
    );

    assert!(!updates.is_empty());

    let content_after = fs::read_to_string(
        fixture.root().join("docs/stories/STORY-042-bar.md"),
    )
    .unwrap();
    assert_eq!(content_before, content_after);
}

#[test]
fn cascade_no_references_returns_empty_vec() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-unrelated.md", "Unrelated", "draft");
    fixture.write_story("STORY-001-other.md", "Other", "draft", None);

    let store = fixture.store();

    let updates = cascade_references(
        fixture.root(),
        &store,
        "docs/rfcs/RFC-999-nonexistent.md",
        "docs/rfcs/RFC-998-nonexistent.md",
        false,
    );

    assert!(updates.is_empty());
}
