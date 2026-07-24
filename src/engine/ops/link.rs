use crate::engine::clickup::{ClickupClient, ClickupHttpClient};
use crate::engine::clickup_cache;
use crate::engine::config::{Config, StoreBackend, CLICKUP_RELATIONS_FIELD};
use crate::engine::credentials::{CredentialStore, LayeredCredentialStore, Token};
use crate::engine::document::{rewrite_frontmatter, DocMeta, RelationType};
use crate::engine::fs::FileSystem;
use crate::engine::gh::{
    GhCli, GhGraphql, GhIssueDependencyApi, GhIssueReader, GhIssueWriter, GhMilestoneApi, GqlVar,
};
use crate::engine::gh_subissue::{ADD_SUB_ISSUE_MUTATION, REMOVE_SUB_ISSUE_MUTATION};
use crate::engine::git_ref_store::GitRefStore;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::ops::resolve::{resolve_to_id, resolve_to_path};
use crate::engine::store::Store;
use crate::engine::store_dispatch::{
    board_number, GithubIssuesStore, GithubProjectsStore, PushOutcome,
};
use crate::engine::task_map::TaskMap;
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
    /// Whether the edge reached the remote. Only git-ref-backed docs defer their
    /// push (and can report `LocalOnly` on an unreachable remote); every other
    /// backend syncs synchronously, so the outcome is `Synced`.
    pub push_outcome: PushOutcome,
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
        GhCli::new,
    )
}

#[allow(clippy::too_many_arguments)]
fn link_inner<
    G: GhIssueReader + GhIssueWriter + GhGraphql + Send + 'static,
    M: GhMilestoneApi,
    P: GhGraphql + 'static,
    D: GhIssueDependencyApi,
