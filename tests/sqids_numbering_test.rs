mod common;

use std::fs;

fn sqids_config(salt: &str, min_length: u8) -> lazyspec::engine::config::Config {
    let toml = format!(
        r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
numbering = "sqids"

[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"

[numbering.sqids]
salt = "{salt}"
min_length = {min_length}
"#
    );
    lazyspec::engine::config::Config::parse(&toml).unwrap()
}

// AC-1: sqids numbering produces sqids-based filename
#[test]
fn create_with_sqids_produces_sqids_filename() {
    let fixture = common::TestFixture::new();
    let root = fixture.root();
    let config = sqids_config("integration-salt", 3);

    let path =
        lazyspec::cli::create::run(root, &config, "rfc", "Test Feature", "author").unwrap();
    let filename = path.file_name().unwrap().to_str().unwrap();

    assert!(filename.starts_with("RFC-"), "expected RFC- prefix, got: {filename}");
    // Should NOT be incremental (RFC-001)
    assert!(
        !filename.starts_with("RFC-001"),
        "sqids filename should not be incremental, got: {filename}"
    );
    // The ID portion (between first and second dash) should be alphabetic (sqids output)
    let parts: Vec<&str> = filename.split('-').collect();
    assert!(parts.len() >= 3, "filename should have at least 3 dash-separated parts: {filename}");
    let id_part = parts[1];
    assert!(
        id_part.chars().all(|c| c.is_ascii_alphanumeric()),
        "sqids ID should be alphanumeric, got: {id_part}"
    );
}

// AC-2: no numbering field defaults to incremental
#[test]
fn create_without_numbering_field_uses_incremental() {
    let fixture = common::TestFixture::new();
    let root = fixture.root();
    let config = fixture.config(); // default config has no sqids

    let path =
        lazyspec::cli::create::run(root, &config, "rfc", "Incremental Test", "author").unwrap();
    let filename = path.file_name().unwrap().to_str().unwrap();

    assert!(
        filename.starts_with("RFC-001"),
        "default numbering should be incremental (RFC-001), got: {filename}"
    );
}

// AC-3: explicit incremental numbering
#[test]
fn create_with_explicit_incremental_uses_incremental() {
    let fixture = common::TestFixture::new();
    let root = fixture.root();

    let toml = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
numbering = "incremental"
"#;
    let config = lazyspec::engine::config::Config::parse(toml).unwrap();

    let path =
        lazyspec::cli::create::run(root, &config, "rfc", "Explicit Incremental", "author")
            .unwrap();
    let filename = path.file_name().unwrap().to_str().unwrap();

    assert!(
        filename.starts_with("RFC-001"),
        "explicit incremental should produce RFC-001, got: {filename}"
    );
}

// AC-4: different salts produce different IDs
#[test]
fn different_salts_produce_different_ids() {
    let fixture_a = common::TestFixture::new();
    let fixture_b = common::TestFixture::new();

    let config_a = sqids_config("salt-alpha", 3);
    let config_b = sqids_config("salt-beta", 3);

    let path_a =
        lazyspec::cli::create::run(fixture_a.root(), &config_a, "rfc", "Same Title", "author")
            .unwrap();
    let path_b =
        lazyspec::cli::create::run(fixture_b.root(), &config_b, "rfc", "Same Title", "author")
            .unwrap();

    let name_a = path_a.file_name().unwrap().to_str().unwrap();
    let name_b = path_b.file_name().unwrap().to_str().unwrap();

    // Extract the sqids ID portion (second segment)
    let id_a = name_a.split('-').nth(1).unwrap();
    let id_b = name_b.split('-').nth(1).unwrap();

    assert_ne!(
        id_a, id_b,
        "different salts should produce different IDs: {id_a} vs {id_b}"
    );
}

// AC-5: min_length = 5 produces IDs >= 5 chars
#[test]
fn min_length_five_produces_ids_at_least_five_chars() {
    let fixture = common::TestFixture::new();
    let config = sqids_config("min-length-test", 5);

    let path =
        lazyspec::cli::create::run(fixture.root(), &config, "rfc", "Length Test", "author")
            .unwrap();
    let filename = path.file_name().unwrap().to_str().unwrap();

    let id_part = filename.split('-').nth(1).unwrap();
    assert!(
        id_part.len() >= 5,
        "min_length=5 should produce ID >= 5 chars, got '{}' (len={})",
        id_part,
        id_part.len()
    );
}

// AC-6: min_length outside 1-10 fails validation
#[test]
fn min_length_zero_fails_validation() {
    let toml = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
numbering = "sqids"

[numbering.sqids]
salt = "test"
min_length = 0
"#;
    let result = lazyspec::engine::config::Config::parse(toml);
    assert!(result.is_err(), "min_length=0 should fail");
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("min_length"), "error should mention min_length, got: {msg}");
}

