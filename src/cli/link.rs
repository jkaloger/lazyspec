use crate::cli::resolve::{resolve_to_id, resolve_to_path};
use crate::engine::cache_lock::CacheLock;
use crate::engine::config::{Config, StoreBackend};
use crate::engine::document::{rewrite_frontmatter, RelationType};
use crate::engine::fs::FileSystem;
use crate::engine::gh::{GhCli, GhGraphql, GhIssueReader, GhIssueWriter, GhMilestoneApi, GqlVar};
use crate::engine::git_ref::{GitCli, GitRefOps};
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::store::Store;
use crate::engine::store_dispatch::{board_number, GithubIssuesStore, GithubProjectsStore};
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

/// Describes the canonical relation that was actually written (or removed),
/// reflecting any inverse-keyword flip. `source` is the document the relation
/// landed on; `target` is the id stored in its frontmatter.
#[derive(Debug)]
pub struct LinkOutcome {
    pub source: PathBuf,
    pub rel_type: RelationType,
    pub target: String,
}

pub fn link_with_config(
    root: &Path,
    store: &Store,
    from: &str,
    rel_type: &str,
    to: &str,
    fs: &dyn FileSystem,
    config: Option<&Config>,
) -> Result<LinkOutcome> {
    link_inner(
        root,
        store,
        from,
        rel_type,
        to,
        fs,
        config,
        GhCli::new,
        GhCli::new,
        GhCli::new,
    )
}

#[allow(clippy::too_many_arguments)]
fn link_inner<G: GhIssueReader + GhIssueWriter + GhGraphql, M: GhMilestoneApi, P: GhGraphql>(
    root: &Path,
    store: &Store,
    from: &str,
    rel_type: &str,
    to: &str,
    fs: &dyn FileSystem,
    config: Option<&Config>,
    client_factory: impl FnOnce() -> G,
    milestone_factory: impl FnOnce() -> M,
    projects_factory: impl FnOnce() -> P,
) -> Result<LinkOutcome> {
    let config = config.ok_or_else(|| {
        anyhow!("link requires a loaded config to resolve relationships from [[relationships]]")
    })?;
    let (rel_str, flipped) = config.resolve_relationship(rel_type)?;
    let (from, to) = if flipped { (to, from) } else { (from, to) };

    let resolved_from = resolve_to_path(store, from)?;
    let to_id = resolve_to_id(store, to)?;
    let from_id = resolve_to_id(store, from)?;
    let full_path = root.join(&resolved_from);
    rewrite_frontmatter(&full_path, fs, |doc| {
        if doc.get("related").is_none() {
            doc["related"] = serde_yaml::Value::Sequence(vec![]);
        }
        let mut entry = serde_yaml::Mapping::new();
        entry.insert(
            serde_yaml::Value::String(rel_str.clone()),
            serde_yaml::Value::String(to_id.clone()),
        );
        doc["related"]
            .as_sequence_mut()
            .unwrap()
            .push(serde_yaml::Value::Mapping(entry));
        Ok(())
    })?;

    let native = apply_native_milestone(
        root,
        config,
        &rel_str,
        &from_id,
        &to_id,
        true,
        milestone_factory,
    )? || apply_native_membership(
        root,
        config,
        &rel_str,
        &from_id,
        &to_id,
        true,
        projects_factory,
    )?;
    push_if_github_backed(root, &resolved_from, Some(config), client_factory, native)?;
    push_if_git_ref_backed(root, &resolved_from, Some(config))?;
    Ok(LinkOutcome {
        source: resolved_from,
        rel_type: RelationType::new(&rel_str),
        target: to_id,
    })
}

/// If `rel_str` declares `github_native = "milestone"`, write the native issue
/// -> milestone association on GitHub (`PATCH issues/{n}` -- the edge of
/// record). `set` true links (milestone number), false unlinks (null).
/// Source/target numbers come from the shared issue map. A no-op for ordinary
/// relationships.
///
/// Returns `true` when a native milestone PATCH was actually performed (so the
/// caller routes the cache mirror through the conflict-free resync), `false` for
/// the ordinary-relationship no-op.
fn apply_native_milestone<M: GhMilestoneApi>(
    root: &Path,
    config: &Config,
    rel_str: &str,
    source_id: &str,
    target_id: &str,
    set: bool,
    milestone_factory: impl FnOnce() -> M,
) -> Result<bool> {
    let is_milestone_rel = config
        .relationship_by_name(rel_str)
        .and_then(|r| r.github_native.as_deref())
        == Some("milestone");
    if !is_milestone_rel {
        return Ok(false);
    }

    let repo = config
        .documents
        .github
        .as_ref()
        .and_then(|g| g.repo.as_ref())
        .ok_or_else(|| anyhow!("github_native milestone relations require [github].repo"))?;

    let issue_map = IssueMap::load(root)?;
    let issue_number = issue_map
        .get(source_id)
        .map(|e| e.issue_number)
        .ok_or_else(|| anyhow!("source '{}' has no GitHub issue number", source_id))?;
    let milestone_number = issue_map
        .get(target_id)
        .map(|e| e.issue_number)
        .ok_or_else(|| anyhow!("target '{}' has no GitHub milestone number", target_id))?;

    let client = milestone_factory();
    let value = if set { Some(milestone_number) } else { None };
    client.issue_set_milestone(repo, issue_number, value)?;
    Ok(true)
}

