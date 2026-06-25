use anyhow::Result;
use chrono::Utc;
use lazyspec::engine::config::{Config, CoordinationConfig, StoreBackend, TypeDef};
use lazyspec::engine::gh::{
    GhGraphql, GhIssue, GhIssueReader, GhMilestone, GhMilestoneApi, GqlVar,
};
use lazyspec::engine::git_ref::{GitCli, GitRefOps};
use lazyspec::engine::git_ref_store::GitRefStore;
use lazyspec::engine::lease::LeaseEngine;
use lazyspec::engine::store_dispatch::DocumentStore;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

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
}
impl GhGraphql for NoopGh {
    fn graphql(&self, _query: &str, _vars: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
        unreachable!("not used in this test (git-ref types only)")
    }
}
impl GhMilestoneApi for NoopGh {
    fn milestone_list(&self, _repo: &str) -> Result<Vec<GhMilestone>> {
        Ok(vec![])
    }
    fn milestone_view(&self, _repo: &str, _number: u64) -> Result<GhMilestone> {
        unreachable!("not used in this test")
    }
    fn milestone_create(
        &self,
        _repo: &str,
        _title: &str,
        _description: &str,
        _due_on: Option<&str>,
        _state: &str,
    ) -> Result<GhMilestone> {
        unreachable!("not used in this test")
    }
    fn milestone_edit(
        &self,
        _repo: &str,
        _number: u64,
        _title: Option<&str>,
        _description: Option<&str>,
        _due_on: Option<&str>,
        _state: Option<&str>,
    ) -> Result<GhMilestone> {
        unreachable!("not used in this test")
    }
    fn milestone_delete(&self, _repo: &str, _number: u64) -> Result<()> {
        unreachable!("not used in this test")
    }
    fn issue_set_milestone(
        &self,
        _repo: &str,
        _issue_number: u64,
        _milestone: Option<u64>,
    ) -> Result<()> {
        unreachable!("not used in this test")
    }
}

