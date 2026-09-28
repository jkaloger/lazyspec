// RFC-074 / STORY-293: `init --template <dir-or-url>` adopts a whole workflow
// pack (its `.lazyspec.toml` and `.lazyspec/templates/`) instead of running
// any wizard.
use crate::common::git;
use lazyspec::cli::init;
use lazyspec::engine::config::Config;
use lazyspec::engine::fs::RealFileSystem;
use lazyspec::engine::git_ref::GitCli;
use lazyspec::engine::store::Store;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// AC6's first consumer: `examples/openspec/` in this repository, a `change`
/// type with a `change/{index,proposal,design,tasks}.md` directory template
/// and a `delta` type created per affected capability with `create --parent`.
fn openspec_pack() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/openspec")
}

fn assert_pack_adopted(root: &Path) {
    assert!(root.join(".lazyspec.toml").is_file());
    assert!(root.join(".lazyspec/templates/change/index.md").is_file());
    assert!(root
        .join(".lazyspec/templates/change/proposal.md")
        .is_file());
    assert!(root.join(".lazyspec/templates/change/design.md").is_file());
    assert!(root.join(".lazyspec/templates/change/tasks.md").is_file());
    assert!(root.join(".lazyspec/templates/delta.md").is_file());
}

// AC1: `init --template <dir>` copies that directory's `.lazyspec.toml` and
// `.lazyspec/templates/` (flat and directory templates alike) into the
// project. Storage stays local: nothing sets `extends`.
#[test]
fn init_template_dir_copies_config_and_templates() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    init::run_from_template(
        root,
        openspec_pack().to_str().unwrap(),
        false,
        false,
        &GitCli,
    )
    .unwrap();

    assert_pack_adopted(root);

    let config = Config::load(root, &RealFileSystem).unwrap();
    assert!(
        config.extends.is_none(),
        "adopting a pack must not set extends"
    );
    assert!(config.type_by_name("change").is_some());
    assert!(config.type_by_name("delta").is_some());
}

// AC2: `init --template <url>` clones into `.lazyspec/cache/config/` (the
// path `extends` already uses) and then copies as AC1. A local `file://`
// clone of a real git repo, so the test needs no network.
#[test]
fn init_template_url_clones_under_the_local_cache_then_copies_the_pack() {
    let source = TempDir::new().unwrap();
    let src_root = source.path();
    fs_extra_copy(&openspec_pack(), src_root);
    git(src_root, &["init", "-q", "-b", "main"]);
    git(src_root, &["config", "user.email", "test@test.com"]);
    git(src_root, &["config", "user.name", "Test"]);
    git(src_root, &["add", "-A"]);
    git(src_root, &["commit", "-q", "-m", "pack"]);

    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let url = format!("file://{}", src_root.display());

    init::run_from_template(root, &url, false, false, &GitCli).unwrap();

    assert_pack_adopted(root);

    let clone_root = root.join(".lazyspec/cache/config");
    assert!(
        clone_root.join(".lazyspec.toml").is_file(),
        "the clone should carry the pack's own .lazyspec.toml"
    );
    assert!(
        root.join(".lazyspec/.gitignore").is_file(),
        "the cache clone should be gitignored, matching a URL extends"
    );
}

// AC3: an existing `.lazyspec.toml` refuses with an error naming `--force`;
// `--force` overwrites.
#[test]
fn init_template_refuses_existing_config_without_force_and_overwrites_with_force() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let pack = openspec_pack();
    let pack_str = pack.to_str().unwrap();

    init::run_from_template(root, pack_str, false, false, &GitCli).unwrap();

    let refused = init::run_from_template(root, pack_str, false, false, &GitCli);
    let err = refused.unwrap_err();
    assert!(
        err.to_string().contains("--force"),
        "refusal should name --force, got: {err}"
    );

    init::run_from_template(root, pack_str, true, false, &GitCli).unwrap();
    assert_pack_adopted(root);
}

// AC4: `--template starter` is unaffected by the pack path -- it still
// pre-selects the built-in starter designer, non-interactively writing the
// starter config unchanged, and sets no `extends` either.
#[test]
fn init_template_starter_is_unaffected_and_sets_no_extends() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    init::run(root, false).unwrap();

    let config = Config::load(root, &RealFileSystem).unwrap();
    assert!(config.extends.is_none());
    assert!(config.type_by_name("rfc").is_some());
}

// AC6: `init --template <pack>` followed by `create change "x"` yields the
// four files a directory template declares.
#[test]
fn init_template_openspec_pack_then_create_change_yields_four_files() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    init::run_from_template(
        root,
        openspec_pack().to_str().unwrap(),
        false,
        false,
        &GitCli,
    )
    .unwrap();

    let config = Config::load(root, &RealFileSystem).unwrap();
    let store = Store::load(root, &config).unwrap();

    let output = lazyspec::cli::create::run_json(
        root,
        &config,
        &store,
        "change",
        "x",
        "agent",
        &GitCli,
        |_| {},
    )
    .unwrap();
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();

    let index_path = root.join(json["path"].as_str().unwrap());
    let change_dir = index_path.parent().unwrap();

    let mut files: Vec<String> = std::fs::read_dir(change_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    assert_eq!(
        files,
        vec![
            "design.md".to_string(),
            "index.md".to_string(),
            "proposal.md".to_string(),
            "tasks.md".to_string(),
        ],
        "create change must scaffold exactly the four declared files"
    );

    // AC6: `delta` is the pack's other first consumer -- `create delta ...
    // --parent <change id>` lands the delta as a sibling file inside that
    // same change folder, not in the flat `deltas/` directory a parent-less
    // delta would use.
    let change_id = json["id"].as_str().unwrap();
    let store = Store::load(root, &config).unwrap();
    let delta_output = lazyspec::cli::create::run_json_with_body(
        root,
        &config,
        &store,
        "delta",
        "cap",
        "agent",
        Some(change_id),
        None,
        &GitCli,
        |_| {},
    )
    .unwrap();
    let delta_json: serde_json::Value = serde_json::from_str(&delta_output).unwrap();
    let delta_path = root.join(delta_json["path"].as_str().unwrap());

    assert_eq!(
        delta_path.parent().unwrap(),
        change_dir,
        "create delta --parent <change id> must land the delta inside the change's own folder"
    );

    let mut files: Vec<String> = std::fs::read_dir(change_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    assert_eq!(
        files,
        vec![
            "DELTA-001-cap.md".to_string(),
            "design.md".to_string(),
            "index.md".to_string(),
            "proposal.md".to_string(),
            "tasks.md".to_string(),
        ],
        "the delta joins the change's four parts as a fifth file in the same folder"
    );
}

/// Recursively copy `src` into `dst` (both already existing directories):
/// this test's own fixture setup, standing in for `cp -r`.
fn fs_extra_copy(src: &Path, dst: &Path) {
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let target = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
            fs_extra_copy(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}
