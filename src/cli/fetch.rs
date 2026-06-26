use crate::engine::cache_lock::CacheLock;
use crate::engine::config::{Config, StoreBackend, TypeDef};
use crate::engine::gh::{GhGraphql, GhIssueReader, GhIssueWriter, GhMilestoneApi};
use crate::engine::git_ref::GitRefOps;
use crate::engine::github::resolve_repo;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::store_dispatch::{milestone_state_to_status, GithubIssuesStore};
use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::path::Path;

pub fn run(
    root: &Path,
    config: &Config,
    gh: &(impl GhIssueReader + GhIssueWriter + GhGraphql + GhMilestoneApi),
    git_ref_ops: &dyn GitRefOps,
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

    if gh_types.is_empty() && milestone_types.is_empty() && git_ref_types.is_empty() {
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
        {
            bail!(
                "type '{}' is not a github-issues, github-milestones, or git-ref type",
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
            let summary = fetch_milestones(root, type_def, gh, &repo, &mut issue_map)?;
            summaries.push(summary);
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

        let all_type_names: Vec<String> = config
            .documents
            .types
            .iter()
            .map(|t| t.name.clone())
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
                &all_type_names,
                config,
            )?;

            for w in &result.warnings {
                eprintln!("warning: {}", w.message);
            }

            // Inject each board's per-item project field values as namespaced
            // `PROJECT-n.<field>` attributes on member docs. Best-effort: a
            // GraphQL failure warns and the cached doc keeps its other fields.
            let gh_store = GithubIssuesStore {
                client: gh,
                root: root.to_path_buf(),
                repo: repo.clone(),
                config: config.clone(),
                issue_map,
                issue_cache: IssueCache::new(root),
            };
            inject_project_fields_into_cache(&gh_store, type_def);
            issue_map = gh_store.issue_map;

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
fn inject_project_fields_into_cache<G: GhIssueReader + GhIssueWriter + GhGraphql>(
    gh_store: &GithubIssuesStore<G>,
    type_def: &TypeDef,
) {
    let cache_dir = gh_store.root.join(".lazyspec/cache").join(&type_def.name);
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
        if let Err(e) = gh_store.inject_project_fields(&mut meta) {
            eprintln!(
                "warning: could not read project fields for {}: {}",
                meta.id, e
            );
            continue;
        }
        if let Err(e) =
            crate::engine::store_dispatch::write_cache_file(&gh_store.root, type_def, &meta, &body)
        {
            eprintln!("warning: could not rewrite cache for {}: {}", meta.id, e);
        }
    }
}

fn filter_types<'a>(all: Vec<&'a str>, filter: Option<&'a str>) -> Vec<&'a str> {
    match filter {
        Some(f) if all.contains(&f) => vec![f],
        Some(_) => vec![],
        None => all,
    }
}

/// Fetch all milestones for a `github-milestones` type and materialize them as
/// cache documents, mapping REST `state` to a lifecycle status. The milestone
/// number is the document id (`make_id(number)`), mirroring github-issues.
fn fetch_milestones(
    root: &Path,
    type_def: &crate::engine::config::TypeDef,
    gh: &impl GhMilestoneApi,
    repo: &str,
    issue_map: &mut IssueMap,
) -> Result<TypeSummary> {
    use crate::engine::document::{AttrValue, DocMeta, DocType};

    let milestones = gh.milestone_list(repo)?;

    let cache = IssueCache::new(root);
    let previously: std::collections::HashSet<String> =
        cache.list_cached(&type_def.name).into_iter().collect();
    let mut fetched_ids = std::collections::HashSet::new();
    let mut new_count = 0usize;

    for m in &milestones {
        let id = type_def.make_id(m.number);
        let mut attributes: std::collections::BTreeMap<String, AttrValue> = Default::default();
        if let Some(due) = &m.due_on {
            attributes.insert("due_on".to_string(), AttrValue::Str(due.clone()));
        }
        attributes.insert(
            "open_issues".to_string(),
            AttrValue::Int(m.open_issues as i64),
        );
        attributes.insert(
            "closed_issues".to_string(),
            AttrValue::Int(m.closed_issues as i64),
        );
        let meta = DocMeta {
            path: std::path::PathBuf::new(),
            title: m.title.clone(),
            doc_type: DocType::new(&type_def.name),
            status: milestone_state_to_status(&m.state),
            author: "github".to_string(),
            date: chrono::Utc::now().date_naive(),
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            attributes,
            id: id.clone(),
        };

        if !previously.contains(&id) {
            new_count += 1;
        }
        crate::engine::store_dispatch::write_cache_file(root, type_def, &meta, &m.description)?;
        cache.touch_lock(&id);
        issue_map.insert_kind(
            &id,
            m.number,
            "",
            "",
            crate::engine::issue_map::EntryKind::Milestone,
        );
        fetched_ids.insert(id);
    }

    let removed: Vec<String> = previously.difference(&fetched_ids).cloned().collect();
    for id in &removed {
        cache.remove(id, &type_def.name);
        issue_map.remove(id);
    }

    Ok(TypeSummary {
        type_name: type_def.name.to_string(),
        fetched: milestones.len(),
        new: new_count,
        removed: removed.len(),
    })
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
    use crate::engine::gh::test_support::MockGhMilestoneClient;
    use crate::engine::gh::GhMilestone;
    use crate::engine::git_ref::test_support::MockGitRefClient;
    use tempfile::TempDir;

    fn milestone_type_def() -> TypeDef {
        TypeDef {
            name: "milestone".to_string(),
            plural: "milestones".to_string(),
            dir: "docs/milestones".to_string(),
            prefix: "MILESTONE".to_string(),
            icon: None,
            numbering: NumberingStrategy::Incremental,
            subdirectory: false,
            store: StoreBackend::GithubMilestones,
            singleton: false,
            parent_type: None,
            agents: Vec::new(),
            intent: None,
            authorship: Default::default(),
            lifecycle: Default::default(),
            attributes: Default::default(),
        }
    }

    // AC1/AC3/AC6: fetch_milestones writes a cache doc per milestone with the
    // state mapped to a lifecycle status and counts stored, all via the mock seam.
    #[test]
    fn fetch_milestones_writes_cache_docs() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let td = milestone_type_def();

        let gh = MockGhMilestoneClient::with_milestones(vec![
            GhMilestone {
                number: 3,
                title: "v1.0".to_string(),
                description: "first".to_string(),
                due_on: Some("2026-09-01T00:00:00Z".to_string()),
                state: "open".to_string(),
                open_issues: 7,
                closed_issues: 3,
                url: String::new(),
            },
            GhMilestone {
                number: 4,
                title: "v2.0".to_string(),
                description: "second".to_string(),
                due_on: None,
                state: "closed".to_string(),
                open_issues: 0,
                closed_issues: 5,
                url: String::new(),
            },
        ]);

        let mut issue_map = IssueMap::load(root).unwrap();
        let summary = fetch_milestones(root, &td, &gh, "owner/repo", &mut issue_map).unwrap();

        assert_eq!(summary.fetched, 2);
        assert_eq!(summary.new, 2);

        let cache_dir = root.join(".lazyspec/cache/milestone");
        let open = std::fs::read_to_string(cache_dir.join("MILESTONE-3.md")).unwrap();
        assert!(open.contains("status: in-progress"), "{open}");
        assert!(open.contains("open_issues: 7"), "{open}");
        let closed = std::fs::read_to_string(cache_dir.join("MILESTONE-4.md")).unwrap();
        assert!(closed.contains("status: complete"), "{closed}");

        assert_eq!(issue_map.get("MILESTONE-3").unwrap().issue_number, 3);
        assert_eq!(issue_map.get("MILESTONE-4").unwrap().issue_number, 4);
    }

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

        let gh_store = GithubIssuesStore {
            client,
            root: root.to_path_buf(),
            repo: "owner/repo".to_string(),
            config,
            issue_map,
            issue_cache: IssueCache::new(root),
        };

        inject_project_fields_into_cache(&gh_store, &td);

        // Proves write_cache_file did NOT bail on empty id AND the id was derived
        // correctly (STORY-7 mapped to the node with the project field).
        let rewritten = std::fs::read_to_string(cache_dir.join("STORY-7.md")).unwrap();
        assert!(
            rewritten.contains("PROJECT-1.Status: In Progress"),
            "expected injected project field, got:\n{rewritten}"
        );
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
