//! The `(reviewed, HEAD)` memo behind [`crate::engine::staleness::compute`].
//!
//! RFC-069 Decision 2 named this key and deferred it for want of a measured
//! need. STORY-276 is that need: `StaleRule` put `compute` on the `validate_full`
//! path, which `status --json` and the TUI's validation refresh both run, and
//! STORY-274 stamps `reviewed` on every status transition -- so without a memo
//! the git cost of a routine command scales with the size of the docs tree.
//!
//! Two answers with two lifetimes. A commit's committer time never changes, so
//! `timestamps` is never invalidated. Drift is a range that ends at `HEAD`, so
//! the whole drift map is dropped the moment `HEAD` moves, and the entries need
//! no `HEAD` of their own in their keys.
//!
//! One memo per process, shared: the `Mutex` is here so that the TUI's two
//! staleness workers can hold the same instance -- two instances over one file
//! would each write the whole memo back and the later write would discard what
//! the other learned. Contention is a git subprocess long and there are two
//! threads, so the lock is not worth splitting.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::engine::git_ref::GitRefOps;
use crate::engine::staleness::Drift;

/// Inside `.lazyspec/cache/`, which `lazyspec init` already gitignores.
const CACHE_FILE: &str = "staleness.json";

#[derive(Debug, Default, Serialize, Deserialize)]
struct Memo {
    /// Commit sha -> its committer time as a unix timestamp.
    timestamps: BTreeMap<String, i64>,
    /// The `HEAD` every `drift` entry below was measured to. Empty for a memo
    /// that has never resolved one.
    head: String,
    /// `<anchor sha> ["<glob>", "<glob>"]` -> what moved between it and `head`.
    drift: BTreeMap<String, Drift>,
}

#[derive(Debug, Default)]
struct State {
    memo: Memo,
    /// Whether this process has already tried to resolve `HEAD`. A repository
    /// git cannot answer for is asked once, not once per document.
    head_attempted: bool,
    dirty: bool,
}

#[derive(Debug)]
pub struct StalenessCache {
    /// Where the memo is read from and written back to. `None` is a memo that
    /// remembers nothing at all -- what a test uses, so that a git call log is
    /// the whole story.
    path: Option<PathBuf>,
    state: Mutex<State>,
}

impl StalenessCache {
    /// The memo for `root`, or an empty one when there is none and when the one
    /// on disk cannot be read in the current shape. A cache miss is never an
    /// error: the answer is a git call away.
    pub fn load(root: &Path) -> Self {
        let path = root.join(".lazyspec").join("cache").join(CACHE_FILE);
        let memo = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        Self {
            path: Some(path),
            state: Mutex::new(State {
                memo,
                ..State::default()
            }),
        }
    }

    /// A memo that reads and writes nothing and remembers nothing, so every
    /// lookup reaches git.
    pub fn off() -> Self {
        Self {
            path: None,
            state: Mutex::new(State::default()),
        }
    }

    /// When the anchor commit's time was, memoized on the sha alone: a commit's
    /// committer time is fixed for as long as the commit exists.
    pub fn commit_timestamp(
        &self,
        git: &dyn GitRefOps,
        root: &Path,
        sha: &str,
    ) -> Result<DateTime<Utc>> {
        if self.path.is_none() {
            return git.read_commit_timestamp(root, sha);
        }
        let mut state = self.lock();
        if let Some(cached) = state
            .memo
            .timestamps
            .get(sha)
            .and_then(|secs| DateTime::from_timestamp(*secs, 0))
        {
            return Ok(cached);
        }
        let timestamp = git.read_commit_timestamp(root, sha)?;
        state
            .memo
            .timestamps
            .insert(sha.to_string(), timestamp.timestamp());
        state.dirty = true;
        Ok(timestamp)
    }

    /// What moved under `globs` between `from` and `HEAD`, memoized on the
    /// anchor and the globs while `HEAD` holds still.
    pub fn drift(
        &self,
        git: &dyn GitRefOps,
        root: &Path,
        from: &str,
        globs: &[String],
    ) -> Result<Drift> {
        if self.path.is_none() {
            return git.diff_stat(root, from, "HEAD", globs);
        }
        let mut state = self.lock();
        if !resolve_head(&mut state, git, root) {
            return git.diff_stat(root, from, "HEAD", globs);
        }
        // Debug-formatted rather than joined: a separator a glob may contain --
        // and a comma is legal in one -- makes `["a,b"]` and `["a", "b"]` the
        // same key, and one root's drift would answer for the other's. `{:?}`
        // quotes and escapes every element, so distinct lists key distinctly.
        let key = format!("{from} {globs:?}");
        if let Some(cached) = state.memo.drift.get(&key) {
            return Ok(*cached);
        }
        let drift = git.diff_stat(root, from, "HEAD", globs)?;
        state.memo.drift.insert(key, drift);
        state.dirty = true;
        Ok(drift)
    }

