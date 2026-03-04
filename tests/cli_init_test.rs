use std::fs;
use tempfile::TempDir;

#[test]
fn init_creates_config_and_directories() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    lazyspec::cli::init::run(root).unwrap();

    assert!(root.join(".lazyspec.toml").exists());
    assert!(root.join("docs/rfcs").is_dir());
    assert!(root.join("docs/adrs").is_dir());
    assert!(root.join("docs/specs").is_dir());
    assert!(root.join("docs/plans").is_dir());
    assert!(root.join(".lazyspec/templates").is_dir());

    let content = fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
    assert!(content.contains("[directories]"));
}

#[test]
fn init_does_not_overwrite_existing_config() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::write(root.join(".lazyspec.toml"), "# custom config").unwrap();

    let result = lazyspec::cli::init::run(root);
    assert!(result.is_err());
}
