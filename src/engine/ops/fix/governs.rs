use std::path::Path;

use anyhow::{anyhow, Result};

use crate::engine::config::Config;
use crate::engine::document::rewrite_frontmatter;
use crate::engine::fs::FileSystem;
use crate::engine::git_ref::GitRefOps;
use crate::engine::store::Store;
use crate::engine::validation::{Checker, GovernsNoMatchRule, ValidationIssue};

use super::{record_write, GovernsFixResult};

/// Rewrite every rotted `governs` glob to the `suggested_glob` its
/// `governs-no-match` finding carried (RFC-068 §Decisions 5).
///
/// The suggestion is read off the finding rather than recomputed: a finding with
/// none -- a document with no `reviewed` anchor, a deletion rather than a move, a
/// `reviewed` commit git cannot read -- is skipped, not an error. Nothing here
/// touches `reviewed`, so RFC-069 still sees the drift after the repair.
pub fn collect_governs_fixes(
    root: &Path,
    store: &Store,
    config: &Config,
    git: Box<dyn GitRefOps>,
    dry_run: bool,
    fs: &dyn FileSystem,
) -> Vec<GovernsFixResult> {
    GovernsNoMatchRule::new(git)
        .check(store, config)
        .into_iter()
        .filter_map(|(_severity, issue)| match issue {
            ValidationIssue::GovernsNoMatch {
                path,
                glob,
                suggested_glob: Some(suggestion),
                ..
            } => Some((path, glob, suggestion)),
            _ => None,
        })
        .map(|(path, old_glob, new_glob)| {
            let (written, error) = record_write(dry_run, || {
                replace_glob(&root.join(&path), fs, &old_glob, &new_glob)
            });
            GovernsFixResult {
                path: path.display().to_string(),
                old_glob,
                new_glob,
                written,
                error,
            }
        })
        .collect()
}

