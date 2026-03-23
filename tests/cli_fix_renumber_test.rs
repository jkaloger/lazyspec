mod common;

use lazyspec::cli::RenumberFormat;

fn sqids_config() -> lazyspec::engine::config::Config {
    let toml = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"

[[types]]
name = "iteration"
plural = "iterations"
dir = "docs/iterations"
prefix = "ITERATION"

[[types]]
name = "adr"
plural = "adrs"
dir = "docs/adrs"
prefix = "ADR"

[numbering.sqids]
salt = "test-renumber-salt"
min_length = 3
"#;
    lazyspec::engine::config::Config::parse(toml).unwrap()
}

fn valid_doc(title: &str, doc_type: &str) -> String {
    format!(
        "---\ntitle: \"{}\"\ntype: {}\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\n---\n",
        title, doc_type
    )
}

fn valid_doc_with_related(title: &str, doc_type: &str, related_path: &str) -> String {
    format!(
        "---\ntitle: \"{}\"\ntype: {}\nstatus: draft\nauthor: test\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: {}\n---\n",
        title, doc_type, related_path
    )
}

// AC-1: incremental -> sqids renames files correctly
#[test]
fn renumber_incremental_to_sqids() {
    let fixture = common::TestFixture::new();
    let config = sqids_config();

    fixture.write_doc("docs/rfcs/RFC-001-foo.md", &valid_doc("RFC-001 Foo", "rfc"));
    fixture.write_doc("docs/rfcs/RFC-002-bar.md", &valid_doc("RFC-002 Bar", "rfc"));

    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();

    let exit_code = lazyspec::cli::fix::run_renumber(
        fixture.root(),
        &store,
        &config,
        &RenumberFormat::Sqids,
        None,
        false,
        false,
    );
    assert_eq!(exit_code, 0);

    // Original files should be gone
    assert!(!fixture.root().join("docs/rfcs/RFC-001-foo.md").exists());
    assert!(!fixture.root().join("docs/rfcs/RFC-002-bar.md").exists());

    // New files should exist with sqids IDs (non-numeric)
    let entries: Vec<String> = std::fs::read_dir(fixture.root().join("docs/rfcs"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(entries.len(), 2);
    for name in &entries {
        assert!(name.starts_with("RFC-"));
        let parts: Vec<&str> = name.split('-').collect();
        assert!(
            !parts[1].chars().all(|c| c.is_ascii_digit()),
            "expected sqids ID, got numeric: {}",
            name
        );
    }
}

// AC-2: sqids -> incremental renames files with zero-padded sequential numbers
#[test]
fn renumber_sqids_to_incremental() {
    let fixture = common::TestFixture::new();
    let config = sqids_config();

    fixture.write_doc("docs/rfcs/RFC-abc-foo.md", &valid_doc("RFC-abc Foo", "rfc"));
    fixture.write_doc("docs/rfcs/RFC-def-bar.md", &valid_doc("RFC-def Bar", "rfc"));

    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();

    let exit_code = lazyspec::cli::fix::run_renumber(
        fixture.root(),
        &store,
        &config,
        &RenumberFormat::Incremental,
        None,
        false,
        false,
    );
    assert_eq!(exit_code, 0);

    // Original sqids files should be gone
    assert!(!fixture.root().join("docs/rfcs/RFC-abc-foo.md").exists());
    assert!(!fixture.root().join("docs/rfcs/RFC-def-bar.md").exists());

    // New files should be sequential zero-padded
    assert!(fixture.root().join("docs/rfcs/RFC-001-foo.md").exists());
    assert!(fixture.root().join("docs/rfcs/RFC-002-bar.md").exists());
}

// AC-3: --type filter limits conversion to specified type
#[test]
fn renumber_type_filter() {
    let fixture = common::TestFixture::new();
    let config = sqids_config();

    fixture.write_doc("docs/rfcs/RFC-001-foo.md", &valid_doc("RFC-001 Foo", "rfc"));
    fixture.write_doc(
        "docs/stories/STORY-001-baz.md",
        &valid_doc("STORY-001 Baz", "story"),
    );

    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();

    lazyspec::cli::fix::run_renumber(
        fixture.root(),
        &store,
        &config,
        &RenumberFormat::Sqids,
        Some("rfc"),
        false,
        false,
    );

    // RFC should have been renamed
    assert!(!fixture.root().join("docs/rfcs/RFC-001-foo.md").exists());

    // Story should NOT have been renamed
    assert!(fixture.root().join("docs/stories/STORY-001-baz.md").exists());
}

// AC-5: dry_run previews without modifying disk
#[test]
fn renumber_dry_run_no_side_effects() {
    let fixture = common::TestFixture::new();
    let config = sqids_config();

    fixture.write_doc("docs/rfcs/RFC-001-foo.md", &valid_doc("RFC-001 Foo", "rfc"));
    fixture.write_doc("docs/rfcs/RFC-002-bar.md", &valid_doc("RFC-002 Bar", "rfc"));

    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();

    lazyspec::cli::fix::run_renumber(
        fixture.root(),
        &store,
        &config,
        &RenumberFormat::Sqids,
        None,
        true,
        false,
    );

    // Files should still exist at original paths
    assert!(fixture.root().join("docs/rfcs/RFC-001-foo.md").exists());
    assert!(fixture.root().join("docs/rfcs/RFC-002-bar.md").exists());
}

// AC-7: already-converted docs are skipped
#[test]
fn renumber_skips_already_converted_sqids() {
    let fixture = common::TestFixture::new();
    let config = sqids_config();

    // Already sqids format
    fixture.write_doc("docs/rfcs/RFC-abc-foo.md", &valid_doc("RFC-abc Foo", "rfc"));

    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();

    lazyspec::cli::fix::run_renumber(
        fixture.root(),
        &store,
        &config,
        &RenumberFormat::Sqids,
        None,
        false,
        false,
    );

    // File should still exist unchanged
    assert!(fixture.root().join("docs/rfcs/RFC-abc-foo.md").exists());
}

#[test]
fn renumber_skips_already_converted_incremental() {
    let fixture = common::TestFixture::new();
    let config = sqids_config();

    // Already incremental format
    fixture.write_doc("docs/rfcs/RFC-001-foo.md", &valid_doc("RFC-001 Foo", "rfc"));

    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();

    lazyspec::cli::fix::run_renumber(
        fixture.root(),
        &store,
        &config,
        &RenumberFormat::Incremental,
        None,
        false,
        false,
    );

    // File should still exist unchanged (nothing to convert)
    assert!(fixture.root().join("docs/rfcs/RFC-001-foo.md").exists());
}

// JSON output includes expected structure
#[test]
fn renumber_json_output() {
    let fixture = common::TestFixture::new();
    let config = sqids_config();

    fixture.write_doc("docs/rfcs/RFC-001-foo.md", &valid_doc("RFC-001 Foo", "rfc"));

    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();

    // Capture stdout by using dry_run + json through the function directly
    // We can't easily capture stdout, so let's verify the logic works via dry_run
    let exit_code = lazyspec::cli::fix::run_renumber(
        fixture.root(),
        &store,
        &config,
        &RenumberFormat::Sqids,
        None,
        true,
        true,
    );
    assert_eq!(exit_code, 0);
}

// AC-4: related frontmatter paths updated after renames
#[test]
fn renumber_updates_related_references() {
    let fixture = common::TestFixture::new();
    let config = sqids_config();

    fixture.write_doc("docs/rfcs/RFC-001-foo.md", &valid_doc("RFC-001 Foo", "rfc"));
    fixture.write_doc(
        "docs/stories/STORY-001-impl.md",
        &valid_doc_with_related("STORY-001 Impl", "story", "docs/rfcs/RFC-001-foo.md"),
    );

    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();

    lazyspec::cli::fix::run_renumber(
        fixture.root(),
        &store,
        &config,
        &RenumberFormat::Sqids,
        Some("rfc"),
        false,
        false,
    );

    // The story file should have its related path updated
    let story_content =
        std::fs::read_to_string(fixture.root().join("docs/stories/STORY-001-impl.md")).unwrap();
    assert!(
        !story_content.contains("RFC-001-foo.md"),
        "related reference should have been updated, still contains old path"
    );
}

// AC-6: external references detected and reported
#[test]
fn renumber_detects_external_references() {
    let fixture = common::TestFixture::new();
    let config = sqids_config();

    fixture.write_doc("docs/rfcs/RFC-001-foo.md", &valid_doc("RFC-001 Foo", "rfc"));

    // Create a README outside managed dirs that references the old filename
    fixture.write_doc("README.md", "# Project\n\nSee [RFC-001-foo.md](docs/rfcs/RFC-001-foo.md) for details.\n");

    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();

    // Collect what changes would be made (dry run)
    let changes = vec![lazyspec::cli::fix::RenumberFixResult {
        old_path: "docs/rfcs/RFC-001-foo.md".to_string(),
        new_path: "docs/rfcs/RFC-xyz-foo.md".to_string(),
        old_id: "RFC-001".to_string(),
        new_id: "RFC-xyz".to_string(),
        references_updated: vec![],
        written: false,
    }];

    let ext_refs =
        lazyspec::cli::fix::renumber::scan_external_references(fixture.root(), &store, &config, &changes);

    assert_eq!(ext_refs.len(), 1);
    assert_eq!(ext_refs[0].file, "README.md");
    assert_eq!(ext_refs[0].old_name, "RFC-001-foo.md");
    assert_eq!(ext_refs[0].line, 3);
}

#[test]
fn renumber_external_refs_skips_managed_files() {
    let fixture = common::TestFixture::new();
    let config = sqids_config();

    fixture.write_doc("docs/rfcs/RFC-001-foo.md", &valid_doc("RFC-001 Foo", "rfc"));
    // A story that references the RFC (managed file, should be skipped)
    fixture.write_doc(
        "docs/stories/STORY-001-impl.md",
        &valid_doc_with_related("STORY-001 Impl", "story", "docs/rfcs/RFC-001-foo.md"),
    );

    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();

    let changes = vec![lazyspec::cli::fix::RenumberFixResult {
        old_path: "docs/rfcs/RFC-001-foo.md".to_string(),
        new_path: "docs/rfcs/RFC-xyz-foo.md".to_string(),
        old_id: "RFC-001".to_string(),
        new_id: "RFC-xyz".to_string(),
        references_updated: vec![],
        written: false,
    }];

    let ext_refs =
        lazyspec::cli::fix::renumber::scan_external_references(fixture.root(), &store, &config, &changes);

    // Managed store files should not appear in external references
    assert!(
        ext_refs.is_empty(),
        "expected no external refs for managed files, got: {:?}",
        ext_refs.iter().map(|r| &r.file).collect::<Vec<_>>()
    );
}

// AC-5: JSON output includes all expected fields
#[test]
fn renumber_json_output_structure() {
    let fixture = common::TestFixture::new();
    let config = sqids_config();

    fixture.write_doc("docs/rfcs/RFC-001-foo.md", &valid_doc("RFC-001 Foo", "rfc"));
    fixture.write_doc(
        "docs/stories/STORY-001-impl.md",
        &valid_doc_with_related("STORY-001 Impl", "story", "docs/rfcs/RFC-001-foo.md"),
    );
    fixture.write_doc("README.md", "See RFC-001-foo.md for details.\n");

    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();

    let json_str = lazyspec::cli::fix::run_renumber_json(
        fixture.root(),
        &store,
        &config,
        &RenumberFormat::Sqids,
        Some("rfc"),
        true,
    );

    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    let renumber = &parsed["renumber"];

    assert_eq!(renumber["format"], "sqids");
    assert_eq!(renumber["doc_type"], "rfc");
    assert_eq!(renumber["dry_run"], true);

    let changes = renumber["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 1);

    let change = &changes[0];
    assert!(change["old_path"].as_str().unwrap().contains("RFC-001"));
    assert!(change["new_path"].as_str().unwrap().starts_with("docs/rfcs/RFC-"));
    assert_eq!(change["old_id"], "RFC-001");
    assert!(change["new_id"].as_str().unwrap().starts_with("RFC-"));
    assert_eq!(change["written"], false);
    assert!(change["references_updated"].is_array());

    let ext_refs = renumber["external_references"].as_array().unwrap();
    assert_eq!(ext_refs.len(), 1);
    assert_eq!(ext_refs[0]["file"], "README.md");
    assert!(ext_refs[0]["old_name"].as_str().unwrap().contains("RFC-001"));
    assert!(ext_refs[0]["line"].is_number());
}

// AUDIT-003 Finding 3: sqids->incremental conversion must not collide with existing incremental IDs
#[test]
fn renumber_sqids_to_incremental_avoids_collision() {
    let fixture = common::TestFixture::new();
    let config = sqids_config();

    // RFC-001 already exists as incremental; RFC-abc is sqids and needs conversion
    fixture.write_doc("docs/rfcs/RFC-001-foo.md", &valid_doc("RFC-001 Foo", "rfc"));
    fixture.write_doc("docs/rfcs/RFC-abc-bar.md", &valid_doc("RFC-abc Bar", "rfc"));

    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();

    lazyspec::cli::fix::run_renumber(
        fixture.root(),
        &store,
        &config,
        &RenumberFormat::Incremental,
        None,
        false,
        false,
    );

    // RFC-001 should still exist untouched (it's already incremental)
    assert!(
        fixture.root().join("docs/rfcs/RFC-001-foo.md").exists(),
        "existing incremental doc RFC-001 should not be touched"
    );

    // The sqids doc should be renamed to RFC-002, not RFC-001
    assert!(
        fixture.root().join("docs/rfcs/RFC-002-bar.md").exists(),
        "sqids doc should be renumbered to RFC-002 to avoid collision with existing RFC-001"
    );
    assert!(
        !fixture.root().join("docs/rfcs/RFC-001-bar.md").exists(),
        "sqids doc must NOT be renumbered to RFC-001 (collision)"
    );
}

// Mixed docs: only those needing conversion are touched
#[test]
fn renumber_mixed_formats_only_converts_needed() {
    let fixture = common::TestFixture::new();
    let config = sqids_config();

    // One incremental, one already sqids
    fixture.write_doc("docs/rfcs/RFC-001-foo.md", &valid_doc("RFC-001 Foo", "rfc"));
    fixture.write_doc("docs/rfcs/RFC-abc-bar.md", &valid_doc("RFC-abc Bar", "rfc"));

    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();

    lazyspec::cli::fix::run_renumber(
        fixture.root(),
        &store,
        &config,
        &RenumberFormat::Sqids,
        None,
        false,
        false,
    );

    // The sqids one should still exist unchanged
    assert!(fixture.root().join("docs/rfcs/RFC-abc-bar.md").exists());
    // The incremental one should be gone (renamed)
    assert!(!fixture.root().join("docs/rfcs/RFC-001-foo.md").exists());
}

// AUDIT-003 Finding 1: noise directories should be skipped during external reference scan
#[test]
fn renumber_external_refs_skips_noise_dirs() {
    let fixture = common::TestFixture::new();
    let config = sqids_config();

    fixture.write_doc("docs/rfcs/RFC-001-foo.md", &valid_doc("RFC-001 Foo", "rfc"));

    // Create files in noise directories that reference the old filename
    let git_dir = fixture.root().join(".git");
    std::fs::create_dir_all(&git_dir).unwrap();
    std::fs::write(
        git_dir.join("config"),
        "# references RFC-001-foo.md in a git config\n",
    )
    .unwrap();

    let nm_dir = fixture.root().join("node_modules").join("pkg");
    std::fs::create_dir_all(&nm_dir).unwrap();
    std::fs::write(
        nm_dir.join("README.md"),
        "See RFC-001-foo.md for details.\n",
    )
    .unwrap();

    let target_dir = fixture.root().join("target").join("debug");
    std::fs::create_dir_all(&target_dir).unwrap();
    std::fs::write(
        target_dir.join("README.md"),
        "Built from RFC-001-foo.md\n",
    )
    .unwrap();

    let venv_dir = fixture.root().join(".venv").join("lib");
    std::fs::create_dir_all(&venv_dir).unwrap();
    std::fs::write(
        venv_dir.join("README.md"),
        "RFC-001-foo.md referenced here\n",
    )
    .unwrap();

    let store = lazyspec::engine::store::Store::load(fixture.root(), &config).unwrap();

    let changes = vec![lazyspec::cli::fix::RenumberFixResult {
        old_path: "docs/rfcs/RFC-001-foo.md".to_string(),
        new_path: "docs/rfcs/RFC-xyz-foo.md".to_string(),
        old_id: "RFC-001".to_string(),
        new_id: "RFC-xyz".to_string(),
        references_updated: vec![],
        written: false,
    }];

    let ext_refs =
        lazyspec::cli::fix::renumber::scan_external_references(fixture.root(), &store, &config, &changes);

    assert!(
        ext_refs.is_empty(),
        "expected no external refs from noise directories, got: {:?}",
        ext_refs.iter().map(|r| &r.file).collect::<Vec<_>>()
    );
}
