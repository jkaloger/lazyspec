//! How stale one document is (RFC-069): a coarse band computed on demand from a
//! review anchor and what moved under the document's `governs` globs since it.
//!
//! Nothing here is stored. [`compute`] is called by the surfaces that show a
//! band -- `show` and the TUI's background badge worker, which bands the
//! selected document off the render path -- and by `StaleRule` on the
//! `validate_full` path, so `validate`, `status --json` and the TUI validation
//! refresh pay for it too, unless `[staleness] finding = "off"` gates the rule
//! out before it runs. A command on neither path issues no git subprocess for
//! staleness.
//!
//! What git is asked goes through
//! [`StalenessCache`](crate::engine::staleness_cache::StalenessCache), so an
//! anchor and a `HEAD` that both held still since the last invocation cost
//! nothing (STORY-276). Two cheaper guards come before it:
//! [`cannot_be_stale`], which a caller looking only for rot uses to skip a
//! document no anchor could band worse than fresh, and [`drifted`], which
//! answers `why`'s one staleness question without reading an anchor commit it
//! would discard.

use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::engine::config::{Config, StalenessConfig, StalenessDriver};
use crate::engine::document::DocMeta;
use crate::engine::git_ref::GitRefOps;
use crate::engine::staleness_cache::StalenessCache;

/// How much a document should be trusted at a glance. Three values, never a
/// score: a reader can argue with a band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Band {
    Fresh,
    Aging,
    Stale,
}

impl std::fmt::Display for Band {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Band::Fresh => "fresh",
            Band::Aging => "aging",
            Band::Stale => "stale",
        })
    }
}

/// What the age is measured from. Serializes as a bare string either way -- a
/// sha or an ISO date -- because every reader only prints it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Anchor {
    Sha(String),
    Date(NaiveDate),
}

impl std::fmt::Display for Anchor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Anchor::Sha(sha) => f.write_str(sha),
            Anchor::Date(date) => write!(f, "{date}"),
        }
    }
}

/// What moved under a document's `governs` globs between its review anchor and
/// `HEAD`, as `git diff` counts it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Drift {
    pub files: u64,
    pub insertions: u64,
    pub deletions: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Staleness {
    pub band: Band,
    pub driver: StalenessDriver,
    pub anchor: Anchor,
    pub age_days: u64,
    pub drift: Drift,
}

/// The band and the facts behind it, as RFC-069 writes them. Lives on the type
/// because two surfaces print it -- `show`'s `staleness:` line and the `stale`
/// validation finding -- and one wording cannot drift from itself.
impl std::fmt::Display for Staleness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({}, {} files since {}, {}d)",
            self.band, self.driver, self.drift.files, self.anchor, self.age_days
        )
    }
}

/// The band, and the facts behind it, for one document.
///
/// The type's configured driver decides which fact bands it, but both facts are
/// reported: an `age` type with a pin still says how much moved under it.
///
/// A `drift` type falls back to banding by age when there is nothing to diff --
/// no `reviewed` anchor, no `governs` globs, or a git that could not answer --
/// and reports `driver: age` so the fallback is visible rather than being
/// mistaken for a document that nothing has touched.
///
/// Takes `governs_root` rather than the `Store` it came from: that is the only
/// thing a computation reads off the store, and a path can cross a thread
/// boundary to the TUI's staleness worker (STORY-275) where a `Store` cannot.
pub fn compute(
    governs_root: &Path,
    config: &Config,
    doc: &DocMeta,
    git: &dyn GitRefOps,
    cache: &StalenessCache,
) -> Staleness {
    let anchor = doc
        .reviewed
        .clone()
        .map_or(Anchor::Date(doc.date), Anchor::Sha);

    let drift = drift_of(governs_root, doc, git, cache);

    let driver = match drift {
        Some(_) => configured_driver(config, doc),
        None => StalenessDriver::Age,
    };

    let age_days = match &anchor {
        Anchor::Sha(sha) => cache
            .commit_timestamp(git, governs_root, sha)
            .map_or_else(|_| days_since(doc.date), |ts| days_since(ts.date_naive())),
        Anchor::Date(date) => days_since(*date),
    };

    let drift = drift.unwrap_or_default();
    let band = match driver {
        StalenessDriver::Drift if drift.files == 0 => Band::Fresh,
        StalenessDriver::Drift => Band::Stale,
        StalenessDriver::Age => band_by_age(age_days, config.staleness),
    };

    Staleness {
        band,
        driver,
        anchor,
        age_days,
        drift,
    }
}