/// Swap one entry of the document's `governs` sequence, through the frontmatter
/// writer `pin` and `update` use rather than a textual edit, so the document's
/// other fields and its other pins survive untouched.
fn replace_glob(full_path: &Path, fs: &dyn FileSystem, old: &str, new: &str) -> Result<()> {
    rewrite_frontmatter(full_path, fs, |value| {
        let entries = value
            .get_mut("governs")
            .and_then(|v| v.as_sequence_mut())
            .ok_or_else(|| anyhow!("no governs sequence in {}", full_path.display()))?;
        for entry in entries.iter_mut() {
            if entry.as_str() == Some(old) {
                *entry = serde_yaml::Value::String(new.to_string());
            }
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::fs::RealFileSystem;
    use crate::engine::git_ref::test_support::MockGitRefClient;
    use crate::engine::store::test_support::store_from_with_config;

    const REVIEWED: &str = "0123456789abcdef0123456789abcdef01234567";
    const DOC: &str = "docs/rfcs/RFC-001-engine.md";
    const MOVED: &[(&str, &str)] = &[("src/old/resolve.rs", "src/context/resolve.rs")];

    /// One rfc pinning `globs` over a tree where `src/engine` and `src/context`
    /// exist but `src/old` is gone: the pin git says moved to `src/context`.
    fn fixture(globs: &[&str], reviewed: Option<&str>) -> (tempfile::TempDir, Store) {
        let entries: String = globs.iter().map(|g| format!("- \"{g}\"\n")).collect();
        let reviewed_line = reviewed.map_or(String::new(), |sha| format!("reviewed: {sha}\n"));
        let doc = format!(
            "---\ntitle: \"Engine\"\ntype: rfc\nstatus: draft\nauthor: t\ndate: 2026-09-01\ntags: []\ngoverns:\n{entries}{reviewed_line}related: []\n---\n\nbody\n"
        );
        store_from_with_config(
            &[
                (DOC, doc.as_str()),
                ("src/engine/store.rs", "fn main() {}\n"),
                ("src/context/resolve.rs", "fn resolve() {}\n"),
            ],
            &Config::default(),
        )
    }

    fn fix(
        tmp: &tempfile::TempDir,
        store: &Store,
        renames: &[(&str, &str)],
        dry_run: bool,
    ) -> Vec<GovernsFixResult> {
        collect_governs_fixes(
            tmp.path(),
            store,
            &Config::default(),
            Box::new(MockGitRefClient::new().with_renames(renames)),
            dry_run,
            &RealFileSystem,
        )
    }

    fn frontmatter(tmp: &tempfile::TempDir) -> String {
        std::fs::read_to_string(tmp.path().join(DOC)).unwrap()
    }

    fn rotted_globs(tmp: &tempfile::TempDir) -> Vec<String> {
        let store = Store::load(tmp.path(), &Config::default()).unwrap();
        GovernsNoMatchRule::new(Box::new(MockGitRefClient::new().with_renames(MOVED)))
            .check(&store, &Config::default())
            .into_iter()
            .map(|(_, issue)| match issue {
                ValidationIssue::GovernsNoMatch { glob, .. } => glob,
                other => panic!("unexpected finding {other:?}"),
            })
            .collect()
    }

    // AC5: the rotted entry becomes the suggestion; the document's other pin and
    // its `reviewed` anchor are left exactly as they were.
    #[test]
    fn the_rotted_glob_is_rewritten_and_its_siblings_and_reviewed_are_untouched() {
        let (tmp, store) = fixture(&["src/old/**", "src/engine/**"], Some(REVIEWED));

        fix(&tmp, &store, MOVED, false);

        let after = frontmatter(&tmp);
        assert!(
            after.contains("- src/context/**"),
            "the rotted glob should be rewritten to the suggestion, got:\n{after}"
        );
        assert!(
            !after.contains("src/old/**"),
            "the rotted glob should be gone, got:\n{after}"
        );
        assert!(
            after.contains("- src/engine/**"),
            "the document's other pin should survive, got:\n{after}"
        );
        assert!(
            after.contains(&format!("reviewed: {REVIEWED}")),
            "`reviewed` stays put so RFC-069 still sees the drift, got:\n{after}"
        );
    }

    // AC6: the result names the document and both globs. The envelope
    // `fix --governs --json` wraps this in is asserted in
    // `tests/integration/cli_fix_governs_test.rs`.
    #[test]
    fn each_rewrite_reports_the_document_and_both_globs() {
        let (tmp, store) = fixture(&["src/old/**"], Some(REVIEWED));

        let results = fix(&tmp, &store, MOVED, false);

        let json = serde_json::to_value(&results).unwrap();
        assert_eq!(
            json,
            serde_json::json!([{
                "path": DOC,
                "old_glob": "src/old/**",
                "new_glob": "src/context/**",
                "written": true,
                "error": null,
            }])
        );
    }

    // AC7: the finding is gone after the repair, because the rewritten glob
    // matches the files git said moved.
    #[test]
    fn no_no_match_finding_remains_for_the_repaired_glob() {
        let (tmp, store) = fixture(&["src/old/**", "src/engine/**"], Some(REVIEWED));
        assert_eq!(
            rotted_globs(&tmp),
            vec!["src/old/**".to_string()],
            "the pin is rotted before the fix"
        );

        fix(&tmp, &store, MOVED, false);

        assert_eq!(
            rotted_globs(&tmp),
            Vec::<String>::new(),
            "the rewritten glob matches, so nothing is rotted"
        );
    }

    /// A document with no `reviewed` has no anchor to diff from, so its finding
    /// carries no suggestion. Skipped, not reported and not an error.
    #[test]
    fn a_finding_with_no_suggestion_is_skipped() {
        let (tmp, store) = fixture(&["src/old/**"], None);
        let before = frontmatter(&tmp);

        assert!(fix(&tmp, &store, MOVED, false).is_empty());
        assert_eq!(
            before,
            frontmatter(&tmp),
            "nothing to apply, nothing written"
        );
    }

    #[test]
    fn dry_run_reports_the_rewrite_without_writing() {
        let (tmp, store) = fixture(&["src/old/**"], Some(REVIEWED));
        let before = frontmatter(&tmp);

        let results = fix(&tmp, &store, MOVED, true);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].new_glob, "src/context/**");
        assert!(!results[0].written, "dry run must not write");
        assert_eq!(before, frontmatter(&tmp), "the document is untouched");
    }
}
