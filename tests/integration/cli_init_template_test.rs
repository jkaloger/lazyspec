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

/// AC6's first consumer: `examples/openspec/` in this repository, a `spec`
/// type for capability specs and a `change` type with a
/// `change/{index,proposal,design,tasks}.md` directory template. Delta specs
/// are frontmatter-less parts added to a change's folder, not a type.
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
    assert!(root.join(".lazyspec/templates/spec.md").is_file());
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
    assert!(config.type_by_name("spec").is_some());
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

// AC4: plain `init` (no `--template` at all) writes the starter config
// unaffected by pack support existing, and sets no `extends`.
#[test]
fn init_run_writes_the_starter_config_unaffected_by_pack_support() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    init::run(root, false).unwrap();

    let config = Config::load(root, &RealFileSystem).unwrap();
    assert!(config.extends.is_none());
    assert!(config.type_by_name("rfc").is_some());
}

// AC4: `--template starter` is `init`'s built-in name, not a pack -- main's
// dispatch (`if let Some(pack) = pack_template(template) { run_from_template
// ... }`) never reaches `run_from_template` for it, so it cannot go through
// the network/local-dir pack path `run_from_template` implements; it falls
// through to the ordinary (wizard-capable, but pre-selected past the first
// screen) `init` path instead.
#[test]
fn template_starter_is_excluded_from_the_pack_path() {
    assert!(init::pack_template(Some("starter")).is_none());
    assert!(init::pack_template(None).is_none());
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

    // AC6: a delta spec is one more part of the change, as in OpenSpec -- a
    // frontmatter-less `<capability>.md` dropped beside `index.md` loads as a
    // part sharing the change's id, not as a document or a parse error.
    std::fs::write(
        change_dir.join("theme.md"),
        "# Spec Delta\n\n## ADDED Requirements\n\n### Requirement: Theme selection\n",
    )
    .unwrap();
    let store = Store::load(root, &config).unwrap();
    assert!(
        store.parse_errors().is_empty(),
        "got: {:?}",
        store.parse_errors()
    );
    let change = store
        .resolve_shorthand(json["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        change
            .parts
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        vec!["design", "proposal", "tasks", "theme"],
        "the delta spec joins the change's three template parts as a fourth part"
    );
    assert_eq!(
        store.all_docs().len(),
        1,
        "the delta spec must not become a document"
    );
}

// AC1: `--json` lists every file written, relative to root -- exercised
// through the real binary since `run_from_template` prints its JSON payload
// rather than returning it.
#[test]
fn init_template_json_lists_the_files_written() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lazyspec"))
        .args([
            "init",
            "--template",
            openspec_pack().to_str().unwrap(),
            "--json",
        ])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    let files: Vec<&str> = json["files"]
        .as_array()
        .expect("files is an array")
        .iter()
        .map(|f| f.as_str().unwrap())
        .collect();

    assert!(files.contains(&".lazyspec.toml"), "got: {files:?}");
    assert!(
        files
            .iter()
            .any(|f| f.contains("templates/change/index.md")),
        "got: {files:?}"
    );
    assert_eq!(
        files[0], ".lazyspec.toml",
        "config first, then templates in directory-listing order: {files:?}"
    );
}

fn pack_with_hook() -> TempDir {
    let pack = TempDir::new().unwrap();
    fs_extra_copy(&openspec_pack(), pack.path());
    let hooks = pack.path().join(".lazyspec/hooks");
    std::fs::create_dir_all(hooks.join("lib")).unwrap();
    let script = hooks.join("check");
    std::fs::write(&script, "#!/bin/sh\necho '{\"findings\": []}'\n").unwrap();
    std::fs::write(hooks.join("lib/helper"), "x").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let toml_path = pack.path().join(".lazyspec.toml");
    let mut toml = std::fs::read_to_string(&toml_path).unwrap();
    toml.push_str(
        "\n[[hooks]]\nname = \"check\"\nevent = \"validate\"\nrun = [\".lazyspec/hooks/check\"]\n",
    );
    std::fs::write(&toml_path, toml).unwrap();
    pack
}

/// A project that adopted `pack`, with its own hook-trust state, and the
/// output of the `init` that adopted it.
struct Adopted {
    root: TempDir,
    state: TempDir,
    output: std::process::Output,
}