const ADD_PROJECT_ITEM_MUTATION: &str = "mutation($projectId: ID!, $contentId: ID!) { addProjectV2ItemById(input: {projectId: $projectId, contentId: $contentId}) { item { id } } }";

const DELETE_PROJECT_ITEM_MUTATION: &str = "mutation($projectId: ID!, $itemId: ID!) { deleteProjectV2Item(input: {projectId: $projectId, itemId: $itemId}) { deletedItemId } }";

const PROJECT_ITEM_FOR_ISSUE_QUERY: &str = "query($id: ID!) { node(id: $id) { ... on Issue { projectItems(first: 100) { nodes { id project { id } } } } } }";

/// If `rel_str` declares `github_native = "membership"`, write the native issue
/// -> Projects v2 board association over GraphQL. `set` true adds the issue to
/// the board (`addProjectV2ItemById`), false removes its item
/// (`deleteProjectV2Item`). `source_id` is the issue doc (its node id comes from
/// the issue map); `target_id` is the board doc (`PROJECT-n`, resolved to a
/// project node id). Each membership relation is one board, synced
/// independently. A no-op for ordinary relationships.
///
/// Returns `true` when a native membership mutation was actually performed (so
/// the caller routes the cache mirror through the conflict-free resync), `false`
/// for the ordinary-relationship no-op.
fn apply_native_membership<P: GhGraphql>(
    root: &Path,
    config: &Config,
    rel_str: &str,
    source_id: &str,
    target_id: &str,
    set: bool,
    projects_factory: impl FnOnce() -> P,
) -> Result<bool> {
    let is_membership_rel = config
        .relationship_by_name(rel_str)
        .and_then(|r| r.github_native.as_deref())
        == Some("membership");
    if !is_membership_rel {
        return Ok(false);
    }

    let repo = config
        .documents
        .github
        .as_ref()
        .and_then(|g| g.repo.as_ref())
        .ok_or_else(|| anyhow!("github_native membership relations require [github].repo"))?;
    let owner = repo
        .split_once('/')
        .map(|(o, _)| o)
        .filter(|o| !o.is_empty())
        .ok_or_else(|| anyhow!("repo '{}' must be in owner/name form", repo))?;

    let issue_map = IssueMap::load(root)?;
    let content_id = issue_map
        .get(source_id)
        .map(|e| e.node_id.clone())
        .filter(|n| !n.is_empty())
        .ok_or_else(|| anyhow!("source '{}' has no GitHub issue node id", source_id))?;

    let client = projects_factory();
    let store = GithubProjectsStore {
        client,
        root: root.to_path_buf(),
        repo: repo.clone(),
        config: config.clone(),
        issue_map: IssueMap::load(root)?,
    };
    let board_no = board_number(target_id)?;
    let project_id = store.resolve_board(owner, board_no)?;

    if set {
        store.client.graphql(
            ADD_PROJECT_ITEM_MUTATION,
            &[
                ("projectId", GqlVar::Str(project_id)),
                ("contentId", GqlVar::Str(content_id)),
            ],
        )?;
    } else {
        let resp = store.client.graphql(
            PROJECT_ITEM_FOR_ISSUE_QUERY,
            &[("id", GqlVar::Str(content_id))],
        )?;
        let item_id = resp
            .pointer("/data/node/projectItems/nodes")
            .and_then(|v| v.as_array())
            .and_then(|arr| {
                arr.iter().find_map(|n| {
                    let pid = n.pointer("/project/id").and_then(|v| v.as_str())?;
                    if pid == project_id {
                        n.get("id").and_then(|v| v.as_str()).map(String::from)
                    } else {
                        None
                    }
                })
            })
            .ok_or_else(|| anyhow!("'{}' is not a member of board '{}'", source_id, target_id))?;
        store.client.graphql(
            DELETE_PROJECT_ITEM_MUTATION,
            &[
                ("projectId", GqlVar::Str(project_id)),
                ("itemId", GqlVar::Str(item_id)),
            ],
        )?;
    }
    Ok(true)
}

pub fn unlink_with_config(
    root: &Path,
    store: &Store,
    from: &str,
    rel_type: &str,
    to: &str,
    fs: &dyn FileSystem,
    config: Option<&Config>,
) -> Result<LinkOutcome> {
    unlink_inner(
        root,
        store,
        from,
        rel_type,
        to,
        fs,
        config,
        GhCli::new,
        GhCli::new,
        GhCli::new,
    )
}

