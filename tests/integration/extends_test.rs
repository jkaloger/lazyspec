use crate::common::{git, git_stdout, TestFixture};
use lazyspec::engine::config::{Config, StoreBackend, TypeDef};
use lazyspec::engine::fs::RealFileSystem;
use lazyspec::engine::store::Store;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_lazyspec")
}

/// A project B whose `.lazyspec.toml` is a bare `extends` one-liner pointing
/// at `a_root`, the shape every sub-test below extends A with.
fn extend(a_root: &Path) -> TempDir {
    let b = TempDir::new().unwrap();
    std::fs::write(
        b.path().join(".lazyspec.toml"),
        format!("extends = \"{}\"\n", a_root.display()),
    )
    .unwrap();
    b
}

// STORY-284 AC1, AC5, AC7 (ITERATION-441): a repo whose `.lazyspec.toml` is a
// bare `extends = "../A"` reports A's types, edges and resolved root through
// `config --json`, and refuses a mutator that would declare anything beside
// `extends` without touching the file.
#[test]
fn extends_one_liner_reports_the_extended_config_and_refuses_mutation() {
    let a = TestFixture::new();

    let add_spike = Command::new(binary())
        .args([
            "config",
            "add-type",
            "spike",
            "spikes",
            "docs/spikes",
            "SPIKE",
        ])
        .current_dir(a.root())
        .output()
        .expect("failed to add-type in A");
    assert!(
        add_spike.status.success(),
        "add-type in A should succeed, stderr: {}",
        String::from_utf8_lossy(&add_spike.stderr)
    );

    let b = TempDir::new().unwrap();
    let b_config_body = format!("extends = \"{}\"\n", a.root().display());
    std::fs::write(b.path().join(".lazyspec.toml"), &b_config_body).unwrap();

    let show = Command::new(binary())
        .args(["config", "show", "--json"])
        .current_dir(b.path())
        .output()
        .expect("failed to run config show in B");
    assert!(
        show.status.success(),
        "config show in B should succeed, stderr: {}",
        String::from_utf8_lossy(&show.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();

    let a_config = Config::load(a.root(), &RealFileSystem).unwrap();
    let a_type_names: Vec<&str> = a_config
        .documents
        .types
        .iter()
        .map(|t| t.name.as_str())
        .collect();
    let b_type_names: Vec<&str> = json["types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(b_type_names, a_type_names);
    assert!(b_type_names.contains(&"spike"));

    assert_eq!(
        json["extends"].as_str().unwrap(),
        a.root().display().to_string(),
        "extends should report A's absolute root"
    );

    assert_eq!(
        json["edges"],
        serde_json::to_value(&a_config.edges).unwrap()
    );

    let add_type_in_b = Command::new(binary())
        .args([
            "config",
            "add-type",
            "spike2",
            "spikes2",
            "docs/spikes2",
            "SPIKE2",
        ])
        .current_dir(b.path())
        .output()
        .expect("failed to run add-type in B");
    assert!(
        !add_type_in_b.status.success(),
        "add-type in B should exit non-zero under extends"
    );
    // `run_add_type` parses B's raw bytes with the strict `Config::parse`
    // before it appends anything (`src/cli/config.rs:298`), so a bare
    // `extends` one-liner refuses at that read -- pointing the reader at the
    // extended config rather than reaching the exclusivity check a config
    // with other sibling keys would trip.
    let stderr = String::from_utf8_lossy(&add_type_in_b.stderr);
    assert!(
        stderr.contains("extends"),
        "stderr should name extends, got: {stderr}"
    );

    let bytes_after = std::fs::read_to_string(b.path().join(".lazyspec.toml")).unwrap();
    assert_eq!(
        bytes_after, b_config_body,
        "B's .lazyspec.toml bytes should be unchanged after a refused mutation"
    );
}

// STORY-284 AC2, AC11 (ITERATION-442): under `extends`, a `filesystem` type's
// documents resolve under the extended root -- `list` and `show` agree on the
// same absolute path through the binary, and `config --json` reports every
// type's `resolved_dir` under that same root.
#[test]
fn filesystem_docs_and_resolved_dirs_move_to_the_extended_root() {
    let a = TestFixture::new();
    a.write_rfc("RFC-001-shared.md", "Shared", "draft");
    let b = extend(a.root());

    let list = Command::new(binary())
        .args(["list", "rfc", "--json"])
        .current_dir(b.path())
        .output()
        .expect("failed to run list in B");
    assert!(
        list.status.success(),
        "list in B should succeed, stderr: {}",
        String::from_utf8_lossy(&list.stderr)
    );
    let docs: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    let doc = &docs.as_array().unwrap()[0];
    let path = doc["path"].as_str().unwrap();
    assert!(
        Path::new(path).starts_with(a.root().join("docs/rfcs")),
        "{path} should be under A's docs/rfcs"
    );

    let id = doc["id"].as_str().unwrap();
    let show = Command::new(binary())
        .args(["show", id, "--json"])
        .current_dir(b.path())
        .output()
        .expect("failed to run show in B");
    assert!(
        show.status.success(),
        "show in B should succeed, stderr: {}",
        String::from_utf8_lossy(&show.stderr)
    );
    let shown: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(
        shown["path"], doc["path"],
        "list and show should agree on path"
    );

    let config_show = Command::new(binary())
        .args(["config", "--json"])
        .current_dir(b.path())
        .output()
        .expect("failed to run config in B");
    assert!(
        config_show.status.success(),
        "config in B should succeed, stderr: {}",
        String::from_utf8_lossy(&config_show.stderr)
    );
    let config_json: serde_json::Value = serde_json::from_slice(&config_show.stdout).unwrap();
    let a_root = a.root().display().to_string();
    for type_entry in config_json["types"].as_array().unwrap() {
        let resolved = type_entry["resolved_dir"].as_str().unwrap();
        assert!(
            resolved.starts_with(&a_root),
            "resolved_dir {resolved} should start with A's root {a_root}"
        );
    }
}

// STORY-284 AC4 (ITERATION-442): `governs`, staleness anchors, `@ref`
// expansion and `.lazyspec/cache/` stay on the local root under `extends` --
// `store.governs_root()` is B, and a `governs` glob declared on a document
// living in A still matches a path under B.
#[test]
fn governs_and_why_stay_anchored_on_the_local_root() {
    let a = TestFixture::new();
    a.write_doc(
        "docs/rfcs/RFC-001-owns-src.md",
        "---\ntitle: \"Owns Src\"\ntype: rfc\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\ngoverns:\n  - \"src/**\"\nrelated: []\n---\n",
    );
    let b = extend(a.root());

    let config = Config::load(b.path(), &RealFileSystem).unwrap();
    let store = Store::load(b.path(), &config).unwrap();
    assert_eq!(store.governs_root(), b.path());

    let why = Command::new(binary())
        .args(["why", "src/main.rs", "--json"])
        .current_dir(b.path())
        .output()
        .expect("failed to run why in B");
    assert!(
        why.status.success(),
        "why in B should succeed, stderr: {}",
        String::from_utf8_lossy(&why.stderr)
    );
    let matches: serde_json::Value = serde_json::from_slice(&why.stdout).unwrap();
    let matches = matches.as_array().unwrap();
    assert_eq!(matches.len(), 1, "got: {matches:?}");
    assert_eq!(matches[0]["id"], "RFC-001");
    assert_eq!(matches[0]["glob"], "src/**");
}

// STORY-284 AC2 (ITERATION-442): `create` in B writes into A's docs, nothing
// under B's, and the rendered template comes from A's
// `.lazyspec/templates/template.md`.
#[test]
fn create_writes_under_the_extended_root_using_its_template() {
    let a = TestFixture::new();
    let templates_dir = a.root().join(".lazyspec/templates");
    std::fs::create_dir_all(&templates_dir).unwrap();
    std::fs::write(
        templates_dir.join("template.md"),
        "---\ntitle: \"{title}\"\ntype: {type}\nstatus: draft\nauthor: \"{author}\"\ndate: {date}\ntags: []\nrelated: []\n---\n<!-- MARKER: template from A -->\n",
    )
    .unwrap();
    let b = extend(a.root());

    let create = Command::new(binary())
        .args(["create", "rfc", "From B", "--author", "tester", "--json"])
        .current_dir(b.path())
        .output()
        .expect("failed to run create in B");
    assert!(
        create.status.success(),
        "create in B should succeed, stderr: {}",
        String::from_utf8_lossy(&create.stderr)
    );
    let created: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    let path = Path::new(created["path"].as_str().unwrap()).to_path_buf();
    assert!(
        path.starts_with(a.root().join("docs/rfcs")),
        "{} should be under A's docs/rfcs",
        path.display()
    );
    assert!(
        !b.path().join("docs").exists(),
        "create should not write anything under B's docs"
    );

    let content = std::fs::read_to_string(&path).unwrap();
    assert!(
        content.contains("MARKER: template from A"),
        "got: {content}"
    );
}

/// A minimal remote carrying one RFC on `main`, scoped to what this file
/// needs: a `git` type to clone from under `extends` (see
/// `git_store_test.rs`'s `shared_repo` for the fuller two-branch fixture).
fn remote_with_one_rfc() -> TempDir {
    let repo = TempDir::new().unwrap();
    let path = repo.path();
    git(path, &["init", "-b", "main"]);
    git(path, &["config", "user.email", "test@test.com"]);
    git(path, &["config", "user.name", "Test"]);
    std::fs::create_dir_all(path.join("docs/rfcs")).unwrap();
    std::fs::write(
        path.join("docs/rfcs/RFC-001-a.md"),
        "---\ntitle: \"A\"\ntype: rfc\nstatus: draft\nauthor: tester\ndate: 2026-04-01\ntags: []\n---\n",
    )
    .unwrap();
    git(path, &["add", "-A"]);
    git(path, &["commit", "-m", "one"]);
    repo
}

// STORY-284 AC4 (ITERATION-442): a `git` type declared in the extended config
// still clones into *this* repo's managed cache -- B's, never A's -- because
// the cache belongs to the repo doing the reading, not the shared one.
#[test]
fn git_type_clones_into_the_local_cache_not_the_extended_root() {
    let remote = remote_with_one_rfc();
    let a = TempDir::new().unwrap();
    let mut config = Config::default();
    config.documents.types = vec![TypeDef {
        dir: "docs/rfcs".to_string(),
        remote: Some(remote.path().to_string_lossy().into_owned()),
        ..TypeDef::test_fixture("rfc", StoreBackend::Git)
    }];
    std::fs::write(a.path().join(".lazyspec.toml"), config.to_toml().unwrap()).unwrap();
    let b = extend(a.path());

    let list = Command::new(binary())
        .args(["list", "rfc", "--json"])
        .current_dir(b.path())
        .output()
        .expect("failed to run list in B");
    assert!(
        list.status.success(),
        "list in B should succeed, stderr: {}",
        String::from_utf8_lossy(&list.stderr)
    );

    assert!(b.path().join(".lazyspec/cache/rfc").exists());
    assert!(!a.path().join(".lazyspec").exists());
}

// Fix 2 (code review of STORY-284): a missing relative `dir` under `extends`
// is silent, exactly like a missing relative `dir` locally -- git cannot
// commit an empty directory, so a shared doc set with a type nobody has
// populated yet (an `rfcs` dir but no `adrs` yet) must not warn on every
// command that reads it.
#[test]
fn missing_relative_dir_under_extends_is_silent() {
    let a = TestFixture::new();
    let add_ghost = Command::new(binary())
        .args([
            "config",
            "add-type",
            "ghost",
            "ghosts",
            "docs/ghosts",
            "GHOST",
        ])
        .current_dir(a.root())
        .output()
        .expect("failed to add-type in A");
    assert!(
        add_ghost.status.success(),
        "add-type in A should succeed, stderr: {}",
        String::from_utf8_lossy(&add_ghost.stderr)
    );
    let b = extend(a.root());

    let list = Command::new(binary())
        .args(["list", "--json"])
        .current_dir(b.path())
        .output()
        .expect("failed to run list in B");
    assert!(
        list.status.success(),
        "list in B should succeed, stderr: {}",
        String::from_utf8_lossy(&list.stderr)
    );
    let stderr = String::from_utf8_lossy(&list.stderr);
    assert!(
        !stderr.contains("warning:"),
        "a missing relative dir under extends must stay silent, got: {stderr}"
    );
}

// The absolute-dir half of STORY-283 AC6 still warns under `extends`: a typo
// in an absolute `dir` is still a typo, regardless of where the config that
// declares it came from.
#[test]
fn missing_absolute_dir_under_extends_still_warns() {
    let a = TestFixture::new();
    let ghost_dir = TempDir::new().unwrap();
    let missing_dir = ghost_dir.path().join("ghosts");
    let add_ghost = Command::new(binary())
        .args([
            "config",
            "add-type",
            "ghost",
            "ghosts",
            missing_dir.to_str().unwrap(),
            "GHOST",
        ])
        .current_dir(a.root())
        .output()
        .expect("failed to add-type in A");
    assert!(
        add_ghost.status.success(),
        "add-type in A should succeed, stderr: {}",
        String::from_utf8_lossy(&add_ghost.stderr)
    );
    let b = extend(a.root());

    let list = Command::new(binary())
        .args(["list", "--json"])
        .current_dir(b.path())
        .output()
        .expect("failed to run list in B");
    assert!(
        list.status.success(),
        "list in B should succeed, stderr: {}",
        String::from_utf8_lossy(&list.stderr)
    );
    let stderr = String::from_utf8_lossy(&list.stderr);
    assert!(
        stderr.contains("warning:"),
        "stderr should carry a warning, got: {stderr}"
    );
    assert!(
        stderr.contains(&*missing_dir.to_string_lossy()),
        "stderr should name {}, got: {stderr}",
        missing_dir.display()
    );
}

/// A remote carrying a full `.lazyspec.toml` + `docs/` on `main` -- what a URL
/// `extends` (ITERATION-443) resolves at, distinct from `remote_with_one_rfc`
/// (a `git` *type*'s remote, which carries no config of its own). A second
/// branch, `next`, declares one extra type so a `#next` fragment is
/// observably different from the default branch.
fn shared_config_repo() -> TestFixture {
    let a = TestFixture::new();
    a.write_rfc("RFC-001-shared.md", "Shared", "draft");
    git(a.root(), &["init", "-b", "main"]);
    git(a.root(), &["config", "user.email", "test@test.com"]);
    git(a.root(), &["config", "user.name", "Test"]);
    git(a.root(), &["add", "-A"]);
    git(a.root(), &["commit", "-m", "one"]);
    git(a.root(), &["checkout", "-b", "next"]);
    let add_spike = Command::new(binary())
        .args([
            "config",
            "add-type",
            "spike",
            "spikes",
            "docs/spikes",
            "SPIKE",
        ])
        .current_dir(a.root())
        .output()
        .expect("failed to add-type on next");
    assert!(
        add_spike.status.success(),
        "add-type on next should succeed, stderr: {}",
        String::from_utf8_lossy(&add_spike.stderr)
    );
    git(a.root(), &["add", "-A"]);
    git(a.root(), &["commit", "-m", "two"]);
    git(a.root(), &["checkout", "main"]);
    a
}

// STORY-284 AC8 (ITERATION-443): a URL `extends` clones under B's local
// `.lazyspec/cache/config/` on first run and lists A's committed docs; `config
// --json` reports `.extends` as that clone path, not A's own root.
#[test]
fn url_extends_clones_under_the_local_cache_and_lists_the_extended_docs() {
    let a = shared_config_repo();
    let b = TempDir::new().unwrap();
    std::fs::write(
        b.path().join(".lazyspec.toml"),
        format!("extends = \"file://{}\"\n", a.root().display()),
    )
    .unwrap();

    let list = Command::new(binary())
        .args(["list", "rfc", "--json"])
        .current_dir(b.path())
        .output()
        .expect("failed to run list in B");
    assert!(
        list.status.success(),
        "list in B should succeed, stderr: {}",
        String::from_utf8_lossy(&list.stderr)
    );
    let docs: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(docs.as_array().unwrap().len(), 1, "got: {docs}");

    let clone_root = b.path().join(".lazyspec/cache/config");
    assert!(
        clone_root.join(".lazyspec.toml").exists(),
        "the clone should carry A's .lazyspec.toml"
    );

    let config_show = Command::new(binary())
        .args(["config", "--json"])
        .current_dir(b.path())
        .output()
        .expect("failed to run config in B");
    assert!(config_show.status.success());
    let json: serde_json::Value = serde_json::from_slice(&config_show.stdout).unwrap();
    // Canonicalize both sides: the child process's cwd resolves macOS's
    // `/tmp` symlink to `/private/tmp`, which `TempDir::path()` does not, so a
    // literal string compare would fail on a platform artifact rather than a
    // real path mismatch.
    assert_eq!(
        Path::new(json["extends"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        clone_root.canonicalize().unwrap(),
        "extends should report the local clone path, not A's own root"
    );
}

// STORY-284 AC10 (ITERATION-443): a `#<branch>` fragment on a URL `extends`
// clones that branch, so `config --json` reports the type only `next`
// declares.
#[test]
fn url_extends_with_branch_fragment_resolves_that_branch() {
    let a = shared_config_repo();
    let b = TempDir::new().unwrap();
    std::fs::write(
        b.path().join(".lazyspec.toml"),
        format!("extends = \"file://{}#next\"\n", a.root().display()),
    )
    .unwrap();

    let config_show = Command::new(binary())
        .args(["config", "--json"])
        .current_dir(b.path())
        .output()
        .expect("failed to run config in B");
    assert!(
        config_show.status.success(),
        "config in B should succeed, stderr: {}",
        String::from_utf8_lossy(&config_show.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&config_show.stdout).unwrap();
    let type_names: Vec<&str> = json["types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(
        type_names.contains(&"spike"),
        "next's extra type should be visible, got: {type_names:?}"
    );
}

// STORY-284 AC8: once the clone exists, a later run reads it as-is and never
// touches the network -- moving A's directory away leaves B's clone reading
// the same docs.
#[test]
fn url_extends_reuses_the_existing_clone_once_the_remote_is_gone() {
    let a = shared_config_repo();
    let a_path = a.root().to_path_buf();
    let b = TempDir::new().unwrap();
    std::fs::write(
        b.path().join(".lazyspec.toml"),
        format!("extends = \"file://{}\"\n", a_path.display()),
    )
    .unwrap();

    let first = Command::new(binary())
        .args(["list", "rfc", "--json"])
        .current_dir(b.path())
        .output()
        .expect("failed to run list in B");
    assert!(first.status.success());

    // A fresh TempDir, not a fixed name beside `a_path`: the OS temp dir is
    // shared across every test run, and a fixed destination left behind by a
    // prior run would make this `rename` fail with "directory not empty".
    let elsewhere = TempDir::new().unwrap();
    let moved = elsewhere.path().join("moved-away");
    std::fs::rename(&a_path, &moved).unwrap();

    let second = Command::new(binary())
        .args(["list", "rfc", "--json"])
        .current_dir(b.path())
        .output()
        .expect("failed to run list in B a second time");
    assert!(
        second.status.success(),
        "an existing clone must not require the remote to still exist, stderr: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(
        first.stdout, second.stdout,
        "the second run should see the same docs as the first"
    );
}

// STORY-284 AC9 (ITERATION-444): `lazyspec fetch` brings a URL `extends`
// clone to the remote's tip before it fetches any type. A doc and a type
// committed straight into A's `main` after B's first read are invisible to B
// until `fetch` runs; afterward the *next* command (not `fetch` itself,
// which still ran with the pre-refresh config) sees both, and B's clone sits
// at exactly A's `HEAD`.
#[test]
fn fetch_refreshes_the_config_clone_so_the_next_command_sees_a_and_gadget() {
    let a = shared_config_repo();
    let b = TempDir::new().unwrap();
    std::fs::write(
        b.path().join(".lazyspec.toml"),
        format!("extends = \"file://{}\"\n", a.root().display()),
    )
    .unwrap();

    let list = Command::new(binary())
        .args(["list", "rfc", "--json"])
        .current_dir(b.path())
        .output()
        .expect("failed to run list in B");
    assert!(
        list.status.success(),
        "list in B should succeed, stderr: {}",
        String::from_utf8_lossy(&list.stderr)
    );

    // A new type and a new doc of it, committed straight onto A's `main` --
    // the branch B's bare `extends` (no `#branch`) tracks.
    let add_gadget = Command::new(binary())
        .args([
            "config",
            "add-type",
            "gadget",
            "gadgets",
            "docs/gadgets",
            "GADGET",
        ])
        .current_dir(a.root())
        .output()
        .expect("failed to add-type in A");
    assert!(
        add_gadget.status.success(),
        "add-type in A should succeed, stderr: {}",
        String::from_utf8_lossy(&add_gadget.stderr)
    );
    std::fs::create_dir_all(a.root().join("docs/gadgets")).unwrap();
    std::fs::write(
        a.root().join("docs/gadgets/GADGET-001-widget.md"),
        "---\ntitle: \"Widget\"\ntype: gadget\nstatus: draft\nauthor: tester\ndate: 2026-04-01\ntags: []\n---\n",
    )
    .unwrap();
    git(a.root(), &["add", "-A"]);
    git(a.root(), &["commit", "-m", "three"]);

    let fetch = Command::new(binary())
        .args(["fetch", "--json"])
        .current_dir(b.path())
        .output()
        .expect("failed to run fetch in B");
    assert!(
        fetch.status.success(),
        "fetch in B should succeed, stderr: {}",
        String::from_utf8_lossy(&fetch.stderr)
    );

    let config_show = Command::new(binary())
        .args(["config", "--json"])
        .current_dir(b.path())
        .output()
        .expect("failed to run config in B");
    assert!(config_show.status.success());
    let json: serde_json::Value = serde_json::from_slice(&config_show.stdout).unwrap();
    let type_names: Vec<&str> = json["types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(
        type_names.contains(&"gadget"),
        "the type added to A after B's first read should be visible, got: {type_names:?}"
    );

    let list_gadget = Command::new(binary())
        .args(["list", "gadget", "--json"])
        .current_dir(b.path())
        .output()
        .expect("failed to run list gadget in B");
    assert!(
        list_gadget.status.success(),
        "list gadget in B should succeed, stderr: {}",
        String::from_utf8_lossy(&list_gadget.stderr)
    );
    let docs: serde_json::Value = serde_json::from_slice(&list_gadget.stdout).unwrap();
    assert_eq!(docs.as_array().unwrap().len(), 1, "got: {docs}");

    let clone_head = git_stdout(
        &b.path().join(".lazyspec/cache/config"),
        &["rev-parse", "HEAD"],
    );
    let a_head = git_stdout(a.root(), &["rev-parse", "HEAD"]);
    assert_eq!(
        clone_head, a_head,
        "B's clone should sit at exactly A's HEAD after fetch"
    );
}

// STORY-284 AC9 (ITERATION-444): when the URL `extends` remote can no longer
// be reached, `fetch` exits non-zero and names it -- distinct from `list`
// (and every other command), which reads an existing clone as-is and never
// requires the remote to still exist.
#[test]
fn fetch_fails_naming_the_remote_when_the_extends_remote_is_gone() {
    let a = shared_config_repo();
    let a_path = a.root().to_path_buf();
    let b = TempDir::new().unwrap();
    std::fs::write(
        b.path().join(".lazyspec.toml"),
        format!("extends = \"file://{}\"\n", a_path.display()),
    )
    .unwrap();

    let list = Command::new(binary())
        .args(["list", "rfc", "--json"])
        .current_dir(b.path())
        .output()
        .expect("failed to run list in B");
    assert!(list.status.success());

    let elsewhere = TempDir::new().unwrap();
    let moved = elsewhere.path().join("moved-away");
    std::fs::rename(&a_path, &moved).unwrap();

    let fetch = Command::new(binary())
        .args(["fetch", "--json"])
        .current_dir(b.path())
        .output()
        .expect("failed to run fetch in B");
    assert!(
        !fetch.status.success(),
        "fetch should exit non-zero when the extends remote is gone"
    );
    let stderr = String::from_utf8_lossy(&fetch.stderr);
    let expected_url = format!("file://{}", a_path.display());
    assert!(
        stderr.contains(&expected_url),
        "stderr should name {expected_url}, got: {stderr}"
    );
}
