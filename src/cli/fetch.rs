use crate::engine::cache_lock::CacheLock;
use crate::engine::clickup::ClickupClient;
use crate::engine::clickup_cache;
use crate::engine::config::{Config, Lifecycle, StoreBackend, TypeDef};
use crate::engine::config_write::write_config_in_place;
use crate::engine::credentials::Token;
use crate::engine::gh::{GhGraphql, GhIssueReader, GhIssueWriter, GhMilestoneApi};
use crate::engine::git_ref::GitRefOps;
use crate::engine::github::resolve_repo;
use crate::engine::issue_body::TypeMatchRule;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::task_map::TaskMap;
use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::path::Path;

#[allow(clippy::too_many_arguments)]
pub fn run(
    root: &Path,
    config: &Config,
    gh: &(impl GhIssueReader + GhIssueWriter + GhGraphql + GhMilestoneApi),
    git_ref_ops: &dyn GitRefOps,
    clickup: &dyn ClickupClient,
    clickup_token: Option<&Token>,
    remote: &str,
    type_filter: Option<&str>,
    json: bool,
) -> Result<()> {
    let gh_types: Vec<&str> = config
        .documents
        .types
        .iter()
        .filter(|t| t.store == StoreBackend::GithubIssues)
        .map(|t| t.name.as_str())
        .collect();

    let milestone_types: Vec<&str> = config
        .documents
        .types
        .iter()
        .filter(|t| t.store == StoreBackend::GithubMilestones)
        .map(|t| t.name.as_str())
        .collect();

    let git_ref_types: Vec<&str> = config
        .documents
        .types
        .iter()
        .filter(|t| t.store == StoreBackend::GitRef)
        .map(|t| t.name.as_str())
        .collect();

    let clickup_types: Vec<&str> = config
        .documents
        .types
        .iter()
        .filter(|t| t.store == StoreBackend::ClickupTasks)
        .map(|t| t.name.as_str())
        .collect();

    if gh_types.is_empty()
        && milestone_types.is_empty()
        && git_ref_types.is_empty()
        && clickup_types.is_empty()
    {
        if json {
            println!("{{\"error\":\"no fetchable types configured\"}}");
        } else {
            println!("No fetchable types configured.");
        }
        return Ok(());
    }

    if let Some(filter) = type_filter {
        if !gh_types.contains(&filter)
            && !milestone_types.contains(&filter)
            && !git_ref_types.contains(&filter)
            && !clickup_types.contains(&filter)
        {
            bail!(
                "type '{}' is not a github-issues, github-milestones, git-ref, or clickup-tasks type",
                filter
            );
        }
    }

    let mut summaries = Vec::new();

    // Milestones MUST be fetched before issues: an issue's native milestone is
    // surfaced as a forward `targets: MILESTONE-n` relation by resolving the
    // milestone number through the issue-map, so the milestone has to be mapped
    // first or the lookup silently drops the relation on a fresh fetch.
    let milestones_to_fetch = filter_types(milestone_types, type_filter);

    if !milestones_to_fetch.is_empty() {
        let repo = resolve_repo(config, root).context(
            "Could not determine GitHub repo. Set [documents.github].repo in .lazyspec.toml",
        )?;
        let mut issue_map = IssueMap::load(root)?;

        for type_name in &milestones_to_fetch {
            let type_def = config
                .type_by_name(type_name)
                .ok_or_else(|| anyhow::anyhow!("type '{}' not found in config", type_name))?;
            let result = crate::engine::milestone_cache::fetch_milestones(
                root,
                type_def,
                gh,
                &repo,
                &mut issue_map,
            )?;

            for w in &result.warnings {
                eprintln!("warning: {}", w.message);
            }

            summaries.push(TypeSummary {
                type_name: type_name.to_string(),
                fetched: result.fetched,
                new: result.new,
                removed: result.removed,
            });
        }

        issue_map.save(root)?;
    }

    let gh_to_fetch = filter_types(gh_types, type_filter);

    if !gh_to_fetch.is_empty() {
        let repo = resolve_repo(config, root).context(
            "Could not determine GitHub repo. Set [documents.github].repo in .lazyspec.toml",
        )?;
        let mut issue_map = IssueMap::load(root)?;
        let cache = IssueCache::new(root);

        let all_type_rules: Vec<TypeMatchRule> = config
            .documents
            .types
            .iter()
            .map(TypeMatchRule::from)
            .collect();

        for type_name in &gh_to_fetch {
            let type_def = config
                .type_by_name(type_name)
                .ok_or_else(|| anyhow::anyhow!("type '{}' not found in config", type_name))?;

            let result = cache.fetch_all(
                root,
                type_def,
                gh,
                gh,
                &repo,
                &mut issue_map,
                &all_type_rules,
                config,
            )?;

            for w in &result.warnings {
                eprintln!("warning: {}", w.message);
            }

            // Inject each board's per-item project field values as namespaced
            // `PROJECT-n.<field>` attributes on member docs. Best-effort: a
            // GraphQL failure warns and the cached doc keeps its other fields.
            inject_project_fields_into_cache(root, gh, &repo, &issue_map, config, type_def);

            summaries.push(TypeSummary {
                type_name: type_name.to_string(),
                fetched: result.fetched,
                new: result.new,
                removed: result.removed,
            });
        }

        issue_map.save(root)?;
    }

    let gitref_to_fetch = filter_types(git_ref_types, type_filter);

    for type_name in &gitref_to_fetch {
        let summary = fetch_git_ref_type(root, git_ref_ops, remote, type_name)?;
        summaries.push(summary);
    }

    let clickup_to_fetch = filter_types(clickup_types, type_filter);

    if !clickup_to_fetch.is_empty() {
        let token = clickup_token.ok_or_else(|| {
            anyhow::anyhow!(
                "no ClickUp token found; run `lazyspec setup clickup` before fetching \
                 clickup-tasks types"
            )
        })?;
        let mut task_map = TaskMap::load(root)?;
        let mut lifecycles: Vec<(String, Lifecycle)> = Vec::new();

        for type_name in &clickup_to_fetch {
            let type_def = config
                .type_by_name(type_name)
                .ok_or_else(|| anyhow::anyhow!("type '{}' not found in config", type_name))?;

            let result =
                clickup_cache::fetch_tasks(root, type_def, clickup, token.expose(), &mut task_map)?;

            // Populate the type's effective lifecycle from the bound List's status
            // set at sync time (RFC-056 §Status handling). fetch_tasks already
            // validated clickup_list_id, so it is present here.
            let list_id = type_def.clickup_list_id.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "type '{}' is clickup-tasks but has no clickup_list_id configured",
                    type_name
                )
            })?;
            let lifecycle = clickup_cache::fetch_lifecycle(clickup, token.expose(), list_id)?;
            lifecycles.push((type_name.to_string(), lifecycle));

            summaries.push(TypeSummary {
                type_name: type_name.to_string(),
                fetched: result.fetched,
                new: result.new,
                removed: result.removed,
            });
        }

        task_map.save(root)?;
        persist_clickup_lifecycles(root, &lifecycles)?;
    }

    if json {
        let json_out: Vec<serde_json::Value> = summaries
            .iter()
            .map(|s| {
                serde_json::json!({
                    "type": s.type_name,
                    "fetched": s.fetched,
                    "new": s.new,
                    "removed": s.removed,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_out)?);
    } else {
        for s in &summaries {
            println!(
                "{}: fetched {}, {} new, {} removed",
                s.type_name, s.fetched, s.new, s.removed
            );
        }
    }

    Ok(())
}

/// For every cached doc of `type_def`, read its board memberships and inject the
/// per-item project field values as `PROJECT-n.<field>` attributes, rewriting the
/// cache file. Best-effort: a per-doc failure warns and the rest still process.
fn inject_project_fields_into_cache(
    root: &Path,
    client: &dyn GhGraphql,
    repo: &str,
    issue_map: &IssueMap,
    config: &Config,
    type_def: &TypeDef,
) {
    let cache_dir = root.join(".lazyspec/cache").join(&type_def.name);
    let entries = match std::fs::read_dir(&cache_dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let (Ok(mut meta), Ok(body)) = (
            crate::engine::document::DocMeta::parse(&content),
            crate::engine::document::DocMeta::extract_body(&content),
        ) else {
            continue;
        };
        // github-issues cache files carry no `id:` in their frontmatter, so the
        // canonical doc id is the filename stem. Derive it when missing so the
        // issue-map lookup resolves and write_cache_file does not bail on empty id.
        if meta.id.is_empty() {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                meta.id = crate::engine::store::extract_id_from_name(stem);
            }
        }
        if let Err(e) = crate::engine::store_dispatch::inject_project_fields_for_meta(
            client, repo, issue_map, config, &mut meta,
        ) {
            eprintln!(
                "warning: could not read project fields for {}: {}",
                meta.id, e
            );
            continue;
        }
        if let Err(e) =
            crate::engine::store_dispatch::write_cache_file(root, type_def, &meta, &body)
        {
            eprintln!("warning: could not rewrite cache for {}: {}", meta.id, e);
        }
    }
}