>(
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
    dependency_factory: impl FnOnce() -> D,
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

    // Pre-write store-aware guard: reject an illegal milestone triple before any
    // native call or cache write (ITER-230).
    validate_milestone_relation(config, store, &from_id, &to_id, &rel_str)?;

    // Native field PATCH (milestone / membership) is its own authoritative
    // last-write-wins edge; it runs before the cache mirror (ITER-222).
    let native = apply_native_milestone(
        root,
        config,
        &rel_str,
        &from_id,
        &to_id,
        true,
        milestone_factory,
    )? || apply_native_graphql_edge(
        root,
        config,
        store,
        &rel_str,
        &from_id,
        &to_id,
        true,
        projects_factory,
    )? || apply_native_dependency(
        root,
        config,
        store,
        &rel_str,
        &from_id,
        &to_id,
        true,
        dependency_factory,
    )?;

    // Push-first: the remote merge (or native resync) lands BEFORE the cache is
    // touched, so a failed push leaves the local cache `related` unchanged.
    push_if_github_backed(
        root,
        &resolved_from,
        Some(config),
        client_factory,
        native,
        &rel_str,
        &to_id,
        true,
    )?;

    // Only on push success: mirror the edge into the cache frontmatter,
    // insert-if-absent so a double-link is a no-op on disk.
    rewrite_frontmatter(&full_path, fs, |doc| {
        // A bare `related:` parses as YAML null; coerce anything that is not
        // already a sequence so the push below never panics (AUDIT-018 C2).
        if !doc.get("related").map(|r| r.is_sequence()).unwrap_or(false) {
            doc["related"] = serde_yaml::Value::Sequence(vec![]);
        }
        let Some(related) = doc["related"].as_sequence_mut() else {
            return Err(anyhow!("frontmatter `related` is not a sequence"));
        };
        let already_present = related.iter().any(|entry| {
            entry
                .as_mapping()
                .and_then(|m| m.get(serde_yaml::Value::String(rel_str.clone())))
                .and_then(|v| v.as_str())
                == Some(to_id.as_str())
        });
        if !already_present {
            let mut entry = serde_yaml::Mapping::new();
            entry.insert(
                serde_yaml::Value::String(rel_str.clone()),
                serde_yaml::Value::String(to_id.clone()),
            );
            related.push(serde_yaml::Value::Mapping(entry));
        }
        Ok(())
    })?;

    let push_outcome = push_if_git_ref_backed(root, &resolved_from, Some(config))?;

    // ClickUp-backed docs persist relations by serializing the doc's complete
    // relation set (now mirrored into the cache above) into the configured text
    // custom field -- a full replace, the same after-mirror posture git-ref
    // takes. Production factories are hardcoded here just as git-ref hardcodes
    // `GitCli`; the fake-injectable seam lives in `push_if_clickup_backed`.
    push_if_clickup_backed(
        root,
        &resolved_from,
        Some(config),
        ClickupHttpClient::new,
        || LayeredCredentialStore::global().load_clickup_token(),
    )?;

    Ok(LinkOutcome {
        source: resolved_from,
        rel_type: RelationType::new(&rel_str),
        target: to_id,
        push_outcome,
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
fn apply_native_membership<P: GhGraphql + 'static>(
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

    let store = GithubProjectsStore {
        client: Box::new(projects_factory()),
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

/// If `rel_str` declares `github_native = "dependency"`, opportunistically write
/// the native GitHub issue-dependency edge. Unlike milestone/membership (which
/// are native-*only* and reject non-issue endpoints), `blocks` is a universal
/// semantic relation that gains an *optional* native edge: the dependency fires
/// only when BOTH endpoints resolve to github-issues docs (necessarily the same
/// repo under lazyspec's one-repo-per-store model). Otherwise — filesystem,
/// cross-store, or a non-dependency relation — it is a no-op with no error, and
/// the relation stays comment/graph-backed as before.
///
/// Canonical direction is `source blocks target`, i.e. `target` is blocked_by
/// `source`, so the edge is written on the target's issue. `set` true adds it
/// (link), false removes it (unlink).
///
/// Returns `true` when a native dependency call was actually performed (so the
/// caller routes the cache mirror through the conflict-free resync), `false` for
/// the opportunistic no-op.
#[allow(clippy::too_many_arguments)]
fn apply_native_dependency<D: GhIssueDependencyApi>(
    root: &Path,
    config: &Config,
    store: &Store,
    rel_str: &str,
    source_id: &str,
    target_id: &str,
    set: bool,
    dependency_factory: impl FnOnce() -> D,
) -> Result<bool> {
    let is_dependency_rel = config
        .relationship_by_name(rel_str)
        .and_then(|r| r.github_native.as_deref())
        == Some("dependency");
    if !is_dependency_rel {
        return Ok(false);
    }

    // Opportunistic: the native edge fires only when both endpoints are
    // github-issues docs. A filesystem, cross-store, or cross-repo endpoint
    // falls through to the ordinary comment/graph-backed record.
    let both_issues = store_of(config, store, source_id) == Some(StoreBackend::GithubIssues)
        && store_of(config, store, target_id) == Some(StoreBackend::GithubIssues);
    if !both_issues {
        return Ok(false);
    }

    let repo = config
        .documents
        .github
        .as_ref()
        .and_then(|g| g.repo.as_ref())
        .ok_or_else(|| anyhow!("github_native dependency relations require [github].repo"))?;

    let issue_map = IssueMap::load(root)?;
    let blocking_number = issue_map
        .get(source_id)
        .map(|e| e.issue_number)
        .ok_or_else(|| anyhow!("source '{}' has no GitHub issue number", source_id))?;
    let blocked_number = issue_map
        .get(target_id)
        .map(|e| e.issue_number)
        .ok_or_else(|| anyhow!("target '{}' has no GitHub issue number", target_id))?;

    let client = dependency_factory();
    if set {
        client.add_blocked_by(repo, blocked_number, blocking_number)?;
    } else {
        client.remove_blocked_by(repo, blocked_number, blocking_number)?;
    }
    Ok(true)
}

const SUB_ISSUE_PARENT_QUERY: &str =
    "query($id: ID!) { node(id: $id) { ... on Issue { parent { id number } } } }";

/// Route a GraphQL-only native edge (`membership` / `sub-issue`) to its writer,
/// consuming the single `GhGraphql` factory exactly once. Both relations use the
/// same seam and are mutually exclusive per relationship, so the dispatch is by
/// the relation's declared `github_native`. A no-op (`Ok(false)`) for any other
/// relation, leaving the factory unused.
#[allow(clippy::too_many_arguments)]
fn apply_native_graphql_edge<P: GhGraphql + 'static>(
    root: &Path,
    config: &Config,
    store: &Store,
    rel_str: &str,
    source_id: &str,
    target_id: &str,
    set: bool,
    projects_factory: impl FnOnce() -> P,
) -> Result<bool> {
    match config
        .relationship_by_name(rel_str)
        .and_then(|r| r.github_native.as_deref())
    {
        Some("membership") => apply_native_membership(
            root,
            config,
            rel_str,
            source_id,
            target_id,
            set,
            projects_factory,
        ),
        Some("sub-issue") => apply_native_subissue(
            root,
            config,
            store,
            rel_str,
            source_id,
            target_id,
            set,
            projects_factory,
        ),
        _ => Ok(false),
    }
}

/// If `rel_str` declares `github_native = "sub-issue"`, opportunistically write
/// the native GitHub sub-issue edge. Like `blocks`/dependency, this is a
/// universal semantic relation that gains an *optional* native edge: it fires
/// only when BOTH endpoints resolve to github-issues docs (necessarily the same
/// repo under lazyspec's one-repo-per-store model). Otherwise — filesystem,
/// cross-store, or a non-sub-issue relation — it is a no-op with no error, and
/// the relation stays comment/graph-backed as before.
///
/// Direction: `source implements target` makes `source` the child and `target`
/// the parent, i.e. `addSubIssue(issueId: target-node, subIssueId: source-node)`.
/// `set` true adds the edge (link), false removes it (unlink). Node ids come
/// from the issue map; an empty node id (legacy map) is a clear error telling the
/// user to re-fetch, with no mutation. Before adding, the child's existing native
/// parent is queried: if it already has a *different* parent the link fails
/// naming it (single-parent; reparenting is explicit unlink+link).
///
/// Returns `true` when a native sub-issue mutation was performed (so the caller
/// routes the cache mirror through the conflict-free resync), `false` for the
/// opportunistic no-op.
#[allow(clippy::too_many_arguments)]
fn apply_native_subissue<P: GhGraphql>(
    root: &Path,
    config: &Config,
    store: &Store,
    rel_str: &str,
    source_id: &str,
    target_id: &str,
    set: bool,
    subissue_factory: impl FnOnce() -> P,
) -> Result<bool> {
    let is_subissue_rel = config
        .relationship_by_name(rel_str)
        .and_then(|r| r.github_native.as_deref())
        == Some("sub-issue");
    if !is_subissue_rel {
        return Ok(false);
    }

    // Opportunistic: the native edge fires only when both endpoints are
    // github-issues docs. A filesystem, cross-store, or cross-repo endpoint
    // falls through to the ordinary comment/graph-backed record.
    let both_issues = store_of(config, store, source_id) == Some(StoreBackend::GithubIssues)
        && store_of(config, store, target_id) == Some(StoreBackend::GithubIssues);
    if !both_issues {
        return Ok(false);
    }

    let issue_map = IssueMap::load(root)?;
    let child_node = issue_map
        .get(source_id)
        .map(|e| e.node_id.clone())
        .filter(|n| !n.is_empty())
        .ok_or_else(|| {
            anyhow!(
                "source '{}' has no GitHub issue node id in the issue map; \
                 run `lazyspec fetch` to populate it before linking sub-issues",
                source_id
            )
        })?;
    let parent_node = issue_map
        .get(target_id)
        .map(|e| e.node_id.clone())
        .filter(|n| !n.is_empty())
        .ok_or_else(|| {
            anyhow!(
                "target '{}' has no GitHub issue node id in the issue map; \
                 run `lazyspec fetch` to populate it before linking sub-issues",
                target_id
            )
        })?;

    let client = subissue_factory();

    if set {
        // Single-parent: a child already nested under a different parent must be
        // unlinked first; we never silently reparent.
        let resp = client.graphql(
            SUB_ISSUE_PARENT_QUERY,
            &[("id", GqlVar::Str(child_node.clone()))],
        )?;
        if let Some(parent) = resp.pointer("/data/node/parent").filter(|p| !p.is_null()) {
            let existing_node = parent
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if existing_node != parent_node {
                let existing_name = parent
                    .get("number")
                    .and_then(|v| v.as_u64())
                    .and_then(|n| issue_map.shorthand_for_number(n).map(str::to_string))
                    .or_else(|| {
                        parent
                            .get("number")
                            .and_then(|v| v.as_u64())
                            .map(|n| format!("#{n}"))
                    })
                    .unwrap_or_else(|| existing_node.to_string());
                return Err(anyhow!(
                    "'{}' is already a native sub-issue of '{}'; unlink it before linking to '{}' \
                     (reparenting is an explicit unlink + link)",
                    source_id,
                    existing_name,
                    target_id
                ));
            }
        }
        client.graphql(
            ADD_SUB_ISSUE_MUTATION,
            &[
                ("issueId", GqlVar::Str(parent_node)),
                ("subIssueId", GqlVar::Str(child_node)),
            ],
        )?;
    } else {
        client.graphql(
            REMOVE_SUB_ISSUE_MUTATION,
            &[
                ("issueId", GqlVar::Str(parent_node)),
                ("subIssueId", GqlVar::Str(child_node)),
            ],
        )?;
    }
    Ok(true)
}

/// The store a document id resolves to, via its type's `[[types]]` declaration.
/// `None` when the id resolves to no doc or its type is undeclared -- the caller
/// treats an unresolved store as non-milestone (the guard only fires on the
/// github-milestones store).
fn store_of(config: &Config, store: &Store, id: &str) -> Option<StoreBackend> {
    let path = resolve_to_path(store, id).ok()?;
    let doc = store.get(&path)?;
    config
        .type_by_name(doc.doc_type.as_str())
        .map(|t| t.store.clone())
}

/// Enforce the store-aware relation vocabulary for `github-milestones` docs: a
/// milestone is a REST object with no body and no native edge for arbitrary
/// relations, so it may only be the *target* of the `targets` relation
/// (`github_native = "milestone"`) and may never be a relation *source*.
/// Ordinary docs are unaffected.
fn validate_milestone_relation(
    config: &Config,
    store: &Store,
    from_id: &str,
    to_id: &str,
    rel_str: &str,
) -> Result<()> {
    let source_store = store_of(config, store, from_id);
    if source_store == Some(StoreBackend::GithubMilestones) {
        return Err(anyhow!(
            "milestone docs cannot be the source of a relation ('{}' is a github-milestones doc)",
            from_id
        ));
    }

    let target_store = store_of(config, store, to_id);
    let is_milestone_rel = config
        .relationship_by_name(rel_str)
        .and_then(|r| r.github_native.as_deref())
        == Some("milestone");

    if is_milestone_rel && source_store != Some(StoreBackend::GithubIssues) {
        return Err(anyhow!(
            "only github-issues docs can target a milestone ('{}' is not a github-issues doc)",
            from_id
        ));
    }

    match (
        is_milestone_rel,
        target_store == Some(StoreBackend::GithubMilestones),
    ) {
        (true, false) => Err(anyhow!(
            "`targets` requires a milestone target ('{}' is not a github-milestones doc)",
            to_id
        )),
        (false, true) => Err(anyhow!(
            "milestone docs can only be targeted by `targets` ('{}' is a github-milestones doc)",
            to_id
        )),
        (true, true) | (false, false) => Ok(()),
    }
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
        GhCli::new,
    )
}

#[allow(clippy::too_many_arguments)]
fn unlink_inner<
    G: GhIssueReader + GhIssueWriter + GhGraphql + Send + 'static,
    M: GhMilestoneApi,
    P: GhGraphql + 'static,
    D: GhIssueDependencyApi,
>(
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
    dependency_factory: impl FnOnce() -> D,
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

    // Pre-retain store-aware guard: reject an illegal milestone triple before any
    // native call or cache mutation (ITER-230), mirroring link_inner.
    validate_milestone_relation(config, store, &from_id, &to_id, &rel_str)?;

    let native = apply_native_milestone(
        root,
        config,
        &rel_str,
        &from_id,
        &to_id,
        false,
        milestone_factory,
    )? || apply_native_graphql_edge(
        root,
        config,
        store,
        &rel_str,
        &from_id,
        &to_id,
        false,
        projects_factory,
    )? || apply_native_dependency(
        root,
        config,
        store,
        &rel_str,
        &from_id,
        &to_id,
        false,
        dependency_factory,
    )?;

    // Push-first: remote retain-drop (or native resync) lands BEFORE the cache
    // is touched, so a failed push leaves the local cache `related` unchanged.
    push_if_github_backed(
        root,
        &resolved_from,
        Some(config),
        client_factory,
        native,
        &rel_str,
        &to_id,
        false,
    )?;

    // Only on push success: retain-drop the edge from the cache frontmatter.
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

    let push_outcome = push_if_git_ref_backed(root, &resolved_from, Some(config))?;

    // Unlink is the same full-replace write as link: the edge was dropped from
    // the cache above, so re-serializing the doc's remaining relations and
    // replacing the field value drops it on ClickUp too.
    push_if_clickup_backed(
        root,
        &resolved_from,
        Some(config),
        ClickupHttpClient::new,
        || LayeredCredentialStore::global().load_clickup_token(),
    )?;

    Ok(LinkOutcome {
        source: resolved_from,
        rel_type: RelationType::new(&rel_str),
        target: to_id,
        push_outcome,
    })
}

/// Persist a ClickUp-backed doc's relations by writing the configured text
/// custom field (RFC-056 §Relations). A no-op unless `doc_path` is a cache doc
/// whose type is [`StoreBackend::ClickupTasks`].
///
/// The write is a *full replace*: the doc's complete relation set -- read from
/// the cache frontmatter the caller mirrored just above -- is serialized into
/// the YAML relations block ([`clickup_cache::encode_relations_block`], the
/// inverse of the read decode so the two round-trip) and written to the field
/// via `POST /task/{id}/field/{field_id}`. No add/rem diffing: link and unlink
/// both re-serialize and replace the whole block.
///
/// The field id resolves through the type's `clickup_custom_field_map` under the
/// reserved [`CLICKUP_RELATIONS_FIELD`] key ([`TypeDef::clickup_field_id`], the
/// name->uuid write direction). A type with no such entry raises a clear config
/// error up front rather than failing mid-write. The token is loaded lazily
/// (only for a clickup-backed doc, so an ordinary link never touches the
/// keychain), mirroring the create/update/delete write paths.
///
/// `clickup_factory`/`token_loader` are injected so a test drives this with a
/// [`FakeClickupClient`](crate::engine::clickup::FakeClickupClient) and a
/// scripted token; production passes [`ClickupHttpClient::new`] and the global
/// credential store.
fn push_if_clickup_backed<C: ClickupClient>(
    root: &Path,
    doc_path: &Path,
    config: Option<&Config>,
    clickup_factory: impl FnOnce() -> C,
    token_loader: impl FnOnce() -> Result<Option<Token>>,
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

    if type_def.store != StoreBackend::ClickupTasks {
        return Ok(());
    }

    // A clickup-tasks type must map the reserved relations key to a pre-created
    // text custom field id; without it there is nowhere to persist relations.
    let field_id = type_def
        .clickup_field_id(CLICKUP_RELATIONS_FIELD)
        .ok_or_else(|| {
            anyhow!(
                "type '{}' is clickup-tasks but has no '{}' entry in \
                 clickup_custom_field_map; add the ClickUp text custom field id to \
                 persist relations",
                type_name,
                CLICKUP_RELATIONS_FIELD
            )
        })?
        .to_string();

    let doc_id = crate::engine::store::extract_id_from_name(
        doc_path.file_stem().and_then(|s| s.to_str()).unwrap_or(""),
    );

    let task_map = TaskMap::load(root)?;
    let task_id = task_map
        .get(&doc_id)
        .map(|e| e.task_id.clone())
        .ok_or_else(|| {
            anyhow!(
                "{} is not mapped to a ClickUp task; run `lazyspec fetch` before linking",
                doc_id
            )
        })?;

    // Full replace: serialize the doc's complete relation set from the cache
    // (mirrored by the caller just above), not a diff of the single edge.
    let content = std::fs::read_to_string(root.join(doc_path))?;
    let meta = DocMeta::parse(&content)?;
    let value = clickup_cache::encode_relations_block(&meta.related);

    let token = token_loader()?.ok_or_else(|| {
        anyhow!(
            "no ClickUp token found; run `lazyspec setup clickup` before linking \
             clickup-tasks documents"
        )
    })?;

    let client = clickup_factory();
    client.set_custom_field(token.expose(), &task_id, &field_id, &value)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn push_if_github_backed<G: GhIssueReader + GhIssueWriter + GhGraphql + Send + 'static>(
    root: &Path,
    doc_path: &Path,
    config: Option<&Config>,
    client_factory: impl FnOnce() -> G,
    native_edge: bool,
    rel_str: &str,
    target_id: &str,
    set: bool,
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
        client: Box::new(client_factory()),
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
        // Ordinary relations round-trip through the issue body: merge just this
        // edge into the remote body (no whole-cache clobber, no optimistic lock).
        gh_store.merge_relation_to_remote(type_def, &doc_id, rel_str, target_id, set)
    }
}

fn push_if_git_ref_backed(
    root: &Path,
    doc_path: &Path,
    config: Option<&Config>,
) -> Result<PushOutcome> {
    let config = match config {
        Some(c) => c,
        None => return Ok(PushOutcome::Synced),
    };

    if !doc_path.starts_with(".lazyspec/cache/") {
        return Ok(PushOutcome::Synced);
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
        return Ok(PushOutcome::Synced);
    }

    let doc_id = crate::engine::store::extract_id_from_name(
        doc_path.file_stem().and_then(|s| s.to_str()).unwrap_or(""),
    );

    let mut git_store = GitRefStore {
        git: Box::new(crate::engine::git_ref::GitCli),
        root: root.to_path_buf(),
        remote: config.git_ref.remote.clone(),
        config: config.clone(),
        reserved_number: None,
    };
    git_store.recommit_cache(type_def, &doc_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::cache_lock::CacheLock;
    use crate::engine::config::{Config, GithubConfig, NumberingStrategy, StoreBackend, TypeDef};
    use crate::engine::fs::RealFileSystem;
    use crate::engine::gh::{
        test_support::{MockGhClient, MockGhDependencyClient, MockGhMilestoneClient},
        GhIssue, GhLabel,
    };
    use crate::engine::git_ref::GitRefOps;
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
            label_override: None,
            github_issue_tag: None,
            github_issue_type: None,
            clickup_list_id: None,
            clickup_task_type: None,
            clickup_custom_field_map: None,
        };
        let mut config = Config::default();
        config.documents.types = vec![
            issue_type("story", "STORY", StoreBackend::GithubIssues),
            issue_type("story2", "STORY2", StoreBackend::GithubIssues),
            issue_type("milestone", "MILESTONE", StoreBackend::GithubMilestones),
            issue_type("spec", "SPEC", StoreBackend::Filesystem),
        ];
        config.documents.github = Some(GithubConfig {
            repo: Some("owner/repo".to_string()),
            cache_ttl: 60,
        });
        config.relationships = vec![
            crate::engine::config::RelationshipDef {
                name: "targets".to_string(),
                inverse: Some("targeted-by".to_string()),
                github_native: Some("milestone".to_string()),
                traversal: None,
            },
            crate::engine::config::RelationshipDef {
                name: "implements".to_string(),
                inverse: Some("implemented-by".to_string()),
                github_native: None,
                traversal: None,
            },
        ];
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
            MockGhDependencyClient::new,
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
            MockGhDependencyClient::new,
        )
        .unwrap();
        assert_eq!(*recorder2.last_set_milestone.borrow(), Some((7, None)));
    }

    // --- ITERATION-230: store-aware relation vocabulary for github-milestones ---

    // Seed STORY-7 (github-issues), STORY2-9 (github-issues), MILESTONE-3
    // (github-milestones) in the cache plus their issue-map numbers, so the
    // guard can resolve each endpoint's store. Returns a loaded store.
    fn seed_milestone_guard_fixture(root: &std::path::Path) -> Store {
        let config = milestone_assoc_config();
        write_cache_doc(
            &root.join(".lazyspec/cache/story"),
            "STORY-7.md",
            "My Story",
            "story",
        );
        write_cache_doc(
            &root.join(".lazyspec/cache/story2"),
            "STORY2-9.md",
            "Other Story",
            "story2",
        );
        write_cache_doc(
            &root.join(".lazyspec/cache/milestone"),
            "MILESTONE-3.md",
            "v1.0",
            "milestone",
        );

        let mut issue_map = IssueMap::load(root).unwrap();
        issue_map.insert("STORY-7", 7, "", "");
        issue_map.insert("STORY2-9", 9, "", "");
        issue_map.insert("MILESTONE-3", 3, "", "");
        issue_map.save(root).unwrap();

        Store::load(root, &config).unwrap()
    }

    // AC1: link STORY-7 --targets--> MILESTONE-3 is legal -- frontmatter carries
    // the relation and the native milestone PATCH is recorded.
    #[test]
    fn link_milestone_target_via_targets_ok() {
        let root = tmp_root("guard_targets_ok");
        let config = milestone_assoc_config();
        let store = seed_milestone_guard_fixture(&root);
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
            MockGhDependencyClient::new,
        )
        .expect("legal targets link must succeed");

        assert_eq!(*recorder.last_set_milestone.borrow(), Some((7, Some(3))));
        let updated =
            std::fs::read_to_string(root.join(".lazyspec/cache/story/STORY-7.md")).unwrap();
        assert!(
            updated.contains("targets: MILESTONE-3"),
            "frontmatter should carry the relation, got:\n{updated}"
        );
    }

    // Inverse flow: from a milestone doc, `MILESTONE-3 --targeted-by--> STORY-7`
    // flips to `STORY-7 targets MILESTONE-3` (link.rs:65) -- legal, since the
    // milestone is the TARGET. The edge lands on the issue's frontmatter, not the
    // milestone, and the native PATCH is recorded. This is the path the TUI link
    // editor takes when adding a relation while viewing a milestone.
    #[test]
    fn link_milestone_inverse_targeted_by_writes_on_issue() {
        let root = tmp_root("guard_inverse_targeted_by");
        let config = milestone_assoc_config();
        let store = seed_milestone_guard_fixture(&root);
        let fs = RealFileSystem;
        let recorder = std::rc::Rc::new(MockGhMilestoneClient::new());

        let outcome = link_inner(
            &root,
            &store,
            "MILESTONE-3",
            "targeted-by",
            "STORY-7",
            &fs,
            Some(&config),
            MockGhClient::new,
            || recorder.clone(),
            MockGhClient::new,
            MockGhDependencyClient::new,
        )
        .expect("inverse `targeted-by` from a milestone must succeed");

        // Direction flipped: the edge is the canonical `targets` written on the
        // issue, with the milestone as the target.
        assert_eq!(outcome.rel_type.to_string(), "targets");
        assert_eq!(outcome.target, "MILESTONE-3");
        assert!(outcome.source.ends_with("STORY-7.md"));

        assert_eq!(*recorder.last_set_milestone.borrow(), Some((7, Some(3))));
        let story = std::fs::read_to_string(root.join(".lazyspec/cache/story/STORY-7.md")).unwrap();
        assert!(
            story.contains("targets: MILESTONE-3"),
            "the edge lands on the issue, got:\n{story}"
        );
        let milestone =
            std::fs::read_to_string(root.join(".lazyspec/cache/milestone/MILESTONE-3.md")).unwrap();
        assert!(
            !milestone.contains("related"),
            "the milestone frontmatter must stay untouched, got:\n{milestone}"
        );
    }

    // AC2: link STORY-7 --implements--> MILESTONE-3 is rejected -- a milestone may
    // only be targeted by `targets`. No frontmatter write.
    #[test]
    fn link_milestone_via_ordinary_rel_rejected() {
        let root = tmp_root("guard_ordinary_rejected");
        let config = milestone_assoc_config();
        let store = seed_milestone_guard_fixture(&root);
        let fs = RealFileSystem;
        let recorder = std::rc::Rc::new(MockGhMilestoneClient::new());

        let err = link_inner(
            &root,
            &store,
            "STORY-7",
            "implements",
            "MILESTONE-3",
            &fs,
            Some(&config),
            MockGhClient::new,
            || recorder.clone(),
            MockGhClient::new,
            MockGhDependencyClient::new,
        )
        .expect_err("ordinary relation to a milestone must be rejected");
        assert!(
            err.to_string()
                .contains("milestone docs can only be targeted by `targets`"),
            "unexpected error: {err}"
        );

        // No native call and no frontmatter write.
        assert!(recorder.last_set_milestone.borrow().is_none());
        let updated =
            std::fs::read_to_string(root.join(".lazyspec/cache/story/STORY-7.md")).unwrap();
        assert!(
            !updated.contains("related") && !updated.contains("implements"),
            "cache must be unchanged after a rejected link, got:\n{updated}"
        );
    }

    // AC3: a milestone may never be a relation source -- even via `targets`.
    // link MILESTONE-3 --targets--> STORY-7 is rejected "cannot be the source".
    #[test]
    fn link_from_milestone_rejected() {
        let root = tmp_root("guard_from_milestone");
        let config = milestone_assoc_config();
        let store = seed_milestone_guard_fixture(&root);
        let fs = RealFileSystem;
        let recorder = std::rc::Rc::new(MockGhMilestoneClient::new());

        let err = link_inner(
            &root,
            &store,
            "MILESTONE-3",
            "targets",
            "STORY-7",
            &fs,
            Some(&config),
            MockGhClient::new,
            || recorder.clone(),
            MockGhClient::new,
            MockGhDependencyClient::new,
        )
        .expect_err("a milestone source must be rejected even via targets");
        assert!(
            err.to_string()
                .contains("milestone docs cannot be the source"),
            "unexpected error: {err}"
        );

        assert!(recorder.last_set_milestone.borrow().is_none());
        let updated =
            std::fs::read_to_string(root.join(".lazyspec/cache/milestone/MILESTONE-3.md")).unwrap();
        assert!(
            !updated.contains("related"),
            "milestone cache must be unchanged after a rejected link, got:\n{updated}"
        );
    }

    // AC4: `targets` to a non-milestone target is rejected.
    // link STORY-7 --targets--> STORY2-9 (both github-issues) is rejected.
    #[test]
    fn targets_to_non_milestone_rejected() {
        let root = tmp_root("guard_targets_non_ms");
        let config = milestone_assoc_config();
        let store = seed_milestone_guard_fixture(&root);
        let fs = RealFileSystem;
        let recorder = std::rc::Rc::new(MockGhMilestoneClient::new());

        let err = link_inner(
            &root,
            &store,
            "STORY-7",
            "targets",
            "STORY2-9",
            &fs,
            Some(&config),
            MockGhClient::new,
            || recorder.clone(),
            MockGhClient::new,
            MockGhDependencyClient::new,
        )
        .expect_err("targets to a non-milestone target must be rejected");
        assert!(
            err.to_string()
                .contains("`targets` requires a milestone target"),
            "unexpected error: {err}"
        );

        assert!(recorder.last_set_milestone.borrow().is_none());
        let updated =
            std::fs::read_to_string(root.join(".lazyspec/cache/story/STORY-7.md")).unwrap();
        assert!(
            !updated.contains("related"),
            "cache must be unchanged after a rejected link, got:\n{updated}"
        );
    }

    // A milestone-native relation may only originate from a github-issues doc.
    // A filesystem doc (here a spec) targeting a milestone is rejected at
    // validate_milestone_relation, before any native PATCH.
    #[test]
    fn targets_from_non_issue_source_rejected() {
        let root = tmp_root("guard_non_issue_source");
        let config = milestone_assoc_config();
        write_cache_doc(
            &root.join(".lazyspec/cache/spec"),
            "SPEC-1.md",
            "A Spec",
            "spec",
        );
        write_cache_doc(
            &root.join(".lazyspec/cache/milestone"),
            "MILESTONE-3.md",
            "v1.0",
            "milestone",
        );
        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("MILESTONE-3", 3, "", "");
        issue_map.save(&root).unwrap();
        let store = Store::load(&root, &config).unwrap();

        let err = validate_milestone_relation(&config, &store, "SPEC-1", "MILESTONE-3", "targets")
            .expect_err("a non-issue source must be rejected for a milestone-native relation");
        assert!(
            err.to_string().contains("SPEC-1"),
            "error should name the source, got: {err}"
        );
        assert!(
            err.to_string().contains("github-issues"),
            "error should explain only github-issues docs can target a milestone, got: {err}"
        );
    }

    // A github-issues source targeting a milestone passes validation.
    #[test]
    fn targets_from_issue_source_ok() {
        let root = tmp_root("guard_issue_source_ok");
        let config = milestone_assoc_config();
        let store = seed_milestone_guard_fixture(&root);

        validate_milestone_relation(&config, &store, "STORY-7", "MILESTONE-3", "targets")
            .expect("a github-issues source targeting a milestone must validate");
    }

    // AC5: unlink honours the same store guard. An illegal unlink from a
    // milestone source is rejected pre-retain (no native call); a legal unlink
    // STORY-7 targets MILESTONE-3 still records the native clear and drops the
    // cache relation.
    #[test]
    fn unlink_honours_store_guard() {
        let root = tmp_root("guard_unlink");
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
        issue_map.insert("STORY-7", 7, "", "");
        issue_map.insert("MILESTONE-3", 3, "", "");
        issue_map.save(&root).unwrap();

        let fs = RealFileSystem;

        // Illegal: a milestone source is rejected before any native clear.
        let bad = std::rc::Rc::new(MockGhMilestoneClient::new());
        let store = Store::load(&root, &config).unwrap();
        let err = unlink_inner(
            &root,
            &store,
            "MILESTONE-3",
            "targets",
            "STORY-7",
            &fs,
            Some(&config),
            MockGhClient::new,
            || bad.clone(),
            MockGhClient::new,
            MockGhDependencyClient::new,
        )
        .expect_err("unlink from a milestone source must be rejected");
        assert!(
            err.to_string()
                .contains("milestone docs cannot be the source"),
            "unexpected error: {err}"
        );
        assert!(bad.last_set_milestone.borrow().is_none());

        // Legal: STORY-7 targets MILESTONE-3 unlink records the native clear and
        // drops the cache relation.
        let recorder = std::rc::Rc::new(MockGhMilestoneClient::new());
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
            || recorder.clone(),
            MockGhClient::new,
            MockGhDependencyClient::new,
        )
        .expect("legal unlink must succeed");
        assert_eq!(*recorder.last_set_milestone.borrow(), Some((7, None)));
        let updated =
            std::fs::read_to_string(root.join(".lazyspec/cache/story/STORY-7.md")).unwrap();
        assert!(
            !updated.contains("targets: MILESTONE-3"),
            "cache relation should be removed, got:\n{updated}"
        );
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
            assignees: vec![],
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
            MockGhDependencyClient::new,
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
            MockGhDependencyClient::new,
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
            label_override: None,
            github_issue_tag: None,
            github_issue_type: None,
            clickup_list_id: None,
            clickup_task_type: None,
            clickup_custom_field_map: None,
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
            traversal: None,
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
            MockGhDependencyClient::new,
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
            MockGhDependencyClient::new,
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
            MockGhDependencyClient::new,
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
            MockGhDependencyClient::new,
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
            MockGhDependencyClient::new,
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
            label_override: None,
            github_issue_tag: None,
            github_issue_type: None,
            clickup_list_id: None,
            clickup_task_type: None,
            clickup_custom_field_map: None,
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
            label_override: None,
            github_issue_tag: None,
            github_issue_type: None,
            clickup_list_id: None,
            clickup_task_type: None,
            clickup_custom_field_map: None,
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
            assignees: vec![],
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
            MockGhDependencyClient::new,
        )
        .unwrap();

        // Re-read the file to check the frontmatter was rewritten with the link
        let updated = std::fs::read_to_string(rfc_cache.join("RFC-001-my-rfc.md")).unwrap();
        assert!(
            updated.contains("implements: STORY-001"),
            "frontmatter should contain the new link, got:\n{}",
            updated
        );

        // Verify the ordinary relation push ran by checking the issue map.
        // merge_relation_to_remote records the remote's current updated_at
        // (rather than clearing it like the old push_cache path).
        let refreshed_map = IssueMap::load(&root).unwrap();
        let entry = refreshed_map.get("RFC-001").unwrap();
        assert_eq!(
            entry.updated_at, "2026-03-27T10:00:00Z",
            "updated_at should record the remote timestamp after the relation merge"
        );
    }

    // An RFC-shaped remote issue carrying a given updated_at and body, used to
    // seed the ordinary-relation merge path (mirrors `view_issue_at` but with
    // the rfc label so `deserialize` reconstructs the rfc type).
    fn rfc_view_issue(number: u64, updated_at: &str, body: &str) -> GhIssue {
        GhIssue {
            number,
            id: format!("I_node{number}"),
            url: String::new(),
            title: "My RFC".to_string(),
            body: body.to_string(),
            labels: vec![GhLabel {
                name: "lazyspec:rfc".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: updated_at.to_string(),
            created_at: "2026-06-26T09:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        }
    }

    fn seed_ordinary_cache(root: &std::path::Path, related: Option<&str>) {
        let rfc_cache = root.join(".lazyspec/cache/rfc");
        let story_cache = root.join(".lazyspec/cache/story");
        std::fs::create_dir_all(&rfc_cache).unwrap();
        std::fs::create_dir_all(&story_cache).unwrap();
        let related_block = related.unwrap_or("");
        let rfc_content = format!(
            "---\ntitle: My RFC\ntype: rfc\nstatus: draft\nauthor: agent-7\ndate: 2026-03-27\ntags: []\n{related_block}---\nRFC body text.\n"
        );
        std::fs::write(rfc_cache.join("RFC-001-my-rfc.md"), rfc_content).unwrap();
        std::fs::write(
            story_cache.join("STORY-001-my-story.md"),
            "---\ntitle: My Story\ntype: story\nstatus: draft\nauthor: agent-7\ndate: 2026-03-27\ntags: []\n---\nStory body.\n",
        )
        .unwrap();
    }

    // AC1: an ordinary (non-native) relation link must survive an out-of-band
    // remote `updated_at` bump -- no "modified on GitHub" abort -- because the
    // relation merge bypasses the optimistic body lock. The cache records the
    // relation and the issue-map baseline reconciles to the remote timestamp.
    #[test]
    fn link_ordinary_relation_survives_out_of_band_updated_at() {
        let root = tmp_root("link_ordinary_oob");
        let config = gh_config_with_rfc_type();
        seed_ordinary_cache(&root, None);

        let mut issue_map = IssueMap::load(&root).unwrap();
        // STALE local baseline; remote has since advanced.
        issue_map.insert("RFC-001", 42, "2026-06-26T10:00:00Z", "I_node42");
        issue_map.save(&root).unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;
        let remote_body = make_issue_body("agent-7", "2026-03-27", "RFC body text.");

        link_inner(
            &root,
            &store,
            "RFC-001",
            "implements",
            "STORY-001",
            &fs,
            Some(&config),
            || {
                MockGhClient::new().with_view_issue(rfc_view_issue(
                    42,
                    "2026-06-26T11:00:00Z",
                    &remote_body,
                ))
            },
            MockGhMilestoneClient::new,
            MockGhClient::new,
            MockGhDependencyClient::new,
        )
        .expect("ordinary relation link must not abort on an out-of-band updated_at bump");

        let updated =
            std::fs::read_to_string(root.join(".lazyspec/cache/rfc/RFC-001-my-rfc.md")).unwrap();
        assert!(
            updated.contains("implements: STORY-001"),
            "cache should carry the relation, got:\n{updated}"
        );

        let reloaded = IssueMap::load(&root).unwrap();
        assert_eq!(
            reloaded.get("RFC-001").unwrap().updated_at,
            "2026-06-26T11:00:00Z",
            "merge should record the remote's current updated_at"
        );
    }

    // AC2: remote prose (and existing remote relations) survive a relation add --
    // the merge re-serializes the REMOTE body, not the local cache.
    #[test]
    fn link_ordinary_relation_preserves_remote_prose() {
        let root = tmp_root("link_ordinary_prose");
        let config = gh_config_with_rfc_type();
        seed_ordinary_cache(&root, None);

        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");
        issue_map.save(&root).unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;
        let remote_body = make_issue_body("agent-7", "2026-03-27", "REMOTE PROSE LINE");

        let recorder = std::sync::Arc::new(std::sync::Mutex::new(
            MockGhClient::new().with_view_issue(rfc_view_issue(
                42,
                "2026-03-27T10:00:00Z",
                &remote_body,
            )),
        ));

        link_inner(
            &root,
            &store,
            "RFC-001",
            "implements",
            "STORY-001",
            &fs,
            Some(&config),
            || recorder.clone(),
            MockGhMilestoneClient::new,
            MockGhClient::new,
            MockGhDependencyClient::new,
        )
        .unwrap();

        let guard = recorder.lock().unwrap();
        let pushed = guard.last_edit_body.borrow();
        let pushed = pushed.as_ref().expect("issue_edit should have been called");
        assert!(
            pushed.contains("REMOTE PROSE LINE"),
            "merged body must preserve remote prose, got:\n{pushed}"
        );
        assert!(
            pushed.contains("- implements: STORY-001"),
            "merged body must carry the new relation, got:\n{pushed}"
        );
    }

    // AC3: double-link is idempotent -- exactly one relation in the cache, and the
    // second call performs no issue_edit (the relation already exists on remote).
    #[test]
    fn link_ordinary_relation_double_link_idempotent() {
        let root = tmp_root("link_ordinary_double");
        let config = gh_config_with_rfc_type();
        seed_ordinary_cache(&root, None);

        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");
        issue_map.save(&root).unwrap();

        let fs = RealFileSystem;
        let first_body = make_issue_body("agent-7", "2026-03-27", "RFC body text.");

        let store = Store::load(&root, &config).unwrap();
        link_inner(
            &root,
            &store,
            "RFC-001",
            "implements",
            "STORY-001",
            &fs,
            Some(&config),
            || {
                MockGhClient::new().with_view_issue(rfc_view_issue(
                    42,
                    "2026-03-27T10:00:00Z",
                    &first_body,
                ))
            },
            MockGhMilestoneClient::new,
            MockGhClient::new,
            MockGhDependencyClient::new,
        )
        .unwrap();

        // Second call: remote already carries the relation, so dedup short-circuits.
        let second_body = {
            let doc = crate::engine::document::DocMeta {
                path: std::path::PathBuf::new(),
                title: "My RFC".to_string(),
                doc_type: crate::engine::document::DocType::new("rfc"),
                status: crate::engine::document::Status::new("draft"),
                author: "agent-7".to_string(),
                date: chrono::NaiveDate::from_ymd_opt(2026, 3, 27).unwrap(),
                tags: vec![],
                provenance: vec![],
                related: vec![crate::engine::document::Relation {
                    rel_type: RelationType::new("implements"),
                    target: "STORY-001".to_string(),
                }],
                validate_ignore: false,
                virtual_doc: false,
                assignee: None,
                attributes: Default::default(),
                id: "RFC-001".to_string(),
            };
            crate::engine::issue_body::serialize(&doc, "RFC body text.")
        };

        let recorder = std::sync::Arc::new(std::sync::Mutex::new(
            MockGhClient::new().with_view_issue(rfc_view_issue(
                42,
                "2026-03-27T10:00:00Z",
                &second_body,
            )),
        ));
        let store = Store::load(&root, &config).unwrap();
        link_inner(
            &root,
            &store,
            "RFC-001",
            "implements",
            "STORY-001",
            &fs,
            Some(&config),
            || recorder.clone(),
            MockGhMilestoneClient::new,
            MockGhClient::new,
            MockGhDependencyClient::new,
        )
        .unwrap();

        // No issue_edit recorded on the second call: dedup short-circuited.
        assert!(
            recorder.lock().unwrap().last_edit_body.borrow().is_none(),
            "second link must record no issue_edit, got: {:?}",
            recorder.lock().unwrap().last_edit_body.borrow()
        );

        // Exactly one relation in the cache.
        let updated =
            std::fs::read_to_string(root.join(".lazyspec/cache/rfc/RFC-001-my-rfc.md")).unwrap();
        let count = updated.matches("implements: STORY-001").count();
        assert_eq!(
            count, 1,
            "cache must carry exactly one relation, got:\n{updated}"
        );
    }

    // AC4: a failed push (issue_edit Err) leaves the cache unchanged -- push-first
    // ordering means the cache is only touched after a successful remote write.
    #[test]
    fn link_ordinary_relation_failed_push_leaves_cache_unchanged() {
        let root = tmp_root("link_ordinary_fail");
        let config = gh_config_with_rfc_type();
        seed_ordinary_cache(&root, None);

        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("RFC-001", 42, "2026-03-27T10:00:00Z", "I_node42");
        issue_map.save(&root).unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;
        let remote_body = make_issue_body("agent-7", "2026-03-27", "RFC body text.");

        let result = link_inner(
            &root,
            &store,
            "RFC-001",
            "implements",
            "STORY-001",
            &fs,
            Some(&config),
            || {
                MockGhClient::new()
                    .with_view_issue(rfc_view_issue(42, "2026-03-27T10:00:00Z", &remote_body))
                    .with_edit_fail()
            },
            MockGhMilestoneClient::new,
            MockGhClient::new,
            MockGhDependencyClient::new,
        );

        assert!(result.is_err(), "failed push must propagate an error");

        let updated =
            std::fs::read_to_string(root.join(".lazyspec/cache/rfc/RFC-001-my-rfc.md")).unwrap();
        assert!(
            !updated.contains("implements: STORY-001"),
            "cache must be unchanged after a failed push, got:\n{updated}"
        );
    }

    // AC6: ordinary unlink is symmetric -- survives an out-of-band updated_at bump,
    // retain-drops the edge on the remote body while keeping remote prose, and
    // removes the relation from the cache.
    #[test]
    fn unlink_ordinary_relation_survives_out_of_band_updated_at() {
        let root = tmp_root("unlink_ordinary_oob");
        let config = gh_config_with_rfc_type();
        // Cache already carries the relation.
        seed_ordinary_cache(&root, Some("related:\n- implements: STORY-001\n"));

        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("RFC-001", 42, "2026-06-26T10:00:00Z", "I_node42");
        issue_map.save(&root).unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        // Remote body carries the relation AND prose to preserve.
        let remote_body = {
            let doc = crate::engine::document::DocMeta {
                path: std::path::PathBuf::new(),
                title: "My RFC".to_string(),
                doc_type: crate::engine::document::DocType::new("rfc"),
                status: crate::engine::document::Status::new("draft"),
                author: "agent-7".to_string(),
                date: chrono::NaiveDate::from_ymd_opt(2026, 3, 27).unwrap(),
                tags: vec![],
                provenance: vec![],
                related: vec![crate::engine::document::Relation {
                    rel_type: RelationType::new("implements"),
                    target: "STORY-001".to_string(),
                }],
                validate_ignore: false,
                virtual_doc: false,
                assignee: None,
                attributes: Default::default(),
                id: "RFC-001".to_string(),
            };
            crate::engine::issue_body::serialize(&doc, "REMOTE PROSE LINE")
        };

        let recorder = std::sync::Arc::new(std::sync::Mutex::new(
            MockGhClient::new().with_view_issue(rfc_view_issue(
                42,
                "2026-06-26T11:00:00Z",
                &remote_body,
            )),
        ));

        unlink_inner(
            &root,
            &store,
            "RFC-001",
            "implements",
            "STORY-001",
            &fs,
            Some(&config),
            || recorder.clone(),
            MockGhMilestoneClient::new,
            MockGhClient::new,
            MockGhDependencyClient::new,
        )
        .expect("ordinary unlink must not abort on an out-of-band updated_at bump");

        let guard = recorder.lock().unwrap();
        let pushed = guard.last_edit_body.borrow();
        let pushed = pushed.as_ref().expect("issue_edit should have been called");
        assert!(
            !pushed.contains("- implements: STORY-001"),
            "merged body must drop the relation, got:\n{pushed}"
        );
        assert!(
            pushed.contains("REMOTE PROSE LINE"),
            "merged body must preserve remote prose, got:\n{pushed}"
        );

        let updated =
            std::fs::read_to_string(root.join(".lazyspec/cache/rfc/RFC-001-my-rfc.md")).unwrap();
        assert!(
            !updated.contains("implements: STORY-001"),
            "cache relation should be removed, got:\n{updated}"
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
            label_override: None,
            github_issue_tag: None,
            github_issue_type: None,
            clickup_list_id: None,
            clickup_task_type: None,
            clickup_custom_field_map: None,
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
            label_override: None,
            github_issue_tag: None,
            github_issue_type: None,
            clickup_list_id: None,
            clickup_task_type: None,
            clickup_custom_field_map: None,
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
            MockGhDependencyClient::new,
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
            MockGhDependencyClient::new,
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

    // --- ITERATION-278: persist clickup relations via the link custom-field write ---

    use crate::engine::clickup::{ClickupUser, FakeClickupClient};
    use crate::engine::config::CLICKUP_RELATIONS_FIELD;
    use crate::engine::credentials::Token;

    fn clickup_user() -> ClickupUser {
        ClickupUser {
            id: 1,
            username: "jack".to_string(),
            email: "jack@example.com".to_string(),
        }
    }

    /// A config with one clickup-tasks type (`task`, prefix `TASK`) whose custom
    /// field map names the reserved relations key to a text field uuid.
    fn clickup_rel_config(with_field_map: bool) -> Config {
        let mut td = TypeDef::test_fixture("task", StoreBackend::ClickupTasks);
        td.prefix = "TASK".to_string();
        td.clickup_list_id = Some("list123".to_string());
        if with_field_map {
            let mut map = std::collections::HashMap::new();
            map.insert(CLICKUP_RELATIONS_FIELD.to_string(), "uuid-rel".to_string());
            td.clickup_custom_field_map = Some(map);
        }
        let mut config = Config::default();
        config.documents.types = vec![td];
        config
    }

    /// Write a clickup cache doc carrying a `related:` block, mirroring the state
    /// `link_inner` leaves before the field push runs.
    fn write_clickup_cache_doc(root: &std::path::Path, doc_id: &str, related: &[(&str, &str)]) {
        let dir = root.join(".lazyspec/cache/task");
        std::fs::create_dir_all(&dir).unwrap();
        let mut content = String::from(
            "---\ntitle: A task\ntype: task\nstatus: open\nauthor: clickup\ndate: 2026-03-27\ntags: []\n",
        );
        if !related.is_empty() {
            content.push_str("related:\n");
            for (rel, target) in related {
                content.push_str(&format!("- {}: {}\n", rel, target));
            }
        }
        content.push_str("---\nbody\n");
        std::fs::write(dir.join(format!("{}.md", doc_id)), content).unwrap();
    }

    fn seed_task_map(root: &std::path::Path, doc_id: &str, task_id: &str) {
        let mut map = TaskMap::load(root).unwrap();
        map.insert(doc_id, task_id, "1774587145901");
        map.save(root).unwrap();
    }

    // AC1: a clickup-tasks doc's relations persist by serializing the full set
    // into the configured text custom field via the ClickUp API (fake records
    // the set), targeting the field id resolved from clickup_custom_field_map.
    #[test]
    fn push_clickup_writes_serialized_relations_to_configured_field() {
        let root = tmp_root("clickup_link_set");
        let config = clickup_rel_config(true);
        write_clickup_cache_doc(&root, "TASK-1", &[("implements", "RFC-056")]);
        seed_task_map(&root, "TASK-1", "task-a");

        let client = FakeClickupClient::valid(clickup_user());
        let calls = client.set_field_calls();
        let doc_path = std::path::Path::new(".lazyspec/cache/task/TASK-1.md");

        push_if_clickup_backed(
            &root,
            doc_path,
            Some(&config),
            || client,
            || Ok(Some(Token::new("pk_test"))),
        )
        .unwrap();

        let recorded = calls.borrow();
        assert_eq!(recorded.len(), 1);
        // Targets the field id resolved via clickup_field_id (name -> uuid).
        assert_eq!(recorded[0].0, "task-a");
        assert_eq!(recorded[0].1, "uuid-rel");
        // The serialized YAML relations block, the same shape 275 parses.
        assert_eq!(recorded[0].2, "- implements: RFC-056");
    }

    // ROUND-TRIP: the block 278 writes decodes back to the original relations via
    // the 275 read direction (clickup_cache::task_to_doc over the custom field).
    #[test]
    fn clickup_relation_write_round_trips_through_read_decode() {
        let root = tmp_root("clickup_link_roundtrip");
        let config = clickup_rel_config(true);
        write_clickup_cache_doc(
            &root,
            "TASK-1",
            &[("implements", "RFC-056"), ("blocks", "RFC-010")],
        );
        seed_task_map(&root, "TASK-1", "task-a");

        let client = FakeClickupClient::valid(clickup_user());
        let calls = client.set_field_calls();
        let doc_path = std::path::Path::new(".lazyspec/cache/task/TASK-1.md");
        push_if_clickup_backed(
            &root,
            doc_path,
            Some(&config),
            || client,
            || Ok(Some(Token::new("pk_test"))),
        )
        .unwrap();

        let written_block = calls.borrow()[0].2.clone();

        // Feed the written block back through the read path: a task whose relations
        // field holds exactly what 278 wrote must decode to the original relations.
        let td = &config.documents.types[0];
        let task_json = serde_json::json!({
            "id": "task-a",
            "name": "A task",
            "status": {"status": "open"},
            "custom_fields": [
                {"id": "uuid-rel", "name": "relations", "value": written_block}
            ]
        });
        let task: crate::engine::clickup::ClickupTask = serde_json::from_value(task_json).unwrap();
        let (meta, _) = crate::engine::clickup_cache::task_to_doc(&task, td, "TASK-1");

        assert_eq!(meta.related.len(), 2);
        assert_eq!(meta.related[0].rel_type, RelationType::new("implements"));
        assert_eq!(meta.related[0].target, "RFC-056");
        assert_eq!(meta.related[1].rel_type, RelationType::new("blocks"));
        assert_eq!(meta.related[1].target, "RFC-010");
    }

    // Unlink is the same full-replace write: an emptied cache `related` set
    // serializes to the empty string, clearing the field on ClickUp.
    #[test]
    fn push_clickup_clears_field_when_no_relations_remain() {
        let root = tmp_root("clickup_unlink_clear");
        let config = clickup_rel_config(true);
        write_clickup_cache_doc(&root, "TASK-1", &[]);
        seed_task_map(&root, "TASK-1", "task-a");

        let client = FakeClickupClient::valid(clickup_user());
        let calls = client.set_field_calls();
        let doc_path = std::path::Path::new(".lazyspec/cache/task/TASK-1.md");
        push_if_clickup_backed(
            &root,
            doc_path,
            Some(&config),
            || client,
            || Ok(Some(Token::new("pk_test"))),
        )
        .unwrap();

        let recorded = calls.borrow();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].2, "");
    }

    // A clickup-tasks type with no relations entry in its custom-field map raises
    // a clear config error up front, not a mid-write failure -- and never calls
    // the client.
    #[test]
    fn push_clickup_missing_field_map_entry_errors() {
        let root = tmp_root("clickup_link_nofieldmap");
        let config = clickup_rel_config(false);
        write_clickup_cache_doc(&root, "TASK-1", &[("implements", "RFC-056")]);
        seed_task_map(&root, "TASK-1", "task-a");

        let client = FakeClickupClient::valid(clickup_user());
        let calls = client.set_field_calls();
        let doc_path = std::path::Path::new(".lazyspec/cache/task/TASK-1.md");
        let err = push_if_clickup_backed(
            &root,
            doc_path,
            Some(&config),
            || client,
            || Ok(Some(Token::new("pk_test"))),
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("clickup_custom_field_map"),
            "got: {err}"
        );
        assert!(
            calls.borrow().is_empty(),
            "no field write on a config error"
        );
    }

    // A missing token surfaces a clear `setup clickup` error, not a transport
    // failure, and performs no field write.
    #[test]
    fn push_clickup_missing_token_errors() {
        let root = tmp_root("clickup_link_notoken");
        let config = clickup_rel_config(true);
        write_clickup_cache_doc(&root, "TASK-1", &[("implements", "RFC-056")]);
        seed_task_map(&root, "TASK-1", "task-a");

        let client = FakeClickupClient::valid(clickup_user());
        let calls = client.set_field_calls();
        let doc_path = std::path::Path::new(".lazyspec/cache/task/TASK-1.md");
        let err = push_if_clickup_backed(&root, doc_path, Some(&config), || client, || Ok(None))
            .unwrap_err();

        assert!(err.to_string().contains("setup clickup"), "got: {err}");
        assert!(calls.borrow().is_empty(), "no field write without a token");
    }

    // A doc not mapped to a ClickUp task cannot be persisted; the error points at
    // `fetch` and no field write fires.
    #[test]
    fn push_clickup_unmapped_doc_errors() {
        let root = tmp_root("clickup_link_unmapped");
        let config = clickup_rel_config(true);
        write_clickup_cache_doc(&root, "TASK-1", &[("implements", "RFC-056")]);
        // No task map entry seeded.

        let client = FakeClickupClient::valid(clickup_user());
        let calls = client.set_field_calls();
        let doc_path = std::path::Path::new(".lazyspec/cache/task/TASK-1.md");
        let err = push_if_clickup_backed(
            &root,
            doc_path,
            Some(&config),
            || client,
            || Ok(Some(Token::new("pk_test"))),
        )
        .unwrap_err();

        assert!(err.to_string().contains("not mapped"), "got: {err}");
        assert!(calls.borrow().is_empty());
    }

    // A non-clickup type is a no-op: the field write never fires and the token
    // loader is never consulted (an ordinary link never touches the keychain).
    #[test]
    fn push_clickup_skips_non_clickup_type() {
        let root = tmp_root("clickup_link_skip_type");
        let config = gh_config_with_rfc_type();
        let client = FakeClickupClient::valid(clickup_user());
        let calls = client.set_field_calls();
        let doc_path = std::path::Path::new(".lazyspec/cache/rfc/RFC-001-my-rfc.md");
        push_if_clickup_backed(
            &root,
            doc_path,
            Some(&config),
            || client,
            || panic!("token loader must not run for a non-clickup doc"),
        )
        .unwrap();
        assert!(calls.borrow().is_empty());
    }

    // AUDIT-018 C2 / STORY-210 AC1: a doc whose frontmatter carries a bare
    // `related:` (YAML null, common after a manual edit) must accept a link --
    // the null is coerced to a sequence instead of panicking on
    // `as_sequence_mut().unwrap()`.
    #[test]
    fn link_with_bare_null_related_coerces_to_sequence() {
        let root = tmp_root("link_bare_null_related");
        let config = milestone_assoc_config();

        let dir = root.join("docs/spec");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SPEC-1.md"),
            "---\ntitle: A\ntype: spec\nstatus: draft\nauthor: a\ndate: 2026-03-27\ntags: []\nrelated:\n---\nbody\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("SPEC-2.md"),
            "---\ntitle: B\ntype: spec\nstatus: draft\nauthor: a\ndate: 2026-03-27\ntags: []\n---\nbody\n",
        )
        .unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;

        link_inner(
            &root,
            &store,
            "SPEC-1",
            "implements",
            "SPEC-2",
            &fs,
            Some(&config),
            MockGhClient::new,
            MockGhMilestoneClient::new,
            MockGhClient::new,
            MockGhDependencyClient::new,
        )
        .expect("link into a doc with bare `related:` must succeed");

        let updated = std::fs::read_to_string(dir.join("SPEC-1.md")).unwrap();
        assert!(
            updated.contains("implements: SPEC-2"),
            "relation should be written, got:\n{updated}"
        );
        // The rewritten doc must parse (related is a real sequence now).
        let meta = DocMeta::parse(&updated).unwrap();
        assert_eq!(meta.related.len(), 1);
    }

    #[test]
    fn push_clickup_skips_non_cache_path() {
        let root = tmp_root("clickup_link_skip_noncache");
        let config = clickup_rel_config(true);
        let client = FakeClickupClient::valid(clickup_user());
        let calls = client.set_field_calls();
        let doc_path = std::path::Path::new("docs/task/TASK-1.md");
        push_if_clickup_backed(
            &root,
            doc_path,
            Some(&config),
            || client,
            || panic!("token loader must not run for a non-cache doc"),
        )
        .unwrap();
        assert!(calls.borrow().is_empty());
    }

    // --- github_native = "dependency" (STORY-244 / ITERATION-345) ---

    // Two github-issues types and one filesystem type, with `blocks` bound to the
    // native dependency edge and an ordinary `implements` alongside it (to prove
    // the guard only fires for the dependency relation).
    fn dependency_config() -> Config {
        let issue = |name: &str, store: StoreBackend| TypeDef::test_fixture(name, store);
        let mut config = Config::default();
        config.documents.types = vec![
            issue("story", StoreBackend::GithubIssues),
            issue("spec", StoreBackend::Filesystem),
        ];
        config.documents.github = Some(GithubConfig {
            repo: Some("owner/repo".to_string()),
            cache_ttl: 60,
        });
        config.relationships = vec![
            crate::engine::config::RelationshipDef {
                name: "blocks".to_string(),
                inverse: Some("blocked-by".to_string()),
                github_native: Some("dependency".to_string()),
                traversal: None,
            },
            crate::engine::config::RelationshipDef {
                name: "implements".to_string(),
                inverse: Some("implemented-by".to_string()),
                github_native: None,
                traversal: None,
            },
        ];
        config
    }

    // AC1: linking two same-repo github-issues docs via a
    // github_native="dependency" relation writes the native blocked-by edge on
    // the TARGET issue (`A blocks B` => B blocked_by A), records the relation, and
    // fires the resync (proven by the issue-map baseline reconciling to the
    // remote's fresh updated_at, the milestone/membership resync signature).
    #[test]
    fn link_dependency_writes_native_blocked_by_and_resyncs() {
        let root = tmp_root("link_dependency");
        let config = dependency_config();
        write_cache_doc(
            &root.join(".lazyspec/cache/story"),
            "STORY-7.md",
            "My Story",
            "story",
        );
        write_cache_doc(
            &root.join(".lazyspec/cache/story"),
            "STORY-8.md",
            "Blocked",
            "story",
        );

        // STORY-7 last fetched at 10:00; remote (returned by the resync view) 11:00.
        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("STORY-7", 7, "2026-06-26T10:00:00Z", "I_node7");
        issue_map.insert("STORY-8", 8, "", "I_node8");
        issue_map.save(&root).unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;
        let recorder = std::rc::Rc::new(MockGhDependencyClient::new());

        link_inner(
            &root,
            &store,
            "STORY-7",
            "blocks",
            "STORY-8",
            &fs,
            Some(&config),
            || MockGhClient::new().with_view_issue(view_issue_at(7, "2026-06-26T11:00:00Z")),
            MockGhMilestoneClient::new,
            MockGhClient::new,
            || recorder.clone(),
        )
        .expect("native dependency link must succeed");

        // Native blocked_by edge on the target (8), blocked by the source (7).
        assert_eq!(*recorder.added.borrow(), vec![(8, 7)]);
        assert!(
            recorder.removed.borrow().is_empty(),
            "link must not remove any edge"
        );

        // The relation is recorded on the source frontmatter.
        let updated =
            std::fs::read_to_string(root.join(".lazyspec/cache/story/STORY-7.md")).unwrap();
        assert!(
            updated.contains("blocks: STORY-8"),
            "frontmatter should carry the relation, got:\n{updated}"
        );

        // Resync fired: the baseline reconciled to the remote's fresh timestamp.
        let reloaded = IssueMap::load(&root).unwrap();
        assert_eq!(
            reloaded.get("STORY-7").unwrap().updated_at,
            "2026-06-26T11:00:00Z",
            "resync_after_native_edge should record the remote's current updated_at"
        );
    }

    // AC2: unlink removes the native blocked-by edge (same direction as the add)
    // and drops the relation from the cache frontmatter.
    #[test]
    fn unlink_dependency_removes_native_blocked_by() {
        let root = tmp_root("unlink_dependency");
        let config = dependency_config();

        // STORY-7 already blocks STORY-8 in the cache.
        std::fs::create_dir_all(root.join(".lazyspec/cache/story")).unwrap();
        let content = "---\ntitle: My Story\ntype: story\nstatus: draft\nauthor: a\ndate: 2026-03-27\ntags: []\nrelated:\n- blocks: STORY-8\n---\nbody\n";
        std::fs::write(root.join(".lazyspec/cache/story/STORY-7.md"), content).unwrap();
        write_cache_doc(
            &root.join(".lazyspec/cache/story"),
            "STORY-8.md",
            "Blocked",
            "story",
        );

        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("STORY-7", 7, "2026-06-26T10:00:00Z", "I_node7");
        issue_map.insert("STORY-8", 8, "", "I_node8");
        issue_map.save(&root).unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;
        let recorder = std::rc::Rc::new(MockGhDependencyClient::new());

        unlink_inner(
            &root,
            &store,
            "STORY-7",
            "blocks",
            "STORY-8",
            &fs,
            Some(&config),
            || MockGhClient::new().with_view_issue(view_issue_at(7, "2026-06-26T11:00:00Z")),
            MockGhMilestoneClient::new,
            MockGhClient::new,
            || recorder.clone(),
        )
        .expect("native dependency unlink must succeed");

        assert_eq!(*recorder.removed.borrow(), vec![(8, 7)]);
        assert!(
            recorder.added.borrow().is_empty(),
            "unlink must not add any edge"
        );

        let updated =
            std::fs::read_to_string(root.join(".lazyspec/cache/story/STORY-7.md")).unwrap();
        assert!(
            !updated.contains("blocks: STORY-8"),
            "cache relation should be removed, got:\n{updated}"
        );
    }

    // AC5 regression: a filesystem-only `blocks` link/unlink makes ZERO native
    // calls -- the opportunistic guard sees non-issue endpoints and records the
    // relation comment/graph-backed exactly as before, with no error.
    #[test]
    fn link_dependency_filesystem_only_makes_no_native_call() {
        let root = tmp_root("link_dependency_fs");
        let config = dependency_config();

        let dir = root.join("docs/spec");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SPEC-1.md"),
            "---\ntitle: A\ntype: spec\nstatus: draft\nauthor: a\ndate: 2026-03-27\ntags: []\n---\nbody\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("SPEC-2.md"),
            "---\ntitle: B\ntype: spec\nstatus: draft\nauthor: a\ndate: 2026-03-27\ntags: []\n---\nbody\n",
        )
        .unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;
        let recorder = std::rc::Rc::new(MockGhDependencyClient::new());

        link_inner(
            &root,
            &store,
            "SPEC-1",
            "blocks",
            "SPEC-2",
            &fs,
            Some(&config),
            MockGhClient::new,
            MockGhMilestoneClient::new,
            MockGhClient::new,
            || recorder.clone(),
        )
        .expect("filesystem blocks link must succeed");

        // Relation recorded, but no native dependency call fired.
        let updated = std::fs::read_to_string(dir.join("SPEC-1.md")).unwrap();
        assert!(
            updated.contains("blocks: SPEC-2"),
            "filesystem relation should be recorded, got:\n{updated}"
        );
        assert!(
            recorder.added.borrow().is_empty() && recorder.removed.borrow().is_empty(),
            "no native call for a filesystem endpoint"
        );

        // Unlink is symmetric: still no native call.
        let store = Store::load(&root, &config).unwrap();
        let recorder2 = std::rc::Rc::new(MockGhDependencyClient::new());
        unlink_inner(
            &root,
            &store,
            "SPEC-1",
            "blocks",
            "SPEC-2",
            &fs,
            Some(&config),
            MockGhClient::new,
            MockGhMilestoneClient::new,
            MockGhClient::new,
            || recorder2.clone(),
        )
        .expect("filesystem blocks unlink must succeed");
        assert!(
            recorder2.added.borrow().is_empty() && recorder2.removed.borrow().is_empty(),
            "no native call for a filesystem endpoint on unlink"
        );
    }

    // The native edge is opportunistic on the RELATION too: an ordinary relation
    // (no github_native) between two github-issues docs makes no dependency call.
    #[test]
    fn apply_native_dependency_non_dependency_relation_returns_false() {
        let root = tmp_root("dep_guard_rel");
        let config = dependency_config();
        write_cache_doc(
            &root.join(".lazyspec/cache/story"),
            "STORY-7.md",
            "My Story",
            "story",
        );
        write_cache_doc(
            &root.join(".lazyspec/cache/story"),
            "STORY-8.md",
            "Other",
            "story",
        );
        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("STORY-7", 7, "", "I_node7");
        issue_map.insert("STORY-8", 8, "", "I_node8");
        issue_map.save(&root).unwrap();
        let store = Store::load(&root, &config).unwrap();
        let recorder = std::rc::Rc::new(MockGhDependencyClient::new());

        let fired = apply_native_dependency(
            &root,
            &config,
            &store,
            "implements",
            "STORY-7",
            "STORY-8",
            true,
            || recorder.clone(),
        )
        .unwrap();

        assert!(
            !fired,
            "a non-dependency relation must not fire the native edge"
        );
        assert!(recorder.added.borrow().is_empty());
    }

    // Opportunistic guard: a non-issue (filesystem) endpoint returns false with no
    // native call and no error, even though the relation IS the dependency one.
    #[test]
    fn apply_native_dependency_non_issue_endpoint_returns_false() {
        let root = tmp_root("dep_guard_endpoint");
        let config = dependency_config();
        write_cache_doc(
            &root.join(".lazyspec/cache/story"),
            "STORY-7.md",
            "My Story",
            "story",
        );
        let dir = root.join("docs/spec");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SPEC-1.md"),
            "---\ntitle: A\ntype: spec\nstatus: draft\nauthor: a\ndate: 2026-03-27\ntags: []\n---\nbody\n",
        )
        .unwrap();
        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("STORY-7", 7, "", "I_node7");
        issue_map.save(&root).unwrap();
        let store = Store::load(&root, &config).unwrap();
        let recorder = std::rc::Rc::new(MockGhDependencyClient::new());

        let fired = apply_native_dependency(
            &root,
            &config,
            &store,
            "blocks",
            "STORY-7",
            "SPEC-1",
            true,
            || recorder.clone(),
        )
        .unwrap();

        assert!(!fired, "a non-issue endpoint must not fire the native edge");
        assert!(recorder.added.borrow().is_empty());
    }

    // --- github_native = "sub-issue" (STORY-245 / ITERATION-347) ---

    // Two github-issues types (story = child, feature = parent) and one filesystem
    // type, with `implements` bound to the native sub-issue edge and an ordinary
    // `mentions` alongside it (to prove the guard only fires for the sub-issue
    // relation).
    fn subissue_config() -> Config {
        let issue = |name: &str, store: StoreBackend| TypeDef::test_fixture(name, store);
        let mut config = Config::default();
        config.documents.types = vec![
            issue("story", StoreBackend::GithubIssues),
            issue("feature", StoreBackend::GithubIssues),
            issue("spec", StoreBackend::Filesystem),
        ];
        config.documents.github = Some(GithubConfig {
            repo: Some("owner/repo".to_string()),
            cache_ttl: 60,
        });
        config.relationships = vec![
            crate::engine::config::RelationshipDef {
                name: "implements".to_string(),
                inverse: Some("implemented-by".to_string()),
                github_native: Some("sub-issue".to_string()),
                traversal: None,
            },
            crate::engine::config::RelationshipDef {
                name: "mentions".to_string(),
                inverse: Some("mentioned-by".to_string()),
                github_native: None,
                traversal: None,
            },
        ];
        config
    }

    fn no_parent() -> serde_json::Value {
        serde_json::json!({"data": {"node": {"parent": null}}})
    }

    fn parent_ref(node: &str, number: u64) -> serde_json::Value {
        serde_json::json!({"data": {"node": {"parent": {"id": node, "number": number}}}})
    }

    fn subissue_mutation_ok() -> serde_json::Value {
        serde_json::json!({"data": {"addSubIssue": {"issue": {"id": "I_x"}}}})
    }

    // AC1: linking two same-repo github-issues docs via a github_native="sub-issue"
    // relation writes addSubIssue with the PARENT as issueId and the CHILD as
    // subIssueId (source = child, target = parent), records the relation on the
    // child, and fires the resync (proven by the issue-map baseline reconciling to
    // the remote's fresh updated_at).
    #[test]
    fn link_subissue_writes_native_edge_and_resyncs() {
        let root = tmp_root("link_subissue");
        let config = subissue_config();
        write_cache_doc(
            &root.join(".lazyspec/cache/story"),
            "STORY-7.md",
            "My Story",
            "story",
        );
        write_cache_doc(
            &root.join(".lazyspec/cache/feature"),
            "FEATURE-3.md",
            "Parent",
            "feature",
        );

        // STORY-7 last fetched at 10:00; remote (returned by the resync view) 11:00.
        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("STORY-7", 7, "2026-06-26T10:00:00Z", "I_node7");
        issue_map.insert("FEATURE-3", 3, "", "I_node3");
        issue_map.save(&root).unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;
        // The graphql seam: first the parent lookup (no existing parent), then the
        // addSubIssue mutation.
        let recorder = std::rc::Rc::new(
            MockGhClient::new().with_graphql_responses(vec![no_parent(), subissue_mutation_ok()]),
        );

        link_inner(
            &root,
            &store,
            "STORY-7",
            "implements",
            "FEATURE-3",
            &fs,
            Some(&config),
            || MockGhClient::new().with_view_issue(view_issue_at(7, "2026-06-26T11:00:00Z")),
            MockGhMilestoneClient::new,
            || recorder.clone(),
            MockGhDependencyClient::new,
        )
        .expect("native sub-issue link must succeed");

        let calls = recorder.graphql_calls.borrow();
        let adds: Vec<_> = calls
            .iter()
            .filter(|(q, _)| q.contains("addSubIssue"))
            .collect();
        assert_eq!(adds.len(), 1, "one addSubIssue, got: {:?}", *calls);
        let (_, vars) = adds[0];
        assert!(
            vars.contains(&("issueId".to_string(), GqlVar::Str("I_node3".to_string()))),
            "parent (FEATURE-3) is the issueId, got: {vars:?}"
        );
        assert!(
            vars.contains(&("subIssueId".to_string(), GqlVar::Str("I_node7".to_string()))),
            "child (STORY-7) is the subIssueId, got: {vars:?}"
        );

        // The relation is recorded on the child's frontmatter.
        let updated =
            std::fs::read_to_string(root.join(".lazyspec/cache/story/STORY-7.md")).unwrap();
        assert!(
            updated.contains("implements: FEATURE-3"),
            "frontmatter should carry the relation, got:\n{updated}"
        );

        // Resync fired: the baseline reconciled to the remote's fresh timestamp.
        let reloaded = IssueMap::load(&root).unwrap();
        assert_eq!(
            reloaded.get("STORY-7").unwrap().updated_at,
            "2026-06-26T11:00:00Z",
            "resync_after_native_edge should record the remote's current updated_at"
        );
    }

    // AC2: unlink removes the native sub-issue edge (removeSubIssue, same
    // parent/child direction) and drops the relation from the cache frontmatter.
    // No parent lookup on unlink.
    #[test]
    fn unlink_subissue_removes_native_edge() {
        let root = tmp_root("unlink_subissue");
        let config = subissue_config();

        std::fs::create_dir_all(root.join(".lazyspec/cache/story")).unwrap();
        let content = "---\ntitle: My Story\ntype: story\nstatus: draft\nauthor: a\ndate: 2026-03-27\ntags: []\nrelated:\n- implements: FEATURE-3\n---\nbody\n";
        std::fs::write(root.join(".lazyspec/cache/story/STORY-7.md"), content).unwrap();
        write_cache_doc(
            &root.join(".lazyspec/cache/feature"),
            "FEATURE-3.md",
            "Parent",
            "feature",
        );

        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("STORY-7", 7, "2026-06-26T10:00:00Z", "I_node7");
        issue_map.insert("FEATURE-3", 3, "", "I_node3");
        issue_map.save(&root).unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;
        let recorder = std::rc::Rc::new(
            MockGhClient::new().with_graphql_responses(vec![subissue_mutation_ok()]),
        );

        unlink_inner(
            &root,
            &store,
            "STORY-7",
            "implements",
            "FEATURE-3",
            &fs,
            Some(&config),
            || MockGhClient::new().with_view_issue(view_issue_at(7, "2026-06-26T11:00:00Z")),
            MockGhMilestoneClient::new,
            || recorder.clone(),
            MockGhDependencyClient::new,
        )
        .expect("native sub-issue unlink must succeed");

        let calls = recorder.graphql_calls.borrow();
        let removes: Vec<_> = calls
            .iter()
            .filter(|(q, _)| q.contains("removeSubIssue"))
            .collect();
        assert_eq!(removes.len(), 1, "one removeSubIssue, got: {:?}", *calls);
        let (_, vars) = removes[0];
        assert!(vars.contains(&("issueId".to_string(), GqlVar::Str("I_node3".to_string()))));
        assert!(vars.contains(&("subIssueId".to_string(), GqlVar::Str("I_node7".to_string()))));
        assert!(
            calls.iter().all(|(q, _)| !q.contains("addSubIssue")),
            "unlink must not add"
        );

        let updated =
            std::fs::read_to_string(root.join(".lazyspec/cache/story/STORY-7.md")).unwrap();
        assert!(
            !updated.contains("implements: FEATURE-3"),
            "cache relation should be removed, got:\n{updated}"
        );
    }

    // AC3: linking a child that already has a DIFFERENT native parent fails with an
    // error naming the existing parent, and no addSubIssue mutation fires.
    #[test]
    fn link_subissue_single_parent_conflict_rejected() {
        let root = tmp_root("link_subissue_conflict");
        let config = subissue_config();
        write_cache_doc(
            &root.join(".lazyspec/cache/story"),
            "STORY-7.md",
            "My Story",
            "story",
        );
        write_cache_doc(
            &root.join(".lazyspec/cache/feature"),
            "FEATURE-3.md",
            "Old Parent",
            "feature",
        );
        write_cache_doc(
            &root.join(".lazyspec/cache/feature"),
            "FEATURE-9.md",
            "New Parent",
            "feature",
        );

        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("STORY-7", 7, "", "I_node7");
        issue_map.insert("FEATURE-3", 3, "", "I_node3");
        issue_map.insert("FEATURE-9", 9, "", "I_node9");
        issue_map.save(&root).unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;
        // The parent lookup returns FEATURE-3 (node I_node3, number 3) as the
        // existing parent -- different from the requested FEATURE-9.
        let recorder = std::rc::Rc::new(
            MockGhClient::new().with_graphql_responses(vec![parent_ref("I_node3", 3)]),
        );

        let err = link_inner(
            &root,
            &store,
            "STORY-7",
            "implements",
            "FEATURE-9",
            &fs,
            Some(&config),
            MockGhClient::new,
            MockGhMilestoneClient::new,
            || recorder.clone(),
            MockGhDependencyClient::new,
        )
        .expect_err("linking a child with a different existing parent must fail");
        let msg = err.to_string();
        assert!(msg.contains("STORY-7"), "names the child, got: {msg}");
        assert!(
            msg.contains("FEATURE-3"),
            "names the existing parent, got: {msg}"
        );

        let calls = recorder.graphql_calls.borrow();
        assert!(
            calls.iter().all(|(q, _)| !q.contains("addSubIssue")),
            "no addSubIssue on a single-parent conflict, got: {:?}",
            *calls
        );
        // Cache untouched.
        let updated =
            std::fs::read_to_string(root.join(".lazyspec/cache/story/STORY-7.md")).unwrap();
        assert!(
            !updated.contains("implements: FEATURE-9"),
            "cache must be unchanged after a rejected link, got:\n{updated}"
        );
    }

    // Re-linking to the SAME parent is idempotent: the parent lookup returns the
    // requested parent, so the addSubIssue still fires (GitHub treats it as a
    // no-op) and no single-parent error is raised.
    #[test]
    fn link_subissue_same_parent_is_allowed() {
        let root = tmp_root("link_subissue_same");
        let config = subissue_config();
        write_cache_doc(
            &root.join(".lazyspec/cache/story"),
            "STORY-7.md",
            "My Story",
            "story",
        );
        write_cache_doc(
            &root.join(".lazyspec/cache/feature"),
            "FEATURE-3.md",
            "Parent",
            "feature",
        );

        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("STORY-7", 7, "2026-06-26T10:00:00Z", "I_node7");
        issue_map.insert("FEATURE-3", 3, "", "I_node3");
        issue_map.save(&root).unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;
        let recorder = std::rc::Rc::new(
            MockGhClient::new()
                .with_graphql_responses(vec![parent_ref("I_node3", 3), subissue_mutation_ok()]),
        );

        link_inner(
            &root,
            &store,
            "STORY-7",
            "implements",
            "FEATURE-3",
            &fs,
            Some(&config),
            || MockGhClient::new().with_view_issue(view_issue_at(7, "2026-06-26T11:00:00Z")),
            MockGhMilestoneClient::new,
            || recorder.clone(),
            MockGhDependencyClient::new,
        )
        .expect("re-linking to the same parent must succeed");

        let calls = recorder.graphql_calls.borrow();
        assert_eq!(
            calls
                .iter()
                .filter(|(q, _)| q.contains("addSubIssue"))
                .count(),
            1,
            "addSubIssue fires when the parent already matches, got: {:?}",
            *calls
        );
    }

    // AC6 regression: a filesystem-only `implements` link/unlink makes ZERO native
    // calls -- the opportunistic guard sees non-issue endpoints and records the
    // relation comment/graph-backed exactly as before, with no error.
    #[test]
    fn link_subissue_filesystem_only_makes_no_native_call() {
        let root = tmp_root("link_subissue_fs");
        let config = subissue_config();

        let dir = root.join("docs/spec");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SPEC-1.md"),
            "---\ntitle: A\ntype: spec\nstatus: draft\nauthor: a\ndate: 2026-03-27\ntags: []\n---\nbody\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("SPEC-2.md"),
            "---\ntitle: B\ntype: spec\nstatus: draft\nauthor: a\ndate: 2026-03-27\ntags: []\n---\nbody\n",
        )
        .unwrap();

        let store = Store::load(&root, &config).unwrap();
        let fs = RealFileSystem;
        // No graphql responses seeded: any graphql call would panic ("no canned
        // response"), proving none fires.
        let recorder = std::rc::Rc::new(MockGhClient::new());

        link_inner(
            &root,
            &store,
            "SPEC-1",
            "implements",
            "SPEC-2",
            &fs,
            Some(&config),
            MockGhClient::new,
            MockGhMilestoneClient::new,
            || recorder.clone(),
            MockGhDependencyClient::new,
        )
        .expect("filesystem implements link must succeed");

        let updated = std::fs::read_to_string(dir.join("SPEC-1.md")).unwrap();
        assert!(
            updated.contains("implements: SPEC-2"),
            "filesystem relation should be recorded, got:\n{updated}"
        );
        assert!(
            recorder.graphql_calls.borrow().is_empty(),
            "no native call for a filesystem endpoint"
        );

        // Unlink is symmetric: still no native call.
        let store = Store::load(&root, &config).unwrap();
        let recorder2 = std::rc::Rc::new(MockGhClient::new());
        unlink_inner(
            &root,
            &store,
            "SPEC-1",
            "implements",
            "SPEC-2",
            &fs,
            Some(&config),
            MockGhClient::new,
            MockGhMilestoneClient::new,
            || recorder2.clone(),
            MockGhDependencyClient::new,
        )
        .expect("filesystem implements unlink must succeed");
        assert!(
            recorder2.graphql_calls.borrow().is_empty(),
            "no native call for a filesystem endpoint on unlink"
        );
    }

    // An ordinary relation (no github_native) between two github-issues docs makes
    // no sub-issue call: the native edge is opportunistic on the RELATION too.
    #[test]
    fn apply_native_subissue_non_subissue_relation_returns_false() {
        let root = tmp_root("subissue_guard_rel");
        let config = subissue_config();
        write_cache_doc(
            &root.join(".lazyspec/cache/story"),
            "STORY-7.md",
            "My Story",
            "story",
        );
        write_cache_doc(
            &root.join(".lazyspec/cache/feature"),
            "FEATURE-3.md",
            "Parent",
            "feature",
        );
        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("STORY-7", 7, "", "I_node7");
        issue_map.insert("FEATURE-3", 3, "", "I_node3");
        issue_map.save(&root).unwrap();
        let store = Store::load(&root, &config).unwrap();
        let recorder = std::rc::Rc::new(MockGhClient::new());

        let fired = apply_native_subissue(
            &root,
            &config,
            &store,
            "mentions",
            "STORY-7",
            "FEATURE-3",
            true,
            || recorder.clone(),
        )
        .unwrap();

        assert!(!fired, "a non-sub-issue relation must not fire the edge");
        assert!(recorder.graphql_calls.borrow().is_empty());
    }

    // Opportunistic guard: a non-issue (filesystem) endpoint returns false with no
    // native call and no error, even though the relation IS the sub-issue one.
    #[test]
    fn apply_native_subissue_non_issue_endpoint_returns_false() {
        let root = tmp_root("subissue_guard_endpoint");
        let config = subissue_config();
        write_cache_doc(
            &root.join(".lazyspec/cache/story"),
            "STORY-7.md",
            "My Story",
            "story",
        );
        let dir = root.join("docs/spec");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SPEC-1.md"),
            "---\ntitle: A\ntype: spec\nstatus: draft\nauthor: a\ndate: 2026-03-27\ntags: []\n---\nbody\n",
        )
        .unwrap();
        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("STORY-7", 7, "", "I_node7");
        issue_map.save(&root).unwrap();
        let store = Store::load(&root, &config).unwrap();
        let recorder = std::rc::Rc::new(MockGhClient::new());

        let fired = apply_native_subissue(
            &root,
            &config,
            &store,
            "implements",
            "STORY-7",
            "SPEC-1",
            true,
            || recorder.clone(),
        )
        .unwrap();

        assert!(!fired, "a non-issue endpoint must not fire the edge");
        assert!(recorder.graphql_calls.borrow().is_empty());
    }

    // An empty node id in the issue map (legacy map) is a clear error telling the
    // user to re-fetch, and no mutation fires.
    #[test]
    fn link_subissue_empty_node_id_errors_without_mutation() {
        let root = tmp_root("subissue_empty_node");
        let config = subissue_config();
        write_cache_doc(
            &root.join(".lazyspec/cache/story"),
            "STORY-7.md",
            "My Story",
            "story",
        );
        write_cache_doc(
            &root.join(".lazyspec/cache/feature"),
            "FEATURE-3.md",
            "Parent",
            "feature",
        );
        // STORY-7 has a REST number but an EMPTY node id (legacy map).
        let mut issue_map = IssueMap::load(&root).unwrap();
        issue_map.insert("STORY-7", 7, "", "");
        issue_map.insert("FEATURE-3", 3, "", "I_node3");
        issue_map.save(&root).unwrap();
        let store = Store::load(&root, &config).unwrap();
        let recorder = std::rc::Rc::new(MockGhClient::new());

        let err = apply_native_subissue(
            &root,
            &config,
            &store,
            "implements",
            "STORY-7",
            "FEATURE-3",
            true,
            || recorder.clone(),
        )
        .expect_err("an empty node id must be a clear error");
        let msg = err.to_string();
        assert!(msg.contains("STORY-7"), "names the doc, got: {msg}");
        assert!(
            msg.contains("fetch"),
            "tells the user to re-fetch, got: {msg}"
        );
        assert!(recorder.graphql_calls.borrow().is_empty());
    }
}