#[allow(clippy::too_many_arguments)]
fn unlink_inner<G: GhIssueReader + GhIssueWriter + GhGraphql, M: GhMilestoneApi, P: GhGraphql>(
    root: &Path,
    store: &Store,
    from: &str,
    rel_type: &str,
    to: &str,
    fs: &dyn FileSystem,
    config: Option<&Config>,
    client_factory: impl FnOnce() -> G,
    milestone_factory: impl FnOnce() -> M,
    projects_factory: impl FnOnce() -> P,
) -> Result<LinkOutcome> {
    let config = config.ok_or_else(|| {
        anyhow!("unlink requires a loaded config to resolve relationships from [[relationships]]")
    })?;
    let (rel_str, flipped) = config.resolve_relationship(rel_type)?;
    let (from, to) = if flipped { (to, from) } else { (from, to) };

    let resolved_from = resolve_to_path(store, from)?;
    let to_id = resolve_to_id(store, to)?;
    let from_id = resolve_to_id(store, from)?;
    let full_path = root.join(&resolved_from);
    rewrite_frontmatter(&full_path, fs, |doc| {
        if let Some(related) = doc.get_mut("related").and_then(|r| r.as_sequence_mut()) {
            related.retain(|entry| {
                if let Some(map) = entry.as_mapping() {
                    let key = serde_yaml::Value::String(rel_str.clone());
                    if let Some(val) = map.get(&key) {
                        return val.as_str() != Some(to_id.as_str());
                    }
                }
                true
            });
        }
        Ok(())
    })?;

    let native = apply_native_milestone(
        root,
        config,
        &rel_str,
        &from_id,
        &to_id,
        false,
        milestone_factory,
    )? || apply_native_membership(
        root,
        config,
        &rel_str,
        &from_id,
        &to_id,
        false,
        projects_factory,
    )?;
    push_if_github_backed(root, &resolved_from, Some(config), client_factory, native)?;
    push_if_git_ref_backed(root, &resolved_from, Some(config))?;
    Ok(LinkOutcome {
        source: resolved_from,
        rel_type: RelationType::new(&rel_str),
        target: to_id,
    })
}

fn push_if_github_backed<G: GhIssueReader + GhIssueWriter + GhGraphql>(
    root: &Path,
    doc_path: &Path,
    config: Option<&Config>,
    client_factory: impl FnOnce() -> G,
    native_edge: bool,
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

    if native_edge {
        // Native relations are last-write-wins; the field PATCH already applied.
        // Skip the body conflict guard so an unrelated remote `updated_at` bump
        // cannot leave a half-applied edge (remote linked, cache not mirrored).
        gh_store.resync_after_native_edge(type_def, &doc_id)
    } else {
        gh_store.push_cache(type_def, &doc_id)
    }
}

