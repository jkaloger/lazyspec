use std::path::Path;

use crate::engine::config::{Config, StoreBackend};
use crate::engine::document::{compose_frontmatter, split_frontmatter, DocMeta};
use crate::engine::fs::FileSystem;
use crate::engine::store::Store;

use super::StatusFixResult;

pub(super) fn collect_status_fixes(
    root: &Path,
    store: &Store,
    config: &Config,
    paths: &[String],
    dry_run: bool,
    fs: &dyn FileSystem,
) -> Vec<StatusFixResult> {
    let targets: Vec<&DocMeta> = if paths.is_empty() {
        store.docs.values().collect()
    } else {
        paths
            .iter()
            .filter_map(|p| {
                store
                    .docs
                    .values()
                    .find(|d| d.path.to_string_lossy() == *p)
                    .or_else(|| store.resolve_shorthand(p).ok())
            })
            .collect()
    };

    let mut results = Vec::new();
    for doc in targets {
        let Some(type_def) = config.type_by_name(doc.doc_type.as_str()) else {
            continue;
        };
        // Only filesystem docs are on-disk markdown we may rewrite; github,
        // git-ref, and clickup docs live in caches or remotes and are owned by
        // their backends.
        if type_def.store != StoreBackend::Filesystem {
            continue;
        }
        if type_def.lifecycle.states.is_empty() {
            continue;
        }
        if type_def.accepts_status(&doc.status) {
            continue;
        }

        let new_status = type_def.lifecycle.states[0].clone();
        let old_status = doc.status.as_str().to_string();

        let written = if dry_run {
            false
        } else {
            repair_status(&root.join(&doc.path), &new_status, fs).is_ok()
        };

        results.push(StatusFixResult {
            path: doc.path.display().to_string(),
            old_status,
            new_status,
            written,
        });
    }

    results.sort_by(|a, b| a.path.cmp(&b.path));
    results
}

fn repair_status(full_path: &Path, new_status: &str, fs: &dyn FileSystem) -> anyhow::Result<()> {
    let content = fs.read_to_string(full_path)?;
    let (yaml, body) = split_frontmatter(&content)?;
    let mut lines: Vec<String> = yaml.lines().map(|l| l.to_string()).collect();
    if let Some(line) = lines
        .iter_mut()
        .find(|l| l.trim_start().starts_with("status:"))
    {
        *line = format!("status: {}", new_status);
    }
    let output = compose_frontmatter(&lines.join("\n"), &body);
    fs.write(full_path, &output)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::engine::config::{Config, Lifecycle, NumberingStrategy, StoreBackend, TypeDef};
    use crate::engine::fs::RealFileSystem;
    use crate::engine::store::Store;

    fn tmp_root(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lazyspec-fix-status-test-{}-{}",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A `bug` type whose lifecycle starts at `reported`, not `draft`. Added
    /// alongside the default starter types so tests can also exercise a
    /// default-lifecycle (`draft`-first) type in the same config.
    fn config_with_bug_type() -> Config {
        let bug = TypeDef {
            name: "bug".to_string(),
            plural: "bugs".to_string(),
            dir: "docs/bugs".to_string(),
            prefix: "BUG".to_string(),
            icon: None,
            numbering: NumberingStrategy::Incremental,
            subdirectory: false,
            store: StoreBackend::Filesystem,
            singleton: false,
            parent_type: None,
            agents: Vec::new(),
            intent: None,
            authorship: Default::default(),
            lifecycle: Lifecycle {
                states: ["reported", "triaged", "in-progress", "fixed", "wontfix"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                edges: Vec::new(),
            },
            attributes: Default::default(),
            label_override: None,
            github_issue_tag: None,
            github_issue_type: None,
            clickup_list_id: None,
            clickup_task_type: None,
            clickup_custom_field_map: None,
        };
        let mut config = Config::default();
        config.documents.types.push(bug);
        config
    }

    fn seed_bug(root: &std::path::Path, status: &str) -> std::path::PathBuf {
        let dir = root.join("docs/bugs");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("BUG-1-broken.md");
        std::fs::write(
            &path,
            format!(
                "---\ntitle: Broken\ntype: bug\nstatus: {status}\nauthor: t\ndate: 2026-07-17\ntags: []\n---\nBug body.\n"
            ),
        )
        .unwrap();
        path
    }

    #[test]
    fn dry_run_reports_repair_without_writing() {
        let root = tmp_root("dry_run");
        let config = config_with_bug_type();
        let bug_path = seed_bug(&root, "draft");
        let before = std::fs::read_to_string(&bug_path).unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        let results = super::collect_status_fixes(&root, &store, &config, &[], true, &fs);

        assert_eq!(results.len(), 1, "one out-of-lifecycle doc");
        assert_eq!(results[0].old_status, "draft");
        assert_eq!(results[0].new_status, "reported");
        assert!(!results[0].written, "dry-run must not write");

        let after = std::fs::read_to_string(&bug_path).unwrap();
        assert_eq!(before, after, "dry-run must leave the file untouched");
    }

    #[test]
    fn apply_rewrites_status_to_first_lifecycle_state() {
        let root = tmp_root("apply");
        let config = config_with_bug_type();
        let bug_path = seed_bug(&root, "draft");

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        let results = super::collect_status_fixes(&root, &store, &config, &[], false, &fs);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].old_status, "draft");
        assert_eq!(results[0].new_status, "reported");
        assert!(results[0].written, "apply must write the file");

        let after = std::fs::read_to_string(&bug_path).unwrap();
        assert!(
            after.contains("status: reported"),
            "status line rewritten, got:\n{after}"
        );
        assert!(
            !after.contains("status: draft"),
            "old status must be gone, got:\n{after}"
        );
    }

    #[test]
    fn valid_status_yields_no_fix() {
        let root = tmp_root("valid");
        let config = config_with_bug_type();
        // A bug already at a valid lifecycle state.
        seed_bug(&root, "reported");
        // A default-lifecycle rfc at draft: draft IS its first state, so it must
        // never be "repaired" (the key regression guard for standard types).
        let rfc_dir = root.join("docs/rfcs");
        std::fs::create_dir_all(&rfc_dir).unwrap();
        std::fs::write(
            rfc_dir.join("RFC-1-thing.md"),
            "---\ntitle: Thing\ntype: rfc\nstatus: draft\nauthor: t\ndate: 2026-07-17\ntags: []\n---\nRFC body.\n",
        )
        .unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        let results = super::collect_status_fixes(&root, &store, &config, &[], false, &fs);
        assert!(
            results.is_empty(),
            "no in-lifecycle doc should be repaired, got: {results:?}"
        );
    }
}
