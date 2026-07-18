use crate::cli::resolve::{resolve_shorthand_or_path, resolve_to_path};
use crate::engine::config::Config;
use crate::engine::document::rewrite_frontmatter;
use crate::engine::fs::FileSystem;
use crate::engine::store::Store;
use crate::engine::store_dispatch::{build_registry, PushOutcome};
use anyhow::Result;
use std::path::Path;

pub fn tag_add_with_config(
    root: &Path,
    store: &Store,
    id: &str,
    tags: &[String],
    fs: &dyn FileSystem,
    config: Option<&Config>,
) -> Result<PushOutcome> {
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
    propagate_tags(root, store, id, config, tags, &[])
}

pub fn tag_remove_with_config(
    root: &Path,
    store: &Store,
    id: &str,
    tags: &[String],
    fs: &dyn FileSystem,
    config: Option<&Config>,
) -> Result<PushOutcome> {
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
    propagate_tags(root, store, id, config, &[], tags)
}

/// Propagate a tag mutation to the document's backend via
/// [`DocumentStore::sync_tags`](crate::engine::store_dispatch::DocumentStore::sync_tags).
///
/// The frontmatter rewrite above is backend-agnostic (source file for
/// filesystem, cache file for materialized backends); this dispatches the remote
/// half through the store registry so each backend either syncs or fails loudly.
/// With no config the type/backend cannot be resolved, so propagation is skipped
/// and only the local rewrite stands.
fn propagate_tags(
    root: &Path,
    store: &Store,
    id: &str,
    config: Option<&Config>,
    add: &[String],
    remove: &[String],
) -> Result<PushOutcome> {
    let Some(config) = config else {
        return Ok(PushOutcome::Synced);
    };
    let doc = resolve_shorthand_or_path(store, id)?;
    let Some(type_def) = config.type_by_name(doc.doc_type.as_str()) else {
        return Ok(PushOutcome::Synced);
    };
    let mut registry = build_registry(root, config);
    registry
        .for_type(type_def)?
        .sync_tags(type_def, &doc.id, add, remove)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{Config, NumberingStrategy, StoreBackend, TypeDef};
    use crate::engine::fs::RealFileSystem;
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

        tag_add_with_config(
            &root,
            &store,
            "RFC-001",
            &[s("security")],
            &fs,
            Some(&config),
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

        tag_add_with_config(
            &root,
            &store,
            "RFC-001",
            &[s("auth"), s("refactor"), s("cleanup")],
            &fs,
            Some(&config),
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

        tag_add_with_config(&root, &store, "RFC-001", &[s("auth")], &fs, Some(&config)).unwrap();

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

        tag_remove_with_config(&root, &store, "RFC-001", &[s("auth")], &fs, Some(&config)).unwrap();

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

        tag_remove_with_config(
            &root,
            &store,
            "RFC-001",
            &[s("cleanup")],
            &fs,
            Some(&config),
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

    // AC: sync routes through the store, not a backend match. The CLI must not
    // branch on the github-issues backend variant; propagation dispatches through
    // `DocumentStore::sync_tags`. Assert the variant literal is absent
    // structurally (the needle is split so this test's own source does not match
    // it). Per-backend propagation behaviour is covered by store-level tests in
    // `store_dispatch` and `git_ref_store`.
    #[test]
    fn cli_tag_does_not_branch_on_github_backend() {
        let needle = concat!("StoreBackend", "::", "Github", "Issues");
        assert!(
            !include_str!("tag.rs").contains(needle),
            "cli/tag.rs must route through DocumentStore::sync_tags, not match on the github backend"
        );
    }
}
