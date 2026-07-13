use crate::cli::resolve::resolve_to_path;
use crate::engine::config::{Config, StoreBackend};
use crate::engine::document::rewrite_frontmatter;
use crate::engine::fs::FileSystem;
use crate::engine::gh::{deterministic_color, GhCli, GhIssueReader, GhIssueWriter};
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::store::Store;
use anyhow::{anyhow, Result};
use std::path::Path;

pub fn tag_add_with_config(
    root: &Path,
    store: &Store,
    id: &str,
    tags: &[String],
    fs: &dyn FileSystem,
    config: Option<&Config>,
) -> Result<()> {
    tag_add_inner(root, store, id, tags, fs, config, GhCli::new)
}

fn tag_add_inner<G: GhIssueReader + GhIssueWriter>(
    root: &Path,
    store: &Store,
    id: &str,
    tags: &[String],
    fs: &dyn FileSystem,
    config: Option<&Config>,
    client_factory: impl FnOnce() -> G,
) -> Result<()> {
    let resolved = resolve_to_path(store, id)?;
    let full_path = root.join(&resolved);
    rewrite_frontmatter(&full_path, fs, |doc| {
        if doc.get("tags").is_none() {
            doc["tags"] = serde_yaml::Value::Sequence(vec![]);
        }
        let seq = doc["tags"].as_sequence_mut().unwrap();
        for tag in tags {
            let already_present = seq.iter().any(|v| v.as_str() == Some(tag.as_str()));
            if !already_present {
                seq.push(serde_yaml::Value::String(tag.clone()));
            }
        }
        Ok(())
    })?;
    push_tags_if_github_backed(
        root,
        &resolved,
        config,
        client_factory,
        &TagOp::Add(tags.to_vec()),
    )?;
    Ok(())
}

pub fn tag_remove_with_config(
    root: &Path,
    store: &Store,
    id: &str,
    tags: &[String],
    fs: &dyn FileSystem,
    config: Option<&Config>,
) -> Result<()> {
    tag_remove_inner(root, store, id, tags, fs, config, GhCli::new)
}

fn tag_remove_inner<G: GhIssueReader + GhIssueWriter>(
    root: &Path,
    store: &Store,
    id: &str,
    tags: &[String],
    fs: &dyn FileSystem,
    config: Option<&Config>,
    client_factory: impl FnOnce() -> G,
) -> Result<()> {
    let resolved = resolve_to_path(store, id)?;
    let full_path = root.join(&resolved);
    rewrite_frontmatter(&full_path, fs, |doc| {
        if let Some(seq) = doc.get_mut("tags").and_then(|v| v.as_sequence_mut()) {
            seq.retain(|v| {
                v.as_str()
                    .map(|s| !tags.iter().any(|t| t == s))
                    .unwrap_or(true)
            });
        }
        Ok(())
    })?;
    push_tags_if_github_backed(
        root,
        &resolved,
        config,
        client_factory,
        &TagOp::Remove(tags.to_vec()),
    )?;
    Ok(())
}

enum TagOp {
    Add(Vec<String>),
    Remove(Vec<String>),
}

