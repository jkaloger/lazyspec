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
    assert!(root.join("docs/stories").is_dir());
    assert!(root.join("docs/iterations").is_dir());
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

#[test]
fn init_creates_convention_skeleton_files() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    lazyspec::cli::init::run(root).unwrap();

    let index = root.join("docs/convention/convention/index.md");
    let example = root.join("docs/convention/convention/example.md");

    assert!(index.exists(), "convention index.md should be created");
    assert!(example.exists(), "convention example.md should be created");

    let index_content = fs::read_to_string(&index).unwrap();
    assert!(index_content.contains("type: convention"));
    assert!(index_content.contains("status: draft"));
    assert!(index_content.contains("author: \"unknown\""));
    assert!(index_content.contains("tags: []"));
    // Date should be YYYY-MM-DD format
    let date_re = regex::Regex::new(r"date: \d{4}-\d{2}-\d{2}").unwrap();
    assert!(date_re.is_match(&index_content), "index.md should contain a date in YYYY-MM-DD format");

    let example_content = fs::read_to_string(&example).unwrap();
    assert!(example_content.contains("type: dictum"));
    assert!(example_content.contains("status: draft"));
    assert!(example_content.contains("author: \"unknown\""));
    assert!(example_content.contains("tags: [example]"));
    assert!(date_re.is_match(&example_content), "example.md should contain a date in YYYY-MM-DD format");
}

#[test]
fn init_does_not_overwrite_existing_convention_files() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    let convention_dir = root.join("docs/convention/convention");
    fs::create_dir_all(&convention_dir).unwrap();
    fs::write(convention_dir.join("index.md"), "# my custom convention").unwrap();

    lazyspec::cli::init::run(root).unwrap();

    let content = fs::read_to_string(convention_dir.join("index.md")).unwrap();
    assert_eq!(content, "# my custom convention");
}