/// What moved under `doc`'s globs since its anchor, or `None` when there is
/// nothing to diff -- no anchor, or no globs to diff over.
///
/// The diff runs in the root the `governs` globs resolve against, which on a
/// docs-repo split is not the docs root, and `reviewed` is stamped from that
/// same root's HEAD -- so the anchor commit is read there too.
fn drift_of(
    governs_root: &Path,
    doc: &DocMeta,
    git: &dyn GitRefOps,
    cache: &StalenessCache,
) -> Option<Drift> {
    let (Some(sha), false) = (&doc.reviewed, doc.governs.is_empty()) else {
        return None;
    };
    cache.drift(git, governs_root, sha, &doc.governs).ok()
}

/// Whether anything under `doc`'s globs has moved since its anchor: the one
/// staleness fact a `why` record carries (RFC-069).
///
/// Not `compute(..).drift.files > 0`. A band needs the anchor commit's time and
/// `drifted` does not, so `why` over N governing documents costs N git
/// subprocesses rather than 2N (STORY-276).
pub fn drifted(
    governs_root: &Path,
    doc: &DocMeta,
    git: &dyn GitRefOps,
    cache: &StalenessCache,
) -> bool {
    drift_of(governs_root, doc, git, cache).is_some_and(|drift| drift.files > 0)
}

/// Whether no anchor could band `doc` worse than `fresh`, so a caller that only
/// wants the rot -- `StaleRule` -- can skip it without asking git anything
/// (STORY-276).
///
/// `reviewed` is stamped from `HEAD` at a status transition (STORY-274), so the
/// anchor commit is never older than the document's own `date`: a document
/// whose `date` falls inside the `aging` window is fresh whatever its anchor
/// turns out to be. Only sound under the `age` driver, which bands on time --
/// drift bands on what moved, and a document created yesterday can govern code
/// that moved this morning.
pub fn cannot_be_stale(config: &Config, doc: &DocMeta) -> bool {
    configured_driver(config, doc) == StalenessDriver::Age
        && days_since(doc.date) < config.staleness.aging.0
}

fn configured_driver(config: &Config, doc: &DocMeta) -> StalenessDriver {
    config
        .type_by_name(doc.doc_type.as_str())
        .map(|t| t.staleness)
        .unwrap_or_default()
}

fn band_by_age(age_days: u64, thresholds: StalenessConfig) -> Band {
    if age_days >= thresholds.stale.0 {
        Band::Stale
    } else if age_days >= thresholds.aging.0 {
        Band::Aging
    } else {
        Band::Fresh
    }
}