/// Push label changes to GitHub for github-issues backed documents.
///
/// Skips optimistic locking (check_lock) because adding/removing a label is an
/// atomic GitHub operation that doesn't depend on the issue body state.
fn push_tags_if_github_backed<G: GhIssueReader + GhIssueWriter>(
    root: &Path,
    doc_path: &Path,
    config: Option<&Config>,
    client_factory: impl FnOnce() -> G,
    op: &TagOp,
) -> Result<()> {
    let config = match config {
        Some(c) => c,
        None => return Ok(()),
    };

    if !doc_path.starts_with(".lazyspec/cache/") {
        return Ok(());
    }

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

    let doc_id = crate::engine::store::extract_id_from_name(
        doc_path.file_stem().and_then(|s| s.to_str()).unwrap_or(""),
    );

    let issue_map = IssueMap::load(root)?;
    let entry = issue_map
        .get(&doc_id)
        .ok_or_else(|| anyhow!("{} not found in issue map", doc_id))?;
    let issue_number = entry.issue_number;

    let client = client_factory();

    match op {
        TagOp::Add(tags) => {
            for tag in tags {
                client.label_ensure(repo, tag, "", &deterministic_color(tag))?;
            }
            client.issue_edit(repo, issue_number, None, None, tags, &[])?;
        }
        TagOp::Remove(tags) => {
            client.issue_edit(repo, issue_number, None, None, &[], tags)?;
        }
    }

    let issue_cache = IssueCache::new(root);
    issue_cache.touch_lock(&doc_id);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{Config, GithubConfig, NumberingStrategy, StoreBackend, TypeDef};
    use crate::engine::fs::RealFileSystem;
    use crate::engine::gh::test_support::MockGhClient;
    use crate::engine::issue_map::IssueMap;
    use crate::engine::store::Store;

    fn tmp_root(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lazyspec-tag-test-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fs_config() -> Config {
        let rfc_type = TypeDef {
            name: "rfc".to_string(),
            plural: "rfcs".to_string(),
            dir: "docs/rfcs".to_string(),
            prefix: "RFC".to_string(),
            icon: None,
            numbering: NumberingStrategy::Incremental,
            subdirectory: false,
            store: StoreBackend::Filesystem,
            singleton: false,
            parent_type: None,
            agents: Vec::new(),
            intent: None,
            authorship: Default::default(),
            lifecycle: Default::default(),
            attributes: Vec::new(),
            label_override: None,
            github_issue_tag: None,
            github_issue_type: None,
            clickup_list_id: None,
            clickup_task_type: None,
            clickup_custom_field_map: None,
        };
        let mut config = Config::default();
        config.documents.types = vec![rfc_type];
        config
    }

    fn write_rfc(root: &std::path::Path, filename: &str, content: &str) {
        let dir = root.join("docs/rfcs");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(filename), content).unwrap();
    }

    #[test]
    fn tag_add_appends_to_existing_tags() {
        let root = tmp_root("add_existing");
        let config = fs_config();
        write_rfc(
            &root,
            "RFC-001-my-rfc.md",
            "---\ntitle: My RFC\ntype: rfc\nstatus: draft\nauthor: agent-7\ndate: 2026-03-27\ntags:\n- architecture\n---\nBody.\n",
        );
        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        tag_add_inner(
            &root,
            &store,
            "RFC-001",
            &[s("security")],
            &fs,
            None,
            MockGhClient::new,
        )
        .unwrap();

        let updated = std::fs::read_to_string(root.join("docs/rfcs/RFC-001-my-rfc.md")).unwrap();
        assert!(updated.contains("architecture"), "should keep existing tag");
        assert!(updated.contains("security"), "should add new tag");
    }

    #[test]
    fn tag_add_multiple_at_once() {
        let root = tmp_root("add_multi");
        let config = fs_config();
        write_rfc(
            &root,
            "RFC-001-my-rfc.md",
            "---\ntitle: My RFC\ntype: rfc\nstatus: draft\nauthor: agent-7\ndate: 2026-03-27\ntags: []\n---\nBody.\n",
        );
        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        tag_add_inner(
            &root,
            &store,
            "RFC-001",
            &[s("auth"), s("refactor"), s("cleanup")],
            &fs,
            None,
            MockGhClient::new,
        )
        .unwrap();

        let updated = std::fs::read_to_string(root.join("docs/rfcs/RFC-001-my-rfc.md")).unwrap();
        assert!(updated.contains("auth"), "should contain auth");
        assert!(updated.contains("refactor"), "should contain refactor");
        assert!(updated.contains("cleanup"), "should contain cleanup");
    }

    #[test]
    fn tag_add_is_idempotent() {
        let root = tmp_root("add_idempotent");
        let config = fs_config();
        write_rfc(
            &root,
            "RFC-001-my-rfc.md",
            "---\ntitle: My RFC\ntype: rfc\nstatus: draft\nauthor: agent-7\ndate: 2026-03-27\ntags:\n- auth\n---\nBody.\n",
        );
        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        tag_add_inner(
            &root,
            &store,
            "RFC-001",
            &[s("auth")],
            &fs,
            None,
            MockGhClient::new,
        )
        .unwrap();

        let updated = std::fs::read_to_string(root.join("docs/rfcs/RFC-001-my-rfc.md")).unwrap();
        let tags = extract_tags(&updated);
        let count = tags.iter().filter(|t| *t == "auth").count();
        assert_eq!(
            count, 1,
            "tag 'auth' should appear exactly once, got {}",
            count
        );
    }

    #[test]
    fn tag_remove_removes_from_list() {
        let root = tmp_root("remove_existing");
        let config = fs_config();
        write_rfc(
            &root,
            "RFC-001-my-rfc.md",
            "---\ntitle: My RFC\ntype: rfc\nstatus: draft\nauthor: agent-7\ndate: 2026-03-27\ntags:\n- auth\n- refactor\n---\nBody.\n",
        );
        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        tag_remove_inner(
            &root,
            &store,
            "RFC-001",
            &[s("auth")],
            &fs,
            None,
            MockGhClient::new,
        )
        .unwrap();

        let updated = std::fs::read_to_string(root.join("docs/rfcs/RFC-001-my-rfc.md")).unwrap();
        let tags = extract_tags(&updated);
        assert!(
            !tags.contains(&"auth".to_string()),
            "should have removed auth"
        );
        assert!(
            tags.contains(&"refactor".to_string()),
            "should keep refactor"
        );
    }

    #[test]
    fn tag_remove_nonexistent_is_noop() {
        let root = tmp_root("remove_noop");
        let config = fs_config();
        write_rfc(
            &root,
            "RFC-001-my-rfc.md",
            "---\ntitle: My RFC\ntype: rfc\nstatus: draft\nauthor: agent-7\ndate: 2026-03-27\ntags:\n- auth\n---\nBody.\n",
        );
        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        tag_remove_inner(
            &root,
            &store,
            "RFC-001",
            &[s("cleanup")],
            &fs,
            None,
            MockGhClient::new,
        )
        .unwrap();

        let updated = std::fs::read_to_string(root.join("docs/rfcs/RFC-001-my-rfc.md")).unwrap();
        assert!(updated.contains("auth"), "should keep auth unchanged");
    }

    fn s(val: &str) -> String {
        val.to_string()
    }

    fn extract_tags(content: &str) -> Vec<String> {
        let (yaml, _) = crate::engine::document::split_frontmatter(content).unwrap();
        let value: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
        value["tags"]
            .as_sequence()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()
    }

    fn gh_config_with_iteration_type() -> Config {
        let iteration_type = TypeDef {
            name: "iteration".to_string(),
            plural: "iterations".to_string(),
            dir: "docs/iterations".to_string(),
            prefix: "ITERATION".to_string(),
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
            attributes: Vec::new(),
            label_override: None,
            github_issue_tag: None,
            github_issue_type: None,
            clickup_list_id: None,
            clickup_task_type: None,
            clickup_custom_field_map: None,
        };
        let mut config = Config::default();
        config.documents.types = vec![iteration_type];
        config.documents.github = Some(GithubConfig {
            repo: Some("owner/repo".to_string()),
            cache_ttl: 60,
        });
        config
    }

    #[test]
    fn tag_add_github_issues_triggers_label_ensure_and_issue_edit() {
        let root = tmp_root("gh_tag_add");
        let config = gh_config_with_iteration_type();

        let cache_dir = root.join(".lazyspec/cache/iteration");
        std::fs::create_dir_all(&cache_dir).unwrap();

        let cache_content = concat!(
            "---\n",
            "title: Some Title\n",
            "type: iteration\n",
            "status: draft\n",
            "author: agent-7\n",
            "date: 2026-03-27\n",
            "tags: []\n",
            "---\n",
            "Body text.\n",
        );
        std::fs::write(cache_dir.join("ITERATION-042-some-title.md"), cache_content).unwrap();

        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("ITERATION-042", 87, "2026-03-27T10:00:00Z", "");
        issue_map.save(&root).unwrap();

        // Create cache.lock so touch_lock has something to work with
        let lock_dir = root.join(".lazyspec");
        std::fs::write(lock_dir.join("cache.lock"), "{}").unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        tag_add_inner(
            &root,
            &store,
            "ITERATION-042",
            &[s("security")],
            &fs,
            Some(&config),
            MockGhClient::new,
        )
        .unwrap();

        // Verify cache file was updated with the new tag
        let updated =
            std::fs::read_to_string(cache_dir.join("ITERATION-042-some-title.md")).unwrap();
        let tags = extract_tags(&updated);
        assert!(
            tags.contains(&"security".to_string()),
            "cache file should contain security tag"
        );
    }

    #[test]
    fn tag_remove_github_issues_triggers_labels_remove() {
        let root = tmp_root("gh_tag_remove");
        let config = gh_config_with_iteration_type();

        let cache_dir = root.join(".lazyspec/cache/iteration");
        std::fs::create_dir_all(&cache_dir).unwrap();

        let cache_content = concat!(
            "---\n",
            "title: Some Title\n",
            "type: iteration\n",
            "status: draft\n",
            "author: agent-7\n",
            "date: 2026-03-27\n",
            "tags:\n",
            "- security\n",
            "- auth\n",
            "- lazyspec:iteration\n",
            "---\n",
            "Body text.\n",
        );
        std::fs::write(cache_dir.join("ITERATION-042-some-title.md"), cache_content).unwrap();

        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("ITERATION-042", 87, "2026-03-27T10:00:00Z", "");
        issue_map.save(&root).unwrap();

        std::fs::write(root.join(".lazyspec/cache.lock"), "{}").unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        tag_remove_inner(
            &root,
            &store,
            "ITERATION-042",
            &[s("security")],
            &fs,
            Some(&config),
            MockGhClient::new,
        )
        .unwrap();

        let updated =
            std::fs::read_to_string(cache_dir.join("ITERATION-042-some-title.md")).unwrap();
        let tags = extract_tags(&updated);
        assert!(
            !tags.contains(&"security".to_string()),
            "security should be removed"
        );
        assert!(tags.contains(&"auth".to_string()), "auth should remain");
        assert!(
            tags.contains(&"lazyspec:iteration".to_string()),
            "lazyspec:iteration should remain"
        );
    }
}