fn config_with_git_ref_iteration() -> Config {
    let mut config = Config::default();
    for t in &mut config.documents.types {
        if t.name == "iteration" {
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

fn iteration_type_def(config: &Config) -> TypeDef {
    config
        .type_by_name("iteration")
        .expect("iteration type")
        .clone()
}

fn make_clone_b(bare: &Path) -> TempDir {
    let dir = TempDir::new().unwrap();
    let bare_str = bare.to_str().unwrap();
    let target_str = dir.path().to_str().unwrap();

    let out = Command::new("git")
        .args(["clone", bare_str, target_str])
        .output()
        .expect("git clone");
    assert!(
        out.status.success(),
        "git clone failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Command::new("git")
        .args(["config", "user.email", "b@test.com"])
        .current_dir(dir.path())
        .output()
        .expect("git config email");
    Command::new("git")
        .args(["config", "user.name", "B"])
        .current_dir(dir.path())
        .output()
        .expect("git config name");

    dir
}

fn run_fetch(root: &Path, config: &Config) -> Result<()> {
    let gh = NoopGh;
    lazyspec::cli::fetch::run(root, config, &gh, &GitCli, "origin", None, true)
}

#[test]
fn fetch_prunes_deleted_remote_doc_refs() {
    // AC1: After delete from clone A and fetch from clone B,
    // B's local doc ref and cache file no longer exist.
    let (fixture_a, bare) = crate::common::TestFixture::with_git_remote();
    let clone_b = make_clone_b(bare.path());

    let config = config_with_git_ref_iteration();
    let type_def = iteration_type_def(&config);

    // Clone A creates ITERATION-001 (pushes ref to bare remote).
    let mut store_a = GitRefStore {
        git: GitCli,
        root: fixture_a.root().to_path_buf(),
        config: config.clone(),
        reserved_number: Some(1),
    };
    let created = store_a
        .create(&type_def, "First Iteration", "agent-a", "body content")
        .expect("create iteration on A");
    assert_eq!(created.id, "ITERATION-001");

    // Clone B fetches; should see the ref + cache file.
    run_fetch(clone_b.path(), &config).expect("first fetch on B");

    let git = GitCli;
    let refname = "refs/lazyspec/iteration/ITERATION-001";
    let cache_file = clone_b
        .path()
        .join(".lazyspec/cache/iteration/ITERATION-001.md");

    assert!(
        git.resolve_ref(clone_b.path(), refname).unwrap().is_some(),
        "B should have local ref after first fetch"
    );
    assert!(
        cache_file.exists(),
        "B should have cache file after first fetch"
    );

    // Clone A deletes ITERATION-001 (removes remote ref).
    store_a
        .delete(&type_def, "ITERATION-001")
        .expect("delete iteration on A");

    // Clone B fetches again; with --prune the local ref should disappear
    // and the cache cleanup loop should remove the cache file.
    run_fetch(clone_b.path(), &config).expect("second fetch on B");

    assert_eq!(
        git.resolve_ref(clone_b.path(), refname).unwrap(),
        None,
        "B's local doc ref should be pruned after fetch"
    );
    assert!(
        !cache_file.exists(),
        "B's cache file should be removed after fetch prunes the ref"
    );
}

#[test]
fn update_rolls_back_on_push_rejection_then_recovers_after_fetch() {
    // AC3: Two clones diverge on the same doc. A pushes first, B's update
    // is rejected (non-FF). B's local state must roll back cleanly so a
    // subsequent fetch + retry succeeds against A's accepted version.
    let (fixture_a, bare) = crate::common::TestFixture::with_git_remote();
    let clone_b = make_clone_b(bare.path());

    let config = config_with_git_ref_iteration();
    let type_def = iteration_type_def(&config);
    let refname = "refs/lazyspec/iteration/ITERATION-001";
    let git = GitCli;

    // Bootstrap: A creates ITERATION-001 (status=draft) and pushes it.
    let mut store_a = GitRefStore {
        git: GitCli,
        root: fixture_a.root().to_path_buf(),
        config: config.clone(),
        reserved_number: Some(1),
    };
    store_a
        .create(&type_def, "First Iteration", "agent-a", "body")
        .expect("create iteration on A");

    let draft_sha = git
        .resolve_ref(fixture_a.root(), refname)
        .unwrap()
        .expect("A has draft ref");

    // B fetches so it sees ITERATION-001 at the draft SHA.
    run_fetch(clone_b.path(), &config).expect("B initial fetch");
    assert_eq!(
        git.resolve_ref(clone_b.path(), refname).unwrap(),
        Some(draft_sha.clone()),
        "B's local ref should be at draft SHA after first fetch"
    );
    let lock_b = lazyspec::engine::cache_lock::CacheLock::load(clone_b.path()).unwrap();
    assert_eq!(
        lock_b.get("iteration/ITERATION-001"),
        Some(draft_sha.as_str()),
        "B's cache.lock should be at draft SHA"
    );
    let cache_file_b = clone_b
        .path()
        .join(".lazyspec/cache/iteration/ITERATION-001.md");
    let draft_cache_content = std::fs::read_to_string(&cache_file_b).unwrap();
    assert!(
        draft_cache_content.contains("status: draft"),
        "B's cache file should reflect draft status, got: {}",
        draft_cache_content
    );

    // A updates ITERATION-001 to status=accepted. Push succeeds; remote advances.
    store_a
        .update(&type_def, "ITERATION-001", &[("status", "accepted")])
        .expect("A updates to accepted and pushes");
    let accepted_sha = git
        .resolve_ref(fixture_a.root(), refname)
        .unwrap()
        .expect("A has accepted ref");
    assert_ne!(accepted_sha, draft_sha);

    // B (stale; still parented on draft) tries to update to status=review.
    // The local CAS succeeds, but the push is rejected (non-FF) by real git.
    // The Task 2 rollback should restore B's local ref to draft, and the
    // cache file + cache.lock save are skipped, so they stay at draft.
    let mut store_b = GitRefStore {
        git: GitCli,
        root: clone_b.path().to_path_buf(),
        config: config.clone(),
        reserved_number: None,
    };
    let result = store_b.update(&type_def, "ITERATION-001", &[("status", "review")]);
    assert!(
        result.is_err(),
        "B's first update should fail (non-fast-forward push)"
    );

    let lock_b_after_fail = lazyspec::engine::cache_lock::CacheLock::load(clone_b.path()).unwrap();
    let lock_sha_after_fail = lock_b_after_fail
        .get("iteration/ITERATION-001")
        .expect("lock entry should still exist");
    assert_eq!(
        lock_sha_after_fail, draft_sha,
        "B's cache.lock should still hold the pre-attempt (draft) SHA"
    );
    let local_ref_after_fail = git
        .resolve_ref(clone_b.path(), refname)
        .unwrap()
        .expect("B's local ref should still exist after rollback");
    assert_eq!(
        local_ref_after_fail, draft_sha,
        "B's local ref should equal cache.lock SHA (rollback restored draft)"
    );
    let cache_after_fail = std::fs::read_to_string(&cache_file_b).unwrap();
    assert_eq!(
        cache_after_fail, draft_cache_content,
        "B's cache file content must be unchanged after the failed update"
    );

    // B fetches: --prune brings the remote's accepted ref down via FF; cache
    // file and cache.lock get updated to the accepted SHA.
    run_fetch(clone_b.path(), &config).expect("B recovery fetch");
    assert_eq!(
        git.resolve_ref(clone_b.path(), refname).unwrap(),
        Some(accepted_sha.clone()),
        "B's local ref should now be at A's accepted SHA"
    );
    let lock_b_after_fetch = lazyspec::engine::cache_lock::CacheLock::load(clone_b.path()).unwrap();
    assert_eq!(
        lock_b_after_fetch.get("iteration/ITERATION-001"),
        Some(accepted_sha.as_str()),
        "B's cache.lock should track A's accepted SHA after fetch"
    );

    // B retries the update; now parented on accepted, the push succeeds.
    store_b
        .update(&type_def, "ITERATION-001", &[("status", "review")])
        .expect("B's retry update should succeed after fetch");

    let b_final_sha = git
        .resolve_ref(clone_b.path(), refname)
        .unwrap()
        .expect("B has final ref");
    assert_ne!(b_final_sha, accepted_sha);

    // Remote should now equal B's commit, chained on top of accepted.
    let bare_resolved = Command::new("git")
        .args(["rev-parse", refname])
        .current_dir(bare.path())
        .output()
        .expect("git rev-parse on bare");
    assert!(bare_resolved.status.success());
    let bare_sha = String::from_utf8_lossy(&bare_resolved.stdout)
        .trim()
        .to_string();
    assert_eq!(
        bare_sha, b_final_sha,
        "remote ref should equal B's final commit"
    );

    let parent_out = Command::new("git")
        .args(["rev-parse", &format!("{}^", b_final_sha)])
        .current_dir(clone_b.path())
        .output()
        .expect("git rev-parse parent");
    assert!(parent_out.status.success());
    let b_parent = String::from_utf8_lossy(&parent_out.stdout)
        .trim()
        .to_string();
    assert_eq!(
        b_parent, accepted_sha,
        "B's final commit should be parented on A's accepted commit"
    );
}

#[test]
fn fetch_prunes_deleted_remote_lease_refs_so_claim_succeeds() {
    // AC2: After release from clone A and claim from clone B,
    // B sees no stale local lease ref and the claim succeeds.
    let (fixture_a, bare) = crate::common::TestFixture::with_git_remote();
    let clone_b = make_clone_b(bare.path());

    let config = config_with_git_ref_iteration();
    let coord = config.coordination.clone().unwrap();
    let type_def = iteration_type_def(&config);

    // Bootstrap: clone A creates ITERATION-001 doc so a lease can target it.
    {
        let mut store_a = GitRefStore {
            git: GitCli,
            root: fixture_a.root().to_path_buf(),
            config: config.clone(),
            reserved_number: Some(1),
        };
        store_a
            .create(&type_def, "First Iteration", "agent-a", "body")
            .expect("create iteration on A");
    }

    let lease_ref = "refs/lazyspec/leases/iteration/ITERATION-001";
    let git = GitCli;

    // Clone A claims ITERATION-001 (pushes lease ref).
    let engine_a = LeaseEngine::new(GitCli, coord.clone());
    engine_a
        .acquire(
            fixture_a.root(),
            "iteration",
            "ITERATION-001",
            "agent-a",
            Utc::now(),
        )
        .expect("A acquires lease");

    // Clone B fetches lease refs so it sees the existing lease locally.
    git.fetch_refs(clone_b.path(), "origin", "refs/lazyspec/leases/*")
        .expect("B fetches leases");
    assert!(
        git.resolve_ref(clone_b.path(), lease_ref)
            .unwrap()
            .is_some(),
        "B should have lease ref locally after fetch"
    );

    // Clone A releases ITERATION-001 (deletes remote lease ref).
    engine_a
        .release(fixture_a.root(), "iteration", "ITERATION-001", "agent-a")
        .expect("A releases lease");

    // Clone B claims ITERATION-001. With --prune in fetch_refs the stale
    // local lease ref disappears during the engine's pre-acquire fetch,
    // so acquire() should succeed without a "lease held" error.
    let engine_b = LeaseEngine::new(GitCli, coord);
    let lease_b = engine_b
        .acquire(
            clone_b.path(),
            "iteration",
            "ITERATION-001",
            "agent-b",
            Utc::now(),
        )
        .expect("B claim should succeed after A released");

    assert_eq!(lease_b.agent, "agent-b");
}