fn push_if_git_ref_backed(root: &Path, doc_path: &Path, config: Option<&Config>) -> Result<()> {
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

    if type_def.store != StoreBackend::GitRef {
        return Ok(());
    }

    let doc_id = crate::engine::store::extract_id_from_name(
        doc_path.file_stem().and_then(|s| s.to_str()).unwrap_or(""),
    );

    let refname = format!("refs/lazyspec/{}/{}", type_name, doc_id);
    let content = std::fs::read_to_string(root.join(doc_path))?;

    let mut cache_lock = CacheLock::load(root)?;
    let cache_key = format!("{}/{}", type_name, doc_id);
    let old_sha = cache_lock
        .get(&cache_key)
        .ok_or_else(|| anyhow!("no cache.lock entry for '{}'", cache_key))?
        .to_string();

    let git = GitCli;
    let new_sha = git.create_commit(root, &refname, &[("doc.md", &content)], Some(&old_sha))?;
    git.update_ref(root, &refname, &new_sha, &old_sha)?;

    cache_lock.set(&cache_key, &new_sha);
    cache_lock.save(root)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{Config, GithubConfig, NumberingStrategy, StoreBackend, TypeDef};
    use crate::engine::fs::RealFileSystem;
    use crate::engine::gh::{
        test_support::{MockGhClient, MockGhMilestoneClient},
        GhIssue, GhLabel,
    };
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

    fn milestone_assoc_config() -> Config {
        let issue_type = |name: &str, prefix: &str, store: StoreBackend| TypeDef {
            name: name.to_string(),
            plural: format!("{}s", name),
            dir: format!("docs/{}", name),
            prefix: prefix.to_string(),
            icon: None,
            numbering: NumberingStrategy::Incremental,
            subdirectory: false,
            store,
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
            issue_type("story", "STORY", StoreBackend::GithubIssues),
            issue_type("milestone", "MILESTONE", StoreBackend::GithubMilestones),
        ];
        config.documents.github = Some(GithubConfig {
            repo: Some("owner/repo".to_string()),
            cache_ttl: 60,
        });
        config.relationships = vec![crate::engine::config::RelationshipDef {
            name: "targets".to_string(),
            inverse: Some("targeted-by".to_string()),
            github_native: Some("milestone".to_string()),
        }];
        config
    }

    fn write_cache_doc(dir: &std::path::Path, file: &str, title: &str, ty: &str) {
        std::fs::create_dir_all(dir).unwrap();
        let content = format!(
            "---\ntitle: {title}\ntype: {ty}\nstatus: draft\nauthor: a\ndate: 2026-03-27\ntags: []\n---\nbody\n"
        );
        std::fs::write(dir.join(file), content).unwrap();
    }

    // AC4: linking a github-issues doc to a github-milestones doc via a
    // github_native="milestone" relationship records issue_set_milestone with
    // (issue_num, Some(milestone_num)); unlink records (issue_num, None).
    #[test]
    fn link_native_milestone_sets_and_clears_association() {
        let root = tmp_root("link_native_ms");
        let config = milestone_assoc_config();

        write_cache_doc(
            &root.join(".lazyspec/cache/story"),
            "STORY-7.md",
            "My Story",
            "story",
        );
        write_cache_doc(
            &root.join(".lazyspec/cache/milestone"),
            "MILESTONE-3.md",
            "v1.0",
            "milestone",
        );

        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("STORY-7", 7, "", "");
        issue_map.insert("MILESTONE-3", 3, "", "");
        issue_map.save(&root).unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        let recorder = std::rc::Rc::new(MockGhMilestoneClient::new());

        link_inner(
            &root,
            &store,
            "STORY-7",
            "targets",
            "MILESTONE-3",
            &fs,
            Some(&config),
            MockGhClient::new,
            || recorder.clone(),
            MockGhClient::new,
        )
        .unwrap();

        assert_eq!(*recorder.last_set_milestone.borrow(), Some((7, Some(3))));

        // The frontmatter relation was recorded too (surfaces via --json related).
        let updated =
            std::fs::read_to_string(root.join(".lazyspec/cache/story/STORY-7.md")).unwrap();
        assert!(
            updated.contains("targets: MILESTONE-3"),
            "frontmatter should carry the relation, got:\n{updated}"
        );

        // Unlink clears the native association.
        let recorder2 = std::rc::Rc::new(MockGhMilestoneClient::new());
        let store = Store::load(&root, &config).unwrap();
        unlink_inner(
            &root,
            &store,
            "STORY-7",
            "targets",
            "MILESTONE-3",
            &fs,
            Some(&config),
            MockGhClient::new,
            || recorder2.clone(),
            MockGhClient::new,
        )
        .unwrap();
        assert_eq!(*recorder2.last_set_milestone.borrow(), Some((7, None)));
    }

    // Build a GhIssue stub carrying a given updated_at, used to seed the
    // source-issue resync after a native edge (simulating an out-of-band remote
    // bump such as a new comment).
    fn view_issue_at(number: u64, updated_at: &str) -> GhIssue {
        GhIssue {
            number,
            id: format!("I_node{number}"),
            url: String::new(),
            title: "My Story".to_string(),
            body: make_issue_body("a", "2026-03-27", "body"),
            labels: vec![GhLabel {
                name: "lazyspec:story".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: updated_at.to_string(),
            created_at: "2026-06-26T09:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
        }
    }

    // AC1 + AC2: an out-of-band remote comment bumps the source issue's
    // updated_at AFTER our last fetch. A native milestone link must still
    // succeed (no "modified on GitHub" abort), record the milestone PATCH, mirror
    // the relation into the cache, and reconcile the issue-map baseline to the
    // remote's fresh timestamp (never left stale or empty).
    #[test]
    fn link_native_milestone_survives_out_of_band_updated_at() {
        let root = tmp_root("link_native_ms_oob");
        let config = milestone_assoc_config();

        write_cache_doc(
            &root.join(".lazyspec/cache/story"),
            "STORY-7.md",
            "My Story",
            "story",
        );
        write_cache_doc(
            &root.join(".lazyspec/cache/milestone"),
            "MILESTONE-3.md",
            "v1.0",
            "milestone",
        );

        // STORY-7 last fetched at 10:00; remote has since moved to 11:00.
        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("STORY-7", 7, "2026-06-26T10:00:00Z", "I_node7");
        issue_map.insert("MILESTONE-3", 3, "", "");
        issue_map.save(&root).unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;
        let recorder = std::rc::Rc::new(MockGhMilestoneClient::new());

        link_inner(
            &root,
            &store,
            "STORY-7",
            "targets",
            "MILESTONE-3",
            &fs,
            Some(&config),
            || MockGhClient::new().with_view_issue(view_issue_at(7, "2026-06-26T11:00:00Z")),
            || recorder.clone(),
            MockGhClient::new,
        )
        .expect("native milestone link must not abort on an out-of-band updated_at bump");

        // AC1: the native PATCH was recorded (remote edge applied).
        assert_eq!(*recorder.last_set_milestone.borrow(), Some((7, Some(3))));

        // AC2: cache relation mirrored (remote edge + cache agree).
        let updated =
            std::fs::read_to_string(root.join(".lazyspec/cache/story/STORY-7.md")).unwrap();
        assert!(
            updated.contains("targets: MILESTONE-3"),
            "cache should carry the relation, got:\n{updated}"
        );

        // AC2: the issue-map baseline reconciled to the remote's fresh timestamp,
        // not left stale (10:00) or empty.
        let reloaded = IssueMap::load(&root).unwrap();
        assert_eq!(
            reloaded.get("STORY-7").unwrap().updated_at,
            "2026-06-26T11:00:00Z",
            "resync should record the remote's current updated_at"
        );
    }

    // AC4: unlink is symmetric -- a stale-then-advanced updated_at must not block
    // clearing a native milestone association nor the cache mirror.
    #[test]
    fn unlink_native_milestone_survives_out_of_band_updated_at() {
        let root = tmp_root("unlink_native_ms_oob");
        let config = milestone_assoc_config();

        // STORY-7 already targets MILESTONE-3 in the cache.
        std::fs::create_dir_all(root.join(".lazyspec/cache/story")).unwrap();
        let content = "---\ntitle: My Story\ntype: story\nstatus: draft\nauthor: a\ndate: 2026-03-27\ntags: []\nrelated:\n- targets: MILESTONE-3\n---\nbody\n";
        std::fs::write(root.join(".lazyspec/cache/story/STORY-7.md"), content).unwrap();
        write_cache_doc(
            &root.join(".lazyspec/cache/milestone"),
            "MILESTONE-3.md",
            "v1.0",
            "milestone",
        );

        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("STORY-7", 7, "2026-06-26T10:00:00Z", "I_node7");
        issue_map.insert("MILESTONE-3", 3, "", "");
        issue_map.save(&root).unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;
        let recorder = std::rc::Rc::new(MockGhMilestoneClient::new());

        unlink_inner(
            &root,
            &store,
            "STORY-7",
            "targets",
            "MILESTONE-3",
            &fs,
            Some(&config),
            || MockGhClient::new().with_view_issue(view_issue_at(7, "2026-06-26T11:00:00Z")),
            || recorder.clone(),
            MockGhClient::new,
        )
        .expect("native milestone unlink must not abort on an out-of-band updated_at bump");

        assert_eq!(*recorder.last_set_milestone.borrow(), Some((7, None)));

        let updated =
            std::fs::read_to_string(root.join(".lazyspec/cache/story/STORY-7.md")).unwrap();
        assert!(
            !updated.contains("targets: MILESTONE-3"),
            "cache relation should be removed, got:\n{updated}"
        );
    }

    // --- github_native = "membership" (ITERATION-216) ---

    fn membership_config() -> Config {
        let issue_type = |name: &str, prefix: &str, store: StoreBackend| TypeDef {
            name: name.to_string(),
            plural: format!("{}s", name),
            dir: format!("docs/{}", name),
            prefix: prefix.to_string(),
            icon: None,
            numbering: NumberingStrategy::Incremental,
            subdirectory: false,
            store,
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
            issue_type("story", "STORY", StoreBackend::GithubIssues),
            issue_type("project", "PROJECT", StoreBackend::GithubProjects),
        ];
        config.documents.github = Some(GithubConfig {
            repo: Some("my-org/repo".to_string()),
            cache_ttl: 60,
        });
        config.relationships = vec![crate::engine::config::RelationshipDef {
            name: "member-of".to_string(),
            inverse: Some("has-member".to_string()),
            github_native: Some("membership".to_string()),
        }];
        config
    }

    fn org_board(id: &str) -> serde_json::Value {
        serde_json::json!({"data": {"organization": {"projectV2": {"id": id}}}})
    }

    fn add_item_ok() -> serde_json::Value {
        serde_json::json!({"data": {"addProjectV2ItemById": {"item": {"id": "PVTI_x"}}}})
    }

    // AC3: link issue-doc --member-of--> PROJECT-n records an addProjectV2ItemById
    // mutation carrying projectId=<board node id> and contentId=<issue node id>.
    #[test]
    fn link_membership_adds_project_item() {
        let root = tmp_root("link_membership_add");
        let config = membership_config();

        write_cache_doc(
            &root.join(".lazyspec/cache/story"),
            "STORY-7.md",
            "My Story",
            "story",
        );
        write_cache_doc(
            &root.join(".lazyspec/cache/project"),
            "PROJECT-3.md",
            "Board",
            "project",
        );

        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("STORY-7", 7, "", "I_issue7");
        issue_map.insert("PROJECT-3", 3, "", "");
        issue_map.save(&root).unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        let recorder = std::rc::Rc::new(
            MockGhClient::new()
                .with_graphql_responses(vec![org_board("PVT_board3"), add_item_ok()]),
        );

        link_inner(
            &root,
            &store,
            "STORY-7",
            "member-of",
            "PROJECT-3",
            &fs,
            Some(&config),
            MockGhClient::new,
            MockGhMilestoneClient::new,
            || recorder.clone(),
        )
        .unwrap();

        let calls = recorder.graphql_calls.borrow();
        let adds: Vec<_> = calls
            .iter()
            .filter(|(q, _)| q.contains("addProjectV2ItemById"))
            .collect();
        assert_eq!(adds.len(), 1, "one addProjectV2ItemById, got: {:?}", *calls);
        let (_, vars) = adds[0];
        assert!(vars.contains(&(
            "projectId".to_string(),
            GqlVar::Str("PVT_board3".to_string())
        )));
        assert!(vars.contains(&("contentId".to_string(), GqlVar::Str("I_issue7".to_string()))));

        // The frontmatter relation persists.
        let updated =
            std::fs::read_to_string(root.join(".lazyspec/cache/story/STORY-7.md")).unwrap();
        assert!(updated.contains("member-of: PROJECT-3"), "got:\n{updated}");
    }

    // AC4: an issue already a member of one board, adding membership to a second,
    // persists both relations and records two independent addProjectV2ItemById
    // calls (one per board).
    #[test]
    fn link_membership_two_boards_two_adds() {
        let root = tmp_root("link_membership_two");
        let config = membership_config();

        write_cache_doc(
            &root.join(".lazyspec/cache/story"),
            "STORY-7.md",
            "My Story",
            "story",
        );
        write_cache_doc(
            &root.join(".lazyspec/cache/project"),
            "PROJECT-3.md",
            "B3",
            "project",
        );
        write_cache_doc(
            &root.join(".lazyspec/cache/project"),
            "PROJECT-9.md",
            "B9",
            "project",
        );

        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("STORY-7", 7, "", "I_issue7");
        issue_map.save(&root).unwrap();

        let fs = RealFileSystem;

        // First board.
        let rec1 = std::rc::Rc::new(
            MockGhClient::new().with_graphql_responses(vec![org_board("PVT_b3"), add_item_ok()]),
        );
        let store = Store::load(&root, &config).unwrap();
        link_inner(
            &root,
            &store,
            "STORY-7",
            "member-of",
            "PROJECT-3",
            &fs,
            Some(&config),
            MockGhClient::new,
            MockGhMilestoneClient::new,
            || rec1.clone(),
        )
        .unwrap();

        // Second board.
        let rec2 = std::rc::Rc::new(
            MockGhClient::new().with_graphql_responses(vec![org_board("PVT_b9"), add_item_ok()]),
        );
        let store = Store::load(&root, &config).unwrap();
        link_inner(
            &root,
            &store,
            "STORY-7",
            "member-of",
            "PROJECT-9",
            &fs,
            Some(&config),
            MockGhClient::new,
            MockGhMilestoneClient::new,
            || rec2.clone(),
        )
        .unwrap();

        let add1 = rec1
            .graphql_calls
            .borrow()
            .iter()
            .filter(|(q, _)| q.contains("addProjectV2ItemById"))
            .count();
        let add2 = rec2
            .graphql_calls
            .borrow()
            .iter()
            .filter(|(q, _)| q.contains("addProjectV2ItemById"))
            .count();
        assert_eq!(add1, 1, "first board add");
        assert_eq!(add2, 1, "second board add");

        let updated =
            std::fs::read_to_string(root.join(".lazyspec/cache/story/STORY-7.md")).unwrap();
        assert!(updated.contains("member-of: PROJECT-3"), "got:\n{updated}");
        assert!(updated.contains("member-of: PROJECT-9"), "got:\n{updated}");
    }

    // AC5: unlink one membership removes that board's item (deleteProjectV2Item)
    // while the other membership relation stays in frontmatter and is untouched.
    #[test]
    fn unlink_membership_removes_only_that_board() {
        let root = tmp_root("unlink_membership");
        let config = membership_config();

        // STORY-7 already a member of PROJECT-3 and PROJECT-9 in frontmatter.
        std::fs::create_dir_all(root.join(".lazyspec/cache/story")).unwrap();
        let content = "---\ntitle: My Story\ntype: story\nstatus: draft\nauthor: a\ndate: 2026-03-27\ntags: []\nrelated:\n- member-of: PROJECT-3\n- member-of: PROJECT-9\n---\nbody\n";
        std::fs::write(root.join(".lazyspec/cache/story/STORY-7.md"), content).unwrap();
        write_cache_doc(
            &root.join(".lazyspec/cache/project"),
            "PROJECT-3.md",
            "B3",
            "project",
        );
        write_cache_doc(
            &root.join(".lazyspec/cache/project"),
            "PROJECT-9.md",
            "B9",
            "project",
        );

        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("STORY-7", 7, "", "I_issue7");
        issue_map.save(&root).unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        // resolve_board(PROJECT-3) -> PVT_b3, then projectItems lookup, then delete.
        let project_items = serde_json::json!({
            "data": {"node": {"projectItems": {"nodes": [
                {"id": "PVTI_3", "project": {"id": "PVT_b3"}},
                {"id": "PVTI_9", "project": {"id": "PVT_b9"}}
            ]}}}
        });
        let delete_ok =
            serde_json::json!({"data": {"deleteProjectV2Item": {"deletedItemId": "PVTI_3"}}});
        let recorder = std::rc::Rc::new(MockGhClient::new().with_graphql_responses(vec![
            org_board("PVT_b3"),
            project_items,
            delete_ok,
        ]));

        unlink_inner(
            &root,
            &store,
            "STORY-7",
            "member-of",
            "PROJECT-3",
            &fs,
            Some(&config),
            MockGhClient::new,
            MockGhMilestoneClient::new,
            || recorder.clone(),
        )
        .unwrap();

        let calls = recorder.graphql_calls.borrow();
        let deletes: Vec<_> = calls
            .iter()
            .filter(|(q, _)| q.contains("deleteProjectV2Item"))
            .collect();
        assert_eq!(deletes.len(), 1, "one delete, got: {:?}", *calls);
        assert!(deletes[0]
            .1
            .contains(&("itemId".to_string(), GqlVar::Str("PVTI_3".to_string()))));
        assert!(deletes[0]
            .1
            .contains(&("projectId".to_string(), GqlVar::Str("PVT_b3".to_string()))));

        // PROJECT-9 membership untouched in frontmatter; PROJECT-3 removed.
        let updated =
            std::fs::read_to_string(root.join(".lazyspec/cache/story/STORY-7.md")).unwrap();
        assert!(
            !updated.contains("member-of: PROJECT-3"),
            "PROJECT-3 should be removed, got:\n{updated}"
        );
        assert!(
            updated.contains("member-of: PROJECT-9"),
            "PROJECT-9 should remain, got:\n{updated}"
        );
    }

    // AC2 (membership): an out-of-band updated_at bump on the source issue must
    // not block a native membership link nor its cache mirror; the issue-map
    // baseline reconciles to the remote's fresh timestamp.
    #[test]
    fn link_membership_survives_out_of_band_updated_at() {
        let root = tmp_root("link_membership_oob");
        let config = membership_config();

        write_cache_doc(
            &root.join(".lazyspec/cache/story"),
            "STORY-7.md",
            "My Story",
            "story",
        );
        write_cache_doc(
            &root.join(".lazyspec/cache/project"),
            "PROJECT-3.md",
            "Board",
            "project",
        );

        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("STORY-7", 7, "2026-06-26T10:00:00Z", "I_issue7");
        issue_map.insert("PROJECT-3", 3, "", "");
        issue_map.save(&root).unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        let recorder = std::rc::Rc::new(
            MockGhClient::new()
                .with_graphql_responses(vec![org_board("PVT_board3"), add_item_ok()]),
        );

        link_inner(
            &root,
            &store,
            "STORY-7",
            "member-of",
            "PROJECT-3",
            &fs,
            Some(&config),
            || MockGhClient::new().with_view_issue(view_issue_at(7, "2026-06-26T11:00:00Z")),
            MockGhMilestoneClient::new,
            || recorder.clone(),
        )
        .expect("native membership link must not abort on an out-of-band updated_at bump");

        // The native add mutation ran (remote edge applied).
        let adds = recorder
            .graphql_calls
            .borrow()
            .iter()
            .filter(|(q, _)| q.contains("addProjectV2ItemById"))
            .count();
        assert_eq!(adds, 1, "one addProjectV2ItemById");

        // Cache relation mirrored.
        let updated =
            std::fs::read_to_string(root.join(".lazyspec/cache/story/STORY-7.md")).unwrap();
        assert!(updated.contains("member-of: PROJECT-3"), "got:\n{updated}");

        // Baseline reconciled to the remote's fresh timestamp.
        let reloaded = IssueMap::load(&root).unwrap();
        assert_eq!(
            reloaded.get("STORY-7").unwrap().updated_at,
            "2026-06-26T11:00:00Z"
        );
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
            agents: Vec::new(),
            intent: None,
            authorship: Default::default(),
            lifecycle: Default::default(),
            attributes: Default::default(),
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
            agents: Vec::new(),
            intent: None,
            authorship: Default::default(),
            lifecycle: Default::default(),
            attributes: Default::default(),
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
        issue_map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");
        issue_map.save(&root).unwrap();

        // Load the store so link can resolve doc IDs
        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        // Set up mock with a view_issue so push_cache can fetch remote state
        let remote_body = make_issue_body("agent-7", "2026-03-27", "RFC body text.");
        let view_issue = GhIssue {
            number: 42,
            id: String::new(),
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
            issue_type: None,
            milestone: None,
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
            MockGhMilestoneClient::new,
            MockGhClient::new,
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

    fn git_ref_config() -> Config {
        let note_type = TypeDef {
            name: "note".to_string(),
            plural: "notes".to_string(),
            dir: "docs/notes".to_string(),
            prefix: "NOTE".to_string(),
            icon: None,
            numbering: NumberingStrategy::Incremental,
            subdirectory: false,
            store: StoreBackend::GitRef,
            singleton: false,
            parent_type: None,
            agents: Vec::new(),
            intent: None,
            authorship: Default::default(),
            lifecycle: Default::default(),
            attributes: Default::default(),
        };
        let story_type = TypeDef {
            name: "story".to_string(),
            plural: "stories".to_string(),
            dir: "docs/stories".to_string(),
            prefix: "STORY".to_string(),
            icon: None,
            numbering: NumberingStrategy::Incremental,
            subdirectory: false,
            store: StoreBackend::GitRef,
            singleton: false,
            parent_type: None,
            agents: Vec::new(),
            intent: None,
            authorship: Default::default(),
            lifecycle: Default::default(),
            attributes: Default::default(),
        };

        let mut config = Config::default();
        config.documents.types = vec![note_type, story_type];
        config
    }

    fn init_git_repo(root: &std::path::Path) {
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(root)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(root)
            .output()
            .unwrap();
    }

    #[test]
    fn link_git_ref_doc_persists_to_ref() {
        let root = tmp_root("link_git_ref");
        init_git_repo(&root);
        let config = git_ref_config();

        let note_cache = root.join(".lazyspec/cache/note");
        let story_cache = root.join(".lazyspec/cache/story");
        std::fs::create_dir_all(&note_cache).unwrap();
        std::fs::create_dir_all(&story_cache).unwrap();

        let note_content = concat!(
            "---\n",
            "title: My Note\n",
            "type: note\n",
            "status: draft\n",
            "author: agent-7\n",
            "date: 2026-03-27\n",
            "tags: []\n",
            "---\n",
            "Note body.\n",
        );
        std::fs::write(note_cache.join("NOTE-001-my-note.md"), note_content).unwrap();

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

        // Create initial git refs for both docs
        let git = crate::engine::git_ref::GitCli;
        let note_sha = git
            .create_ref_commit(
                &root,
                "refs/lazyspec/note/NOTE-001",
                &[("doc.md", note_content)],
            )
            .unwrap();
        let story_sha = git
            .create_ref_commit(
                &root,
                "refs/lazyspec/story/STORY-001",
                &[("doc.md", story_content)],
            )
            .unwrap();

        // Set up cache.lock with the initial SHAs
        let mut cache_lock = CacheLock::default();
        cache_lock.set("note/NOTE-001", &note_sha);
        cache_lock.set("story/STORY-001", &story_sha);
        cache_lock.save(&root).unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        link_inner(
            &root,
            &store,
            "NOTE-001",
            "implements",
            "STORY-001",
            &fs,
            Some(&config),
            MockGhClient::new,
            MockGhMilestoneClient::new,
            MockGhClient::new,
        )
        .unwrap();

        // Read the ref blob and verify it contains the relationship
        let updated_lock = CacheLock::load(&root).unwrap();
        let new_sha = updated_lock.get("note/NOTE-001").unwrap();
        assert_ne!(new_sha, note_sha, "SHA should have changed after link");

        let blob_content = git.read_ref_blob(&root, new_sha, "doc.md").unwrap();
        assert!(
            blob_content.contains("implements: STORY-001"),
            "ref blob should contain the link, got:\n{}",
            blob_content
        );
    }

    #[test]
    fn link_git_ref_doc_survives_cold_cache() {
        let root = tmp_root("link_git_ref_cold");
        init_git_repo(&root);
        let config = git_ref_config();

        let note_cache = root.join(".lazyspec/cache/note");
        let story_cache = root.join(".lazyspec/cache/story");
        std::fs::create_dir_all(&note_cache).unwrap();
        std::fs::create_dir_all(&story_cache).unwrap();

        let note_content = concat!(
            "---\n",
            "title: My Note\n",
            "type: note\n",
            "status: draft\n",
            "author: agent-7\n",
            "date: 2026-03-27\n",
            "tags: []\n",
            "---\n",
            "Note body.\n",
        );
        std::fs::write(note_cache.join("NOTE-001-my-note.md"), note_content).unwrap();

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

        let git = crate::engine::git_ref::GitCli;
        let note_sha = git
            .create_ref_commit(
                &root,
                "refs/lazyspec/note/NOTE-001",
                &[("doc.md", note_content)],
            )
            .unwrap();
        let story_sha = git
            .create_ref_commit(
                &root,
                "refs/lazyspec/story/STORY-001",
                &[("doc.md", story_content)],
            )
            .unwrap();

        let mut cache_lock = CacheLock::default();
        cache_lock.set("note/NOTE-001", &note_sha);
        cache_lock.set("story/STORY-001", &story_sha);
        cache_lock.save(&root).unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        link_inner(
            &root,
            &store,
            "NOTE-001",
            "implements",
            "STORY-001",
            &fs,
            Some(&config),
            MockGhClient::new,
            MockGhMilestoneClient::new,
            MockGhClient::new,
        )
        .unwrap();

        // Delete the cache file to simulate cold cache
        std::fs::remove_file(note_cache.join("NOTE-001-my-note.md")).unwrap();

        // Re-materialize from the ref
        let updated_lock = CacheLock::load(&root).unwrap();
        let new_sha = updated_lock.get("note/NOTE-001").unwrap();
        let blob_content = git.read_ref_blob(&root, new_sha, "doc.md").unwrap();

        assert!(
            blob_content.contains("implements: STORY-001"),
            "relationship should survive cold cache, got:\n{}",
            blob_content
        );
    }

    #[test]
    fn push_if_git_ref_backed_skips_non_cache_path() {
        let root = tmp_root("git_ref_skip_noncache");
        let config = git_ref_config();
        let doc_path = std::path::Path::new("docs/notes/NOTE-001-my-note.md");
        let result = push_if_git_ref_backed(&root, doc_path, Some(&config));
        assert!(result.is_ok());
    }

    #[test]
    fn push_if_git_ref_backed_skips_non_git_ref_type() {
        let root = tmp_root("git_ref_skip_ghtype");
        let config = gh_config_with_rfc_type();
        let doc_path = std::path::Path::new(".lazyspec/cache/rfc/RFC-001-my-rfc.md");
        let result = push_if_git_ref_backed(&root, doc_path, Some(&config));
        assert!(result.is_ok());
    }

    #[test]
    fn push_if_git_ref_backed_skips_when_no_config() {
        let root = tmp_root("git_ref_skip_noconfig");
        let doc_path = std::path::Path::new(".lazyspec/cache/note/NOTE-001-my-note.md");
        let result = push_if_git_ref_backed(&root, doc_path, None);
        assert!(result.is_ok());
    }
}