impl Adopted {
    fn new(pack: &Path, json: bool) -> Self {
        let root = TempDir::new().unwrap();
        let state = TempDir::new().unwrap();
        let mut args = vec!["init", "--template", pack.to_str().unwrap()];
        if json {
            args.push("--json");
        }
        let adopted = Self {
            output: Self::bin_in(root.path(), state.path(), &args),
            root,
            state,
        };
        assert!(
            adopted.output.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&adopted.output.stderr)
        );
        adopted
    }

    fn bin_in(root: &Path, state: &Path, args: &[&str]) -> std::process::Output {
        std::process::Command::new(env!("CARGO_BIN_EXE_lazyspec"))
            .args(args)
            .env("LAZYSPEC_STATE_DIR", state)
            .current_dir(root)
            .output()
            .unwrap()
    }

    fn bin(&self, args: &[&str]) -> std::process::Output {
        Self::bin_in(self.root.path(), self.state.path(), args)
    }

    fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.output.stdout).unwrap()
    }

    fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.output.stdout).into_owned()
    }
}

#[test]
fn init_template_copies_and_lists_nested_hook_files() {
    let pack = pack_with_hook();
    let adopted = Adopted::new(pack.path(), true);

    let json = adopted.json();
    let files: Vec<&str> = json["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap())
        .collect();
    assert!(files.contains(&".lazyspec/hooks/check"), "got: {files:?}");
    assert!(
        files.contains(&".lazyspec/hooks/lib/helper"),
        "got: {files:?}"
    );
    assert!(adopted
        .root
        .path()
        .join(".lazyspec/hooks/lib/helper")
        .is_file());
}

#[cfg(unix)]
#[test]
fn init_template_keeps_a_hook_scripts_exec_bit() {
    use std::os::unix::fs::PermissionsExt;
    let pack = pack_with_hook();
    let adopted = Adopted::new(pack.path(), true);

    let mode = std::fs::metadata(adopted.root.path().join(".lazyspec/hooks/check"))
        .unwrap()
        .permissions()
        .mode();
    assert!(mode & 0o111 != 0, "hook script stays executable");
}

#[test]
fn init_template_hooks_start_untrusted() {
    let pack = pack_with_hook();
    let adopted = Adopted::new(pack.path(), true);

    let listed = adopted.bin(&["hook", "list", "--json"]);
    let hooks: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(hooks[0]["name"], "check");
    assert_eq!(hooks[0]["trust"], "untrusted");
}

#[test]
fn init_template_json_names_the_trust_command() {
    let pack = pack_with_hook();
    let adopted = Adopted::new(pack.path(), true);

    assert_eq!(adopted.json()["trust"], "lazyspec hook trust");
}

#[test]
fn init_template_prints_the_trust_hint() {
    let pack = pack_with_hook();
    let adopted = Adopted::new(pack.path(), false);

    assert!(adopted.stdout().contains("lazyspec hook trust"));
}

// A `[[hooks]]` entry whose `run` lives outside `.lazyspec/hooks/` still
// gets the trust hint: it is the table that trust covers, not the directory.
#[test]
fn init_template_hints_trust_for_hooks_run_from_elsewhere() {
    let pack = TempDir::new().unwrap();
    fs_extra_copy(&openspec_pack(), pack.path());
    let toml_path = pack.path().join(".lazyspec.toml");
    let mut toml = std::fs::read_to_string(&toml_path).unwrap();
    toml.push_str(
        "\n[[hooks]]\nname = \"check\"\nevent = \"validate\"\nrun = [\"scripts/check\"]\n",
    );
    std::fs::write(&toml_path, toml).unwrap();

    let adopted = Adopted::new(pack.path(), true);

    assert_eq!(adopted.json()["trust"], "lazyspec hook trust");
}

#[test]
fn init_template_without_hooks_has_no_trust_hint() {
    let dir = TempDir::new().unwrap();
    let state = TempDir::new().unwrap();
    let output = Adopted::bin_in(
        dir.path(),
        state.path(),
        &[
            "init",
            "--template",
            openspec_pack().to_str().unwrap(),
            "--json",
        ],
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json.get("trust").is_none());
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