#[test]
fn min_length_eleven_fails_validation() {
    let toml = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
numbering = "sqids"

[numbering.sqids]
salt = "test"
min_length = 11
"#;
    let result = lazyspec::engine::config::Config::parse(toml);
    assert!(result.is_err(), "min_length=11 should fail");
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("min_length"), "error should mention min_length, got: {msg}");
}

// AC-7: sqids without salt fails validation
#[test]
fn sqids_without_salt_section_fails_validation() {
    let toml = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
numbering = "sqids"
"#;
    let result = lazyspec::engine::config::Config::parse(toml);
    assert!(result.is_err(), "sqids without salt should fail");
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("salt"), "error should mention salt, got: {msg}");
}

// AC-8: collision retry with pre-existing files
#[test]
fn create_retries_on_collision() {
    let fixture = common::TestFixture::new();
    let root = fixture.root();
    let config = sqids_config("collision-integration", 3);

    // Create the first doc to get its ID
    let path1 =
        lazyspec::cli::create::run(root, &config, "rfc", "First Doc", "author").unwrap();
    let name1 = path1.file_name().unwrap().to_str().unwrap();
    let id1 = name1.split('-').nth(1).unwrap();

    // Create a second doc; it should get a different ID
    let path2 =
        lazyspec::cli::create::run(root, &config, "rfc", "Second Doc", "author").unwrap();
    let name2 = path2.file_name().unwrap().to_str().unwrap();
    let id2 = name2.split('-').nth(1).unwrap();

    assert_ne!(id1, id2, "sequential creates should produce different IDs");

    // Verify both files exist
    assert!(path1.exists());
    assert!(path2.exists());
}

#[test]
fn create_handles_preexisting_colliding_file() {
    let fixture = common::TestFixture::new();
    let root = fixture.root();
    let config = sqids_config("collision-pre-existing", 3);

    // First, generate what the first ID would be by creating and removing
    let path1 =
        lazyspec::cli::create::run(root, &config, "rfc", "Probe", "author").unwrap();
    let name1 = path1.file_name().unwrap().to_str().unwrap().to_string();
    let id1: String = name1.split('-').nth(1).unwrap().to_string();

    // Remove it and recreate a file with the same ID prefix but different slug
    // to simulate a collision scenario
    fs::remove_file(&path1).unwrap();
    let colliding = format!("RFC-{}-pre-existing.md", id1);
    fs::write(root.join("docs/rfcs").join(&colliding), "---\ntitle: \"X\"\ntype: rfc\nstatus: draft\nauthor: \"a\"\ndate: 2026-01-01\ntags: []\n---\n").unwrap();

    // Now create should detect the collision and use the next ID
    let path2 =
        lazyspec::cli::create::run(root, &config, "rfc", "After Collision", "author").unwrap();
    let name2 = path2.file_name().unwrap().to_str().unwrap();
    let id2 = name2.split('-').nth(1).unwrap();

    assert_ne!(
        id1, id2,
        "should have retried past the colliding ID: {id1} vs {id2}"
    );
}

// AC-9: generated ID is all lowercase
#[test]
fn sqids_id_is_lowercase() {
    let fixture = common::TestFixture::new();
    let config = sqids_config("lowercase-test", 3);

    let path =
        lazyspec::cli::create::run(fixture.root(), &config, "rfc", "Case Test", "author")
            .unwrap();
    let filename = path.file_name().unwrap().to_str().unwrap();
    let id_part = filename.split('-').nth(1).unwrap();

    assert_eq!(
        id_part,
        id_part.to_lowercase(),
        "sqids ID should be all lowercase, got: {id_part}"
    );
}

// AC-2 + AC-1: mixed types - sqids on one, incremental on another
#[test]
fn mixed_numbering_types_work_together() {
    let fixture = common::TestFixture::new();
    let root = fixture.root();
    let config = sqids_config("mixed-test", 3);

    // rfc uses sqids
    let rfc_path =
        lazyspec::cli::create::run(root, &config, "rfc", "Sqids RFC", "author").unwrap();
    let rfc_name = rfc_path.file_name().unwrap().to_str().unwrap();

    // story uses incremental (default in our config helper)
    let story_path =
        lazyspec::cli::create::run(root, &config, "story", "Incremental Story", "author")
            .unwrap();
    let story_name = story_path.file_name().unwrap().to_str().unwrap();

    assert!(
        !rfc_name.starts_with("RFC-001"),
        "rfc should use sqids, got: {rfc_name}"
    );
    assert!(
        story_name.starts_with("STORY-001"),
        "story should use incremental, got: {story_name}"
    );
}
