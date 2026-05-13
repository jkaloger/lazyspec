mod common;

use anyhow::Result;
use lazyspec::engine::config::{Config, CoordinationConfig, StoreBackend, TypeDef};
use lazyspec::engine::gh::{GhIssue, GhIssueReader};
use lazyspec::engine::git_ref::GitCli;
use lazyspec::engine::git_ref_store::GitRefStore;
use lazyspec::engine::store_dispatch::DocumentStore;
use std::path::Path;

/// Stub GH reader for `cli::fetch::run` — fetch handles git-ref types separately,
/// but the function still requires a `GhIssueReader` arg.
struct NoopGh;
impl GhIssueReader for NoopGh {
    fn issue_list(
        &self,
        _repo: &str,
        _labels: &[String],
        _json_fields: &[String],
        _limit: Option<u64>,
    ) -> Result<Vec<GhIssue>> {
        Ok(vec![])
    }
    fn issue_view(&self, _repo: &str, _number: u64) -> Result<GhIssue> {
        unreachable!("not used in this test")
    }
    fn user_exists(&self, _login: &str) -> Result<bool> {
        Ok(true)
    }
}

/// Config with `story` switched to the git-ref backend + coordination enabled
/// (so the store pushes refs to `origin`, matching the real CLI flow).
fn config_with_git_ref_story() -> Config {
    let mut config = Config::default();
    for t in &mut config.documents.types {
        if t.name == "story" {
            t.store = StoreBackend::GitRef;
        }
    }
    config.coordination = Some(CoordinationConfig {
        remote: "origin".to_string(),
        lease_duration: "60m".to_string(),
        grace_period: "2m".to_string(),
        max_push_retries: 5,
        max_clock_skew: "5m".to_string(),
    });
    config
}

fn story_type_def(config: &Config) -> TypeDef {
    config
        .type_by_name("story")
        .expect("story type configured")
        .clone()
}

fn run_fetch(root: &Path, config: &Config) -> Result<()> {
    lazyspec::cli::fetch::run(root, config, &NoopGh, &GitCli, "origin", None, true)
}

/// Cache file path for a story under the git-ref cache layout.
fn cache_file(root: &Path, doc_id: &str) -> std::path::PathBuf {
    root.join(format!(".lazyspec/cache/story/{}.md", doc_id))
}

/// Parse `assignees` list out of a doc's YAML frontmatter (verbatim, order preserved).
fn read_assignees(path: &Path) -> Vec<String> {
    let content = std::fs::read_to_string(path).expect("read cache file");
    let parts: Vec<&str> = content.splitn(3, "---").collect();
    assert!(parts.len() >= 3, "doc missing frontmatter: {:?}", path);
    let yaml: serde_yaml::Value = serde_yaml::from_str(parts[1]).expect("parse frontmatter yaml");
    yaml.get("assignees")
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Drop both the cache markdown file and the doc's `cache.lock` entry so the
/// next `fetch` is forced to re-materialise the cache from the ref blob.
fn wipe_cache(root: &Path, doc_id: &str) {
    let cache_path = cache_file(root, doc_id);
    if cache_path.exists() {
        std::fs::remove_file(&cache_path).expect("remove cache file");
    }
    let mut lock =
        lazyspec::engine::cache_lock::CacheLock::load(root).expect("load cache.lock");
    lock.remove(&format!("story/{}", doc_id));
    lock.save(root).expect("save cache.lock");
}

#[test]
fn git_ref_round_trip_assignees() {
    // AC2: git-ref backend round-trips assignees = ["bob"] through a real
    // ref push + cache-wipe + fetch re-materialisation.
    let (fixture, _bare) = common::TestFixture::with_git_remote();
    let config = config_with_git_ref_story();
    let type_def = story_type_def(&config);

    // Create STORY-001 via the git-ref store (push to bare remote).
    let mut store = GitRefStore {
        git: GitCli,
        root: fixture.root().to_path_buf(),
        config: config.clone(),
        reserved_number: Some(1),
    };
    let created = store
        .create(&type_def, "Round-trip Story", "agent", "body")
        .expect("create story");
    assert_eq!(created.id, "STORY-001");

    // Assign `bob`. set_assignees rewrites the ref, pushes to origin, then
    // updates the local cache file.
    store
        .set_assignees(&type_def, "STORY-001", &["bob".to_string()])
        .expect("set_assignees bob");

    let cache_path = cache_file(fixture.root(), "STORY-001");
    assert_eq!(
        read_assignees(&cache_path),
        vec!["bob".to_string()],
        "pre-wipe cache should hold the just-written assignees"
    );

    // Blow away the cache file + cache.lock entry, forcing fetch to
    // re-materialise from the ref's `doc.md` blob.
    wipe_cache(fixture.root(), "STORY-001");
    assert!(
        !cache_path.exists(),
        "cache file should be gone after wipe"
    );

    // Fetch reads remote refs, writes cache files from the ref blobs.
    run_fetch(fixture.root(), &config).expect("fetch re-materialise");

    assert!(
        cache_path.exists(),
        "fetch should have re-materialised the cache file from the ref"
    );
    assert_eq!(
        read_assignees(&cache_path),
        vec!["bob".to_string()],
        "git-ref round-trip must preserve assignees through ref->cache rematerialisation"
    );
}

#[test]
fn git_ref_round_trip_preserves_free_form_assignees() {
    // AC6 reinforcement: free-form strings (non-GitHub-user-shaped) survive
    // the git-ref push/fetch round-trip verbatim and in order.
    let (fixture, _bare) = common::TestFixture::with_git_remote();
    let config = config_with_git_ref_story();
    let type_def = story_type_def(&config);

    let mut store = GitRefStore {
        git: GitCli,
        root: fixture.root().to_path_buf(),
        config: config.clone(),
        reserved_number: Some(1),
    };
    store
        .create(&type_def, "Free-form Story", "agent", "body")
        .expect("create story");

    // Two sequential assigns (mirrors how `lazyspec assign` appends): first
    // alice, then a non-GH-shaped string. The store replaces the whole list
    // on each call, so we pass the full target list each time.
    store
        .set_assignees(&type_def, "STORY-001", &["alice".to_string()])
        .expect("set_assignees alice");
    store
        .set_assignees(
            &type_def,
            "STORY-001",
            &[
                "alice".to_string(),
                "not-a-real-github-user@example.com".to_string(),
            ],
        )
        .expect("set_assignees alice + free-form");

    let cache_path = cache_file(fixture.root(), "STORY-001");
    wipe_cache(fixture.root(), "STORY-001");
    assert!(!cache_path.exists(), "cache file should be gone after wipe");

    run_fetch(fixture.root(), &config).expect("fetch re-materialise");

    assert!(
        cache_path.exists(),
        "fetch should have re-materialised the cache file from the ref"
    );
    assert_eq!(
        read_assignees(&cache_path),
        vec![
            "alice".to_string(),
            "not-a-real-github-user@example.com".to_string(),
        ],
        "git-ref store must preserve free-form assignee strings verbatim and in order through ref->cache rematerialisation"
    );
}
