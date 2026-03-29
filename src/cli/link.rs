use crate::cli::resolve::{resolve_to_id, resolve_to_path};
use crate::engine::config::{Config, StoreBackend};
use crate::engine::document::rewrite_frontmatter;
use crate::engine::fs::FileSystem;
use crate::engine::gh::{GhCli, GhIssueReader, GhIssueWriter};
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::store::Store;
use crate::engine::store_dispatch::GithubIssuesStore;
use anyhow::{anyhow, Result};
use std::path::Path;

pub fn link(
    root: &Path,
    store: &Store,
    from: &str,
    rel_type: &str,
    to: &str,
    fs: &dyn FileSystem,
) -> Result<()> {
    link_with_config(root, store, from, rel_type, to, fs, None)
}

pub fn link_with_config(
    root: &Path,
    store: &Store,
    from: &str,
    rel_type: &str,
    to: &str,
    fs: &dyn FileSystem,
    config: Option<&Config>,
) -> Result<()> {
    link_inner(root, store, from, rel_type, to, fs, config, GhCli::new)
}

#[allow(clippy::too_many_arguments)]
fn link_inner<G: GhIssueReader + GhIssueWriter>(
    root: &Path,
    store: &Store,
    from: &str,
    rel_type: &str,
    to: &str,
    fs: &dyn FileSystem,
    config: Option<&Config>,
    client_factory: impl FnOnce() -> G,
) -> Result<()> {
    let resolved_from = resolve_to_path(store, from)?;
    let to_id = resolve_to_id(store, to)?;
    let full_path = root.join(&resolved_from);
    rewrite_frontmatter(&full_path, fs, |doc| {
        if doc.get("related").is_none() {
            doc["related"] = serde_yaml::Value::Sequence(vec![]);
        }
        let mut entry = serde_yaml::Mapping::new();
        entry.insert(
            serde_yaml::Value::String(rel_type.to_string()),
            serde_yaml::Value::String(to_id.clone()),
        );
        doc["related"]
            .as_sequence_mut()
            .unwrap()
            .push(serde_yaml::Value::Mapping(entry));
        Ok(())
    })?;

    push_if_github_backed(root, &resolved_from, config, client_factory)?;
    Ok(())
}

pub fn unlink(
    root: &Path,
    store: &Store,
    from: &str,
    rel_type: &str,
    to: &str,
    fs: &dyn FileSystem,
) -> Result<()> {
    unlink_with_config(root, store, from, rel_type, to, fs, None)
}

pub fn unlink_with_config(
    root: &Path,
    store: &Store,
    from: &str,
    rel_type: &str,
    to: &str,
    fs: &dyn FileSystem,
    config: Option<&Config>,
) -> Result<()> {
    let resolved_from = resolve_to_path(store, from)?;
    let to_id = resolve_to_id(store, to)?;
    let full_path = root.join(&resolved_from);
    rewrite_frontmatter(&full_path, fs, |doc| {
        if let Some(related) = doc.get_mut("related").and_then(|r| r.as_sequence_mut()) {
            related.retain(|entry| {
                if let Some(map) = entry.as_mapping() {
                    let key = serde_yaml::Value::String(rel_type.to_string());
                    if let Some(val) = map.get(&key) {
                        return val.as_str() != Some(to_id.as_str());
                    }
                }
                true
            });
        }
        Ok(())
    })?;

    push_if_github_backed(root, &resolved_from, config, GhCli::new)?;
    Ok(())
}

