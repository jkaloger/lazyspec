use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;

use crate::engine::document::rewrite_frontmatter;
use crate::engine::fs::FileSystem;
use crate::engine::store::Store;

use super::RelationFixResult;

fn is_path_target(target: &str) -> bool {
    target.contains('/') || target.ends_with(".md")
}

pub(super) fn collect_relation_fixes(
    root: &Path,
    store: &Store,
    dry_run: bool,
    fs: &dyn FileSystem,
) -> Vec<RelationFixResult> {
    // Build path -> ID lookup from store docs
    let path_to_id: HashMap<String, String> = store
        .all_docs()
        .iter()
        .map(|doc| (doc.path.to_string_lossy().to_string(), doc.id.clone()))
        .collect();

    let mut results = Vec::new();

    for doc in store.all_docs() {
        if doc.virtual_doc {
            continue;
        }

        // Check if any related targets look like paths
        let path_targets: Vec<(String, String)> = doc
            .related
            .iter()
            .filter(|rel| is_path_target(&rel.target))
            .filter_map(|rel| {
                path_to_id
                    .get(&rel.target)
                    .map(|id| (rel.target.clone(), id.clone()))
            })
            .collect();

        // Detect duplicate (rel_type, target) pairs after normalizing path
        // targets to ids, so two entries differing only by path-vs-id collapse.
        // Retain the first occurrence; report each later duplicate (in id form).
        let mut seen: HashSet<(String, String)> = HashSet::new();
        let mut deduped: Vec<(String, String)> = Vec::new();
        for rel in &doc.related {
            let normalized = path_to_id
                .get(&rel.target)
                .cloned()
                .unwrap_or_else(|| rel.target.clone());
            let key = (rel.rel_type.as_str().to_string(), normalized);
            if !seen.insert(key.clone()) {
                deduped.push(key);
            }
        }

        if path_targets.is_empty() && deduped.is_empty() {
            continue;
        }

        let full_path = root.join(&doc.path);
        let written = if !dry_run {
            let targets = path_targets.clone();
            let res = rewrite_frontmatter(&full_path, fs, |value| {
                if let Some(related_seq) =
                    value.get_mut("related").and_then(|v| v.as_sequence_mut())
                {
                    // 1. Normalize path targets to ids in place.
                    for entry in related_seq.iter_mut() {
                        if let Some(mapping) = entry.as_mapping_mut() {
                            for (_key, val) in mapping.iter_mut() {
                                let replacement = val.as_str().and_then(|s| {
                                    targets.iter().find_map(|(old_path, new_id)| {
                                        if s == old_path {
                                            Some(new_id.clone())
                                        } else {
                                            None
                                        }
                                    })
                                });
                                if let Some(new_val) = replacement {
                                    *val = serde_yaml::Value::String(new_val);
                                }
                            }
                        }
                    }

                    // 2. Drop later duplicate (key, value) mappings, keeping the
                    //    first occurrence of each. Runs after normalization so
                    //    path-vs-id forms have already collapsed to the same id.
                    //    serde_yaml::Value isn't Hash, so dedup via PartialEq on
                    //    a kept-so-far list (related sequences are short).
                    let mut kept: Vec<serde_yaml::Value> = Vec::new();
                    related_seq.retain(|entry| {
                        if kept.contains(entry) {
                            false
                        } else {
                            kept.push(entry.clone());
                            true
                        }
                    });
                }
                Ok(())
            });
            res.is_ok()
        } else {
            false
        };

        let replacements: Vec<(String, String)> = path_targets;

        results.push(RelationFixResult {
            path: doc.path.display().to_string(),
            replacements,
            deduped,
            written,
        });
    }

    results
}

#[cfg(test)]
mod tests {
    use crate::engine::config::{Config, GithubConfig, NumberingStrategy, StoreBackend, TypeDef};
    use crate::engine::fs::RealFileSystem;
    use crate::engine::store::Store;

    fn tmp_root(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lazyspec-fix-relations-test-{}-{}",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn gh_config_with_rfc_type() -> Config {
        let issue_type = |name: &str, plural: &str, dir: &str, prefix: &str| TypeDef {
            name: name.to_string(),
            plural: plural.to_string(),
            dir: dir.to_string(),
            prefix: prefix.to_string(),
            icon: None,
            numbering: NumberingStrategy::Incremental,
            subdirectory: false,
            store: StoreBackend::GithubIssues,
            singleton: false,
            parent_type: None,
            agents: Vec::new(),
            intent: None,
            authorship: Default::default(),
            lifecycle: Default::default(),
            attributes: Default::default(),
        };
        let mut config = Config::default();
        config.documents.types = vec![
            issue_type("rfc", "rfcs", "docs/rfcs", "RFC"),
            issue_type("story", "stories", "docs/stories", "STORY"),
        ];
        config.documents.github = Some(GithubConfig {
            repo: Some("owner/repo".to_string()),
            cache_ttl: 60,
        });
        config
    }

