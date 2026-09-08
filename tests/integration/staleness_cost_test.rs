//! STORY-276: what the staleness rule costs on the `validate_full` path, which
//! `validate`, `status --json` and the TUI's validation refresh all run.
//!
//! Asserted on the git call log rather than on a clock. A count is the whole
//! claim: the findings these passes produce are STORY-273's tests, and a wrong
//! count produces exactly the same findings.

use crate::common::TestFixture;
use chrono::{Duration, Utc};
use lazyspec::engine::config::Config;
use lazyspec::engine::git_ref::test_support::MockGitRefClient;
use lazyspec::engine::staleness::Drift;
use lazyspec::engine::staleness_cache::StalenessCache;
use lazyspec::engine::store::Store;
use lazyspec::engine::validation::stale_findings;

const DOCS: usize = 12;

/// `DOCS` age-driven documents, every one of them pinned to an anchor of its
/// own, which is the tree STORY-274's stamping produces.
fn pinned_tree(age_days: i64) -> (TestFixture, Store) {
    let fixture = TestFixture::new();
    let date = Utc::now().date_naive() - Duration::days(age_days);
    for n in 1..=DOCS {
        fixture.write_doc(
            &format!("docs/rfcs/RFC-{n:03}-pinned.md"),
            &format!(
                "---\ntitle: \"Pinned {n}\"\ntype: rfc\nstatus: draft\nauthor: t\ndate: {date}\ntags: []\ngoverns:\n  - \"src/engine/**\"\nreviewed: \"anchor{n:03}\"\nrelated: []\n---\n\nbody\n"
            ),
        );
    }
    let store = fixture.store();
    assert_eq!(store.all_docs().len(), DOCS);
    (fixture, store)
}

/// A git double that answers both lookups the band needs, as often as it is
/// asked. `read_commit_timestamp` is a queue that bails when it runs dry, so it
/// is stocked deeper than any pass below could drain it.
fn git_answering(age_days: i64) -> MockGitRefClient {
    let mut git = MockGitRefClient::new().with_diff_stat(Drift {
        files: 4,
        insertions: 10,
        deletions: 2,
    });
    for _ in 0..DOCS * 4 {
        git = git.with_read_commit_timestamp_result(Ok(Utc::now() - Duration::days(age_days)));
    }
    git
}

/// One invocation's worth of the rule -- its own memo, loaded and written back
/// the way a fresh process would -- and what it cost in git subprocesses.
fn one_pass(store: &Store, config: &Config, git: &MockGitRefClient) -> (usize, usize) {
    let before = git.call_log().borrow().len();
    let findings = stale_findings(
        store.governs_root(),
        store.all_docs(),
        config,
        git,
        &StalenessCache::load(store.root()),
    )
    .len();
    let calls = git.call_log().borrow().len() - before;
    (findings, calls)
}

/// AC1 and AC3 together: a second invocation over a tree whose anchors and
/// whose `HEAD` have not moved costs one `rev-parse` and nothing else, however
/// many documents are pinned. The first invocation is what it costs to learn
/// the answers; the memo is what stops every one after it paying again.
#[test]
fn a_second_pass_over_an_unchanged_tree_costs_one_subprocess() {
    let config = Config::default();
    let (_fixture, store) = pinned_tree(200);
    let git = git_answering(200);

    let (_, first) = one_pass(&store, &config, &git);
    let (_, second) = one_pass(&store, &config, &git);

    assert!(
        first >= DOCS,
        "the first pass must actually cost git calls, or the second proves nothing: {first}"
    );
    assert_eq!(
        second, 1,
        "one rev-parse to confirm HEAD held still, then nothing"
    );
}

/// The memo does not change what the rule reports, only what it costs.
#[test]
fn the_memo_reports_what_a_cold_pass_reports() {
    let config = Config::default();
    let (_fixture, store) = pinned_tree(200);
    let git = git_answering(200);

    let (cold, _) = one_pass(&store, &config, &git);
    let (warm, _) = one_pass(&store, &config, &git);

    assert_eq!(cold, DOCS, "every document is 200 days past its anchor");
    assert_eq!(warm, cold);
}

/// AC4: an age-driven document dated inside the `aging` window cannot be stale
/// whatever its anchor turns out to be, so the rule asks git nothing about it --
/// not even on a cold cache, and not even though every one of them is pinned.
#[test]
fn a_young_age_driven_tree_costs_nothing_at_all() {
    let config = Config::default();
    let (_fixture, store) = pinned_tree(1);
    let git = git_answering(1);

    let (findings, calls) = one_pass(&store, &config, &git);

    assert_eq!(findings, 0);
    assert_eq!(
        calls,
        0,
        "no anchor could band these worse than fresh: {:?}",
        git.call_log().borrow()
    );
}
