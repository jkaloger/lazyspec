use std::process::Command;
use tempfile::TempDir;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_lazyspec")
}

// AC1 (CLI surface): `create` in a directory with no .lazyspec.toml fails with a
// non-zero exit and an `init` hint on stderr.
#[test]
fn cli_create_without_config_errors() {
    let tmp = TempDir::new().unwrap();

    let output = Command::new(binary())
        .args(["create", "rfc", "X", "--json"])
        .current_dir(tmp.path())
        .output()
        .expect("failed to run lazyspec");

    assert!(
        !output.status.success(),
        "create with no config should exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("init"),
        "stderr should hint at init, got: {stderr}"
    );
}

// AC3: an arbitrary type set (no rfc/story/iteration/adr) supports create, list,
// and validate end-to-end, proving the engine assumes no specific type names.
#[test]
fn arbitrary_types_create_list_validate() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    let config = r#"
[[types]]
name = "ticket"
plural = "tickets"
dir = "docs/tickets"
prefix = "TICKET"

[[types]]
name = "epic"
plural = "epics"
dir = "docs/epics"
prefix = "EPIC"

[[relationships]]
name = "implements"
inverse = "implemented-by"

[[relationships]]
name = "related-to"

[naming]
pattern = "{type}-{n:03}-{title}.md"

[templates]
dir = ".lazyspec/templates"
"#;
    std::fs::write(root.join(".lazyspec.toml"), config).unwrap();

    let create = Command::new(binary())
        .args(["create", "ticket", "First", "--json"])
        .current_dir(root)
        .output()
        .expect("failed to run create");
    assert!(
        create.status.success(),
        "create should succeed, stderr: {}",
        String::from_utf8_lossy(&create.stderr)
    );
    assert!(
        root.join("docs/tickets").is_dir(),
        "ticket dir should be created"
    );

    let list = Command::new(binary())
        .args(["list", "--json"])
        .current_dir(root)
        .output()
        .expect("failed to run list");
    assert!(list.status.success(), "list should succeed");
    let list_out = String::from_utf8_lossy(&list.stdout);
    assert!(
        list_out.contains("ticket") || list_out.contains("TICKET"),
        "list should include the created ticket, got: {list_out}"
    );

    let validate = Command::new(binary())
        .args(["validate", "--json"])
        .current_dir(root)
        .output()
        .expect("failed to run validate");
    assert!(
        validate.status.success(),
        "validate should succeed, stderr: {}",
        String::from_utf8_lossy(&validate.stderr)
    );
    let validate_out = String::from_utf8_lossy(&validate.stdout);
    assert!(
        !validate_out.contains("unknown"),
        "validate should not report unknown/missing built-in types, got: {validate_out}"
    );
}

// AC6 (CLI surface): a .lazyspec.toml with [[types]] but no [[relationships]]
// fails to load with a non-zero exit and an error naming the missing section and
// the `fix` remedy.
#[test]
fn cli_without_relationships_block_errors() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    let config = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[naming]
pattern = "{type}-{n:03}-{title}.md"

[templates]
dir = ".lazyspec/templates"
"#;
    std::fs::write(root.join(".lazyspec.toml"), config).unwrap();

    let output = Command::new(binary())
        .args(["list", "--json"])
        .current_dir(root)
        .output()
        .expect("failed to run lazyspec");

    assert!(
        !output.status.success(),
        "loading a config without [[relationships]] should exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("[[relationships]]"),
        "stderr should name the missing section, got: {stderr}"
    );
    assert!(
        stderr.contains("lazyspec fix"),
        "stderr should point at the fix remedy, got: {stderr}"
    );
}