    /// Write the memo back, when there is something new in it to write. Public
    /// and not only on `Drop` because the TUI's two workers share one instance
    /// that outlives every request and is never dropped in an ordinary session:
    /// they flush after each pass instead. Cheap to call on an unchanged memo --
    /// that is what `dirty` is for -- so a warm cache writes nothing.
    pub fn flush(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let mut state = self.lock();
        if !state.dirty {
            return;
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let Ok(raw) = serde_json::to_string(&state.memo) else {
            return;
        };
        // Written beside the memo and renamed over it, rather than into it: a
        // reader that catches a half-written file parses nothing, and `load`
        // answers that with an empty memo -- dropping `head`, and with it every
        // drift entry the file held. A rename is atomic, so a concurrent reader
        // sees one whole memo or the other.
        let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
        match std::fs::write(&tmp, raw).and_then(|()| std::fs::rename(&tmp, path)) {
            Ok(()) => state.dirty = false,
            Err(_) => {
                let _ = std::fs::remove_file(&tmp);
            }
        }
    }

    /// A poisoned memo carries no corrupt state -- worst case an entry is
    /// missing and git is asked again -- so the guard is recovered rather than
    /// taking the process down with it.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Bring the memo's `HEAD` up to date, at most one `rev-parse` per process,
/// dropping every drift entry measured to a `HEAD` that has since moved.
/// Returns whether drift can be memoized at all: a repository whose `HEAD` git
/// will not name has no key to file an entry under.
///
/// The `head_attempted` latch does not record which `root` answered, so it is
/// only sound while one cache is asked about one root -- which holds because
/// every caller passes the `governs_root` the memo was loaded for.
fn resolve_head(state: &mut State, git: &dyn GitRefOps, root: &Path) -> bool {
    if !state.head_attempted {
        state.head_attempted = true;
        let head = git.head(root).unwrap_or_default();
        if head != state.memo.head {
            state.memo.drift.clear();
            state.memo.head = head;
            state.dirty = true;
        }
    }
    !state.memo.head.is_empty()
}

/// Written back once, when the memo goes out of scope, rather than per entry:
/// the whole point is that the next process does not shell out. A one-shot
/// command needs no more than this; a long-lived one flushes as it goes.
impl Drop for StalenessCache {
    fn drop(&mut self) {
        self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::git_ref::test_support::{MockGitRefClient, FAKE_HEAD};
    use tempfile::TempDir;

    const ANCHOR: &str = "0123456789abcdef0123456789abcdef01234567";

    /// `read_commit_timestamp` is a queue that bails when it runs dry, so a
    /// fixture stocks it deeper than any test asks of it.
    fn git_answering(drift: Drift) -> MockGitRefClient {
        let mut git = MockGitRefClient::new().with_diff_stat(drift);
        for _ in 0..4 {
            git = git.with_read_commit_timestamp_result(Ok(Utc::now()));
        }
        git
    }

    fn git_calls(git: &MockGitRefClient, prefix: &str) -> usize {
        git.call_log()
            .borrow()
            .iter()
            .filter(|call| call.starts_with(prefix))
            .count()
    }

    fn drift_of(files: u64) -> Drift {
        Drift {
            files,
            insertions: files,
            deletions: 0,
        }
    }

    // AC3: the anchor and HEAD both unchanged, so no new subprocess. Across two
    // cache instances, because the win STORY-276 needs is the one `status --json`
    // gets on its second invocation, not the one a single process gets.
    #[test]
    fn a_second_process_reissues_neither_lookup() {
        let tmp = TempDir::new().unwrap();
        let globs = vec!["src/engine/**".to_string()];
        let first = git_answering(drift_of(3));
        {
            let cache = StalenessCache::load(tmp.path());
            cache.drift(&first, tmp.path(), ANCHOR, &globs).unwrap();
            cache.commit_timestamp(&first, tmp.path(), ANCHOR).unwrap();
        }
        assert_eq!(git_calls(&first, "diff_stat:"), 1);

        let second = git_answering(drift_of(99));
        let cache = StalenessCache::load(tmp.path());
        let drift = cache.drift(&second, tmp.path(), ANCHOR, &globs).unwrap();
        cache.commit_timestamp(&second, tmp.path(), ANCHOR).unwrap();

        assert_eq!(drift, drift_of(3), "the memoized counts, not a fresh diff");
        assert_eq!(git_calls(&second, "diff_stat:"), 0);
        assert_eq!(git_calls(&second, "read_commit_timestamp:"), 0);
        assert_eq!(git_calls(&second, "head:"), 1, "one rev-parse, bounded");
    }

    /// A moved `HEAD` invalidates every drift entry, because the range they were
    /// measured over ended at the old one. The commit times survive it: a
    /// commit's own time does not move when a branch does.
    #[test]
    fn a_moved_head_drops_the_drift_entries_and_keeps_the_timestamps() {
        let tmp = TempDir::new().unwrap();
        let globs = vec!["src/engine/**".to_string()];
        {
            let git = git_answering(drift_of(3));
            let cache = StalenessCache::load(tmp.path());
            cache.drift(&git, tmp.path(), ANCHOR, &globs).unwrap();
            cache.commit_timestamp(&git, tmp.path(), ANCHOR).unwrap();
        }

        let moved = git_answering(drift_of(7)).with_head_result(Ok("beefbeef".to_string()));
        let cache = StalenessCache::load(tmp.path());
        let drift = cache.drift(&moved, tmp.path(), ANCHOR, &globs).unwrap();
        cache.commit_timestamp(&moved, tmp.path(), ANCHOR).unwrap();

        assert_eq!(drift, drift_of(7), "re-diffed against the new HEAD");
        assert_eq!(git_calls(&moved, "read_commit_timestamp:"), 0);
    }

    /// The other half of the key: a different anchor is a different range, so
    /// the entry filed for the first one does not answer for the second.
    #[test]
    fn a_different_anchor_is_a_different_entry() {
        let tmp = TempDir::new().unwrap();
        let globs = vec!["src/engine/**".to_string()];
        let git = git_answering(drift_of(3));
        let cache = StalenessCache::load(tmp.path());

        cache.drift(&git, tmp.path(), ANCHOR, &globs).unwrap();
        cache
            .drift(&git, tmp.path(), "otheranchor", &globs)
            .unwrap();

        assert_eq!(git_calls(&git, "diff_stat:"), 2);
        assert_eq!(
            git.call_log().borrow()[0],
            format!("head:{}", tmp.path().display()),
            "HEAD is resolved once, before the first diff, not once per anchor"
        );
    }

    /// Two documents pinned to the same commit but governing different code are
    /// two ranges over two file sets, and one answer cannot serve both.
    #[test]
    fn different_globs_under_one_anchor_are_different_entries() {
        let tmp = TempDir::new().unwrap();
        let git = git_answering(drift_of(3));
        let cache = StalenessCache::load(tmp.path());

        cache
            .drift(&git, tmp.path(), ANCHOR, &["src/engine/**".to_string()])
            .unwrap();
        cache
            .drift(&git, tmp.path(), ANCHOR, &["src/cli/**".to_string()])
            .unwrap();

        assert_eq!(git_calls(&git, "diff_stat:"), 2);
    }

    /// STORY-277: a comma is legal inside a glob, so a key that joins the list on
    /// one files `["a,b"]` and `["a", "b"]` under the same entry -- and a warm
    /// memo hands one governs root's drift back as the other's. Order counts as
    /// distinct too: a redundant diff is cheap, a wrong answer is not.
    #[test]
    fn glob_lists_that_differ_only_in_splitting_or_order_are_different_entries() {
        let tmp = TempDir::new().unwrap();
        let git = git_answering(drift_of(3));
        let cache = StalenessCache::load(tmp.path());

        for globs in [
            vec!["a,b".to_string()],
            vec!["a".to_string(), "b".to_string()],
            vec!["b".to_string(), "a".to_string()],
        ] {
            cache.drift(&git, tmp.path(), ANCHOR, &globs).unwrap();
        }

        assert_eq!(git_calls(&git, "diff_stat:"), 3);
    }

    /// `off()` is the seam a call-log assertion needs: it must not quietly
    /// answer the second lookup from the first.
    #[test]
    fn an_off_memo_remembers_nothing() {
        let tmp = TempDir::new().unwrap();
        let git = git_answering(drift_of(3));
        let cache = StalenessCache::off();

        for _ in 0..2 {
            cache
                .drift(&git, tmp.path(), ANCHOR, &["src/**".to_string()])
                .unwrap();
            cache.commit_timestamp(&git, tmp.path(), ANCHOR).unwrap();
        }

        assert_eq!(git_calls(&git, "diff_stat:"), 2);
        assert_eq!(git_calls(&git, "read_commit_timestamp:"), 2);
        assert_eq!(git_calls(&git, "head:"), 0, "no key, so no HEAD to key on");
        assert!(!tmp.path().join(".lazyspec").exists(), "nothing written");
    }

    /// A repository git will not name a `HEAD` for still answers diffs, it just
    /// has nothing to file them under -- and is asked for `HEAD` once, not once
    /// per document.
    #[test]
    fn a_repository_without_a_head_is_asked_for_it_once() {
        let tmp = TempDir::new().unwrap();
        let git = MockGitRefClient::new()
            .with_diff_stat(drift_of(3))
            .with_head_result(Err(anyhow::anyhow!("not a git repository")));
        let cache = StalenessCache::load(tmp.path());

        for _ in 0..3 {
            cache
                .drift(&git, tmp.path(), ANCHOR, &["src/**".to_string()])
                .unwrap();
        }

        assert_eq!(git_calls(&git, "head:"), 1);
        assert_eq!(git_calls(&git, "diff_stat:"), 3);
    }

    /// The shape the TUI's two staleness workers run in: one instance for the
    /// process, flushed after each pass, never dropped. Both workers' learning
    /// has to survive -- two instances over one file each write the whole memo
    /// back, and the later write discards what the other found. And a pass that
    /// learned nothing must write nothing, or the memo trades a git subprocess
    /// per cursor move for a JSON write per cursor move.
    #[test]
    fn one_shared_memo_flushed_per_pass_keeps_both_workers_entries() {
        let tmp = TempDir::new().unwrap();
        let globs = vec!["src/engine/**".to_string()];
        let path = tmp.path().join(".lazyspec").join("cache").join(CACHE_FILE);
        let first = git_answering(drift_of(3));
        let shared = StalenessCache::load(tmp.path());

        shared.commit_timestamp(&first, tmp.path(), ANCHOR).unwrap();
        shared.flush();
        shared.drift(&first, tmp.path(), ANCHOR, &globs).unwrap();
        shared.flush();

        std::fs::remove_file(&path).unwrap();
        shared.flush();
        assert!(!path.exists(), "a flush with nothing new writes nothing");

        shared
            .commit_timestamp(&first, tmp.path(), "second")
            .unwrap();
        shared.flush();

        let second = git_answering(drift_of(99));
        let next = StalenessCache::load(tmp.path());
        let drift = next.drift(&second, tmp.path(), ANCHOR, &globs).unwrap();
        next.commit_timestamp(&second, tmp.path(), ANCHOR).unwrap();

        assert_eq!(drift, drift_of(3), "the findings worker's entry survived");
        assert_eq!(git_calls(&second, "diff_stat:"), 0);
        assert_eq!(
            git_calls(&second, "read_commit_timestamp:"),
            0,
            "the badge worker's entry survived"
        );
    }

    /// A memo nothing was learned from is not rewritten, so a read-only command
    /// over a warm cache touches no file at all.
    #[test]
    fn a_clean_memo_is_not_written_back() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(".lazyspec").join("cache").join(CACHE_FILE);
        drop(StalenessCache::load(tmp.path()));

        assert!(!path.exists());
    }

    /// The head the mock reports by default, so the fixtures above are keying on
    /// something rather than on the empty string.
    #[test]
    fn the_memo_records_the_head_it_measured_to() {
        let tmp = TempDir::new().unwrap();
        let git = git_answering(drift_of(1));
        {
            let cache = StalenessCache::load(tmp.path());
            cache
                .drift(&git, tmp.path(), ANCHOR, &["src/**".to_string()])
                .unwrap();
        }

        let raw =
            std::fs::read_to_string(tmp.path().join(".lazyspec").join("cache").join(CACHE_FILE))
                .unwrap();
        let memo: Memo = serde_json::from_str(&raw).unwrap();
        assert_eq!(memo.head, FAKE_HEAD);
    }
}