/// Write each `(type, lifecycle)` derived from a bound List's status set back
/// into `.lazyspec.toml`, so the type's effective lifecycle reflects the live
/// List. Rewrites the config in place (preserving decor/comments) only when a
/// lifecycle actually changed, so an unchanged sync leaves the file untouched.
fn persist_clickup_lifecycles(root: &Path, lifecycles: &[(String, Lifecycle)]) -> Result<()> {
    if lifecycles.is_empty() {
        return Ok(());
    }
    let path = root.join(".lazyspec.toml");
    let src =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let mut config = Config::parse(&src)?;

    let mut changed = false;
    for (type_name, lifecycle) in lifecycles {
        if let Some(type_def) = config
            .documents
            .types
            .iter_mut()
            .find(|t| &t.name == type_name)
        {
            if &type_def.lifecycle != lifecycle {
                type_def.lifecycle = lifecycle.clone();
                changed = true;
            }
        }
    }

    if changed {
        let out = write_config_in_place(&src, &config)?;
        std::fs::write(&path, out).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

fn filter_types<'a>(all: Vec<&'a str>, filter: Option<&'a str>) -> Vec<&'a str> {
    match filter {
        Some(f) if all.contains(&f) => vec![f],
        Some(_) => vec![],
        None => all,
    }
}

fn fetch_git_ref_type(
    root: &Path,
    git_ref_ops: &dyn GitRefOps,
    remote: &str,
    type_name: &str,
) -> Result<TypeSummary> {
    let ref_pattern = format!("refs/lazyspec/{}/*", type_name);
    git_ref_ops.fetch_refs(root, remote, &ref_pattern)?;

    let ref_prefix = format!("refs/lazyspec/{}/", type_name);
    let current_refs = git_ref_ops.list_refs(root, &ref_prefix)?;

    let mut cache_lock = CacheLock::load(root)?;

    let mut fetched = 0;
    let mut new_count = 0;

    let current_ref_keys: HashSet<String> = current_refs
        .iter()
        .map(|(refname, _)| {
            let id = refname.strip_prefix(&ref_prefix).unwrap_or(refname);
            format!("{}/{}", type_name, id)
        })
        .collect();

    let cache_dir = root.join(format!(".lazyspec/cache/{}", type_name));

    for (refname, sha) in &current_refs {
        let id = refname.strip_prefix(&ref_prefix).unwrap_or(refname);
        let doc_key = format!("{}/{}", type_name, id);

        let cached_sha = cache_lock.get(&doc_key);
        if cached_sha == Some(sha.as_str()) {
            continue;
        }

        let is_new = cached_sha.is_none();

        let content = git_ref_ops.read_ref_blob(root, sha, "doc.md")?;

        std::fs::create_dir_all(&cache_dir)?;
        let cache_file = cache_dir.join(format!("{}.md", id));
        std::fs::write(&cache_file, &content)?;

        cache_lock.set(&doc_key, sha);
        fetched += 1;
        if is_new {
            new_count += 1;
        }
    }

    let existing_keys = cache_lock.keys_for_type(type_name);
    let mut removed = 0;
    for key in existing_keys {
        if !current_ref_keys.contains(&key) {
            let id = key.strip_prefix(&format!("{}/", type_name)).unwrap_or(&key);
            let cache_file = cache_dir.join(format!("{}.md", id));
            if cache_file.exists() {
                std::fs::remove_file(&cache_file)?;
            }
            cache_lock.remove(&key);
            removed += 1;
        }
    }

    cache_lock.save(root)?;

    Ok(TypeSummary {
        type_name: type_name.to_string(),
        fetched,
        new: new_count,
        removed,
    })
}

struct TypeSummary {
    type_name: String,
    fetched: usize,
    new: usize,
    removed: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{NumberingStrategy, StoreBackend, TypeDef};
    use crate::engine::git_ref::test_support::MockGitRefClient;
    use tempfile::TempDir;

    // ITERATION-226 task 3: github-issues cache files carry no `id:` in their
    // frontmatter, so DocMeta::parse yields meta.id == "". The fix derives the id
    // from the filename stem before injection so write_cache_file does not bail on
    // "refusing cache write for empty doc id" and PROJECT-n fields are injected.
    #[test]
    fn inject_project_fields_derives_id_from_filename_when_frontmatter_lacks_id() {
        use crate::engine::config::{Config, RelationshipDef};
        use crate::engine::gh::test_support::MockGhClient;
        use crate::engine::gh::{GhFieldKind, GhFieldValueRepr, ProjectFieldValue};

        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let td = TypeDef {
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
            agents: Vec::new(),
            intent: None,
            authorship: Default::default(),
            lifecycle: Default::default(),
            attributes: Default::default(),
            label_override: None,
            github_issue_tag: None,
            github_issue_type: None,
            clickup_list_id: None,
        };

        // A realistic cache file: no `id:` in frontmatter (mirrors what
        // render_cache_content emits), with a membership relation to PROJECT-1.
        let cache_dir = root.join(".lazyspec/cache/story");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let cache_content = "---\n\
title: A Story\n\
type: story\n\
status: draft\n\
author: github\n\
date: 2026-06-26\n\
tags: []\n\
provenance: []\n\
related:\n\
- member-of: PROJECT-1\n\
attributes: {}\n\
---\n\
body text\n";
        std::fs::write(cache_dir.join("STORY-7.md"), cache_content).unwrap();

        let config = Config {
            relationships: vec![RelationshipDef {
                name: "member-of".to_string(),
                inverse: Some("has-member".to_string()),
                github_native: Some("membership".to_string()),
                traversal: None,
            }],
            ..Default::default()
        };

        let client = MockGhClient::new().with_project_field_values(vec![ProjectFieldValue {
            project_number: 1,
            field_name: "Status".into(),
            kind: GhFieldKind::SingleSelect,
            value: GhFieldValueRepr::OptionName("In Progress".into()),
        }]);

        let mut issue_map = IssueMap::load(root).unwrap();
        // The derived id (STORY-7) must map to a node id for the lookup to succeed.
        issue_map.insert("STORY-7", 7, "", "I_issue7");

        inject_project_fields_into_cache(root, &client, "owner/repo", &issue_map, &config, &td);

        // Proves write_cache_file did NOT bail on empty id AND the id was derived
        // correctly (STORY-7 mapped to the node with the project field).
        let rewritten = std::fs::read_to_string(cache_dir.join("STORY-7.md")).unwrap();
        assert!(
            rewritten.contains("PROJECT-1.Status: In Progress"),
            "expected injected project field, got:\n{rewritten}"
        );
    }

    const CLICKUP_CONFIG_SRC: &str = r#"[naming]
pattern = "{type}-{n:03}-{title}.md"

[templates]
dir = ".lazyspec/templates"

# task type follows
[[types]]
name = "task"
plural = "tasks"
dir = "docs/tasks"
prefix = "TASK"
store = "clickup-tasks"
clickup_list_id = "list123"
lifecycle = { states = ["stale"], edges = [] }

[[relationships]]
name = "related-to"
"#;

    #[test]
    fn persist_clickup_lifecycles_writes_derived_states_into_config() {
        use crate::engine::config::Lifecycle;
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join(".lazyspec.toml"), CLICKUP_CONFIG_SRC).unwrap();

        let lifecycle = Lifecycle {
            states: vec![
                "to do".to_string(),
                "in progress".to_string(),
                "done".to_string(),
            ],
            edges: vec![],
        };
        persist_clickup_lifecycles(root, &[("task".to_string(), lifecycle)]).unwrap();

        let out = std::fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
        // The comment (decor) survives the in-place rewrite.
        assert!(out.contains("# task type follows"), "got:\n{out}");
        let config = Config::parse(&out).unwrap();
        let td = config.type_by_name("task").unwrap();
        assert_eq!(td.lifecycle.states, vec!["to do", "in progress", "done"]);
        assert!(td.lifecycle.edges.is_empty());
    }

    #[test]
    fn persist_clickup_lifecycles_leaves_config_untouched_when_unchanged() {
        use crate::engine::config::Lifecycle;
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join(".lazyspec.toml"), CLICKUP_CONFIG_SRC).unwrap();

        // A lifecycle equal to what the config already declares must not rewrite.
        let lifecycle = Lifecycle {
            states: vec!["stale".to_string()],
            edges: vec![],
        };
        persist_clickup_lifecycles(root, &[("task".to_string(), lifecycle)]).unwrap();

        let out = std::fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
        assert_eq!(out, CLICKUP_CONFIG_SRC);
    }

    #[test]
    fn fetch_git_ref_writes_cache_and_updates_lock() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_list_result(Ok(vec![
                (
                    "refs/lazyspec/iteration/ITERATION-042".to_string(),
                    "abc123".to_string(),
                ),
                (
                    "refs/lazyspec/iteration/ITERATION-043".to_string(),
                    "def456".to_string(),
                ),
            ]))
            .with_read_blob_result(Ok("# Iteration 42\ncontent".to_string()))
            .with_read_blob_result(Ok("# Iteration 43\ncontent".to_string()));

        let summary = fetch_git_ref_type(root, &mock, "origin", "iteration").unwrap();

        assert_eq!(summary.fetched, 2);
        assert_eq!(summary.new, 2);
        assert_eq!(summary.removed, 0);

        let cache_file_42 = root.join(".lazyspec/cache/iteration/ITERATION-042.md");
        assert!(cache_file_42.exists());
        assert_eq!(
            std::fs::read_to_string(&cache_file_42).unwrap(),
            "# Iteration 42\ncontent"
        );

        let cache_file_43 = root.join(".lazyspec/cache/iteration/ITERATION-043.md");
        assert!(cache_file_43.exists());

        let lock = CacheLock::load(root).unwrap();
        assert_eq!(lock.get("iteration/ITERATION-042"), Some("abc123"));
        assert_eq!(lock.get("iteration/ITERATION-043"), Some("def456"));

        let calls = mock.calls.borrow();
        assert_eq!(calls[0], "fetch_refs:origin:refs/lazyspec/iteration/*");
        assert_eq!(calls[1], "list_refs:refs/lazyspec/iteration/");
    }

    #[test]
    fn fetch_git_ref_removes_deleted_documents() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Pre-populate cache with a document that will be "deleted" on remote
        let cache_dir = root.join(".lazyspec/cache/iteration");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(cache_dir.join("ITERATION-042.md"), "old content").unwrap();
        let mut lock = CacheLock::default();
        lock.set("iteration/ITERATION-042", "oldsha");
        lock.save(root).unwrap();

        // Remote returns no refs for this type
        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_list_result(Ok(vec![]));

        let summary = fetch_git_ref_type(root, &mock, "origin", "iteration").unwrap();

        assert_eq!(summary.fetched, 0);
        assert_eq!(summary.new, 0);
        assert_eq!(summary.removed, 1);

        assert!(!cache_dir.join("ITERATION-042.md").exists());

        let lock = CacheLock::load(root).unwrap();
        assert!(lock.get("iteration/ITERATION-042").is_none());
    }

    #[test]
    fn fetch_git_ref_no_remote_documents_succeeds() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_list_result(Ok(vec![]));

        let summary = fetch_git_ref_type(root, &mock, "origin", "iteration").unwrap();

        assert_eq!(summary.fetched, 0);
        assert_eq!(summary.new, 0);
        assert_eq!(summary.removed, 0);

        let lock = CacheLock::load(root).unwrap();
        assert!(lock.keys_for_type("iteration").is_empty());
    }

    #[test]
    fn fetch_git_ref_skips_unchanged_sha() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Pre-populate cache lock with matching SHA
        let cache_dir = root.join(".lazyspec/cache/iteration");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(cache_dir.join("ITERATION-042.md"), "existing content").unwrap();
        let mut lock = CacheLock::default();
        lock.set("iteration/ITERATION-042", "abc123");
        lock.save(root).unwrap();

        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_list_result(Ok(vec![(
                "refs/lazyspec/iteration/ITERATION-042".to_string(),
                "abc123".to_string(),
            )]));

        let summary = fetch_git_ref_type(root, &mock, "origin", "iteration").unwrap();

        assert_eq!(summary.fetched, 0);
        assert_eq!(summary.new, 0);
        assert_eq!(summary.removed, 0);

        // read_ref_blob should not have been called
        let calls = mock.calls.borrow();
        assert!(!calls.iter().any(|c| c.starts_with("read_ref_blob")));
    }

    #[test]
    fn fetch_git_ref_updates_changed_sha() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Pre-populate with old SHA
        let cache_dir = root.join(".lazyspec/cache/iteration");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(cache_dir.join("ITERATION-042.md"), "old content").unwrap();
        let mut lock = CacheLock::default();
        lock.set("iteration/ITERATION-042", "oldsha");
        lock.save(root).unwrap();

        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_list_result(Ok(vec![(
                "refs/lazyspec/iteration/ITERATION-042".to_string(),
                "newsha".to_string(),
            )]))
            .with_read_blob_result(Ok("updated content".to_string()));

        let summary = fetch_git_ref_type(root, &mock, "origin", "iteration").unwrap();

        assert_eq!(summary.fetched, 1);
        assert_eq!(summary.new, 0); // existing doc updated, not new
        assert_eq!(summary.removed, 0);

        assert_eq!(
            std::fs::read_to_string(cache_dir.join("ITERATION-042.md")).unwrap(),
            "updated content"
        );

        let lock = CacheLock::load(root).unwrap();
        assert_eq!(lock.get("iteration/ITERATION-042"), Some("newsha"));
    }
}