/// Whole days from `date` to today, floored at zero so a document dated in the
/// future reads as new rather than as a negative age.
fn days_since(date: NaiveDate) -> u64 {
    (Utc::now().date_naive() - date).num_days().max(0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::git_ref::test_support::MockGitRefClient;
    use crate::engine::store::test_support::store_from_with_config;
    use crate::engine::store::Store;
    use chrono::Duration;
    use tempfile::TempDir;

    const REVIEWED: &str = "0123456789abcdef0123456789abcdef01234567";

    fn days_ago(n: i64) -> NaiveDate {
        Utc::now().date_naive() - Duration::days(n)
    }

    /// Every configured type driven by `driver`, so a fixture picks its driver
    /// without also picking a document type.
    fn config_driven_by(driver: StalenessDriver) -> Config {
        let mut config = Config::default();
        for type_def in &mut config.documents.types {
            type_def.staleness = driver;
        }
        config
    }

    /// One rfc on disk, loaded through `Store::load` so `governs` and `reviewed`
    /// arrive parsed exactly as they do in production.
    fn store_with(governs: &[&str], reviewed: Option<&str>, date: NaiveDate) -> (TempDir, Store) {
        let governs_block = if governs.is_empty() {
            "governs: []\n".to_string()
        } else {
            let entries: String = governs.iter().map(|g| format!("  - \"{g}\"\n")).collect();
            format!("governs:\n{entries}")
        };
        let reviewed_line = reviewed.map_or(String::new(), |sha| format!("reviewed: {sha}\n"));
        let doc = format!(
            "---\ntitle: \"Engine\"\ntype: rfc\nstatus: draft\nauthor: t\ndate: {date}\ntags: []\n{governs_block}{reviewed_line}related: []\n---\n\nbody\n"
        );
        store_from_with_config(&[("docs/rfcs/RFC-001-engine.md", &doc)], &Config::default())
    }

    /// Through an `off()` memo, so the git call log every assertion below reads
    /// is the whole story rather than the first invocation's share of it.
    fn compute_in(store: &Store, config: &Config, git: &MockGitRefClient) -> Staleness {
        let doc = store.docs.values().next().expect("one document loaded");
        compute(
            store.governs_root(),
            config,
            doc,
            git,
            &StalenessCache::off(),
        )
    }

    fn mock_with_drift(drift: Drift) -> MockGitRefClient {
        MockGitRefClient::new()
            .with_diff_stat(drift)
            .with_read_commit_timestamp_result(Ok(Utc::now()))
    }

    // AC1: a drift type with an anchor and pins, and commits under them.
    #[test]
    fn drift_under_a_pin_is_stale_and_carries_the_counts() {
        let (_tmp, store) = store_with(&["src/engine/**"], Some(REVIEWED), days_ago(1));
        let drift = Drift {
            files: 12,
            insertions: 310,
            deletions: 85,
        };

        let staleness = compute_in(
            &store,
            &config_driven_by(StalenessDriver::Drift),
            &mock_with_drift(drift),
        );

        assert_eq!(staleness.band, Band::Stale);
        assert_eq!(staleness.driver, StalenessDriver::Drift);
        assert_eq!(staleness.drift, drift);
    }

    // AC2: same document, nothing changed under the pin since the anchor.
    #[test]
    fn no_drift_under_a_pin_is_fresh() {
        let (_tmp, store) = store_with(&["src/engine/**"], Some(REVIEWED), days_ago(1));

        let staleness = compute_in(
            &store,
            &config_driven_by(StalenessDriver::Drift),
            &mock_with_drift(Drift::default()),
        );

        assert_eq!(staleness.band, Band::Fresh);
        assert_eq!(staleness.driver, StalenessDriver::Drift);
    }

    // AC3: the default driver bands on the configured thresholds.
    #[test]
    fn age_bands_step_at_the_configured_thresholds() {
        let config = config_driven_by(StalenessDriver::Age);
        let bands: Vec<Band> = [10, 100, 200]
            .into_iter()
            .map(|age| {
                let (_tmp, store) = store_with(&[], None, days_ago(age));
                compute_in(&store, &config, &MockGitRefClient::new()).band
            })
            .collect();

        assert_eq!(bands, vec![Band::Fresh, Band::Aging, Band::Stale]);
    }

    // AC4: a drift type with nothing to diff bands by age and says so, without
    // asking git anything it cannot answer.
    #[test]
    fn a_drift_type_with_nothing_to_diff_falls_back_to_age() {
        let config = config_driven_by(StalenessDriver::Drift);

        for (governs, reviewed) in [(&[][..], Some(REVIEWED)), (&["src/engine/**"][..], None)] {
            let (_tmp, store) = store_with(governs, reviewed, days_ago(200));
            let git = MockGitRefClient::new()
                .with_read_commit_timestamp_result(Ok(Utc::now() - Duration::days(200)));

            let staleness = compute_in(&store, &config, &git);

            assert_eq!(staleness.driver, StalenessDriver::Age);
            assert_eq!(staleness.band, Band::Stale);
            assert_eq!(staleness.drift, Drift::default());
            let calls = git.call_log();
            assert!(
                !calls
                    .borrow()
                    .iter()
                    .any(|call| call.starts_with("diff_stat:")),
                "nothing to diff, so git is never asked: {:?}",
                calls.borrow()
            );
        }
    }

    /// STORY-274 AC3: transitioning a stale `drift` document re-anchors it, so
    /// the next `compute` diffs `HEAD..HEAD` -- the range real git answers
    /// empty, and so the one that stops reporting `stale`. The mock's `Drift` is
    /// fixed, so the range it was asked for is the assertion; the counts would
    /// look the same for a document that was never re-anchored at all.
    #[test]
    fn a_status_transition_re_anchors_a_stale_drift_document() {
        let config = config_driven_by(StalenessDriver::Drift);
        let drift = Drift {
            files: 12,
            insertions: 310,
            deletions: 85,
        };
        let (tmp, store) = store_with(&["src/engine/**"], Some("staleanchor"), days_ago(1));
        let git = mock_with_drift(drift);
        assert_eq!(compute_in(&store, &config, &git).band, Band::Stale);

        crate::engine::ops::update::run_with_config(
            tmp.path(),
            &store,
            "RFC-001",
            &[("status", "review")],
            Some(&config),
            &git,
        )
        .unwrap();

        let store = Store::load(tmp.path(), &config).unwrap();
        let calls = git.call_log();
        calls.borrow_mut().clear();
        compute_in(&store, &config, &git);

        assert_eq!(
            calls
                .borrow()
                .iter()
                .filter(|call| call.starts_with("diff_stat:"))
                .cloned()
                .collect::<Vec<_>>(),
            [format!(
                "diff_stat:{}:{}..HEAD:src/engine/**",
                store.governs_root().display(),
                crate::engine::git_ref::test_support::FAKE_HEAD
            )],
            "the anchor is now HEAD, so nothing is between it and HEAD"
        );
    }

    // AC5: `reviewed` is the anchor and dates the document; its absence falls
    // back to the frontmatter date without reading any commit.
    #[test]
    fn a_reviewed_sha_anchors_the_age_at_its_commit_time() {
        let (_tmp, store) = store_with(&["src/engine/**"], Some(REVIEWED), days_ago(0));
        let git = MockGitRefClient::new()
            .with_diff_stat(Drift::default())
            .with_read_commit_timestamp_result(Ok(Utc::now() - Duration::days(140)));

        let staleness = compute_in(&store, &config_driven_by(StalenessDriver::Age), &git);

        assert_eq!(staleness.anchor, Anchor::Sha(REVIEWED.to_string()));
        assert_eq!(
            staleness.age_days, 140,
            "measured from the commit, not the frontmatter date"
        );
    }

    #[test]
    fn without_a_reviewed_sha_the_document_date_anchors_it() {
        let (_tmp, store) = store_with(&["src/engine/**"], None, days_ago(140));
        let git = MockGitRefClient::new();

        let staleness = compute_in(&store, &config_driven_by(StalenessDriver::Age), &git);

        assert_eq!(staleness.anchor, Anchor::Date(days_ago(140)));
        assert_eq!(staleness.age_days, 140);
        let calls = git.call_log();
        assert!(
            calls.borrow().is_empty(),
            "no anchor commit to read: {:?}",
            calls.borrow()
        );
    }

    /// What git is actually asked, which the count assertions above cannot see:
    /// the document's own globs, forward from its anchor to `HEAD`, in the root
    /// those globs resolve against. A reversed range, an empty pathspec list or
    /// the docs root would all still produce the counts the mock replays.
    #[test]
    fn the_diff_runs_forward_from_the_anchor_over_the_documents_globs() {
        let (_tmp, mut store) = store_with(
            &["src/engine/**", "src/cli.rs"],
            Some(REVIEWED),
            days_ago(1),
        );
        // A docs-repo split, so the governed root is a path the docs root is not.
        store.governs_root = store.root().join("code");
        let git = mock_with_drift(Drift::default());

        compute_in(&store, &config_driven_by(StalenessDriver::Drift), &git);

        assert_eq!(
            git.call_log().borrow()[0],
            format!(
                "diff_stat:{}:{REVIEWED}..HEAD:src/engine/**,src/cli.rs",
                store.governs_root().display()
            )
        );
    }

    /// The shape RFC-069 publishes. The next slice prints this object; it does
    /// not reshape it.
    #[test]
    fn serializes_to_the_published_shape() {
        let staleness = Staleness {
            band: Band::Stale,
            driver: StalenessDriver::Drift,
            anchor: Anchor::Sha("0123456".to_string()),
            age_days: 140,
            drift: Drift {
                files: 12,
                insertions: 310,
                deletions: 85,
            },
        };

        assert_eq!(
            serde_json::to_value(&staleness).unwrap(),
            serde_json::json!({
                "band": "stale",
                "driver": "drift",
                "anchor": "0123456",
                "age_days": 140,
                "drift": {"files": 12, "insertions": 310, "deletions": 85}
            })
        );
    }

    #[test]
    fn a_date_anchor_serializes_as_an_iso_date() {
        let anchor = Anchor::Date(NaiveDate::from_ymd_opt(2026, 9, 8).unwrap());
        assert_eq!(
            serde_json::to_value(&anchor).unwrap(),
            serde_json::json!("2026-09-08")
        );
    }

    /// STORY-276 AC5: `why` wants the drift bit and nothing else, and pays for
    /// nothing else. The band's other input, the anchor commit's time, is not
    /// read -- and the mock would bail on being asked for one it has no result
    /// for, so the call log is the assertion.
    #[test]
    fn drifted_reads_the_diff_and_not_the_anchor_commit() {
        let (_tmp, store) = store_with(&["src/engine/**"], Some(REVIEWED), days_ago(1));
        let doc = store.docs.values().next().unwrap();
        let git = MockGitRefClient::new().with_diff_stat(Drift {
            files: 2,
            insertions: 4,
            deletions: 1,
        });

        let drifted = drifted(store.governs_root(), doc, &git, &StalenessCache::off());

        assert!(drifted);
        assert_eq!(
            git.call_log().borrow().len(),
            1,
            "one subprocess per record, not two: {:?}",
            git.call_log().borrow()
        );
    }

    /// A document with nothing to diff has not drifted, which is what an
    /// unpinned document honestly is.
    #[test]
    fn nothing_to_diff_has_not_drifted() {
        for (governs, reviewed) in [(&[][..], Some(REVIEWED)), (&["src/engine/**"][..], None)] {
            let (_tmp, store) = store_with(governs, reviewed, days_ago(1));
            let doc = store.docs.values().next().unwrap();
            let git = MockGitRefClient::new();

            assert!(!drifted(
                store.governs_root(),
                doc,
                &git,
                &StalenessCache::off()
            ));
            assert!(git.call_log().borrow().is_empty());
        }
    }

    /// STORY-276 AC4: under the `age` driver a document dated inside the `aging`
    /// window cannot be stale, whatever its anchor says, because the anchor is
    /// never older than the date. Bounded by the same threshold `compute` bands
    /// on, so the two can never disagree about which documents are skipped.
    #[test]
    fn a_young_age_driven_document_cannot_be_stale() {
        let config = config_driven_by(StalenessDriver::Age);
        let inside = config.staleness.aging.0 as i64 - 1;

        for (age, expected) in [(0, true), (inside, true), (inside + 1, false)] {
            let (_tmp, store) = store_with(&["src/engine/**"], Some(REVIEWED), days_ago(age));
            let doc = store.docs.values().next().unwrap();

            assert_eq!(
                cannot_be_stale(&config, doc),
                expected,
                "a document dated {age} days ago"
            );
        }
    }

    /// The guard is about time, and a `drift` document's band is not: code under
    /// a document written this morning can have moved this afternoon.
    #[test]
    fn a_young_drift_driven_document_still_has_to_be_diffed() {
        let (_tmp, store) = store_with(&["src/engine/**"], Some(REVIEWED), days_ago(0));
        let doc = store.docs.values().next().unwrap();

        assert!(!cannot_be_stale(
            &config_driven_by(StalenessDriver::Drift),
            doc
        ));
    }

    /// Git failing is not evidence of freshness, so a drift type whose diff
    /// errors bands by age like any other document with nothing to diff.
    #[test]
    fn a_failing_diff_falls_back_to_age() {
        let (_tmp, store) = store_with(&["src/engine/**"], Some(REVIEWED), days_ago(200));
        let git = MockGitRefClient::new()
            .with_diff_stat_error("bad object")
            .with_read_commit_timestamp_result(Ok(Utc::now() - Duration::days(200)));

        let staleness = compute_in(&store, &config_driven_by(StalenessDriver::Drift), &git);

        assert_eq!(staleness.driver, StalenessDriver::Age);
        assert_eq!(staleness.band, Band::Stale);
    }
}