fn push_if_github_backed<G: GhIssueReader + GhIssueWriter>(
    root: &Path,
    doc_path: &Path,
    config: Option<&Config>,
    client_factory: impl FnOnce() -> G,
) -> Result<()> {
    let config = match config {
        Some(c) => c,
        None => return Ok(()),
    };

    if !doc_path.starts_with(".lazyspec/cache/") {
        return Ok(());
    }

    // Extract type name from cache path: .lazyspec/cache/<type_name>/...
    let type_name = doc_path
        .components()
        .nth(2)
        .and_then(|c| c.as_os_str().to_str())
        .ok_or_else(|| {
            anyhow!(
                "cannot determine type from cache path: {}",
                doc_path.display()
            )
        })?;

    let type_def = config
        .type_by_name(type_name)
        .ok_or_else(|| anyhow!("unknown type '{}' from cache path", type_name))?;

    if type_def.store != StoreBackend::GithubIssues {
        return Ok(());
    }

    let gh_config = config.documents.github.as_ref().ok_or_else(|| {
        anyhow!(
            "type '{}' uses github-issues store but no [github] config found",
            type_name
        )
    })?;
    let repo = gh_config.repo.as_ref().ok_or_else(|| {
        anyhow!(
            "type '{}' uses github-issues store but no github.repo configured",
            type_name
        )
    })?;

    // Extract doc_id from filename
    let doc_id = crate::engine::store::extract_id_from_name(
        doc_path.file_stem().and_then(|s| s.to_str()).unwrap_or(""),
    );

    let mut gh_store = GithubIssuesStore {
        client: client_factory(),
        root: root.to_path_buf(),
        repo: repo.clone(),
        config: config.clone(),
        issue_map: IssueMap::load(root)?,
        issue_cache: IssueCache::new(root),
    };

    gh_store.push_cache(type_def, &doc_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{Config, GithubConfig, NumberingStrategy, StoreBackend, TypeDef};
    use crate::engine::fs::RealFileSystem;
    use crate::engine::gh::{test_support::MockGhClient, GhIssue, GhLabel};
    use crate::engine::issue_map::IssueMap;
    use crate::engine::store::Store;

    fn tmp_root(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lazyspec-link-test-{}-{}",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn gh_config_with_rfc_type() -> Config {
        let rfc_type = TypeDef {
            name: "rfc".to_string(),
            plural: "rfcs".to_string(),
            dir: "docs/rfcs".to_string(),
            prefix: "RFC".to_string(),
            icon: None,
            numbering: NumberingStrategy::Incremental,
            subdirectory: false,
            store: StoreBackend::GithubIssues,
            singleton: false,
            parent_type: None,
        };
        let story_type = TypeDef {
            name: "story".to_string(),
            plural: "stories".to_string(),
            dir: "docs/stories".to_string(),
            prefix: "STORY".to_string(),
            icon: None,
            numbering: NumberingStrategy::Incremental,
            subdirectory: false,
            store: StoreBackend::GithubIssues,
            singleton: false,
            parent_type: None,
        };

        let mut config = Config::default();
        config.documents.types = vec![rfc_type, story_type];
        config.documents.github = Some(GithubConfig {
            repo: Some("owner/repo".to_string()),
            cache_ttl: 60,
        });
        config
    }

    fn make_issue_body(author: &str, date: &str, body: &str) -> String {
        let body_part = if body.is_empty() {
            String::new()
        } else {
            format!("\n\n{}", body)
        };
        format!(
            "<!-- lazyspec\n---\nauthor: {}\ndate: {}\n---\n-->{}",
            author, date, body_part
        )
    }

    #[test]
    fn link_with_config_triggers_github_push_for_cached_doc() {
        let root = tmp_root("link_gh_push");
        let config = gh_config_with_rfc_type();

        // Create cache directories for both types
        let rfc_cache = root.join(".lazyspec/cache/rfc");
        let story_cache = root.join(".lazyspec/cache/story");
        std::fs::create_dir_all(&rfc_cache).unwrap();
        std::fs::create_dir_all(&story_cache).unwrap();

        // Write the "from" doc (RFC) in the cache
        let rfc_content = concat!(
            "---\n",
            "title: My RFC\n",
            "type: rfc\n",
            "status: draft\n",
            "author: agent-7\n",
            "date: 2026-03-27\n",
            "tags: []\n",
            "---\n",
            "RFC body text.\n",
        );
        std::fs::write(rfc_cache.join("RFC-001-my-rfc.md"), rfc_content).unwrap();

        // Write the "to" doc (STORY) in the cache
        let story_content = concat!(
            "---\n",
            "title: My Story\n",
            "type: story\n",
            "status: draft\n",
            "author: agent-7\n",
            "date: 2026-03-27\n",
            "tags: []\n",
            "---\n",
            "Story body.\n",
        );
        std::fs::write(story_cache.join("STORY-001-my-story.md"), story_content).unwrap();

        // Set up issue map so push_cache can find the issue number
        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("RFC-001", 42, "2026-03-27T10:00:00Z");
        issue_map.save(&root).unwrap();

        // Load the store so link can resolve doc IDs
        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        // Set up mock with a view_issue so push_cache can fetch remote state
        let remote_body = make_issue_body("agent-7", "2026-03-27", "RFC body text.");
        let view_issue = GhIssue {
            number: 42,
            url: String::new(),
            title: "My RFC".to_string(),
            body: remote_body,
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
        };

        link_inner(
            &root,
            &store,
            "RFC-001",
            "implements",
            "STORY-001",
            &fs,
            Some(&config),
            || MockGhClient::new().with_view_issue(view_issue),
        )
        .unwrap();

        // Re-read the file to check the frontmatter was rewritten with the link
        let updated = std::fs::read_to_string(rfc_cache.join("RFC-001-my-rfc.md")).unwrap();
        assert!(
            updated.contains("implements: STORY-001"),
            "frontmatter should contain the new link, got:\n{}",
            updated
        );

        // Verify push_cache was triggered by checking the issue map was updated.
        // push_cache clears updated_at after a successful push.
        let refreshed_map = IssueMap::load(&root).unwrap();
        let entry = refreshed_map.get("RFC-001").unwrap();
        assert_eq!(
            entry.updated_at, "",
            "updated_at should be cleared after push, indicating push_cache ran"
        );
    }
}