    /// Seed an RFC cache doc whose `related:` block is given verbatim, plus a
    /// STORY-001 target doc so the store resolves both.
    fn seed_cache(root: &std::path::Path, related_block: &str) -> std::path::PathBuf {
        let rfc_cache = root.join(".lazyspec/cache/rfc");
        let story_cache = root.join(".lazyspec/cache/story");
        std::fs::create_dir_all(&rfc_cache).unwrap();
        std::fs::create_dir_all(&story_cache).unwrap();
        let rfc_path = rfc_cache.join("RFC-001-my-rfc.md");
        let rfc_content = format!(
            "---\ntitle: My RFC\ntype: rfc\nstatus: draft\nauthor: agent-7\ndate: 2026-03-27\ntags: []\n{related_block}---\nRFC body text.\n"
        );
        std::fs::write(&rfc_path, rfc_content).unwrap();
        std::fs::write(
            story_cache.join("STORY-001-my-story.md"),
            "---\ntitle: My Story\ntype: story\nstatus: draft\nauthor: agent-7\ndate: 2026-03-27\ntags: []\n---\nStory body.\n",
        )
        .unwrap();
        rfc_path
    }

    fn count_implements_story_001(content: &str) -> usize {
        content
            .lines()
            .filter(|l| l.trim_start_matches("- ").trim() == "implements: STORY-001")
            .count()
    }

    // AC8: a cache doc whose `related:` carries `implements: STORY-001` twice is
    // deduped to a single survivor when dry_run = false; the result reports the
    // deduped pair and written = true.
    #[test]
    fn dedup_drops_duplicate_related_pair() {
        let root = tmp_root("dedup_apply");
        let config = gh_config_with_rfc_type();
        let rfc_path = seed_cache(
            &root,
            "related:\n- implements: STORY-001\n- implements: STORY-001\n",
        );

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        let results = super::collect_relation_fixes(&root, &store, false, &fs);

        let result = results
            .iter()
            .find(|r| r.path.contains("RFC-001"))
            .expect("expected a RelationFixResult for the seeded RFC");
        assert_eq!(
            result.deduped,
            vec![("implements".to_string(), "STORY-001".to_string())],
            "should report exactly one deduped pair"
        );
        assert!(result.written, "non-dry-run dedup must write the file");

        let updated = std::fs::read_to_string(&rfc_path).unwrap();
        assert_eq!(
            count_implements_story_001(&updated),
            1,
            "exactly one survivor after dedup, got:\n{updated}"
        );
    }

    // AC8 (dry_run): the duplicate is reported but the file is untouched and
    // written = false.
    #[test]
    fn dedup_dry_run_reports_without_writing() {
        let root = tmp_root("dedup_dry_run");
        let config = gh_config_with_rfc_type();
        let rfc_path = seed_cache(
            &root,
            "related:\n- implements: STORY-001\n- implements: STORY-001\n",
        );
        let before = std::fs::read_to_string(&rfc_path).unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        let results = super::collect_relation_fixes(&root, &store, true, &fs);

        let result = results
            .iter()
            .find(|r| r.path.contains("RFC-001"))
            .expect("expected a RelationFixResult for the seeded RFC");
        assert_eq!(
            result.deduped,
            vec![("implements".to_string(), "STORY-001".to_string())],
            "dry-run should still report the deduped pair"
        );
        assert!(!result.written, "dry-run must not write");

        let after = std::fs::read_to_string(&rfc_path).unwrap();
        assert_eq!(before, after, "dry-run must leave the file untouched");
        assert_eq!(
            count_implements_story_001(&after),
            2,
            "both duplicates remain under dry-run"
        );
    }

    // The same target written once as a repo-relative path and once as the id
    // form collapses to a single survivor: path_to_id normalization runs before
    // dedup, so both entries reduce to `implements: STORY-001`. The result must
    // report the path->id replacement and the deduped pair, and write the file.
    #[test]
    fn dedup_collapses_path_and_id_forms_of_same_target() {
        let root = tmp_root("dedup_path_and_id");
        let config = gh_config_with_rfc_type();
        // The STORY-001 cache doc lives at this repo-relative path, which is the
        // key path_to_id uses; that is the path form that must normalize to the id.
        let story_path = ".lazyspec/cache/story/STORY-001-my-story.md";
        let rfc_path = seed_cache(
            &root,
            &format!("related:\n- implements: {story_path}\n- implements: STORY-001\n"),
        );

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        let results = super::collect_relation_fixes(&root, &store, false, &fs);

        let result = results
            .iter()
            .find(|r| r.path.contains("RFC-001"))
            .expect("expected a RelationFixResult for the seeded RFC");
        assert_eq!(
            result.replacements,
            vec![(story_path.to_string(), "STORY-001".to_string())],
            "the path form should be reported as a path->id replacement"
        );
        assert_eq!(
            result.deduped,
            vec![("implements".to_string(), "STORY-001".to_string())],
            "after normalizing the path form to the id, the pair is a duplicate"
        );
        assert!(result.written, "non-dry-run fix must write the file");

        let updated = std::fs::read_to_string(&rfc_path).unwrap();
        assert_eq!(
            count_implements_story_001(&updated),
            1,
            "path form normalized to id then deduped against id form, got:\n{updated}"
        );
    }

    // A doc with no duplicates and no path targets produces no fix result.
    #[test]
    fn no_duplicates_yields_no_result() {
        let root = tmp_root("dedup_none");
        let config = gh_config_with_rfc_type();
        seed_cache(&root, "related:\n- implements: STORY-001\n");

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        let results = super::collect_relation_fixes(&root, &store, false, &fs);
        assert!(
            !results.iter().any(|r| r.path.contains("RFC-001")),
            "a doc with no duplicates and no path targets should not produce a fix"
        );
    }
}
