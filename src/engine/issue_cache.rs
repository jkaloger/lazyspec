use chrono::{DateTime, Duration, Utc};
use std::fs;
use std::path::{Path, PathBuf};

use crate::engine::cache_lock::CacheLock;
use crate::engine::config::{AttrDef, Config, Lifecycle, TypeDef};
use crate::engine::document::{AttrValue, DocMeta, Relation, RelationType, Status};
use crate::engine::gh::{
    search_issue_numbers_by_type, GhGraphql, GhIssue, GhIssueDependencyApi, GhIssueReader,
    ISSUE_TYPE_SEARCH_PAGE_SIZE,
};
use crate::engine::gh_fetch::{self, FetchSnapshot};
use crate::engine::gh_schema;
use crate::engine::gh_subissue;
use crate::engine::issue_body::{self, IssueContext, TypeMatchRule};
use crate::engine::issue_map::IssueMap;
use crate::engine::store;
use crate::engine::store_dispatch;

#[derive(Debug)]
pub struct FetchResult {
    pub fetched: usize,
    pub new: usize,
    pub removed: usize,
    pub warnings: Vec<RefreshWarning>,
}

#[derive(Debug)]
pub struct RefreshResult {
    pub refreshed: usize,
    pub unchanged: usize,
    pub warnings: Vec<RefreshWarning>,
}

#[derive(Debug)]
pub struct RefreshWarning {
    pub message: String,
}

/// Remote sub-issue parentage, keyed by parent doc id. Each value is the
/// parent's children in GitHub sub-issue order (doc ids). TASK-3 consumes this
/// to write the nested cache layout; built best-effort by `fetch_all`.
pub type ParentageMap = std::collections::HashMap<String, Vec<String>>;

pub struct IssueCache {
    root: PathBuf,
}

impl IssueCache {
    pub fn new(root: &Path) -> Self {
        IssueCache {
            root: root.to_path_buf(),
        }
    }

    fn cache_dir(&self) -> PathBuf {
        self.root.join(".lazyspec").join("cache")
    }

    fn doc_path(&self, id: &str, doc_type: &str) -> PathBuf {
        self.cache_dir().join(doc_type).join(format!("{}.md", id))
    }

    /// Load the cache lock. Absent file -> clean default; present but
    /// unparseable -> hard error (AUDIT-018 C1). Mutators must propagate the
    /// error rather than saving a defaulted lock over the corrupt file, which
    /// would erase every freshness timestamp.
    fn load_lock(&self) -> anyhow::Result<CacheLock> {
        CacheLock::load(&self.root)
    }

    fn entry_is_fresh(lock: &CacheLock, id: &str, ttl: Duration) -> bool {
        let Some(value) = lock.get(id) else {
            return false;
        };
        let Ok(cached_at) = value.parse::<DateTime<Utc>>() else {
            return false;
        };
        Utc::now() - cached_at < ttl
    }

    /// A corrupt lock reads as not fresh; this probe never persists anything,
    /// and the hard error surfaces on the first mutating cache operation.
    pub fn is_fresh(&self, id: &str, ttl: Duration) -> bool {
        self.load_lock()
            .map(|lock| Self::entry_is_fresh(&lock, id, ttl))
            .unwrap_or(false)
    }

    pub fn read_if_fresh(&self, id: &str, doc_type: &str, ttl: Duration) -> Option<String> {
        if !self.is_fresh(id, ttl) {
            return None;
        }
        fs::read_to_string(self.doc_path(id, doc_type)).ok()
    }

    pub fn read_stale(&self, id: &str, doc_type: &str) -> Option<String> {
        fs::read_to_string(self.doc_path(id, doc_type)).ok()
    }

    pub fn write(&self, id: &str, doc_type: &str, content: &str) -> anyhow::Result<()> {
        // Load before writing the doc so a corrupt lock aborts the whole op.
        let mut lock = self.load_lock()?;

        let path = self.doc_path(id, doc_type);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        crate::engine::fs::atomic_write(&path, content)?;

        lock.set(id, &Utc::now().to_rfc3339());
        lock.save(&self.root)
    }

    pub fn touch_lock(&self, id: &str) -> anyhow::Result<()> {
        let mut lock = self.load_lock()?;
        lock.set(id, &Utc::now().to_rfc3339());
        lock.save(&self.root)
    }

    pub fn remove(&self, id: &str, doc_type: &str) -> anyhow::Result<()> {
        // Load before deleting anything so a corrupt lock aborts the whole op.
        let mut lock = self.load_lock()?;
        let type_dir = self.cache_dir().join(doc_type);
        let flat = self.doc_path(id, doc_type);
        if flat.is_file() {
            let _ = fs::remove_file(&flat);
        } else if let Some(child) = find_nested_child_path(&type_dir, id) {
            let _ = fs::remove_file(&child);
            if let Some(folder) = child.parent() {
                remove_dir_if_empty(folder);
            }
        } else {
            let folder = type_dir.join(id);
            if folder.is_dir() {
                let _ = fs::remove_dir_all(&folder);
            }
        }

        lock.remove(id);
        lock.save(&self.root)
    }

    /// Refresh stale cache entries for a given type with a single `issue_list` call.
    ///
    /// Returns early with zero API calls if all cached documents are fresh.
    /// On API failure, leaves stale cache in place and returns a warning.
    /// A corrupt cache lock is a hard `Err` before any API call.
    #[allow(clippy::too_many_arguments)]
    pub fn refresh_stale(
        &self,
        root: &Path,
        type_def: &TypeDef,
        gh: &dyn GhIssueReader,
        gh_graphql: &dyn GhGraphql,
        repo: &str,
        issue_map: &mut IssueMap,
        ttl: Duration,
        known_types: &[TypeMatchRule],
        config: &Config,
    ) -> anyhow::Result<RefreshResult> {
        let cached_ids = self.list_cached(&type_def.name);
        if cached_ids.is_empty() {
            return Ok(RefreshResult {
                refreshed: 0,
                unchanged: 0,
                warnings: vec![],
            });
        }

        // Loaded once up front: a corrupt lock is a hard error before any API
        // call, and never overwritten with a default.
        let mut lock = self.load_lock()?;

        let any_stale = cached_ids
            .iter()
            .any(|id| !Self::entry_is_fresh(&lock, id, ttl));
        if !any_stale {
            return Ok(RefreshResult {
                refreshed: 0,
                unchanged: cached_ids.len(),
                warnings: vec![],
            });
        }

        let fields = vec![
            "id".into(),
            "number".into(),
            "title".into(),
            "body".into(),
            "labels".into(),
            "state".into(),
            "updatedAt".into(),
            "createdAt".into(),
            "milestone".into(),
            "assignees".into(),
        ];

        let milestone_rel = config
            .relationship_by_github_native("milestone")
            .map(|r| r.name.as_str());

        let rule = TypeMatchRule::from(type_def);
        let Discovery {
            issues,
            search_truncated,
        } = match discover_issues(gh, gh_graphql, repo, &rule, &fields, None) {
            Ok(d) => d,
            Err(e) => {
                return Ok(RefreshResult {
                    refreshed: 0,
                    unchanged: cached_ids.len(),
                    warnings: vec![RefreshWarning {
                        message: format!(
                            "API unreachable for type '{}', serving stale cache: {}",
                            type_def.name, e
                        ),
                    }],
                });
            }
        };

        let mut refreshed = 0usize;
        let mut unchanged = 0usize;
        let mut write_warnings = Vec::new();

        let lifecycle = type_def.effective_lifecycle();
        let (open_status, closed_status) = open_closed_statuses(type_def, &lifecycle);
        for issue in &issues {
            let (meta, body) = parse_issue(
                issue,
                &type_def.name,
                known_types,
                &type_def.attributes,
                milestone_rel,
                issue_map,
                open_status,
                closed_status,
            );
            let id = type_def.make_id(issue.number);
            let meta = DocMeta {
                id: id.clone(),
                ..meta
            };

            let existing = self.read_stale(&id, &type_def.name);

            // The TTL path never reads the authority board, so a board-bound
            // doc's status is carried over wholesale rather than re-derived from
            // the issue; a background refresh must not silently move a doc.
            let meta = match board_owned_status(root, type_def, &id) {
                Some(status) => DocMeta { status, ..meta },
                None => meta,
            };

            let new_content = build_cache_content(&meta, &body);

            if existing.as_deref() == Some(&new_content) {
                unchanged += 1;
            } else {
                if let Err(e) = store_dispatch::write_cache_file(root, type_def, &meta, &body) {
                    // Non-fatal: skip this doc but keep going
                    write_warnings.push(RefreshWarning {
                        message: format!("failed to write cache for {}: {}", id, e),
                    });
                    continue;
                }
                refreshed += 1;
            }

            lock.set(&id, &Utc::now().to_rfc3339());
            issue_map.insert(&id, issue.number, &issue.updated_at, &issue.id);
        }

        lock.save(&self.root)?;

        let mut warnings = write_warnings;
        if search_truncated {
            warnings.push(search_truncation_warning(&type_def.name));
        }
        // The TTL refresh is not driven by `sync_all`, so it runs its own round
        // for the schema snapshot rather than inheriting one.
        let round = gh_fetch::fetch_round_best_effort(gh_graphql, repo);
        warnings.extend(self.refresh_schema_snapshot(gh_graphql, Some(&round), repo, config));
        warnings.extend(round.warnings);

        Ok(RefreshResult {
            refreshed,
            unchanged,
            warnings,
        })
    }

    /// Persist the native field schema snapshot: the round's org issue types,
    /// plus the project fields of every board a type nominates as its status
    /// authority. A merge, not an overwrite -- boards that are not re-fetched
    /// keep the ids they already had, so offline resolution never regresses
    /// because of a board this call did not touch.
    ///
    /// Issue types come from `fetch`, read once for the whole round rather than
    /// once per type. `None` -- no round ran, or its owner subtree failed --
    /// keeps the prior issue types; the round already warned about why, so this
    /// pass stays silent about it. Per-board failures are individually
    /// non-fatal, hence one warning each.
    fn refresh_schema_snapshot(
        &self,
        gh_graphql: &dyn GhGraphql,
        fetch: Option<&FetchSnapshot>,
        repo: &str,
        config: &Config,
    ) -> Vec<RefreshWarning> {
        let prior = gh_schema::GhSchemaSnapshot::load(&self.root);
        let mut warnings = Vec::new();
        let issue_types = fetch
            .and_then(|f| f.issue_types.clone())
            .unwrap_or_else(|| prior.issue_types.clone());
        let mut snapshot = gh_schema::GhSchemaSnapshot {
            issue_types,
            fetched_at: Utc::now().to_rfc3339(),
            ..Default::default()
        };
        snapshot.project_fields = prior.project_fields;
        snapshot.single_select_options = prior.single_select_options;
        snapshot.iterations = prior.iterations;

        for number in store_dispatch::authority_board_numbers(config) {
            match gh_schema::fetch_project_fields(gh_graphql, repo, number) {
                Ok((fields, options, iterations)) => {
                    snapshot.replace_board_fields(number, fields, options, iterations)
                }
                Err(e) => warnings.push(RefreshWarning {
                    message: format!(
                        "could not refresh field schema for board {} (keeping prior, projects need `gh auth refresh -s project`): {}",
                        number, e
                    ),
                }),
            }
        }

        if let Err(e) = snapshot.save(&self.root) {
            warnings.push(RefreshWarning {
                message: format!("could not persist gh schema snapshot: {}", e),
            });
        }
        warnings
    }

    pub fn list_cached(&self, doc_type: &str) -> Vec<String> {
        let dir = self.cache_dir().join(doc_type);
        let Ok(entries) = fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut ids = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(parent_id) = path.file_name().and_then(|s| s.to_str()) {
                    if path.join("index.md").is_file() {
                        ids.push(parent_id.to_string());
                    }
                }
                ids.extend(list_nested_children(&path));
                continue;
            }
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                ids.push(stem.to_string());
            }
        }
        ids
    }

    /// Full fetch of all issues for a type, with pagination and cleanup of removed issues.
    #[allow(clippy::too_many_arguments)]
    pub fn fetch_all(
        &self,
        root: &Path,
        type_def: &TypeDef,
        gh: &dyn GhIssueReader,
        gh_graphql: &dyn GhGraphql,
        gh_dependency: &dyn GhIssueDependencyApi,
        fetch: Option<&FetchSnapshot>,
        repo: &str,
        issue_map: &mut IssueMap,
        known_types: &[TypeMatchRule],
        config: &Config,
    ) -> anyhow::Result<FetchResult> {
        const FETCH_LIMIT: u64 = 500;

        let rule = TypeMatchRule::from(type_def);
        let Discovery {
            issues,
            search_truncated,
        } = discover_issues(gh, gh_graphql, repo, &rule, &[], Some(FETCH_LIMIT))?;

        let mut warnings: Vec<RefreshWarning> = Vec::new();
        if issues.len() as u64 == FETCH_LIMIT {
            warnings.push(RefreshWarning {
                message: format!(
                    "fetched exactly {} issues for type '{}'; there may be more",
                    FETCH_LIMIT, type_def.name
                ),
            });
        }
        if search_truncated {
            warnings.push(search_truncation_warning(&type_def.name));
        }

        let previously_cached: std::collections::HashSet<String> =
            self.list_cached(&type_def.name).into_iter().collect();

        // Loaded before any cache mutation: a corrupt lock is a hard error and
        // is never overwritten with a default (AUDIT-018 C1).
        let mut lock = self.load_lock()?;

        let cache_root = root.join(".lazyspec/cache");
        let live_dir = cache_root.join(&type_def.name);
        fs::create_dir_all(&cache_root)?;

        // Parse every fetched issue up front so the parentage query can resolve
        // child node ids back to doc ids before we decide each doc's layout.
        struct Parsed {
            id: String,
            meta: DocMeta,
            body: String,
        }
        let mut parsed: Vec<Parsed> = Vec::with_capacity(issues.len());
        let mut node_to_doc: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        // Rel name for the GitHub-native milestone edge, resolved from config so
        // the forward `targets` relation is never hardcoded. `None` when no
        // relationship declares `github_native = "milestone"`.
        let milestone_rel = config
            .relationship_by_github_native("milestone")
            .map(|r| r.name.as_str());
        let lifecycle = type_def.effective_lifecycle();
        let (open_status, closed_status) = open_closed_statuses(type_def, &lifecycle);
        for issue in &issues {
            let (meta, body) = parse_issue(
                issue,
                &type_def.name,
                known_types,
                &type_def.attributes,
                milestone_rel,
                issue_map,
                open_status,
                closed_status,
            );
            let id = type_def.make_id(issue.number);
            let meta = DocMeta {
                id: id.clone(),
                ..meta
            };
            // The project-field pass that follows this fetch overwrites a
            // board-bound doc's status with the live cell -- but it can fail
            // (a token without the `project` scope), and then only what is
            // written here survives. Keep the last known board status rather
            // than blanking every doc of the type on a warned-about failure.
            let meta = match board_owned_status(root, type_def, &id) {
                Some(status) => DocMeta { status, ..meta },
                None => meta,
            };
            if !issue.id.is_empty() {
                node_to_doc.insert(issue.id.clone(), id.clone());
            }
            parsed.push(Parsed { id, meta, body });
        }

        // Native sub-issue edges split by this type's shape. A flat (non-subdir)
        // type with a sub-issue-native relationship reads the edge back as that
        // relation (child holds the forward name toward its parent) instead of
        // nesting; every other type keeps materializing nested docs
        // (ITERATION-224). The two are mutually exclusive, so nesting is skipped
        // entirely in relation-injection mode.
        let subissue_rel = config.relationship_by_github_native("sub-issue");
        let inject_subissue_relation = subissue_rel.is_some() && !type_def.subdirectory;

        // Best-effort: learn remote native sub-issue parentage so we can write
        // the nested cache layout. A GraphQL failure warns and falls back to a
        // flat layout for the affected parent; it never aborts the fetch.
        let parentage = if inject_subissue_relation {
            ParentageMap::new()
        } else {
            let (parentage, subissue_warnings) = fetch_subissue_parentage(gh_graphql, &node_to_doc);
            warnings.extend(subissue_warnings);
            parentage
        };

        // Best-effort: read each issue's native blocked-by dependencies and
        // inject the declared inverse relation (`blocked-by`) toward each
        // blocking issue's doc. Mirrors the milestone read-back -- the forward
        // `blocks` edge on the blocker is derived virtually in `build_links`,
        // never stored. One batched read for the whole fetch
        // (`GhIssueDependencyApi::list_blocked_by_batch`, chunked to
        // `gh::GH_NODES_BATCH_MAX`) rather than one request per issue; a
        // chunk's failure warns and skips every issue in that chunk, same as a
        // single-issue read failure did before batching.
        if let Some(dep_rel) = config
            .relationship_by_github_native("dependency")
            .and_then(|r| r.inverse.as_deref())
        {
            // number -> doc id for the current fetch batch, so same-type
            // blockers resolve before the batch is written into the issue map
            // (cross-type blockers resolve via the map once their type fetches,
            // the same ordering caveat milestones carry).
            let batch: std::collections::HashMap<u64, String> = issues
                .iter()
                .zip(parsed.iter())
                .map(|(issue, p)| (issue.number, p.id.clone()))
                .collect();
            // An issue with no node id can't key the GraphQL batch -- the same
            // constraint the sub-issue parentage read above already accepts.
            let pairs: Vec<(String, u64)> = issues
                .iter()
                .filter(|i| !i.id.is_empty())
                .map(|i| (i.id.clone(), i.number))
                .collect();
            match gh_dependency.list_blocked_by_batch(repo, &pairs) {
                Ok(blocked_by) => {
                    for (issue, p) in issues.iter().zip(parsed.iter_mut()) {
                        let Some(blockers) = blocked_by.get(&issue.number) else {
                            continue;
                        };
                        for &blocker in blockers {
                            let target = batch.get(&blocker).cloned().or_else(|| {
                                issue_map.shorthand_for_number(blocker).map(String::from)
                            });
                            if let Some(target) = target {
                                p.meta.related.push(Relation {
                                    rel_type: RelationType::new(dep_rel),
                                    target,
                                });
                            }
                        }
                    }
                }
                Err(e) => {
                    warnings.push(RefreshWarning {
                        message: format!(
                            "could not read native dependencies for {} issues, skipping: {}",
                            pairs.len(),
                            e
                        ),
                    });
                }
            }
        }

        // Flat-doc read-back: inject the sub-issue-native relation on each child
        // toward its remote parent (the forward name; the parent's inverse is
        // derived in the graph, never stored -- mirrors the dependency path). A
        // dropped remote edge simply yields no parent on re-fetch, so the
        // relation vanishes with the authoritative rebuild, no duplicates.
        if let Some(rel) = subissue_rel.filter(|_| inject_subissue_relation) {
            let (parent_by_child, parent_warnings) =
                fetch_subissue_parent_numbers(gh_graphql, &node_to_doc);
            warnings.extend(parent_warnings);
            // number -> doc id for the in-flight batch, so a same-type parent
            // resolves before the batch is written into the issue map; cross-type
            // parents resolve via the map once their type has fetched (the same
            // ordering caveat milestones and dependencies carry).
            let batch: std::collections::HashMap<u64, String> = issues
                .iter()
                .zip(parsed.iter())
                .map(|(issue, p)| (issue.number, p.id.clone()))
                .collect();
            for (issue, p) in issues.iter().zip(parsed.iter_mut()) {
                let Some(parent_number) = parent_by_child.get(&issue.id) else {
                    continue;
                };
                let target = batch.get(parent_number).cloned().or_else(|| {
                    issue_map
                        .shorthand_for_number(*parent_number)
                        .map(String::from)
                });
                if let Some(target) = target {
                    p.meta.related.push(Relation {
                        rel_type: RelationType::new(&rel.name),
                        target,
                    });
                }
            }
        }

        // child doc id -> (parent id, order, sibling count) for nested children.
        let mut child_layout: std::collections::HashMap<String, (String, usize, usize)> =
            std::collections::HashMap::new();
        for (parent_id, children) in &parentage {
            let total = children.len();
            for (order, child_id) in children.iter().enumerate() {
                child_layout.insert(child_id.clone(), (parent_id.clone(), order, total));
            }
        }

        // A full fetch is authoritative for the whole type directory, and a doc
        // can move between flat <-> nested layouts (or change order/parent)
        // across fetches. `write_cache_*` writes by CURRENT layout but never
        // removes a doc's prior file at a different path, so rebuild the type
        // dir from scratch to guarantee no stale or duplicated cache entries.
        // The rebuild happens in a staging dir swapped into place only when
        // every write succeeded, so a failure partway (disk full, crash) leaves
        // the previous cache intact (AUDIT-018 C4).
        let staging_root = cache_root.join(format!(".staging-{}", type_def.name));
        if staging_root.exists() {
            fs::remove_dir_all(&staging_root)?;
        }
        // write_cache_* derive `<root>/.lazyspec/cache/<type>` from the root
        // they are handed, so staging just means handing them a staging root.
        let staged_type_dir = staging_root.join(".lazyspec/cache").join(&type_def.name);

        let write_result = (|| -> anyhow::Result<()> {
            fs::create_dir_all(&staged_type_dir)?;
            for Parsed { id, meta, body } in parsed.iter() {
                if let Some((parent_id, order, total)) = child_layout.get(id) {
                    store_dispatch::write_cache_child(
                        &staging_root,
                        type_def,
                        parent_id,
                        *order,
                        *total,
                        meta,
                        body,
                    )?;
                } else if parentage.contains_key(id) {
                    store_dispatch::write_cache_parent(&staging_root, type_def, meta, body)?;
                } else {
                    store_dispatch::write_cache_file(&staging_root, type_def, meta, body)?;
                }
            }
            Ok(())
        })();
        if let Err(e) = write_result {
            let _ = fs::remove_dir_all(&staging_root);
            return Err(e);
        }

        // Swap: move the previous dir aside, promote the staged one, drop the
        // old. Only after this point does the lock/issue-map reflect the fetch.
        let old_dir = cache_root.join(format!(".old-{}", type_def.name));
        if old_dir.exists() {
            fs::remove_dir_all(&old_dir)?;
        }
        if live_dir.exists() {
            fs::rename(&live_dir, &old_dir)?;
        }
        fs::rename(&staged_type_dir, &live_dir)?;
        let _ = fs::remove_dir_all(&old_dir);
        let _ = fs::remove_dir_all(&staging_root);

        let mut new_count = 0usize;
        let mut fetched_ids = std::collections::HashSet::new();

        for (issue, Parsed { id, .. }) in issues.iter().zip(parsed.iter()) {
            if !previously_cached.contains(id) {
                new_count += 1;
            }

            lock.set(id, &Utc::now().to_rfc3339());
            issue_map.insert(id, issue.number, &issue.updated_at, &issue.id);
            fetched_ids.insert(id.clone());
        }

        let removed: Vec<String> = previously_cached
            .difference(&fetched_ids)
            .cloned()
            .collect();

        // Cache files were already replaced by the swap above; the removed
        // set only needs lock + issue-map cleanup for docs gone from the remote.
        for id in &removed {
            lock.remove(id);
            issue_map.remove(id);
        }

        lock.save(&self.root)?;

        warnings.extend(self.refresh_schema_snapshot(gh_graphql, fetch, repo, config));

        Ok(FetchResult {
            fetched: issues.len(),
            new: new_count,
            removed: removed.len(),
            warnings,
        })
    }
}

/// Outcome of a type's discovery step. `search_truncated` is true when the
/// GraphQL issue-type search returned a full page, signalling that candidates
/// beyond the first page were dropped.
struct Discovery {
    issues: Vec<GhIssue>,
    search_truncated: bool,
}

/// Resolve the candidate issues for a type's discovery step from its
/// [`TypeMatchRule`]. Three branches keyed on `tag`/`issue_type`:
///
/// - no `issue_type`: REST-only, filtering on `tag` when set else `label`;
/// - `issue_type` only: GraphQL issue-type search, each number resolved to a
///   full issue via `issue_view` -- no REST list call is made;
/// - both `tag` and `issue_type`: the REST list on `tag` INTERSECTED with the
///   search result set by issue number (AND, per RFC-055 -- never a union).
fn discover_issues(
    gh: &dyn GhIssueReader,
    gh_graphql: &dyn GhGraphql,
    repo: &str,
    rule: &TypeMatchRule,
    fields: &[String],
    limit: Option<u64>,
) -> anyhow::Result<Discovery> {
    match (&rule.tag, &rule.issue_type) {
        (_, None) => {
            let label = rule.tag.clone().unwrap_or_else(|| rule.label.clone());
            let issues = gh.issue_list(repo, &[label], fields, limit)?;
            Ok(Discovery {
                issues,
                search_truncated: false,
            })
        }
        (None, Some(issue_type)) => {
            let numbers = search_issue_numbers_by_type(gh_graphql, repo, issue_type)?;
            let search_truncated = numbers.len() == ISSUE_TYPE_SEARCH_PAGE_SIZE;
            let issues = numbers
                .into_iter()
                .map(|n| gh.issue_view(repo, n))
                .collect::<anyhow::Result<Vec<_>>>()?;
            Ok(Discovery {
                issues,
                search_truncated,
            })
        }
        (Some(tag), Some(issue_type)) => {
            let rest = gh.issue_list(repo, std::slice::from_ref(tag), fields, limit)?;
            let numbers = search_issue_numbers_by_type(gh_graphql, repo, issue_type)?;
            let search_truncated = numbers.len() == ISSUE_TYPE_SEARCH_PAGE_SIZE;
            let keep: std::collections::HashSet<u64> = numbers.into_iter().collect();
            let issues = rest
                .into_iter()
                .filter(|i| keep.contains(&i.number))
                .collect();
            Ok(Discovery {
                issues,
                search_truncated,
            })
        }
    }
}

/// Warning emitted when a type's issue-type search filled its first page, so
/// candidates beyond it were not discovered. Mirrors the REST `FETCH_LIMIT`
/// truncation warning.
fn search_truncation_warning(type_name: &str) -> RefreshWarning {
    RefreshWarning {
        message: format!(
            "issue-type search for type '{}' returned exactly {} issues; there may be more",
            type_name, ISSUE_TYPE_SEARCH_PAGE_SIZE
        ),
    }
}

/// Query each fetched issue's native sub-issues and resolve the ordered child
/// node ids back to fetched doc ids. Returns only parents that have at least one
/// resolvable child; child node ids not present in `node_to_doc` are dropped (no
/// phantom entries).
///
/// Batched: one `nodes(ids:)` GraphQL request per chunk of
/// `SUB_ISSUE_BATCH_MAX` parents, so the call count is `ceil(N / 100)` rather
/// than N. Best-effort per chunk: a chunk's GraphQL failure is returned as a
/// warning and skips that chunk's parents rather than aborting. Engine emits no
/// stderr.
fn fetch_subissue_parentage(
    gh_graphql: &dyn GhGraphql,
    node_to_doc: &std::collections::HashMap<String, String>,
) -> (ParentageMap, Vec<RefreshWarning>) {
    let mut map = ParentageMap::new();
    let mut warnings = Vec::new();
    let parent_nodes: Vec<String> = node_to_doc.keys().cloned().collect();
    for chunk in parent_nodes.chunks(gh_subissue::SUB_ISSUE_BATCH_MAX) {
        let by_node = match gh_subissue::fetch_sub_issue_nodes_batch(gh_graphql, chunk) {
            Ok(m) => m,
            Err(e) => {
                warnings.push(RefreshWarning {
                    message: format!(
                        "could not fetch sub-issues for {} parents, skipping nesting: {}",
                        chunk.len(),
                        e
                    ),
                });
                continue;
            }
        };
        for (parent_node, child_nodes) in by_node {
            let Some(parent_doc) = node_to_doc.get(&parent_node) else {
                continue;
            };
            let children: Vec<String> = child_nodes
                .iter()
                .filter_map(|n| node_to_doc.get(n).cloned())
                .collect();
            if !children.is_empty() {
                map.insert(parent_doc.clone(), children);
            }
        }
    }
    (map, warnings)
}

/// Best-effort batched read of each fetched issue's native sub-issue parent
/// number, keyed by the child's node id. Chunked to `SUB_ISSUE_BATCH_MAX`; a
/// chunk's GraphQL failure warns and skips that chunk rather than aborting the
/// fetch. Feeds the flat-doc relation read-back, mirroring the dependency path.
fn fetch_subissue_parent_numbers(
    gh_graphql: &dyn GhGraphql,
    node_to_doc: &std::collections::HashMap<String, String>,
) -> (std::collections::HashMap<String, u64>, Vec<RefreshWarning>) {
    let mut map = std::collections::HashMap::new();
    let mut warnings = Vec::new();
    let child_nodes: Vec<String> = node_to_doc.keys().cloned().collect();
    for chunk in child_nodes.chunks(gh_subissue::SUB_ISSUE_BATCH_MAX) {
        match gh_subissue::fetch_sub_issue_parent_numbers_batch(gh_graphql, chunk) {
            Ok(m) => map.extend(m),
            Err(e) => warnings.push(RefreshWarning {
                message: format!(
                    "could not read sub-issue parents for {} issues, skipping relation injection: {}",
                    chunk.len(),
                    e
                ),
            }),
        }
    }
    (map, warnings)
}

/// Real doc ids of every `NN-<id>.md` child inside a parent folder.
fn list_nested_children(folder: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                return None;
            }
            let stem = path.file_stem().and_then(|s| s.to_str())?;
            store::strip_order_prefix(stem).map(|s| s.to_string())
        })
        .collect()
}

/// Path to a nested child `NN-<id>.md` anywhere one level under the type dir.
fn find_nested_child_path(type_dir: &Path, id: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(type_dir).ok()?;
    for entry in entries.flatten() {
        let folder = entry.path();
        if !folder.is_dir() {
            continue;
        }
        for child in fs::read_dir(&folder).ok().into_iter().flatten().flatten() {
            let path = child.path();
            let stem = path.file_stem().and_then(|s| s.to_str());
            if stem.and_then(store::strip_order_prefix) == Some(id) {
                return Some(path);
            }
        }
    }
    None
}

fn remove_dir_if_empty(dir: &Path) {
    if dir.join("index.md").is_file() {
        return;
    }
    if fs::read_dir(dir)
        .map(|mut e| e.next().is_none())
        .unwrap_or(false)
    {
        let _ = fs::remove_dir(dir);
    }
}

fn parse_created_date(created_at: &str) -> chrono::NaiveDate {
    chrono::DateTime::parse_from_rfc3339(created_at)
        .map(|dt| dt.date_naive())
        .unwrap_or_else(|_| Utc::now().date_naive())
}

/// The board number a type's `status_authority` nominates, if it nominates one.
/// A value that names no board number nominates no authority at all -- the same
/// predicate the read path applies (`store_dispatch::authority_board_for`), so a
/// typo like `PROJECT-seven` must not suppress the ordinary lifecycle in exchange
/// for a board nothing will ever consult.
fn authority_board(type_def: &TypeDef) -> Option<u64> {
    type_def
        .status_authority
        .as_deref()
        .and_then(|id| store_dispatch::board_number(id).ok())
}

/// The first-active / terminal lifecycle states an issue's open/closed bit maps
/// to. Both are empty for a `status_authority`-bound type: its lifecycle is the
/// board's columns, and open/closed is a second, disjoint lifecycle that would
/// otherwise write `open`/`closed` over a doc the board never placed there
/// (STORY-248). Such a type takes its status from the board's `Status` cell
/// alone, so a bodyless issue simply parses with no status.
fn open_closed_statuses<'a>(type_def: &TypeDef, lifecycle: &'a Lifecycle) -> (&'a str, &'a str) {
    if authority_board(type_def).is_some() {
        ("", "")
    } else {
        (lifecycle.first_active_status(), lifecycle.terminal_status())
    }
}

/// The status to give an authority-bound doc parsed off an issue: whatever the
/// cache file already holds, or unset when the cache has never seen the doc.
/// `None` for a type that nominates no board, leaving the parsed status alone.
///
/// Nothing on the issue may set a board-bound doc's status. Not the open/closed
/// bit (see [`open_closed_statuses`]), and not the `status:` line lazyspec
/// embeds in the issue body either: that line is a snapshot of what the board
/// said when the doc was last written, so honouring it would revert a doc the
/// board has since moved -- the exact drift STORY-248 removes. Only the
/// project-field pass after a full fetch reads the board, so until it runs the
/// last value the cache holds stands.
///
/// Located with the same lookup `write_cache_file` uses, so a slugged or nested
/// cache file is read rather than treated as a doc the cache has never seen.
///
/// The write path applies the same rule: an `update` that carries no status of
/// its own leaves a board-bound doc at the status this reports
/// (`GithubIssuesStore::update`).
pub(crate) fn board_owned_status(root: &Path, type_def: &TypeDef, id: &str) -> Option<Status> {
    authority_board(type_def)?;
    let cache_dir = root.join(".lazyspec/cache").join(&type_def.name);
    Some(
        store_dispatch::find_cache_file(&cache_dir, id)
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|c| DocMeta::parse(&c).ok())
            .map(|m| m.status)
            .unwrap_or_else(|| Status::new("")),
    )
}

#[allow(clippy::too_many_arguments)]
fn parse_issue(
    issue: &GhIssue,
    type_name: &str,
    known_types: &[TypeMatchRule],
    attr_defs: &[AttrDef],
    milestone_rel: Option<&str>,
    issue_map: &IssueMap,
    open_status: &str,
    closed_status: &str,
) -> (DocMeta, String) {
    let ctx = IssueContext {
        title: issue.title.clone(),
        labels: issue.labels.iter().map(|l| l.name.clone()).collect(),
        is_open: issue.state.eq_ignore_ascii_case("open"),
        known_types: known_types.to_vec(),
        issue_type: issue.issue_type.clone(),
        default_type: type_name.to_string(),
        attr_defs: attr_defs.to_vec(),
        open_status: open_status.to_string(),
        closed_status: closed_status.to_string(),
    };

    let author = issue
        .author
        .as_ref()
        .map(|a| format!("@{}", a.login))
        .unwrap_or_else(|| "unknown".to_string());

    if let Ok((mut meta, body)) = issue_body::deserialize(&issue.body, &ctx) {
        meta.author = author;
        insert_issue_type(&mut meta, issue);
        inject_milestone_target(&mut meta, issue, milestone_rel, issue_map);
        inject_assignee(&mut meta, issue);
        return (meta, body);
    }

    let mut meta = fallback_meta(issue, &ctx);
    insert_issue_type(&mut meta, issue);
    inject_milestone_target(&mut meta, issue, milestone_rel, issue_map);
    inject_assignee(&mut meta, issue);

    (meta, issue.body.clone())
}

/// Meta synthesized from the remote issue's own fields, for bodies that carry
/// no lazyspec HTML comment (e.g. GitHub-authored issues): labels drive type +
/// tags, open/closed state drives the lifecycle status, and `related` starts
/// empty.
pub(crate) fn fallback_meta(issue: &GhIssue, ctx: &IssueContext) -> DocMeta {
    let status = if ctx.is_open {
        Status::new(&ctx.open_status)
    } else {
        Status::new(&ctx.closed_status)
    };

    let (doc_type, tags) = issue_body::extract_type_and_tags(
        &ctx.labels,
        &ctx.known_types,
        issue.issue_type.as_deref(),
        &ctx.default_type,
    );

    DocMeta {
        path: PathBuf::new(),
        title: issue.title.clone(),
        doc_type,
        status,
        author: issue
            .author
            .as_ref()
            .map(|a| format!("@{}", a.login))
            .unwrap_or_else(|| "unknown".to_string()),
        date: parse_created_date(&issue.created_at),
        tags,
        provenance: vec![],
        related: vec![],
        validate_ignore: false,
        virtual_doc: false,
        assignee: None,
        attributes: Default::default(),
        id: String::new(),
    }
}

/// Surface an issue's native GitHub milestone as a forward relation (the
/// `targets` edge by default; whatever rel declares `github_native =
/// "milestone"`). The milestone number resolves to its `MILESTONE-n` doc via
/// the issue map; an unmapped milestone is skipped so no dangling target is
/// written. The inverse `targeted-by` is derived virtually in `build_links`,
/// never stored here.
fn inject_milestone_target(
    meta: &mut DocMeta,
    issue: &GhIssue,
    milestone_rel: Option<&str>,
    issue_map: &IssueMap,
) {
    let (Some(rel), Some(ms)) = (milestone_rel, &issue.milestone) else {
        return;
    };
    if let Some(shorthand) = issue_map.milestone_shorthand_for_number(ms.number) {
        meta.related.push(Relation {
            rel_type: RelationType::new(rel),
            target: shorthand.to_string(),
        });
    }
}

/// Surface the native GitHub issue-type as the orthogonal `issue_type`
/// attribute. Absent (not empty) when the issue has no native type. Sourced
/// only from the native field, never from labels.
fn insert_issue_type(meta: &mut DocMeta, issue: &GhIssue) {
    if let Some(name) = &issue.issue_type {
        meta.attributes
            .insert("issue_type".to_string(), AttrValue::Str(name.clone()));
    }
}

/// Inherit the issue's native assignee into `DocMeta.assignee` (STORY-222 AC3):
/// the FIRST of GitHub's `assignees` (multi-assignee maps to first; the rest are
/// out of scope for the single-assignee model), `None` when unassigned. Remote
/// is source of truth, so this overwrites whatever the body/local carried. Kept
/// native like the milestone edge -- never round-tripped through the body HTML
/// comment.
fn inject_assignee(meta: &mut DocMeta, issue: &GhIssue) {
    meta.assignee = issue.assignees.first().map(|a| a.login.clone());
}

fn build_cache_content(meta: &DocMeta, body: &str) -> String {
    let tags_str = if meta.tags.is_empty() {
        "[]".to_string()
    } else {
        format!(
            "[{}]",
            meta.tags
                .iter()
                .map(|t| format!("\"{}\"", t))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    let related_str = if meta.related.is_empty() {
        "[]".to_string()
    } else {
        let lines: Vec<String> = meta
            .related
            .iter()
            .map(|r| format!("\n- {}: {}", r.rel_type, r.target))
            .collect();
        lines.join("")
    };

    format!(
        "---\ntitle: \"{}\"\ntype: {}\nstatus: {}\nauthor: \"{}\"\ndate: {}\ntags: {}\nrelated: {}\n---\n{}",
        meta.title,
        meta.doc_type.as_str(),
        meta.status,
        meta.author,
        meta.date,
        tags_str,
        related_str,
        body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{NumberingStrategy, StoreBackend};
    use crate::engine::document::DocType;
    use crate::engine::gh::{
        test_support::MockGhClient, GhAuthor, GhGraphql, GhIssueDependencyApi, GhIssueReader,
        GhLabel, GqlVar,
    };
    use anyhow::Result;
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    fn make_cache() -> (IssueCache, TempDir) {
        let tmp = TempDir::new().unwrap();
        let cache = IssueCache::new(tmp.path());
        (cache, tmp)
    }

    fn story_type_def() -> TypeDef {
        TypeDef {
            name: "story".to_string(),
            plural: "stories".to_string(),
            dir: "docs/story".to_string(),
            prefix: "STORY".to_string(),
            icon: None,
            numbering: NumberingStrategy::default(),
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
            status_authority: None,
            clickup_list_id: None,
            clickup_task_type: None,
            clickup_custom_field_map: None,
        }
    }

    // A second github-issues TypeDef, distinct prefix/name/dir from
    // story_type_def(), used to prove two types can independently match the
    // same underlying GitHub issue number without colliding (RFC-055).
    fn ticket_type_def() -> TypeDef {
        TypeDef {
            name: "ticket".to_string(),
            plural: "tickets".to_string(),
            dir: "docs/ticket".to_string(),
            prefix: "TICKET".to_string(),
            icon: None,
            numbering: NumberingStrategy::default(),
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
            status_authority: None,
            clickup_list_id: None,
            clickup_task_type: None,
            clickup_custom_field_map: None,
        }
    }

    fn story_match_rule() -> TypeMatchRule {
        TypeMatchRule {
            name: "story".to_string(),
            label: "lazyspec:story".to_string(),
            tag: None,
            issue_type: None,
        }
    }

    fn ticket_match_rule() -> TypeMatchRule {
        TypeMatchRule {
            name: "ticket".to_string(),
            label: "Ticket".to_string(),
            tag: None,
            issue_type: None,
        }
    }

    fn empty_issue_types_response() -> serde_json::Value {
        serde_json::json!({
            "data": { "organization": { "issueTypes": { "nodes": [] } } }
        })
    }

    fn make_gh_issue(number: u64, title: &str, body: &str, labels: &[&str]) -> GhIssue {
        GhIssue {
            number,
            id: String::new(),
            url: format!("https://github.com/owner/repo/issues/{}", number),
            title: title.to_string(),
            body: body.to_string(),
            labels: labels
                .iter()
                .map(|l| GhLabel {
                    name: l.to_string(),
                    color: String::new(),
                })
                .collect(),
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: vec![],
        }
    }

    struct MockReader {
        issues: Vec<GhIssue>,
        fail: bool,
        list_call_count: AtomicUsize,
        list_labels: RefCell<Vec<Vec<String>>>,
        graphql_responses: RefCell<Vec<serde_json::Value>>,
        graphql_call_count: AtomicUsize,
        round_issue_types: RefCell<Vec<gh_schema::IssueTypeId>>,
        view_issues: Vec<GhIssue>,
        view_call_count: AtomicUsize,
    }

    impl MockReader {
        fn new(issues: Vec<GhIssue>) -> Self {
            Self {
                issues,
                fail: false,
                list_call_count: AtomicUsize::new(0),
                list_labels: RefCell::new(vec![]),
                graphql_responses: RefCell::new(vec![]),
                graphql_call_count: AtomicUsize::new(0),
                round_issue_types: RefCell::new(vec![]),
                view_issues: vec![],
                view_call_count: AtomicUsize::new(0),
            }
        }

        fn failing() -> Self {
            let me = Self::new(vec![]);
            Self { fail: true, ..me }
        }

        fn with_graphql_responses(self, responses: Vec<serde_json::Value>) -> Self {
            *self.graphql_responses.borrow_mut() = responses;
            self
        }

        /// The org issue types a composed fetch round resolves for this double.
        fn with_issue_types(self, issue_types: Vec<gh_schema::IssueTypeId>) -> Self {
            *self.round_issue_types.borrow_mut() = issue_types;
            self
        }

        fn with_view_issues(mut self, issues: Vec<GhIssue>) -> Self {
            self.view_issues = issues;
            self
        }

        fn call_count(&self) -> usize {
            self.list_call_count.load(Ordering::SeqCst)
        }

        fn recorded_list_labels(&self) -> Vec<Vec<String>> {
            self.list_labels.borrow().clone()
        }

        fn graphql_call_count(&self) -> usize {
            self.graphql_call_count.load(Ordering::SeqCst)
        }

        fn view_call_count(&self) -> usize {
            self.view_call_count.load(Ordering::SeqCst)
        }
    }

    impl GhGraphql for MockReader {
        fn graphql(&self, query: &str, _vars: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
            self.graphql_call_count.fetch_add(1, Ordering::SeqCst);
            if gh_fetch::is_round_query(query) {
                return Ok(crate::engine::gh::test_support::round_response(
                    &[],
                    &self.round_issue_types.borrow(),
                ));
            }
            let mut responses = self.graphql_responses.borrow_mut();
            if responses.is_empty() {
                anyhow::bail!("graphql unreachable");
            }
            Ok(responses.remove(0))
        }

        fn project_items(
            &self,
            _repo: &str,
            _content_node_id: &str,
        ) -> Result<Vec<crate::engine::gh::ProjectItem>> {
            Ok(vec![])
        }

        fn update_project_v2_item_field_value(
            &self,
            _project_id: &str,
            _item_id: &str,
            _field_id: &str,
            _value: &crate::engine::gh::GhFieldValueInput,
        ) -> Result<()> {
            Ok(())
        }

        fn clear_project_field(
            &self,
            _project_id: &str,
            _item_id: &str,
            _field_id: &str,
        ) -> Result<()> {
            Ok(())
        }
    }

    impl GhIssueReader for MockReader {
        fn issue_list(
            &self,
            _repo: &str,
            labels: &[String],
            _json_fields: &[String],
            _limit: Option<u64>,
        ) -> Result<Vec<GhIssue>> {
            self.list_call_count.fetch_add(1, Ordering::SeqCst);
            self.list_labels.borrow_mut().push(labels.to_vec());
            if self.fail {
                anyhow::bail!("API unreachable");
            }
            Ok(self.issues.clone())
        }

        fn issue_view(&self, _repo: &str, number: u64) -> Result<GhIssue> {
            self.view_call_count.fetch_add(1, Ordering::SeqCst);
            if let Some(issue) = self.view_issues.iter().find(|i| i.number == number) {
                return Ok(issue.clone());
            }
            Ok(make_gh_issue(number, "Viewed", "Viewed body", &[]))
        }

        fn issue_comments(
            &self,
            _repo: &str,
            _number: u64,
        ) -> Result<Vec<crate::engine::gh::GhComment>> {
            unimplemented!()
        }
    }

    /// No native dependencies: the dependency read-back is exercised in its own
    /// test; every other `fetch_all` test uses a `Config::default()` that
    /// declares no `dependency` relation, so this reader is never called.
    impl GhIssueDependencyApi for MockReader {
        fn list_blocked_by(&self, _repo: &str, _blocked_number: u64) -> Result<Vec<u64>> {
            Ok(vec![])
        }
        fn add_blocked_by(&self, _repo: &str, _blocked: u64, _blocking: u64) -> Result<()> {
            unimplemented!()
        }
        fn remove_blocked_by(&self, _repo: &str, _blocked: u64, _blocking: u64) -> Result<()> {
            unimplemented!()
        }
    }

    #[test]
    fn test_issue_cache_write_and_fresh_read() {
        let (cache, _tmp) = make_cache();
        let ttl = Duration::seconds(60);

        cache
            .write(
                "ITERATION-042",
                "iteration",
                "# Iteration 042\nSome content",
            )
            .unwrap();

        let result = cache.read_if_fresh("ITERATION-042", "iteration", ttl);
        assert_eq!(result, Some("# Iteration 042\nSome content".to_string()));

        let doc_path = cache.doc_path("ITERATION-042", "iteration");
        assert!(doc_path.exists());

        let lock = cache.load_lock().unwrap();
        assert!(lock.get("ITERATION-042").is_some());
    }

    #[test]
    fn test_issue_cache_stale_returns_none_from_fresh() {
        let (cache, _tmp) = make_cache();
        let ttl = Duration::seconds(60);

        cache
            .write("STORY-075", "story", "# Story 075\nStale content")
            .unwrap();

        // Backdate the cached_at to 2 minutes ago
        let mut lock = cache.load_lock().unwrap();
        let two_min_ago = Utc::now() - Duration::seconds(120);
        lock.set("STORY-075", &two_min_ago.to_rfc3339());
        lock.save(&cache.root).unwrap();

        let fresh = cache.read_if_fresh("STORY-075", "story", ttl);
        assert_eq!(fresh, None);

        let stale = cache.read_stale("STORY-075", "story");
        assert_eq!(stale, Some("# Story 075\nStale content".to_string()));
    }

    #[test]
    fn test_issue_cache_cold_returns_none() {
        let (cache, _tmp) = make_cache();
        let ttl = Duration::seconds(60);

        assert_eq!(cache.read_if_fresh("NONEXISTENT-001", "rfc", ttl), None);
        assert_eq!(cache.read_stale("NONEXISTENT-001", "rfc"), None);
    }

    #[test]
    fn test_issue_cache_remove_deletes_file_and_lock_entry() {
        let (cache, _tmp) = make_cache();

        cache
            .write("ITERATION-001", "iteration", "content one")
            .unwrap();
        cache
            .write("ITERATION-002", "iteration", "content two")
            .unwrap();

        cache.remove("ITERATION-001", "iteration").unwrap();

        assert!(!cache.doc_path("ITERATION-001", "iteration").exists());
        assert!(cache.doc_path("ITERATION-002", "iteration").exists());

        let lock = cache.load_lock().unwrap();
        assert!(lock.get("ITERATION-001").is_none());
        assert!(lock.get("ITERATION-002").is_some());
    }

    // --- refresh_stale tests ---

    fn backdate_all(cache: &IssueCache, ids: &[&str]) {
        let mut lock = cache.load_lock().unwrap();
        let old = (Utc::now() - Duration::seconds(300)).to_rfc3339();
        for id in ids {
            if lock.get(id).is_some() {
                lock.set(id, &old);
            }
        }
        lock.save(&cache.root).unwrap();
    }

    #[test]
    fn test_refresh_stale_fetches_all_via_issue_list() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();
        let ttl = Duration::seconds(60);

        // Seed 3 stale cache entries
        cache.write("STORY-10", "story", "old content 1").unwrap();
        cache.write("STORY-11", "story", "old content 2").unwrap();
        cache.write("STORY-12", "story", "old content 3").unwrap();
        backdate_all(&cache, &["STORY-10", "STORY-11", "STORY-12"]);

        let gh = MockReader::new(vec![
            make_gh_issue(10, "STORY-001 First story", "Body 1", &["lazyspec:story"]),
            make_gh_issue(11, "STORY-002 Second story", "Body 2", &["lazyspec:story"]),
            make_gh_issue(12, "STORY-003 Third story", "Body 3", &["lazyspec:story"]),
        ])
        .with_graphql_responses(vec![empty_issue_types_response()]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let known_types = vec![story_match_rule()];
        let result = cache
            .refresh_stale(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                "owner/repo",
                &mut issue_map,
                ttl,
                &known_types,
                &Config::default(),
            )
            .unwrap();

        assert_eq!(
            gh.call_count(),
            1,
            "should make exactly one issue_list call"
        );
        assert_eq!(result.refreshed, 3);
        assert!(result.warnings.is_empty());

        // All 3 cache files should exist and lock entries should be fresh
        for id in &["STORY-10", "STORY-11", "STORY-12"] {
            assert!(
                cache.is_fresh(id, ttl),
                "cache entry {} should be fresh after refresh",
                id
            );
        }

        // Issue map should be updated
        assert_eq!(issue_map.get("STORY-10").unwrap().issue_number, 10);
        assert_eq!(issue_map.get("STORY-11").unwrap().issue_number, 11);
        assert_eq!(issue_map.get("STORY-12").unwrap().issue_number, 12);
    }

    // AC4: refresh hook fetches + persists schema ids to gh-schema.json
    #[test]
    fn test_refresh_stale_persists_schema_snapshot_ids() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();
        let ttl = Duration::seconds(60);

        cache.write("STORY-10", "story", "old content 1").unwrap();
        backdate_all(&cache, &["STORY-10"]);

        let gh = MockReader::new(vec![make_gh_issue(
            10,
            "STORY-001 First",
            "Body 1",
            &["lazyspec:story"],
        )])
        .with_issue_types(vec![gh_schema::IssueTypeId {
            name: "Bug".to_string(),
            id: "IT_kwBug".to_string(),
        }]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let result = cache
            .refresh_stale(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                "octo-org/repo",
                &mut issue_map,
                ttl,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();
        assert!(
            result.warnings.is_empty(),
            "warnings: {:?}",
            result.warnings
        );

        let snapshot_file = tmp.path().join(".lazyspec/cache/gh-schema.json");
        assert!(snapshot_file.exists(), "gh-schema.json should be written");

        let snapshot = gh_schema::GhSchemaSnapshot::load(tmp.path());
        assert_eq!(snapshot.issue_type_id("Bug"), Some("IT_kwBug"));
    }

    /// A round that resolved one org issue type and nothing else.
    fn round_with_bug_issue_type() -> FetchSnapshot {
        FetchSnapshot {
            issue_types: Some(vec![gh_schema::IssueTypeId {
                name: "Bug".to_string(),
                id: "IT_kwBug".to_string(),
            }]),
            ..Default::default()
        }
    }

    fn board_fields_response(field_id: &str, options: &[&str]) -> serde_json::Value {
        let opts: Vec<serde_json::Value> = options
            .iter()
            .map(|name| {
                serde_json::json!({
                    "id": format!("opt_{}", name.to_lowercase().replace(' ', "_")),
                    "name": name
                })
            })
            .collect();
        serde_json::json!({
            "data": {"organization": {"projectV2": {"fields": {"nodes": [
                {
                    "__typename": "ProjectV2SingleSelectField",
                    "id": field_id,
                    "name": "Status",
                    "dataType": "SINGLE_SELECT",
                    "options": opts
                }
            ]}}}}
        })
    }

    fn config_with_status_authority(authority: Option<&str>) -> Config {
        let mut config = Config::default();
        config.documents.types = vec![TypeDef {
            status_authority: authority.map(String::from),
            ..story_type_def()
        }];
        config
    }

    fn write_board_7_snapshot(root: &Path, option_name: &str) {
        gh_schema::GhSchemaSnapshot {
            project_fields: vec![gh_schema::ProjectFieldId {
                project_number: 7,
                field_name: "Status".to_string(),
                id: "PVTSSF_prior".to_string(),
                data_type: "SINGLE_SELECT".to_string(),
            }],
            single_select_options: vec![gh_schema::OptionId {
                field_id: "PVTSSF_prior".to_string(),
                name: option_name.to_string(),
                id: "opt_prior".to_string(),
            }],
            ..Default::default()
        }
        .save(root)
        .unwrap();
    }

    #[test]
    fn refresh_schema_snapshot_merges_authority_board_fields() {
        let (cache, tmp) = make_cache();
        let gh = MockGhClient::new().with_graphql_responses(vec![board_fields_response(
            "PVTSSF_b7",
            &["Ready To Start", "In Progress", "Review", "Done"],
        )]);

        let warnings = cache.refresh_schema_snapshot(
            &gh,
            Some(&round_with_bug_issue_type()),
            "octo-org/repo",
            &config_with_status_authority(Some("PROJECT-7")),
        );

        assert!(warnings.is_empty(), "warnings: {:?}", warnings);
        let saved = gh_schema::GhSchemaSnapshot::load(tmp.path());
        assert_eq!(saved.issue_type_id("Bug"), Some("IT_kwBug"));
        assert_eq!(saved.field_id(7, "Status"), Some("PVTSSF_b7"));
        assert_eq!(
            saved.option_id("PVTSSF_b7", "In Progress"),
            Some("opt_in_progress")
        );
        assert_eq!(
            saved.status_lifecycle(7).unwrap().states,
            vec!["ready to start", "in progress", "review", "done"]
        );
    }

    #[test]
    fn refresh_schema_snapshot_makes_no_project_calls_without_status_authority() {
        let (cache, tmp) = make_cache();
        let gh = MockGhClient::new();

        let warnings = cache.refresh_schema_snapshot(
            &gh,
            Some(&round_with_bug_issue_type()),
            "octo-org/repo",
            &config_with_status_authority(None),
        );

        assert!(warnings.is_empty(), "warnings: {:?}", warnings);
        // Issue types ride the round, so persisting them costs no request here.
        assert!(gh.graphql_calls.borrow().is_empty());
        let saved = gh_schema::GhSchemaSnapshot::load(tmp.path());
        assert!(saved.project_fields.is_empty());
    }

    #[test]
    fn refresh_schema_snapshot_keeps_prior_board_fields_when_project_fetch_fails() {
        let (cache, tmp) = make_cache();
        write_board_7_snapshot(tmp.path(), "Review");

        // No canned response, so the project-fields query errors.
        let gh = MockGhClient::new();

        let warnings = cache.refresh_schema_snapshot(
            &gh,
            Some(&round_with_bug_issue_type()),
            "octo-org/repo",
            &config_with_status_authority(Some("PROJECT-7")),
        );

        assert_eq!(warnings.len(), 1, "warnings: {:?}", warnings);
        assert!(
            warnings[0].message.contains('7'),
            "warning should name the board: {}",
            warnings[0].message
        );
        let saved = gh_schema::GhSchemaSnapshot::load(tmp.path());
        assert_eq!(saved.issue_type_id("Bug"), Some("IT_kwBug"));
        assert_eq!(saved.field_id(7, "Status"), Some("PVTSSF_prior"));
        assert_eq!(saved.status_lifecycle(7).unwrap().states, vec!["review"]);
    }

    #[test]
    fn refresh_schema_snapshot_replaces_a_boards_stale_options() {
        let (cache, tmp) = make_cache();
        write_board_7_snapshot(tmp.path(), "Retired Column");

        let gh = MockGhClient::new().with_graphql_responses(vec![board_fields_response(
            "PVTSSF_b7",
            &["Review", "Done"],
        )]);

        let warnings = cache.refresh_schema_snapshot(
            &gh,
            Some(&round_with_bug_issue_type()),
            "octo-org/repo",
            &config_with_status_authority(Some("PROJECT-7")),
        );

        assert!(warnings.is_empty(), "warnings: {:?}", warnings);
        let saved = gh_schema::GhSchemaSnapshot::load(tmp.path());
        assert_eq!(
            saved.status_lifecycle(7).unwrap().states,
            vec!["review", "done"]
        );
        assert!(saved
            .single_select_options
            .iter()
            .all(|o| o.name != "Retired Column"));
        assert_eq!(saved.project_fields.len(), 1);
    }

    #[test]
    fn test_refresh_stale_skips_api_when_all_fresh() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();
        let ttl = Duration::seconds(60);

        // Seed 3 fresh cache entries (default write sets cached_at to now)
        cache.write("STORY-10", "story", "content 1").unwrap();
        cache.write("STORY-11", "story", "content 2").unwrap();
        cache.write("STORY-12", "story", "content 3").unwrap();

        let gh = MockReader::new(vec![]);
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let known_types = vec![story_match_rule()];
        let result = cache
            .refresh_stale(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                "owner/repo",
                &mut issue_map,
                ttl,
                &known_types,
                &Config::default(),
            )
            .unwrap();

        assert_eq!(gh.call_count(), 0, "should not call API when all fresh");
        assert_eq!(
            gh.graphql_call_count(),
            0,
            "should not call graphql when all fresh"
        );
        assert_eq!(result.refreshed, 0);
        assert_eq!(result.unchanged, 3);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_refresh_stale_returns_stale_on_api_failure() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();
        let ttl = Duration::seconds(60);

        // Seed stale cache entries
        cache.write("STORY-10", "story", "stale content 1").unwrap();
        cache.write("STORY-11", "story", "stale content 2").unwrap();
        backdate_all(&cache, &["STORY-10", "STORY-11"]);

        let gh = MockReader::failing();
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let known_types = vec![story_match_rule()];
        let result = cache
            .refresh_stale(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                "owner/repo",
                &mut issue_map,
                ttl,
                &known_types,
                &Config::default(),
            )
            .unwrap();

        assert_eq!(result.refreshed, 0);
        assert_eq!(result.unchanged, 2);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].message.contains("API unreachable"));

        // Stale content should still be readable
        assert_eq!(
            cache.read_stale("STORY-10", "story"),
            Some("stale content 1".to_string())
        );
        assert_eq!(
            cache.read_stale("STORY-11", "story"),
            Some("stale content 2".to_string())
        );
    }

    // AC: a type configured with only `github_issue_type` refreshes its stale
    // entries by discovering candidates via the GraphQL issue-type search resolved
    // through `issue_view`, making zero REST `issue_list` calls.
    #[test]
    fn refresh_stale_issue_type_only_discovers_via_search_not_rest_list() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def_signals(None, Some("Bug"));
        let ttl = Duration::seconds(60);

        cache.write("STORY-10", "story", "old 10").unwrap();
        cache.write("STORY-11", "story", "old 11").unwrap();
        backdate_all(&cache, &["STORY-10", "STORY-11"]);

        // First graphql response is the issue-type search; second is the schema
        // snapshot refresh that runs at the end of refresh_stale.
        let gh = MockReader::new(vec![])
            .with_view_issues(vec![
                make_gh_issue(10, "STORY-001", "Body 10", &[]),
                make_gh_issue(11, "STORY-002", "Body 11", &[]),
            ])
            .with_graphql_responses(vec![
                search_response(&[10, 11]),
                empty_issue_types_response(),
            ]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let result = cache
            .refresh_stale(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                "owner/repo",
                &mut issue_map,
                ttl,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(
            gh.call_count(),
            0,
            "issue_type-only refresh must make zero REST issue_list calls"
        );
        assert_eq!(
            gh.view_call_count(),
            2,
            "each search hit is resolved via issue_view"
        );
        assert_eq!(result.refreshed, 2);
        assert!(
            result.warnings.is_empty(),
            "warnings: {:?}",
            result.warnings
        );
        assert!(cache.is_fresh("STORY-10", ttl));
        assert!(cache.is_fresh("STORY-11", ttl));
        assert_eq!(issue_map.get("STORY-10").unwrap().issue_number, 10);
        assert_eq!(issue_map.get("STORY-11").unwrap().issue_number, 11);
    }

    // AC: with both `tag` and `issue_type` set, refresh_stale refreshes only the
    // AND of the REST-list and search result sets -- an issue in only one is
    // dropped from discovery and left stale.
    #[test]
    fn refresh_stale_both_signals_keeps_only_intersection() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def_signals(Some("Ticket"), Some("Bug"));
        let ttl = Duration::seconds(60);

        cache.write("STORY-10", "story", "old 10").unwrap();
        cache.write("STORY-11", "story", "old 11").unwrap();
        cache.write("STORY-12", "story", "old 12").unwrap();
        backdate_all(&cache, &["STORY-10", "STORY-11", "STORY-12"]);

        // REST returns 10,11,12; search returns 11,12,99. Intersection: 11,12.
        let gh = MockReader::new(vec![
            make_gh_issue(10, "STORY-010", "Body 10", &["Ticket"]),
            make_gh_issue(11, "STORY-011", "Body 11", &["Ticket"]),
            make_gh_issue(12, "STORY-012", "Body 12", &["Ticket"]),
        ])
        .with_graphql_responses(vec![
            search_response(&[11, 12, 99]),
            empty_issue_types_response(),
        ]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let result = cache
            .refresh_stale(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                "owner/repo",
                &mut issue_map,
                ttl,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(
            result.refreshed, 2,
            "only the REST/search intersection is refreshed"
        );
        assert_eq!(
            gh.view_call_count(),
            0,
            "both-signals refresh reuses REST data, never re-fetches via issue_view"
        );
        assert!(cache.is_fresh("STORY-11", ttl));
        assert!(cache.is_fresh("STORY-12", ttl));
        assert!(
            !cache.is_fresh("STORY-10", ttl),
            "REST-only issue is dropped from discovery and left stale"
        );
    }

    // AC: a full first page from the issue-type search (exactly PAGE_SIZE numbers)
    // surfaces the truncation warning through refresh_stale's own warnings.
    #[test]
    fn refresh_stale_warns_when_issue_type_search_fills_first_page() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def_signals(None, Some("Bug"));
        let ttl = Duration::seconds(60);

        // A stale entry so refresh proceeds past the all-fresh short-circuit.
        cache.write("STORY-1", "story", "old 1").unwrap();
        backdate_all(&cache, &["STORY-1"]);

        let full_page: Vec<u64> = (1..=ISSUE_TYPE_SEARCH_PAGE_SIZE as u64).collect();
        let gh = MockReader::new(vec![]).with_graphql_responses(vec![
            search_response(&full_page),
            empty_issue_types_response(),
        ]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let result = cache
            .refresh_stale(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                "owner/repo",
                &mut issue_map,
                ttl,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.message.contains("returned exactly 100")),
            "full-page search should surface a truncation warning via refresh: {:?}",
            result.warnings
        );
    }

    // AC: the tag-only discovery branch filters `issue_list` by the configured
    // `github_issue_tag` (not the plain type-name label) and issues no GraphQL
    // issue-type search.
    #[test]
    fn discover_issues_tag_only_lists_by_tag_and_skips_search() {
        let gh = MockReader::new(vec![make_gh_issue(1, "STORY-001", "Body", &["Ticket"])]);
        let rule = TypeMatchRule {
            name: "story".to_string(),
            label: "lazyspec:story".to_string(),
            tag: Some("Ticket".to_string()),
            issue_type: None,
        };
        let fields = vec!["number".to_string()];

        let discovery = discover_issues(&gh, &gh, "owner/repo", &rule, &fields, None).unwrap();

        assert_eq!(
            gh.call_count(),
            1,
            "tag-only discovery uses exactly one REST issue_list call"
        );
        assert_eq!(
            gh.graphql_call_count(),
            0,
            "tag-only discovery makes no GraphQL issue-type search"
        );
        assert_eq!(
            gh.recorded_list_labels(),
            vec![vec!["Ticket".to_string()]],
            "issue_list must be filtered by the configured tag, not the type-name label"
        );
        assert!(!discovery.search_truncated);
        assert_eq!(discovery.issues.len(), 1);
    }

    // --- fetch_all tests ---

    #[test]
    fn test_fetch_all_populates_cache_with_frontmatter() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();

        let gh = MockReader::new(vec![
            make_gh_issue(10, "STORY-001 First story", "Body 1", &["lazyspec:story"]),
            make_gh_issue(11, "STORY-002 Second story", "Body 2", &["lazyspec:story"]),
            make_gh_issue(12, "STORY-003 Third story", "Body 3", &["lazyspec:story"]),
        ]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let result = cache
            .fetch_all(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                &gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(result.fetched, 3);
        assert_eq!(result.new, 3);
        assert_eq!(result.removed, 0);

        // All cache files exist with parseable frontmatter
        let cache_dir = tmp.path().join(".lazyspec/cache/story");
        for id in &["STORY-10", "STORY-11", "STORY-12"] {
            let path = cache_dir.join(format!("{}.md", id));
            assert!(path.exists(), "cache file for {} should exist", id);
            let content = std::fs::read_to_string(&path).unwrap();
            assert!(content.contains("title:"), "should have title frontmatter");
            assert!(
                content.contains("type: story"),
                "should have type frontmatter"
            );
            assert!(
                content.contains("status:"),
                "should have status frontmatter"
            );
        }

        // cache.lock updated
        let ttl = Duration::seconds(60);
        for id in &["STORY-10", "STORY-11", "STORY-12"] {
            assert!(
                cache.is_fresh(id, ttl),
                "cache.lock for {} should be fresh",
                id
            );
        }

        // issue map entries
        assert_eq!(issue_map.get("STORY-10").unwrap().issue_number, 10);
        assert_eq!(issue_map.get("STORY-11").unwrap().issue_number, 11);
        assert_eq!(issue_map.get("STORY-12").unwrap().issue_number, 12);

        // Verify Store::load can find the documents
        use crate::engine::config::{Config, GithubConfig};
        use crate::engine::store::Store;
        let mut config = Config::default();
        config.documents.types = vec![story_type_def()];
        config.documents.github = Some(GithubConfig {
            repo: Some("owner/repo".to_string()),
            cache_ttl: 60,
        });
        let store = Store::load(tmp.path(), &config).unwrap();
        let filter = crate::engine::store::Filter {
            doc_type: Some(DocType::new("story")),
            status: None,
            tag: None,
        };
        let docs = store.list(&filter);
        assert_eq!(docs.len(), 3);
    }

    // AC (STORY-194): two github-issues types with distinct prefix/name/dir can
    // both independently match the same underlying GitHub issue number. Each
    // fetch_all call is scoped entirely to its own TypeDef -- doc id via
    // type_def.make_id, cache dir via type_def.name -- so both materialize
    // side by side under the same root and shared IssueMap without colliding.
    #[test]
    fn test_fetch_all_dual_materializes_overlapping_issue_across_types() {
        let (cache, tmp) = make_cache();
        let story_type = story_type_def();
        let ticket_type = ticket_type_def();

        // MockReader::issue_list ignores the label filter and always returns
        // this same issue #42 -- standing in for both types' discovery
        // independently surfacing the same GitHub issue.
        let gh = MockReader::new(vec![make_gh_issue(42, "Some Issue", "Body", &[])])
            .with_graphql_responses(vec![
                empty_issue_types_response(),
                empty_issue_types_response(),
            ]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();

        cache
            .fetch_all(
                tmp.path(),
                &story_type,
                &gh,
                &gh,
                &gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        cache
            .fetch_all(
                tmp.path(),
                &ticket_type,
                &gh,
                &gh,
                &gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[ticket_match_rule()],
                &Config::default(),
            )
            .unwrap();

        let story_path = tmp.path().join(".lazyspec/cache/story/STORY-42.md");
        let ticket_path = tmp.path().join(".lazyspec/cache/ticket/TICKET-42.md");
        assert!(story_path.exists(), "story cache file should exist");
        assert!(ticket_path.exists(), "ticket cache file should exist");

        let story_content = std::fs::read_to_string(&story_path).unwrap();
        let ticket_content = std::fs::read_to_string(&ticket_path).unwrap();
        assert!(
            story_content.contains("type: story"),
            "story doc should carry its own type, not ticket's: {}",
            story_content
        );
        assert!(
            ticket_content.contains("type: ticket"),
            "ticket doc should carry its own type, not story's: {}",
            ticket_content
        );

        assert_eq!(issue_map.get("STORY-42").unwrap().issue_number, 42);
        assert_eq!(issue_map.get("TICKET-42").unwrap().issue_number, 42);
    }

    // AC (STORY-194): refresh_stale for one type never touches another type's
    // cache dir, cache.lock entry, or issue-map row, even when both types
    // resolved the same GitHub issue number during their own fetch_all.
    #[test]
    fn test_refresh_stale_isolates_overlapping_issue_across_types() {
        let (cache, tmp) = make_cache();
        let story_type = story_type_def();
        let ticket_type = ticket_type_def();
        let ttl = Duration::seconds(60);

        // Arrange: seed both types' caches for the same issue #42 via fetch_all,
        // exactly as their independent discovery would.
        let seed_gh = MockReader::new(vec![make_gh_issue(42, "Some Issue", "Body v1", &[])])
            .with_graphql_responses(vec![
                empty_issue_types_response(),
                empty_issue_types_response(),
            ]);
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        cache
            .fetch_all(
                tmp.path(),
                &story_type,
                &seed_gh,
                &seed_gh,
                &seed_gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();
        cache
            .fetch_all(
                tmp.path(),
                &ticket_type,
                &seed_gh,
                &seed_gh,
                &seed_gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[ticket_match_rule()],
                &Config::default(),
            )
            .unwrap();

        backdate_all(&cache, &["STORY-42", "TICKET-42"]);

        let ticket_path = tmp.path().join(".lazyspec/cache/ticket/TICKET-42.md");
        let ticket_content_before = std::fs::read_to_string(&ticket_path).unwrap();
        let ticket_lock_before = cache
            .load_lock()
            .unwrap()
            .get("TICKET-42")
            .unwrap()
            .to_string();

        // Act: refresh only the story type, with a changed upstream body so a
        // real write happens (not a no-op "unchanged" skip).
        let story_refresh_gh =
            MockReader::new(vec![make_gh_issue(42, "Some Issue", "Body v2", &[])])
                .with_graphql_responses(vec![empty_issue_types_response()]);
        let story_result = cache
            .refresh_stale(
                tmp.path(),
                &story_type,
                &story_refresh_gh,
                &story_refresh_gh,
                "owner/repo",
                &mut issue_map,
                ttl,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(story_result.refreshed, 1);
        assert!(
            cache.is_fresh("STORY-42", ttl),
            "story entry should be refreshed and fresh"
        );

        // Assert: the story-only refresh left ticket's cache file, lock entry,
        // and issue-map row completely untouched.
        let ticket_content_after = std::fs::read_to_string(&ticket_path).unwrap();
        let ticket_lock_after = cache
            .load_lock()
            .unwrap()
            .get("TICKET-42")
            .unwrap()
            .to_string();
        assert_eq!(
            ticket_content_before, ticket_content_after,
            "ticket cache content must be untouched by a story-only refresh"
        );
        assert_eq!(
            ticket_lock_before, ticket_lock_after,
            "ticket lock entry must be untouched by a story-only refresh"
        );
        assert!(
            !cache.is_fresh("TICKET-42", ttl),
            "ticket entry remains stale since only story was refreshed"
        );
        assert_eq!(issue_map.get("TICKET-42").unwrap().issue_number, 42);

        // Act again, the other way around: refresh only ticket now, and
        // confirm story (already refreshed above) is left alone this time.
        let story_path = tmp.path().join(".lazyspec/cache/story/STORY-42.md");
        let story_content_before = std::fs::read_to_string(&story_path).unwrap();
        let story_lock_before = cache
            .load_lock()
            .unwrap()
            .get("STORY-42")
            .unwrap()
            .to_string();

        let ticket_refresh_gh =
            MockReader::new(vec![make_gh_issue(42, "Some Issue", "Body v3", &[])])
                .with_graphql_responses(vec![empty_issue_types_response()]);
        let ticket_result = cache
            .refresh_stale(
                tmp.path(),
                &ticket_type,
                &ticket_refresh_gh,
                &ticket_refresh_gh,
                "owner/repo",
                &mut issue_map,
                ttl,
                &[ticket_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(ticket_result.refreshed, 1);
        assert!(cache.is_fresh("TICKET-42", ttl));

        let story_content_after = std::fs::read_to_string(&story_path).unwrap();
        let story_lock_after = cache
            .load_lock()
            .unwrap()
            .get("STORY-42")
            .unwrap()
            .to_string();
        assert_eq!(
            story_content_before, story_content_after,
            "story cache content must be untouched by a ticket-only refresh"
        );
        assert_eq!(
            story_lock_before, story_lock_after,
            "story lock entry must be untouched by a ticket-only refresh"
        );
    }

    #[test]
    // A round that resolved nothing (transport failure, or none run at all)
    // must leave the ids already on disk alone: the fetch still writes its
    // issues, and offline resolution keeps working against the prior schema.
    // The failure itself is the round's to report, so this pass adds no warning.
    fn test_fetch_all_without_a_round_keeps_prior_issue_types() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();
        gh_schema::GhSchemaSnapshot {
            issue_types: vec![gh_schema::IssueTypeId {
                name: "Bug".to_string(),
                id: "IT_kwPrior".to_string(),
            }],
            ..Default::default()
        }
        .save(tmp.path())
        .unwrap();

        let gh = MockReader::new(vec![make_gh_issue(
            10,
            "STORY-001 First",
            "Body",
            &["lazyspec:story"],
        )]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let result = cache
            .fetch_all(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                &gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(result.fetched, 1);
        assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
        let saved = gh_schema::GhSchemaSnapshot::load(tmp.path());
        assert_eq!(saved.issue_type_id("Bug"), Some("IT_kwPrior"));
    }

    // A round that resolved a user-owned repo's (empty) issue types is an
    // answer, so it replaces what was on disk rather than being mistaken for a
    // failed read.
    #[test]
    fn test_fetch_all_with_an_empty_round_clears_stale_issue_types() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();
        gh_schema::GhSchemaSnapshot {
            issue_types: vec![gh_schema::IssueTypeId {
                name: "Bug".to_string(),
                id: "IT_kwPrior".to_string(),
            }],
            ..Default::default()
        }
        .save(tmp.path())
        .unwrap();

        let gh = MockReader::new(vec![]);
        let round = FetchSnapshot {
            issue_types: Some(Vec::new()),
            ..Default::default()
        };

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        cache
            .fetch_all(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                &gh,
                Some(&round),
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(
            gh_schema::GhSchemaSnapshot::load(tmp.path()).issue_type_id("Bug"),
            None
        );
    }

    #[test]
    fn test_fetch_all_cleans_up_removed_issues() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();

        // Pre-populate cache with 3 docs
        let initial_gh = MockReader::new(vec![
            make_gh_issue(10, "STORY-001 First", "Body 1", &["lazyspec:story"]),
            make_gh_issue(11, "STORY-002 Second", "Body 2", &["lazyspec:story"]),
            make_gh_issue(12, "STORY-003 Third", "Body 3", &["lazyspec:story"]),
        ]);
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        cache
            .fetch_all(
                tmp.path(),
                &type_def,
                &initial_gh,
                &initial_gh,
                &initial_gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        // Second fetch returns only 2 of the 3
        let updated_gh = MockReader::new(vec![
            make_gh_issue(10, "STORY-001 First", "Body 1 updated", &["lazyspec:story"]),
            make_gh_issue(
                11,
                "STORY-002 Second",
                "Body 2 updated",
                &["lazyspec:story"],
            ),
        ]);
        let result = cache
            .fetch_all(
                tmp.path(),
                &type_def,
                &updated_gh,
                &updated_gh,
                &updated_gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(result.fetched, 2);
        assert_eq!(result.removed, 1);

        // STORY-12 should be gone
        let cache_dir = tmp.path().join(".lazyspec/cache/story");
        assert!(cache_dir.join("STORY-10.md").exists());
        assert!(cache_dir.join("STORY-11.md").exists());
        assert!(!cache_dir.join("STORY-12.md").exists());

        // cache.lock should not contain STORY-12
        let lock = cache.load_lock().unwrap();
        assert!(lock.get("STORY-10").is_some());
        assert!(lock.get("STORY-11").is_some());
        assert!(lock.get("STORY-12").is_none());

        // issue map should not contain STORY-12
        assert!(issue_map.get("STORY-10").is_some());
        assert!(issue_map.get("STORY-11").is_some());
        assert!(issue_map.get("STORY-12").is_none());
    }

    #[test]
    fn test_fetch_all_derives_id_from_prefix_and_number() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();

        // Issue with plain title (no STORY-XXX pattern), issue number 33
        let gh = MockReader::new(vec![make_gh_issue(
            33,
            "test",
            "Plain body",
            &["lazyspec:story"],
        )]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let result = cache
            .fetch_all(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                &gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(result.fetched, 1);
        assert_eq!(result.new, 1);

        // ID should be "STORY-33", not "33"
        let cache_dir = tmp.path().join(".lazyspec/cache/story");
        assert!(
            cache_dir.join("STORY-33.md").exists(),
            "cache file should be STORY-33.md"
        );

        let ttl = Duration::seconds(60);
        assert!(
            cache.is_fresh("STORY-33", ttl),
            "lock entry should use STORY-33"
        );

        assert_eq!(issue_map.get("STORY-33").unwrap().issue_number, 33);
    }

    #[test]
    fn test_fetch_all_ignores_title_embedded_id() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();

        // Issue with title "STORY-999 Some title" but issue number 10
        // ID should be STORY-10 (from number), not STORY-999 (from title)
        let gh = MockReader::new(vec![make_gh_issue(
            10,
            "STORY-999 Some title",
            "Body here",
            &["lazyspec:story"],
        )]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let result = cache
            .fetch_all(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                &gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(result.fetched, 1);
        assert_eq!(result.new, 1);

        // Should use issue number, not title-embedded ID
        let cache_dir = tmp.path().join(".lazyspec/cache/story");
        assert!(
            cache_dir.join("STORY-10.md").exists(),
            "cache file should be STORY-10.md"
        );
        assert!(
            !cache_dir.join("STORY-999.md").exists(),
            "should NOT use title-derived ID STORY-999"
        );

        let ttl = Duration::seconds(60);
        assert!(cache.is_fresh("STORY-10", ttl));
        assert!(!cache.is_fresh("STORY-999", ttl));

        assert_eq!(issue_map.get("STORY-10").unwrap().issue_number, 10);
        assert!(issue_map.get("STORY-999").is_none());
    }

    fn story_type_def_signals(tag: Option<&str>, issue_type: Option<&str>) -> TypeDef {
        TypeDef {
            github_issue_tag: tag.map(|s| s.to_string()),
            github_issue_type: issue_type.map(|s| s.to_string()),
            ..story_type_def()
        }
    }

    fn search_response(numbers: &[u64]) -> serde_json::Value {
        let nodes: Vec<serde_json::Value> = numbers
            .iter()
            .map(|n| serde_json::json!({ "number": n }))
            .collect();
        serde_json::json!({ "data": { "search": { "nodes": nodes } } })
    }

    // AC: a type configured with only `github_issue_type` discovers candidates via
    // the GraphQL issue-type search resolved through `issue_view`, making zero REST
    // `issue_list` calls.
    #[test]
    fn fetch_all_issue_type_only_discovers_via_search_not_rest_list() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def_signals(None, Some("Bug"));

        let gh = MockReader::new(vec![])
            .with_view_issues(vec![
                make_gh_issue(10, "STORY-001", "Body 10", &[]),
                make_gh_issue(11, "STORY-002", "Body 11", &[]),
            ])
            .with_graphql_responses(vec![search_response(&[10, 11])]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let result = cache
            .fetch_all(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                &gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(
            gh.call_count(),
            0,
            "issue_type-only discovery must make zero REST issue_list calls"
        );
        assert_eq!(
            gh.view_call_count(),
            2,
            "each search hit is resolved via issue_view"
        );
        assert_eq!(result.fetched, 2);

        let cache_dir = tmp.path().join(".lazyspec/cache/story");
        assert!(cache_dir.join("STORY-10.md").exists());
        assert!(cache_dir.join("STORY-11.md").exists());
    }

    // AC: with both `tag` and `issue_type` set the candidate set is the AND of the
    // REST-list result and the search result -- an issue in only one is dropped.
    #[test]
    fn fetch_all_both_signals_keeps_only_intersection() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def_signals(Some("Ticket"), Some("Bug"));

        // REST returns 10, 11, 12; search returns 11, 12, 99. Intersection: 11, 12.
        let gh = MockReader::new(vec![
            make_gh_issue(10, "STORY-010", "Body 10", &["Ticket"]),
            make_gh_issue(11, "STORY-011", "Body 11", &["Ticket"]),
            make_gh_issue(12, "STORY-012", "Body 12", &["Ticket"]),
        ])
        .with_graphql_responses(vec![search_response(&[11, 12, 99])]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let result = cache
            .fetch_all(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                &gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(
            result.fetched, 2,
            "only the REST/search intersection survives"
        );
        assert_eq!(
            gh.view_call_count(),
            0,
            "both-signals intersection reuses REST data, never re-fetches via issue_view"
        );
        let cache_dir = tmp.path().join(".lazyspec/cache/story");
        assert!(cache_dir.join("STORY-11.md").exists());
        assert!(cache_dir.join("STORY-12.md").exists());
        assert!(
            !cache_dir.join("STORY-10.md").exists(),
            "REST-only issue must be dropped"
        );
        assert!(
            !cache_dir.join("STORY-99.md").exists(),
            "search-only issue must be dropped"
        );
    }

    // AC: a full first page from the issue-type search (exactly PAGE_SIZE numbers)
    // surfaces a truncation warning mirroring the REST FETCH_LIMIT warning.
    #[test]
    fn fetch_all_warns_when_issue_type_search_fills_first_page() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def_signals(None, Some("Bug"));

        let full_page: Vec<u64> = (1..=ISSUE_TYPE_SEARCH_PAGE_SIZE as u64).collect();
        let gh = MockReader::new(vec![]).with_graphql_responses(vec![search_response(&full_page)]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let result = cache
            .fetch_all(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                &gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.message.contains("returned exactly 100")),
            "full-page search should warn about truncation: {:?}",
            result.warnings
        );
    }

    // AC: a partial page from the issue-type search does not warn about truncation.
    #[test]
    fn fetch_all_no_truncation_warning_when_search_below_page_size() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def_signals(None, Some("Bug"));

        let gh = MockReader::new(vec![]).with_graphql_responses(vec![search_response(&[1, 2, 3])]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let result = cache
            .fetch_all(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                &gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert!(
            !result
                .warnings
                .iter()
                .any(|w| w.message.contains("returned exactly")),
            "partial-page search must not warn about truncation: {:?}",
            result.warnings
        );
    }

    // Custom `label_override`: an issue whose only type label is "Ticket" (no
    // `lazyspec:` prefix) resolves to that type on read, and the custom label is
    // not carried as a tag.
    #[test]
    fn parse_issue_recognizes_custom_label_type() {
        let issue = make_gh_issue(
            5,
            "Ticketed work",
            "<!-- lazyspec\n---\ndate: 2026-03-27\n---\n-->\n\nbody",
            &["Ticket", "team-x"],
        );
        let known_types = vec![ticket_match_rule()];
        let (meta, _) = parse_issue(
            &issue,
            "ticket",
            &known_types,
            &[],
            None,
            &IssueMap::default(),
            "draft",
            "complete",
        );
        assert_eq!(meta.doc_type.as_str(), "ticket");
        assert_eq!(meta.tags, vec!["team-x"]);
    }

    // STORY-223 AC1/AC3: an issue with no explicit lifecycle status inherits the
    // remote open/closed state into the type's OWN custom lifecycle -- open maps
    // to the first active state and closed to the terminal state, not the
    // hardcoded draft/complete.
    #[test]
    fn parse_issue_inherits_remote_state_into_custom_lifecycle() {
        let body = "<!-- lazyspec\n---\ndate: 2026-03-27\n---\n-->\n\nbody";

        let mut open_issue = make_gh_issue(1, "work", body, &["lazyspec:story"]);
        open_issue.state = "OPEN".to_string();
        let mut closed_issue = make_gh_issue(2, "work", body, &["lazyspec:story"]);
        closed_issue.state = "CLOSED".to_string();

        let known_types = vec![story_match_rule()];
        let (open_meta, _) = parse_issue(
            &open_issue,
            "story",
            &known_types,
            &[],
            None,
            &IssueMap::default(),
            "backlog",
            "shipped",
        );
        let (closed_meta, _) = parse_issue(
            &closed_issue,
            "story",
            &known_types,
            &[],
            None,
            &IssueMap::default(),
            "backlog",
            "shipped",
        );

        assert_eq!(open_meta.status.as_str(), "backlog");
        assert_eq!(closed_meta.status.as_str(), "shipped");
    }

    // STORY-222 AC3: an issue's native assignee is inherited into
    // `DocMeta.assignee` (the first entry when multiple), and an unassigned
    // issue yields `None`.
    #[test]
    fn parse_issue_inherits_native_assignee_first_entry() {
        use crate::engine::gh::GhAssignee;

        let body = "<!-- lazyspec\n---\ndate: 2026-03-27\n---\n-->\n\nbody";
        let mut issue = make_gh_issue(1, "work", body, &["lazyspec:story"]);
        issue.assignees = vec![
            GhAssignee {
                login: "carol".to_string(),
            },
            GhAssignee {
                login: "dave".to_string(),
            },
        ];

        let (meta, _) = parse_issue(
            &issue,
            "story",
            &[story_match_rule()],
            &[],
            None,
            &IssueMap::default(),
            "draft",
            "complete",
        );

        assert_eq!(
            meta.assignee,
            Some("carol".to_string()),
            "multi-assignee maps to the first login"
        );
    }

    #[test]
    fn parse_issue_unassigned_issue_yields_none_assignee() {
        let body = "<!-- lazyspec\n---\ndate: 2026-03-27\n---\n-->\n\nbody";
        let issue = make_gh_issue(1, "work", body, &["lazyspec:story"]);
        assert!(issue.assignees.is_empty());

        let (meta, _) = parse_issue(
            &issue,
            "story",
            &[story_match_rule()],
            &[],
            None,
            &IssueMap::default(),
            "draft",
            "complete",
        );

        assert_eq!(meta.assignee, None);
    }

    // STORY-222 AC3 (remote is source of truth): a body that has no assignee but
    // the native `assignees` field is set -- the native field wins and the doc
    // inherits it, proving the read path does not depend on the body comment.
    #[test]
    fn parse_issue_native_assignee_overrides_bodyless_value() {
        use crate::engine::gh::GhAssignee;

        // Body deserialize fails (no lazyspec comment), exercising the fallback
        // path; the native assignee must still be inherited.
        let mut issue = make_gh_issue(1, "work", "plain body, no comment", &["lazyspec:story"]);
        issue.assignees = vec![GhAssignee {
            login: "erin".to_string(),
        }];

        let (meta, _) = parse_issue(
            &issue,
            "story",
            &[story_match_rule()],
            &[],
            None,
            &IssueMap::default(),
            "draft",
            "complete",
        );

        assert_eq!(meta.assignee, Some("erin".to_string()));
    }

    // Regression: a type with no override still resolves via its default
    // `lazyspec:{name}` label, unchanged from before.
    #[test]
    fn parse_issue_recognizes_default_label_type() {
        let issue = make_gh_issue(
            6,
            "Default labelled",
            "<!-- lazyspec\n---\ndate: 2026-03-27\n---\n-->\n\nbody",
            &["lazyspec:story", "team-y"],
        );
        let known_types = vec![story_match_rule()];
        let (meta, _) = parse_issue(
            &issue,
            "story",
            &known_types,
            &[],
            None,
            &IssueMap::default(),
            "draft",
            "complete",
        );
        assert_eq!(meta.doc_type.as_str(), "story");
        assert_eq!(meta.tags, vec!["team-y"]);
    }

    // Fallback path (no lazyspec comment): a custom label is excluded from tags,
    // just like the default-prefixed case.
    #[test]
    fn parse_issue_fallback_excludes_custom_label_from_tags() {
        let issue = make_gh_issue(7, "Plain ticket", "Just a plain body", &["Ticket", "extra"]);
        let known_types = vec![ticket_match_rule()];
        let (meta, _) = parse_issue(
            &issue,
            "ticket",
            &known_types,
            &[],
            None,
            &IssueMap::default(),
            "draft",
            "complete",
        );
        assert_eq!(meta.doc_type.as_str(), "ticket");
        assert_eq!(meta.tags, vec!["extra"]);
    }

    fn make_gh_issue_with_author(
        number: u64,
        title: &str,
        body: &str,
        labels: &[&str],
        author: Option<&str>,
    ) -> GhIssue {
        let mut issue = make_gh_issue(number, title, body, labels);
        issue.author = author.map(|login| GhAuthor {
            login: login.to_string(),
        });
        issue
    }

    #[test]
    fn parse_issue_uses_gh_author() {
        let issue = make_gh_issue_with_author(
            1,
            "Test issue",
            "<!-- lazyspec\n---\ndate: 2026-03-27\n---\n-->\n\nbody",
            &["lazyspec:story"],
            Some("jkaloger"),
        );
        let known_types = vec![story_match_rule()];
        let (meta, _) = parse_issue(
            &issue,
            "story",
            &known_types,
            &[],
            None,
            &IssueMap::default(),
            "draft",
            "complete",
        );
        assert_eq!(meta.author, "@jkaloger");
    }

    #[test]
    fn parse_issue_with_no_author_returns_unknown() {
        let issue = make_gh_issue_with_author(
            2,
            "Test issue",
            "<!-- lazyspec\n---\ndate: 2026-03-27\n---\n-->\n\nbody",
            &["lazyspec:story"],
            None,
        );
        let known_types = vec![story_match_rule()];
        let (meta, _) = parse_issue(
            &issue,
            "story",
            &known_types,
            &[],
            None,
            &IssueMap::default(),
            "draft",
            "complete",
        );
        assert_eq!(meta.author, "unknown");
    }

    #[test]
    fn parse_issue_fallback_path_uses_gh_author() {
        // Body without lazyspec comment triggers fallback path
        let issue = make_gh_issue_with_author(
            3,
            "Plain issue",
            "Just a plain body",
            &["lazyspec:story"],
            Some("octocat"),
        );
        let known_types = vec![story_match_rule()];
        let (meta, _) = parse_issue(
            &issue,
            "story",
            &known_types,
            &[],
            None,
            &IssueMap::default(),
            "draft",
            "complete",
        );
        assert_eq!(meta.author, "@octocat");
    }

    #[test]
    fn parse_issue_overrides_embedded_author() {
        // Body has author in YAML, but parse_issue should override with GH author
        let issue = make_gh_issue_with_author(
            4,
            "Test issue",
            "<!-- lazyspec\n---\nauthor: embedded-author\ndate: 2026-03-27\n---\n-->\n\nbody",
            &["lazyspec:story"],
            Some("jkaloger"),
        );
        let known_types = vec![story_match_rule()];
        let (meta, _) = parse_issue(
            &issue,
            "story",
            &known_types,
            &[],
            None,
            &IssueMap::default(),
            "draft",
            "complete",
        );
        assert_eq!(meta.author, "@jkaloger");
    }

    #[test]
    fn fetch_all_populates_author_from_gh_issue() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();

        let gh = MockReader::new(vec![make_gh_issue_with_author(
            10,
            "Story with author",
            "Body 1",
            &["lazyspec:story"],
            Some("jkaloger"),
        )]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        cache
            .fetch_all(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                &gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        let cache_dir = tmp.path().join(".lazyspec/cache/story");
        let content = std::fs::read_to_string(cache_dir.join("STORY-10.md")).unwrap();
        assert!(
            content.contains("@jkaloger"),
            "cache file should contain author from GH issue, got: {}",
            content
        );
    }

    // --- ITERATION-353: open/closed never drives a board-bound type ---

    fn authority_story_type_def() -> TypeDef {
        TypeDef {
            status_authority: Some("PROJECT-7".to_string()),
            lifecycle: Lifecycle {
                states: vec![
                    "ready to start".to_string(),
                    "in progress".to_string(),
                    "review".to_string(),
                    "done".to_string(),
                ],
                edges: vec![],
            },
            ..story_type_def()
        }
    }

    fn cached_status(tmp: &TempDir, id: &str) -> String {
        let path = tmp
            .path()
            .join(".lazyspec/cache/story")
            .join(format!("{}.md", id));
        let content = std::fs::read_to_string(path).unwrap();
        DocMeta::parse(&content)
            .unwrap()
            .status
            .as_str()
            .to_string()
    }

    // AC2: a bodyless CLOSED issue on a board-bound type parses with NO status --
    // the board's Status cell (injected right after the fetch) is the only
    // lifecycle, so open/closed must not write a second, disjoint one.
    #[test]
    fn fetch_all_gives_a_board_bound_type_no_status_from_open_closed() {
        let (cache, tmp) = make_cache();
        let type_def = authority_story_type_def();

        let mut issue = make_gh_issue(10, "Closed on GitHub", "plain body", &["lazyspec:story"]);
        issue.state = "CLOSED".to_string();
        let gh = MockReader::new(vec![issue]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        cache
            .fetch_all(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                &gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(cached_status(&tmp, "STORY-10"), "");
    }

    // A `status_authority` that names no board number nominates no board: the
    // read path consults none, so suppressing open/closed as well would leave
    // every doc of the type with no status at all and nothing to explain it.
    #[test]
    fn fetch_all_derives_open_closed_for_an_unparseable_status_authority() {
        let (cache, tmp) = make_cache();
        let type_def = TypeDef {
            status_authority: Some("PROJECT-seven".to_string()),
            ..story_type_def()
        };

        let mut issue = make_gh_issue(10, "Closed on GitHub", "plain body", &["lazyspec:story"]);
        issue.state = "CLOSED".to_string();
        let gh = MockReader::new(vec![issue]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        cache
            .fetch_all(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                &gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(cached_status(&tmp, "STORY-10"), "closed");
    }

    // AC10 regression: a type nominating no authority board still maps the
    // open/closed bit onto its lifecycle exactly as before.
    #[test]
    fn fetch_all_still_derives_open_closed_without_a_status_authority() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();

        let mut issue = make_gh_issue(10, "Closed on GitHub", "plain body", &["lazyspec:story"]);
        issue.state = "CLOSED".to_string();
        let gh = MockReader::new(vec![issue]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        cache
            .fetch_all(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                &gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(cached_status(&tmp, "STORY-10"), "closed");
    }

    // The TTL path reads no board, so it must leave the status the last full
    // fetch read off the board standing rather than blanking it.
    #[test]
    fn refresh_stale_carries_over_the_board_status_for_a_board_bound_type() {
        let (cache, tmp) = make_cache();
        let type_def = authority_story_type_def();
        let ttl = Duration::seconds(60);

        cache
            .write(
                "STORY-10",
                "story",
                "---\ntitle: \"Work\"\ntype: story\nstatus: review\nauthor: \"@octocat\"\ndate: 2026-01-01\ntags: []\n---\nplain body\n",
            )
            .unwrap();
        backdate_all(&cache, &["STORY-10"]);

        let mut issue = make_gh_issue(10, "Work", "plain body", &["lazyspec:story"]);
        issue.state = "CLOSED".to_string();
        let gh =
            MockReader::new(vec![issue]).with_graphql_responses(vec![empty_issue_types_response()]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        cache
            .refresh_stale(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                "owner/repo",
                &mut issue_map,
                ttl,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(cached_status(&tmp, "STORY-10"), "review");
    }

    // AC2 on the TTL path: lazyspec embeds `status:` in the issue body, so an
    // issue the board has since moved still advertises the status it held when
    // last written. The body must not win over what the board last said.
    #[test]
    fn refresh_stale_ignores_the_body_status_for_a_board_bound_type() {
        let (cache, tmp) = make_cache();
        let type_def = authority_story_type_def();
        let ttl = Duration::seconds(60);

        cache
            .write(
                "STORY-10",
                "story",
                "---\ntitle: \"Work\"\ntype: story\nstatus: done\nauthor: \"@octocat\"\ndate: 2026-01-01\ntags: []\n---\nplain body\n",
            )
            .unwrap();
        backdate_all(&cache, &["STORY-10"]);

        let issue = make_gh_issue(
            10,
            "Work",
            "<!-- lazyspec\n---\nstatus: ready to start\ndate: 2026-01-01\n---\n-->\n\nplain body",
            &["lazyspec:story"],
        );
        let gh =
            MockReader::new(vec![issue]).with_graphql_responses(vec![empty_issue_types_response()]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        cache
            .refresh_stale(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                "owner/repo",
                &mut issue_map,
                ttl,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(cached_status(&tmp, "STORY-10"), "done");
    }

    // The mirror of the above: with no authority board the body's status is the
    // doc's status, exactly as before this slice.
    #[test]
    fn refresh_stale_takes_the_body_status_without_a_status_authority() {
        let (cache, tmp) = make_cache();
        let type_def = TypeDef {
            lifecycle: Lifecycle {
                states: vec![
                    "ready to start".to_string(),
                    "in progress".to_string(),
                    "done".to_string(),
                ],
                edges: vec![],
            },
            ..story_type_def()
        };
        let ttl = Duration::seconds(60);

        cache
            .write(
                "STORY-10",
                "story",
                "---\ntitle: \"Work\"\ntype: story\nstatus: done\nauthor: \"@octocat\"\ndate: 2026-01-01\ntags: []\n---\nplain body\n",
            )
            .unwrap();
        backdate_all(&cache, &["STORY-10"]);

        let issue = make_gh_issue(
            10,
            "Work",
            "<!-- lazyspec\n---\nstatus: ready to start\ndate: 2026-01-01\n---\n-->\n\nplain body",
            &["lazyspec:story"],
        );
        let gh =
            MockReader::new(vec![issue]).with_graphql_responses(vec![empty_issue_types_response()]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        cache
            .refresh_stale(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                "owner/repo",
                &mut issue_map,
                ttl,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(cached_status(&tmp, "STORY-10"), "ready to start");
    }

    // The project-field pass that follows a full fetch can fail (a token without
    // the `project` scope). Blanking every board-bound doc's status on the way in
    // would turn that warning into data loss, so the last known board status is
    // kept for the pass to overwrite.
    #[test]
    fn fetch_all_keeps_the_last_known_board_status_for_the_injection_pass() {
        let (cache, tmp) = make_cache();
        let type_def = authority_story_type_def();

        cache
            .write(
                "STORY-10",
                "story",
                "---\ntitle: \"Work\"\ntype: story\nstatus: in progress\nauthor: \"@octocat\"\ndate: 2026-01-01\ntags: []\n---\nplain body\n",
            )
            .unwrap();

        let issue = make_gh_issue(10, "Work", "plain body", &["lazyspec:story"]);
        let gh = MockReader::new(vec![issue]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        cache
            .fetch_all(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                &gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(cached_status(&tmp, "STORY-10"), "in progress");
    }

    // Nothing to carry over: a doc the cache has never seen at a board column
    // stays unset rather than inheriting `closed`.
    #[test]
    fn refresh_stale_leaves_an_uncached_board_bound_doc_unset() {
        let (cache, tmp) = make_cache();
        let type_def = authority_story_type_def();
        let ttl = Duration::seconds(60);

        cache.write("STORY-10", "story", "old content").unwrap();
        backdate_all(&cache, &["STORY-10"]);

        let mut issue = make_gh_issue(10, "Work", "plain body", &["lazyspec:story"]);
        issue.state = "CLOSED".to_string();
        let gh =
            MockReader::new(vec![issue]).with_graphql_responses(vec![empty_issue_types_response()]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        cache
            .refresh_stale(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                "owner/repo",
                &mut issue_map,
                ttl,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(cached_status(&tmp, "STORY-10"), "");
    }

    #[test]
    fn parse_issue_uses_created_at_for_date() {
        let mut issue = make_gh_issue(1, "Test issue", "Just a plain body", &["lazyspec:story"]);
        issue.created_at = "2025-06-15T09:30:00Z".to_string();
        let known_types = vec![story_match_rule()];
        let (meta, _) = parse_issue(
            &issue,
            "story",
            &known_types,
            &[],
            None,
            &IssueMap::default(),
            "draft",
            "complete",
        );
        assert_eq!(
            meta.date,
            chrono::NaiveDate::from_ymd_opt(2025, 6, 15).unwrap()
        );
    }

    #[test]
    fn parse_issue_falls_back_to_today_on_bad_created_at() {
        let mut issue = make_gh_issue(2, "Test issue", "Just a plain body", &["lazyspec:story"]);
        issue.created_at = "not-a-date".to_string();
        let known_types = vec![story_match_rule()];
        let (meta, _) = parse_issue(
            &issue,
            "story",
            &known_types,
            &[],
            None,
            &IssueMap::default(),
            "draft",
            "complete",
        );
        assert_eq!(meta.date, Utc::now().date_naive());
    }

    #[test]
    fn parse_issue_falls_back_to_today_on_empty_created_at() {
        let mut issue = make_gh_issue(3, "Test issue", "Just a plain body", &["lazyspec:story"]);
        issue.created_at = String::new();
        let known_types = vec![story_match_rule()];
        let (meta, _) = parse_issue(
            &issue,
            "story",
            &known_types,
            &[],
            None,
            &IssueMap::default(),
            "draft",
            "complete",
        );
        assert_eq!(meta.date, Utc::now().date_naive());
    }

    // AC1: native issue_type surfaces as an `issue_type` string attribute,
    // orthogonal to the lazyspec:story type label.
    #[test]
    fn parse_issue_surfaces_native_issue_type_as_attribute() {
        let mut issue = make_gh_issue(
            1,
            "Test issue",
            "<!-- lazyspec\n---\ndate: 2026-03-27\n---\n-->\n\nbody",
            &["lazyspec:story"],
        );
        issue.issue_type = Some("Bug".to_string());
        let known_types = vec![story_match_rule()];
        let (meta, _) = parse_issue(
            &issue,
            "story",
            &known_types,
            &[],
            None,
            &IssueMap::default(),
            "draft",
            "complete",
        );
        assert_eq!(
            meta.attributes.get("issue_type"),
            Some(&AttrValue::Str("Bug".to_string()))
        );
    }

    // AC2: no native type -> attribute absent (not empty, not default).
    #[test]
    fn parse_issue_omits_issue_type_when_unset() {
        let issue = make_gh_issue(
            2,
            "Test issue",
            "<!-- lazyspec\n---\ndate: 2026-03-27\n---\n-->\n\nbody",
            &["lazyspec:story"],
        );
        assert!(issue.issue_type.is_none());
        let known_types = vec![story_match_rule()];
        let (meta, _) = parse_issue(
            &issue,
            "story",
            &known_types,
            &[],
            None,
            &IssueMap::default(),
            "draft",
            "complete",
        );
        assert!(!meta.attributes.contains_key("issue_type"));
    }

    // AC6 (read half): lazyspec:story label and native Bug type coexist.
    #[test]
    fn parse_issue_doc_type_and_issue_type_are_orthogonal() {
        let mut issue = make_gh_issue(
            3,
            "Test issue",
            "<!-- lazyspec\n---\ndate: 2026-03-27\n---\n-->\n\nbody",
            &["lazyspec:story"],
        );
        issue.issue_type = Some("Bug".to_string());
        let known_types = vec![story_match_rule()];
        let (meta, _) = parse_issue(
            &issue,
            "story",
            &known_types,
            &[],
            None,
            &IssueMap::default(),
            "draft",
            "complete",
        );
        assert_eq!(meta.doc_type.as_str(), "story");
        assert_eq!(
            meta.attributes.get("issue_type"),
            Some(&AttrValue::Str("Bug".to_string()))
        );
    }

    // AC2 (fallback body branch): a non-lazyspec body also surfaces issue_type.
    #[test]
    fn parse_issue_fallback_body_surfaces_issue_type() {
        let mut issue = make_gh_issue(4, "Plain issue", "Just a plain body", &["lazyspec:story"]);
        issue.issue_type = Some("Task".to_string());
        let known_types = vec![story_match_rule()];
        let (meta, _) = parse_issue(
            &issue,
            "story",
            &known_types,
            &[],
            None,
            &IssueMap::default(),
            "draft",
            "complete",
        );
        assert_eq!(
            meta.attributes.get("issue_type"),
            Some(&AttrValue::Str("Task".to_string()))
        );
    }

    // ITERATION-225 AC1 (fetch wiring): fetch_all reads the issue's native
    // milestone (json `milestone` field) and, resolving the rel name from the
    // config github_native lookup, writes a forward `targets: MILESTONE-n` into
    // the cached doc.
    #[test]
    fn fetch_all_writes_targets_for_mapped_milestone() {
        use crate::engine::config::RelationshipDef;
        use crate::engine::gh::GhIssueMilestone;
        use crate::engine::issue_map::EntryKind;

        let (cache, tmp) = make_cache();
        let type_def = story_type_def();

        let config = Config {
            relationships: vec![RelationshipDef {
                name: "targets".to_string(),
                inverse: Some("targeted-by".to_string()),
                github_native: Some("milestone".to_string()),
                traversal: None,
            }],
            ..Config::default()
        };

        let mut issue = make_gh_issue(10, "STORY-001 First", "Body", &["lazyspec:story"]);
        issue.milestone = Some(GhIssueMilestone { number: 3 });
        let gh =
            MockReader::new(vec![issue]).with_graphql_responses(vec![empty_issue_types_response()]);

        let mut issue_map = IssueMap::default();
        issue_map.insert_kind("MILESTONE-1", 3, "", "", EntryKind::Milestone);

        cache
            .fetch_all(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                &gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &config,
            )
            .unwrap();

        let cached = cache.read_stale("STORY-10", "story").unwrap();
        assert!(
            cached.contains("targets: MILESTONE-1"),
            "cached doc must carry forward targets, got:\n{cached}"
        );
    }

    // ITERATION-225 AC1: an issue with a native GitHub milestone gets a forward
    // `targets: MILESTONE-n` relation, the milestone number resolved to its doc
    // via a milestone-kind issue-map entry. Rel name comes from the caller (the
    // config github_native lookup), not a hardcode.
    #[test]
    fn parse_issue_injects_targets_for_mapped_milestone() {
        use crate::engine::gh::GhIssueMilestone;
        use crate::engine::issue_map::EntryKind;

        let mut issue = make_gh_issue(3, "Test issue", "body", &["lazyspec:story"]);
        issue.milestone = Some(GhIssueMilestone { number: 7 });

        let mut map = IssueMap::default();
        map.insert_kind("MILESTONE-2", 7, "", "", EntryKind::Milestone);

        let known_types = vec![story_match_rule()];
        let (meta, _) = parse_issue(
            &issue,
            "story",
            &known_types,
            &[],
            Some("targets"),
            &map,
            "draft",
            "complete",
        );

        let targets: Vec<(&str, &str)> = meta
            .related
            .iter()
            .map(|r| (r.rel_type.as_str(), r.target.as_str()))
            .collect();
        assert_eq!(targets, vec![("targets", "MILESTONE-2")]);
    }

    // AC3: a milestone number with no synced doc (or no configured milestone rel)
    // produces no relation -- no dangling target is written.
    #[test]
    fn parse_issue_skips_unmapped_or_unconfigured_milestone() {
        use crate::engine::gh::GhIssueMilestone;

        let mut issue = make_gh_issue(3, "Test issue", "body", &["lazyspec:story"]);
        issue.milestone = Some(GhIssueMilestone { number: 7 });
        let known_types = vec![story_match_rule()];

        // Milestone #7 maps to no doc -> skipped even with a configured rel.
        let empty = IssueMap::default();
        let (meta, _) = parse_issue(
            &issue,
            "story",
            &known_types,
            &[],
            Some("targets"),
            &empty,
            "draft",
            "complete",
        );
        assert!(
            meta.related.is_empty(),
            "unmapped milestone must be skipped"
        );

        // No configured milestone rel (None) -> skipped even if mapped.
        use crate::engine::issue_map::EntryKind;
        let mut map = IssueMap::default();
        map.insert_kind("MILESTONE-2", 7, "", "", EntryKind::Milestone);
        let (meta, _) = parse_issue(
            &issue,
            "story",
            &known_types,
            &[],
            None,
            &map,
            "draft",
            "complete",
        );
        assert!(
            meta.related.is_empty(),
            "no github_native=milestone rel -> no relation"
        );
    }

    fn nested_meta(id: &str) -> DocMeta {
        DocMeta {
            path: PathBuf::new(),
            title: id.to_string(),
            doc_type: DocType::new("story"),
            status: Status::new("draft"),
            author: "a".to_string(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 6, 26).unwrap(),
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            attributes: Default::default(),
            id: id.to_string(),
        }
    }

    #[test]
    fn list_cached_descends_into_nested_parent_folders() {
        let (cache, tmp) = make_cache();
        let td = story_type_def();
        let root = tmp.path();

        store_dispatch::write_cache_parent(root, &td, &nested_meta("STORY-100"), "p").unwrap();
        store_dispatch::write_cache_child(
            root,
            &td,
            "STORY-100",
            0,
            2,
            &nested_meta("STORY-12"),
            "c",
        )
        .unwrap();
        store_dispatch::write_cache_child(
            root,
            &td,
            "STORY-100",
            1,
            2,
            &nested_meta("STORY-13"),
            "c",
        )
        .unwrap();
        store_dispatch::write_cache_file(root, &td, &nested_meta("STORY-7"), "flat").unwrap();

        let mut ids = cache.list_cached("story");
        ids.sort();
        assert_eq!(ids, vec!["STORY-100", "STORY-12", "STORY-13", "STORY-7"]);
    }

    #[test]
    fn remove_prunes_nested_child_and_keeps_parent() {
        let (cache, tmp) = make_cache();
        let td = story_type_def();
        let root = tmp.path();

        store_dispatch::write_cache_parent(root, &td, &nested_meta("STORY-100"), "p").unwrap();
        store_dispatch::write_cache_child(
            root,
            &td,
            "STORY-100",
            0,
            2,
            &nested_meta("STORY-12"),
            "c",
        )
        .unwrap();
        store_dispatch::write_cache_child(
            root,
            &td,
            "STORY-100",
            1,
            2,
            &nested_meta("STORY-13"),
            "c",
        )
        .unwrap();

        cache.remove("STORY-12", "story").unwrap();

        let folder = root.join(".lazyspec/cache/story/STORY-100");
        assert!(!folder.join("00-STORY-12.md").exists());
        assert!(folder.join("01-STORY-13.md").is_file());
        assert!(folder.join("index.md").is_file());
    }

    #[test]
    fn remove_parent_deletes_folder() {
        let (cache, tmp) = make_cache();
        let td = story_type_def();
        let root = tmp.path();

        store_dispatch::write_cache_parent(root, &td, &nested_meta("STORY-100"), "p").unwrap();
        store_dispatch::write_cache_child(
            root,
            &td,
            "STORY-100",
            0,
            1,
            &nested_meta("STORY-12"),
            "c",
        )
        .unwrap();

        cache.remove("STORY-100", "story").unwrap();

        assert!(!root.join(".lazyspec/cache/story/STORY-100").exists());
    }

    // --- fetch_subissue_parentage tests ---

    /// GhGraphql mock for the batched `nodes(ids:)` parentage query. Maps each
    /// parent node id to `Some(child node ids)`, or `None` to emit a `null`
    /// node (inaccessible / non-Issue). `fail` makes every query error,
    /// simulating a chunk-level GraphQL failure. `calls` counts graphql
    /// invocations so tests can assert batching.
    struct ParentGraphql {
        by_node: std::collections::HashMap<String, Option<Vec<String>>>,
        fail: bool,
        calls: std::cell::Cell<usize>,
    }

    impl ParentGraphql {
        fn new(entries: &[(&str, Option<&[&str]>)]) -> Self {
            Self {
                by_node: entries
                    .iter()
                    .map(|(n, kids)| {
                        let kids = kids.map(|k| k.iter().map(|s| s.to_string()).collect());
                        (n.to_string(), kids)
                    })
                    .collect(),
                fail: false,
                calls: std::cell::Cell::new(0),
            }
        }

        fn failing() -> Self {
            Self {
                by_node: std::collections::HashMap::new(),
                fail: true,
                calls: std::cell::Cell::new(0),
            }
        }
    }

    impl GhGraphql for ParentGraphql {
        fn graphql(&self, _query: &str, vars: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
            let ids = vars
                .iter()
                .find(|(k, _)| *k == "ids")
                .and_then(|(_, v)| match v {
                    GqlVar::StrList(l) => Some(l.clone()),
                    _ => None,
                });
            // Schema-snapshot refresh has no `ids` var.
            let Some(ids) = ids else {
                return Ok(empty_issue_types_response());
            };
            self.calls.set(self.calls.get() + 1);
            if self.fail {
                anyhow::bail!("graphql failed");
            }
            let nodes: Vec<serde_json::Value> = ids
                .iter()
                .map(|id| match self.by_node.get(id) {
                    Some(Some(kids)) => serde_json::json!({
                        "id": id,
                        "subIssues": {
                            "nodes": kids
                                .iter()
                                .map(|k| serde_json::json!({"id": k}))
                                .collect::<Vec<_>>()
                        }
                    }),
                    Some(None) => serde_json::Value::Null,
                    None => serde_json::json!({"id": id, "subIssues": {"nodes": []}}),
                })
                .collect();
            Ok(serde_json::json!({"data": {"nodes": nodes}}))
        }

        fn project_items(
            &self,
            _repo: &str,
            _content_node_id: &str,
        ) -> Result<Vec<crate::engine::gh::ProjectItem>> {
            Ok(vec![])
        }

        fn update_project_v2_item_field_value(
            &self,
            _project_id: &str,
            _item_id: &str,
            _field_id: &str,
            _value: &crate::engine::gh::GhFieldValueInput,
        ) -> Result<()> {
            Ok(())
        }

        fn clear_project_field(
            &self,
            _project_id: &str,
            _item_id: &str,
            _field_id: &str,
        ) -> Result<()> {
            Ok(())
        }
    }

    fn node_to_doc(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
        pairs
            .iter()
            .map(|(n, d)| (n.to_string(), d.to_string()))
            .collect()
    }

    #[test]
    fn parentage_resolves_children_in_subissue_order() {
        let gql = ParentGraphql::new(&[("I_parent", Some(&["I_b", "I_a"]))]);
        let map = node_to_doc(&[
            ("I_parent", "STORY-1"),
            ("I_a", "STORY-2"),
            ("I_b", "STORY-3"),
        ]);

        let (parentage, _) = fetch_subissue_parentage(&gql, &map);

        // Children preserve GitHub sub-issue order (I_b before I_a).
        assert_eq!(
            parentage.get("STORY-1"),
            Some(&vec!["STORY-3".to_string(), "STORY-2".to_string()])
        );
    }

    #[test]
    fn parentage_drops_unresolvable_child_nodes() {
        // I_unknown is not among the fetched issues.
        let gql = ParentGraphql::new(&[("I_parent", Some(&["I_a", "I_unknown"]))]);
        let map = node_to_doc(&[("I_parent", "STORY-1"), ("I_a", "STORY-2")]);

        let (parentage, _) = fetch_subissue_parentage(&gql, &map);

        assert_eq!(parentage.get("STORY-1"), Some(&vec!["STORY-2".to_string()]));
    }

    #[test]
    fn parentage_skips_inaccessible_parent_node() {
        // I_p1 comes back as a `null` node (inaccessible / non-Issue); I_p2 resolves.
        let gql = ParentGraphql::new(&[("I_p1", None), ("I_p2", Some(&["I_c"]))]);
        let map = node_to_doc(&[("I_p1", "STORY-1"), ("I_p2", "STORY-2"), ("I_c", "STORY-3")]);

        let (parentage, _) = fetch_subissue_parentage(&gql, &map);

        assert!(!parentage.contains_key("STORY-1"));
        assert_eq!(parentage.get("STORY-2"), Some(&vec!["STORY-3".to_string()]));
    }

    #[test]
    fn parentage_warns_and_skips_chunk_on_graphql_failure() {
        let gql = ParentGraphql::failing();
        let map = node_to_doc(&[("I_p1", "STORY-1"), ("I_p2", "STORY-2")]);

        let (parentage, warnings) = fetch_subissue_parentage(&gql, &map);

        assert!(parentage.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("skipping nesting"));
    }

    #[test]
    fn parentage_omits_childless_parents() {
        let gql = ParentGraphql::new(&[("I_parent", Some(&[]))]);
        let map = node_to_doc(&[("I_parent", "STORY-1")]);

        let (parentage, _) = fetch_subissue_parentage(&gql, &map);

        assert!(parentage.is_empty());
    }

    #[test]
    fn parentage_batches_parents_into_one_query() {
        let gql = ParentGraphql::new(&[
            ("I_a", Some(&["I_c"])),
            ("I_b", Some(&[])),
            ("I_c", Some(&[])),
        ]);
        let map = node_to_doc(&[("I_a", "STORY-1"), ("I_b", "STORY-2"), ("I_c", "STORY-3")]);

        let (parentage, _) = fetch_subissue_parentage(&gql, &map);

        // Three parents, one GraphQL call (no N+1).
        assert_eq!(gql.calls.get(), 1);
        assert_eq!(parentage.get("STORY-1"), Some(&vec!["STORY-3".to_string()]));
    }

    #[test]
    fn parentage_chunks_above_batch_max() {
        let n = gh_subissue::SUB_ISSUE_BATCH_MAX + 1;
        let entries: Vec<(String, String)> = (0..n)
            .map(|i| (format!("I_{i}"), format!("STORY-{i}")))
            .collect();
        let gql = ParentGraphql::new(
            &entries
                .iter()
                .map(|(node, _)| (node.as_str(), Some(&[][..])))
                .collect::<Vec<_>>(),
        );
        let map: std::collections::HashMap<String, String> = entries.into_iter().collect();

        let (_parentage, _) = fetch_subissue_parentage(&gql, &map);

        // N+1 parents span two chunks -> two GraphQL calls (ceil(N/100)).
        assert_eq!(gql.calls.get(), 2);
    }

    #[test]
    fn fetch_all_succeeds_when_parentage_graphql_fails() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();

        // Issues carry node ids so parentage queries fire, but the mock has an
        // empty graphql queue -> every query bails. fetch_all must still Ok.
        let mut issue = make_gh_issue(10, "STORY-001", "Body", &["lazyspec:story"]);
        issue.id = "I_node10".to_string();
        let gh = MockReader::new(vec![issue]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let result = cache
            .fetch_all(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                &gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(result.fetched, 1);
        assert!(cache.doc_path("STORY-10", "story").exists());
    }

    #[test]
    fn remove_flat_doc_still_works() {
        let (cache, tmp) = make_cache();
        let td = story_type_def();
        let root = tmp.path();

        store_dispatch::write_cache_file(root, &td, &nested_meta("STORY-7"), "flat").unwrap();
        cache.remove("STORY-7", "story").unwrap();

        assert!(!root.join(".lazyspec/cache/story/STORY-7.md").exists());
    }

    // --- TASK-3: nested materialization on fetch ---

    /// Combined GhIssueReader + GhGraphql mock for nested-fetch tests. Answers
    /// the batched `nodes(ids:)` parentage query, returning a node per requested
    /// id with its configured sub-issue children; any query without an `ids` var
    /// is the schema-snapshot refresh and returns an empty issue-types response.
    struct NestingReader {
        issues: Vec<GhIssue>,
        sub_issues_by_node: std::collections::HashMap<String, Vec<&'static str>>,
    }

    impl NestingReader {
        fn new(issues: Vec<GhIssue>, sub_issues_by_node: &[(&str, &[&'static str])]) -> Self {
            Self {
                issues,
                sub_issues_by_node: sub_issues_by_node
                    .iter()
                    .map(|(node, kids)| (node.to_string(), kids.to_vec()))
                    .collect(),
            }
        }
    }

    impl GhIssueReader for NestingReader {
        fn issue_list(
            &self,
            _repo: &str,
            _labels: &[String],
            _json_fields: &[String],
            _limit: Option<u64>,
        ) -> Result<Vec<GhIssue>> {
            Ok(self.issues.clone())
        }
        fn issue_view(&self, _repo: &str, _number: u64) -> Result<GhIssue> {
            unimplemented!()
        }
        fn issue_comments(
            &self,
            _repo: &str,
            _number: u64,
        ) -> Result<Vec<crate::engine::gh::GhComment>> {
            unimplemented!()
        }
    }

    impl GhGraphql for NestingReader {
        fn graphql(&self, _query: &str, vars: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
            let ids = vars
                .iter()
                .find(|(k, _)| *k == "ids")
                .and_then(|(_, v)| match v {
                    GqlVar::StrList(l) => Some(l.clone()),
                    _ => None,
                });
            // Schema-snapshot refresh: no `ids` var.
            let Some(ids) = ids else {
                return Ok(empty_issue_types_response());
            };
            let nodes: Vec<serde_json::Value> = ids
                .iter()
                .map(|node| {
                    let kids = self
                        .sub_issues_by_node
                        .get(node)
                        .cloned()
                        .unwrap_or_default();
                    serde_json::json!({
                        "id": node,
                        "subIssues": {
                            "nodes": kids
                                .iter()
                                .map(|k| serde_json::json!({"id": k}))
                                .collect::<Vec<_>>()
                        }
                    })
                })
                .collect();
            Ok(serde_json::json!({"data": {"nodes": nodes}}))
        }
        fn project_items(
            &self,
            _repo: &str,
            _content_node_id: &str,
        ) -> Result<Vec<crate::engine::gh::ProjectItem>> {
            Ok(vec![])
        }
        fn update_project_v2_item_field_value(
            &self,
            _project_id: &str,
            _item_id: &str,
            _field_id: &str,
            _value: &crate::engine::gh::GhFieldValueInput,
        ) -> Result<()> {
            Ok(())
        }
        fn clear_project_field(
            &self,
            _project_id: &str,
            _item_id: &str,
            _field_id: &str,
        ) -> Result<()> {
            Ok(())
        }
    }

    impl GhIssueDependencyApi for NestingReader {
        fn list_blocked_by(&self, _repo: &str, _blocked_number: u64) -> Result<Vec<u64>> {
            Ok(vec![])
        }
        fn add_blocked_by(&self, _repo: &str, _blocked: u64, _blocking: u64) -> Result<()> {
            unimplemented!()
        }
        fn remove_blocked_by(&self, _repo: &str, _blocked: u64, _blocking: u64) -> Result<()> {
            unimplemented!()
        }
    }

    fn issue_with_node(number: u64, node: &str) -> GhIssue {
        let mut i = make_gh_issue(number, "title", "body", &["lazyspec:story"]);
        i.id = node.to_string();
        i
    }

    fn fetch_story(
        cache: &IssueCache,
        tmp: &TempDir,
        gh: &NestingReader,
        issue_map: &mut IssueMap,
    ) -> FetchResult {
        cache
            .fetch_all(
                tmp.path(),
                &story_type_def(),
                gh,
                gh,
                gh,
                None,
                "owner/repo",
                issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap()
    }

    fn load_story_store(tmp: &TempDir) -> crate::engine::store::Store {
        use crate::engine::config::{Config, GithubConfig};
        let mut config = Config::default();
        config.documents.types = vec![story_type_def()];
        config.documents.github = Some(GithubConfig {
            repo: Some("owner/repo".to_string()),
            cache_ttl: 60,
        });
        crate::engine::store::Store::load(tmp.path(), &config).unwrap()
    }

    fn path_of(store: &crate::engine::store::Store, id: &str) -> PathBuf {
        let filter = crate::engine::store::Filter {
            doc_type: None,
            status: None,
            tag: None,
        };
        store
            .list(&filter)
            .into_iter()
            .find(|d| d.id == id)
            .unwrap_or_else(|| panic!("doc {} not in store", id))
            .path
            .clone()
    }

    // AC1: fetch materializes <PARENT>/index.md + ordered NN-<child>.md.
    #[test]
    fn fetch_all_materializes_nested_layout_in_subissue_order() {
        let (cache, tmp) = make_cache();
        let gh = NestingReader::new(
            vec![
                issue_with_node(100, "I_parent"),
                issue_with_node(11, "I_a"),
                issue_with_node(12, "I_b"),
            ],
            // GitHub sub-issue order: STORY-12 (I_b) before STORY-11 (I_a).
            &[("I_parent", &["I_b", "I_a"])],
        );
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        fetch_story(&cache, &tmp, &gh, &mut issue_map);

        let folder = tmp.path().join(".lazyspec/cache/story/STORY-100");
        assert!(folder.join("index.md").is_file(), "parent index.md");
        assert!(
            folder.join("00-STORY-12.md").is_file(),
            "first child by GH order"
        );
        assert!(
            folder.join("01-STORY-11.md").is_file(),
            "second child by GH order"
        );
        // The parent is NOT also written flat.
        assert!(!tmp
            .path()
            .join(".lazyspec/cache/story/STORY-100.md")
            .exists());
    }

    // AC2: the loader nests the materialized children under the parent.
    #[test]
    fn fetch_all_nested_layout_is_loaded_nested() {
        let (cache, tmp) = make_cache();
        let gh = NestingReader::new(
            vec![
                issue_with_node(100, "I_parent"),
                issue_with_node(11, "I_a"),
                issue_with_node(12, "I_b"),
            ],
            &[("I_parent", &["I_a", "I_b"])],
        );
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        fetch_story(&cache, &tmp, &gh, &mut issue_map);

        let store = load_story_store(&tmp);
        let parent_path = path_of(&store, "STORY-100");
        let children = store.children_of(&parent_path);
        assert_eq!(children.len(), 2, "parent should have 2 nested children");
    }

    // AC3: a childless issue alongside a parent stays flat.
    #[test]
    fn fetch_all_childless_issue_stays_flat() {
        let (cache, tmp) = make_cache();
        let gh = NestingReader::new(
            vec![
                issue_with_node(100, "I_parent"),
                issue_with_node(11, "I_a"),
                issue_with_node(50, "I_lone"),
            ],
            &[("I_parent", &["I_a"])],
        );
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        fetch_story(&cache, &tmp, &gh, &mut issue_map);

        assert!(tmp
            .path()
            .join(".lazyspec/cache/story/STORY-50.md")
            .is_file());
        assert!(!tmp.path().join(".lazyspec/cache/story/STORY-50").exists());
    }

    // AC4: a sub-issue removed on GitHub un-nests on re-fetch with no duplicates.
    #[test]
    fn refetch_unnests_removed_subissue_without_duplicates() {
        let (cache, tmp) = make_cache();
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();

        // First fetch: STORY-11 nested under STORY-100.
        let first = NestingReader::new(
            vec![issue_with_node(100, "I_parent"), issue_with_node(11, "I_a")],
            &[("I_parent", &["I_a"])],
        );
        fetch_story(&cache, &tmp, &first, &mut issue_map);
        assert!(tmp
            .path()
            .join(".lazyspec/cache/story/STORY-100/00-STORY-11.md")
            .is_file());

        // Second fetch: parent no longer reports STORY-11 as a sub-issue, so it
        // becomes a flat, childless doc.
        let second = NestingReader::new(
            vec![issue_with_node(100, "I_parent"), issue_with_node(11, "I_a")],
            &[("I_parent", &[])],
        );
        fetch_story(&cache, &tmp, &second, &mut issue_map);

        let story_dir = tmp.path().join(".lazyspec/cache/story");
        // STORY-11 now flat, no longer nested, and not duplicated.
        assert!(
            story_dir.join("STORY-11.md").is_file(),
            "STORY-11 re-parented flat"
        );
        assert!(!story_dir.join("STORY-100/00-STORY-11.md").exists());
        // STORY-100 has no children now, so it is flat (no folder).
        assert!(story_dir.join("STORY-100.md").is_file());
        assert!(!story_dir.join("STORY-100").exists());

        let mut ids = cache.list_cached("story");
        ids.sort();
        assert_eq!(
            ids,
            vec!["STORY-100", "STORY-11"],
            "no duplicate cache entries"
        );
    }

    // AC4 (drop case): a removed sub-issue gone from the remote entirely is pruned.
    #[test]
    fn refetch_prunes_subissue_removed_from_remote() {
        let (cache, tmp) = make_cache();
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();

        let first = NestingReader::new(
            vec![issue_with_node(100, "I_parent"), issue_with_node(11, "I_a")],
            &[("I_parent", &["I_a"])],
        );
        fetch_story(&cache, &tmp, &first, &mut issue_map);

        // STORY-11 deleted from GitHub: not returned by issue_list at all.
        let second =
            NestingReader::new(vec![issue_with_node(100, "I_parent")], &[("I_parent", &[])]);
        let result = fetch_story(&cache, &tmp, &second, &mut issue_map);

        assert_eq!(result.removed, 1);
        let story_dir = tmp.path().join(".lazyspec/cache/story");
        assert!(!story_dir.join("STORY-100/00-STORY-11.md").exists());
        assert!(!story_dir.join("STORY-11.md").exists());
        assert!(issue_map.get("STORY-11").is_none());
        assert_eq!(cache.list_cached("story"), vec!["STORY-100".to_string()]);
    }

    // Transition flat -> nested: a doc cached flat that becomes a child must move
    // into the parent folder with no stale flat file left behind.
    #[test]
    fn refetch_flat_to_nested_leaves_no_stale_flat_file() {
        let (cache, tmp) = make_cache();
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();

        // First fetch: both flat (parent reports no sub-issues).
        let first = NestingReader::new(
            vec![issue_with_node(100, "I_parent"), issue_with_node(11, "I_a")],
            &[("I_parent", &[])],
        );
        fetch_story(&cache, &tmp, &first, &mut issue_map);
        assert!(tmp
            .path()
            .join(".lazyspec/cache/story/STORY-11.md")
            .is_file());

        // Second fetch: STORY-11 is now a sub-issue of STORY-100.
        let second = NestingReader::new(
            vec![issue_with_node(100, "I_parent"), issue_with_node(11, "I_a")],
            &[("I_parent", &["I_a"])],
        );
        fetch_story(&cache, &tmp, &second, &mut issue_map);

        let story_dir = tmp.path().join(".lazyspec/cache/story");
        assert!(story_dir.join("STORY-100/00-STORY-11.md").is_file());
        assert!(
            !story_dir.join("STORY-11.md").exists(),
            "stale flat file removed"
        );
        assert!(
            !story_dir.join("STORY-100.md").exists(),
            "parent no longer flat"
        );

        let mut ids = cache.list_cached("story");
        ids.sort();
        assert_eq!(ids, vec!["STORY-100", "STORY-11"]);
    }

    // --- STORY-245: relationship read-back for flat (non-subdir) docs ---

    /// GhIssueReader + GhGraphql fake for the flat-doc relation read-back path.
    /// Answers the batched `parent { number }` query, mapping each requested
    /// child node to its remote parent's issue number; a query without an `ids`
    /// var is the schema-snapshot refresh and returns an empty issue-types
    /// response. Mirrors `NestingReader` but walks the parent edge.
    struct ParentReader {
        issues: Vec<GhIssue>,
        parent_number_by_node: std::collections::HashMap<String, u64>,
    }

    impl ParentReader {
        fn new(issues: Vec<GhIssue>, parents: &[(&str, u64)]) -> Self {
            Self {
                issues,
                parent_number_by_node: parents
                    .iter()
                    .map(|(node, num)| (node.to_string(), *num))
                    .collect(),
            }
        }
    }

    impl GhIssueReader for ParentReader {
        fn issue_list(
            &self,
            _repo: &str,
            _labels: &[String],
            _json_fields: &[String],
            _limit: Option<u64>,
        ) -> Result<Vec<GhIssue>> {
            Ok(self.issues.clone())
        }
        fn issue_view(&self, _repo: &str, _number: u64) -> Result<GhIssue> {
            unimplemented!()
        }
        fn issue_comments(
            &self,
            _repo: &str,
            _number: u64,
        ) -> Result<Vec<crate::engine::gh::GhComment>> {
            unimplemented!()
        }
    }

    impl GhGraphql for ParentReader {
        fn graphql(&self, _query: &str, vars: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
            let ids = vars
                .iter()
                .find(|(k, _)| *k == "ids")
                .and_then(|(_, v)| match v {
                    GqlVar::StrList(l) => Some(l.clone()),
                    _ => None,
                });
            let Some(ids) = ids else {
                return Ok(empty_issue_types_response());
            };
            let nodes: Vec<serde_json::Value> = ids
                .iter()
                .map(|node| match self.parent_number_by_node.get(node) {
                    Some(num) => serde_json::json!({"id": node, "parent": {"number": num}}),
                    None => serde_json::json!({"id": node, "parent": null}),
                })
                .collect();
            Ok(serde_json::json!({"data": {"nodes": nodes}}))
        }
        fn project_items(
            &self,
            _repo: &str,
            _content_node_id: &str,
        ) -> Result<Vec<crate::engine::gh::ProjectItem>> {
            Ok(vec![])
        }
        fn update_project_v2_item_field_value(
            &self,
            _project_id: &str,
            _item_id: &str,
            _field_id: &str,
            _value: &crate::engine::gh::GhFieldValueInput,
        ) -> Result<()> {
            Ok(())
        }
        fn clear_project_field(
            &self,
            _project_id: &str,
            _item_id: &str,
            _field_id: &str,
        ) -> Result<()> {
            Ok(())
        }
    }

    impl GhIssueDependencyApi for ParentReader {
        fn list_blocked_by(&self, _repo: &str, _blocked_number: u64) -> Result<Vec<u64>> {
            Ok(vec![])
        }
        fn add_blocked_by(&self, _repo: &str, _blocked: u64, _blocking: u64) -> Result<()> {
            unimplemented!()
        }
        fn remove_blocked_by(&self, _repo: &str, _blocked: u64, _blocking: u64) -> Result<()> {
            unimplemented!()
        }
    }

    fn subissue_relationship() -> crate::engine::config::RelationshipDef {
        crate::engine::config::RelationshipDef {
            name: "implements".to_string(),
            inverse: Some("implemented-by".to_string()),
            github_native: Some("sub-issue".to_string()),
            traversal: None,
        }
    }

    fn subdir_story_type_def() -> TypeDef {
        TypeDef {
            subdirectory: true,
            ..story_type_def()
        }
    }

    fn config_with_subissue_rel(type_def: TypeDef) -> Config {
        let mut config = Config::default();
        config.documents.types = vec![type_def];
        config.relationships = vec![subissue_relationship()];
        config
    }

    fn fetch_typed<R>(
        cache: &IssueCache,
        tmp: &TempDir,
        gh: &R,
        issue_map: &mut IssueMap,
        type_def: TypeDef,
        config: &Config,
    ) -> FetchResult
    where
        R: GhIssueReader + GhGraphql + GhIssueDependencyApi,
    {
        cache
            .fetch_all(
                tmp.path(),
                &type_def,
                gh,
                gh,
                gh,
                None,
                "owner/repo",
                issue_map,
                &[story_match_rule()],
                config,
            )
            .unwrap()
    }

    // AC4: a native sub-issue edge between two flat docs reads back as the
    // configured relation on the child (forward name toward its parent), and the
    // docs stay flat -- no subdir nesting.
    #[test]
    fn fetch_injects_subissue_relation_on_flat_child_instead_of_nesting() {
        let (cache, tmp) = make_cache();
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        // STORY-11 (I_a) is a native sub-issue of STORY-100 (#100).
        let gh = ParentReader::new(
            vec![issue_with_node(100, "I_parent"), issue_with_node(11, "I_a")],
            &[("I_a", 100)],
        );
        let config = config_with_subissue_rel(story_type_def());
        fetch_typed(&cache, &tmp, &gh, &mut issue_map, story_type_def(), &config);

        let story_dir = tmp.path().join(".lazyspec/cache/story");
        assert!(story_dir.join("STORY-11.md").is_file(), "child stays flat");
        assert!(
            story_dir.join("STORY-100.md").is_file(),
            "parent stays flat"
        );
        assert!(
            !story_dir.join("STORY-100").exists(),
            "no nesting folder is created"
        );

        // The child carries the forward relation toward its parent.
        let child = std::fs::read_to_string(story_dir.join("STORY-11.md")).unwrap();
        assert!(
            child.contains("implements: STORY-100"),
            "child must carry the forward sub-issue relation, got:\n{child}"
        );
        // The parent stores nothing: its `implemented-by` inverse is virtual.
        let parent = std::fs::read_to_string(story_dir.join("STORY-100.md")).unwrap();
        assert!(
            !parent.contains("implemented-by") && !parent.contains("implements:"),
            "parent must not store the relation, got:\n{parent}"
        );
    }

    // AC5: a subdir-type parent keeps materializing nested docs even when a
    // sub-issue-native relationship is configured -- the subdir path wins.
    #[test]
    fn fetch_subdir_type_still_nests_even_with_subissue_relation() {
        let (cache, tmp) = make_cache();
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let gh = NestingReader::new(
            vec![issue_with_node(100, "I_parent"), issue_with_node(11, "I_a")],
            &[("I_parent", &["I_a"])],
        );
        let config = config_with_subissue_rel(subdir_story_type_def());
        fetch_typed(
            &cache,
            &tmp,
            &gh,
            &mut issue_map,
            subdir_story_type_def(),
            &config,
        );

        let story_dir = tmp.path().join(".lazyspec/cache/story");
        assert!(
            story_dir.join("STORY-100/index.md").is_file(),
            "parent index.md materialized"
        );
        assert!(
            story_dir.join("STORY-100/00-STORY-11.md").is_file(),
            "child nested under parent"
        );
        // Nested, not related: no injected relation on the child.
        let child = std::fs::read_to_string(story_dir.join("STORY-100/00-STORY-11.md")).unwrap();
        assert!(
            !child.contains("implements:"),
            "subdir child must not carry an injected relation, got:\n{child}"
        );
    }

    // AC4 (drop case): a native sub-issue edge removed on the remote drops the
    // injected relation on re-fetch, with no duplicate cache entries.
    #[test]
    fn refetch_drops_subissue_relation_when_edge_removed_no_duplicates() {
        let (cache, tmp) = make_cache();
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let config = config_with_subissue_rel(story_type_def());
        let story_dir = tmp.path().join(".lazyspec/cache/story");

        // First fetch: STORY-11 is a native sub-issue of STORY-100.
        let first = ParentReader::new(
            vec![issue_with_node(100, "I_parent"), issue_with_node(11, "I_a")],
            &[("I_a", 100)],
        );
        fetch_typed(
            &cache,
            &tmp,
            &first,
            &mut issue_map,
            story_type_def(),
            &config,
        );
        assert!(std::fs::read_to_string(story_dir.join("STORY-11.md"))
            .unwrap()
            .contains("implements: STORY-100"));

        // Second fetch: the edge is gone on the remote.
        let second = ParentReader::new(
            vec![issue_with_node(100, "I_parent"), issue_with_node(11, "I_a")],
            &[],
        );
        fetch_typed(
            &cache,
            &tmp,
            &second,
            &mut issue_map,
            story_type_def(),
            &config,
        );

        let child = std::fs::read_to_string(story_dir.join("STORY-11.md")).unwrap();
        assert!(
            !child.contains("implements"),
            "relation dropped on re-fetch, got:\n{child}"
        );
        let mut ids = cache.list_cached("story");
        ids.sort();
        assert_eq!(
            ids,
            vec!["STORY-100", "STORY-11"],
            "no duplicate cache entries"
        );
    }

    // --- STORY-209: crash-safe persistence ---

    // AC1: an absent cache.lock still yields a clean default.
    #[test]
    fn load_lock_absent_file_yields_default() {
        let (cache, _tmp) = make_cache();
        let lock = cache.load_lock().unwrap();
        assert!(lock.get("anything").is_none());
    }

    // AC1: a corrupt (present but unparseable) cache.lock hard-errors every
    // mutator, and the defaulted lock is never persisted over the corrupt file.
    #[test]
    fn corrupt_lock_fails_mutators_and_never_persists_default() {
        let (cache, tmp) = make_cache();
        cache.write("STORY-1", "story", "content").unwrap();

        let lock_path = tmp.path().join(".lazyspec/cache.lock");
        let corrupt = r#"{"STORY-1": "2026-03-27T10:0"#;
        std::fs::write(&lock_path, corrupt).unwrap();

        assert!(cache.load_lock().is_err());
        assert!(cache.write("STORY-2", "story", "c").is_err());
        assert!(cache.touch_lock("STORY-1").is_err());
        assert!(cache.remove("STORY-1", "story").is_err());

        // Read probes degrade to stale without persisting anything.
        assert!(!cache.is_fresh("STORY-1", Duration::seconds(60)));

        // The corrupt file is byte-for-byte untouched, and the failed remove
        // did not delete the doc file.
        assert_eq!(std::fs::read_to_string(&lock_path).unwrap(), corrupt);
        assert!(cache.doc_path("STORY-1", "story").exists());
    }

    // AC1: refresh_stale hard-errors on a corrupt lock before any API call.
    #[test]
    fn refresh_stale_corrupt_lock_is_hard_error_before_api() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();
        cache.write("STORY-10", "story", "old content").unwrap();

        let lock_path = tmp.path().join(".lazyspec/cache.lock");
        std::fs::write(&lock_path, "{ truncated").unwrap();

        let gh = MockReader::new(vec![]);
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let err = cache
            .refresh_stale(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                "owner/repo",
                &mut issue_map,
                Duration::seconds(60),
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap_err();

        assert!(err.to_string().contains("corrupt"), "got: {err}");
        assert_eq!(gh.call_count(), 0, "no API call before the lock error");
        assert_eq!(std::fs::read_to_string(&lock_path).unwrap(), "{ truncated");
    }

    // AC1 + AC3: fetch_all hard-errors on a corrupt lock and leaves the
    // previous cache docs untouched.
    #[test]
    fn fetch_all_corrupt_lock_errors_and_leaves_cache_untouched() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();
        cache.write("STORY-10", "story", "old content").unwrap();

        let lock_path = tmp.path().join(".lazyspec/cache.lock");
        std::fs::write(&lock_path, "{ truncated").unwrap();

        let gh = MockReader::new(vec![make_gh_issue(
            10,
            "STORY-001 First",
            "new body",
            &["lazyspec:story"],
        )]);
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let err = cache
            .fetch_all(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                &gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap_err();

        assert!(err.to_string().contains("corrupt"), "got: {err}");
        assert_eq!(
            cache.read_stale("STORY-10", "story"),
            Some("old content".to_string()),
            "previous cache doc untouched"
        );
        assert_eq!(std::fs::read_to_string(&lock_path).unwrap(), "{ truncated");
    }

    // AC3: a fetch_all that fails partway through its writes (injected here by
    // making the cache root refuse new entries, so creating the staging dir
    // fails) leaves the previously cached docs and the lock intact.
    #[cfg(unix)]
    #[test]
    fn fetch_all_write_failure_leaves_previous_cache_intact() {
        use std::os::unix::fs::PermissionsExt;

        let (cache, tmp) = make_cache();
        let type_def = story_type_def();

        // Seed a previous successful fetch.
        let gh = MockReader::new(vec![make_gh_issue(
            10,
            "STORY-001 First",
            "old body",
            &["lazyspec:story"],
        )]);
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        cache
            .fetch_all(
                tmp.path(),
                &type_def,
                &gh,
                &gh,
                &gh,
                None,
                "owner/repo",
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();
        let doc_path = tmp.path().join(".lazyspec/cache/story/STORY-10.md");
        let old_doc = std::fs::read_to_string(&doc_path).unwrap();
        let lock_path = tmp.path().join(".lazyspec/cache.lock");
        let old_lock = std::fs::read_to_string(&lock_path).unwrap();

        // Inject the write failure.
        let cache_root = tmp.path().join(".lazyspec/cache");
        let mut perms = std::fs::metadata(&cache_root).unwrap().permissions();
        perms.set_mode(0o555);
        std::fs::set_permissions(&cache_root, perms).unwrap();

        let gh = MockReader::new(vec![
            make_gh_issue(10, "STORY-001 First", "new body", &["lazyspec:story"]),
            make_gh_issue(11, "STORY-002 Second", "another", &["lazyspec:story"]),
        ]);
        let result = cache.fetch_all(
            tmp.path(),
            &type_def,
            &gh,
            &gh,
            &gh,
            None,
            "owner/repo",
            &mut issue_map,
            &[story_match_rule()],
            &Config::default(),
        );

        // Restore permissions before asserting so TempDir cleanup works.
        let mut perms = std::fs::metadata(&cache_root).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&cache_root, perms).unwrap();

        assert!(result.is_err(), "interrupted fetch must error");
        assert_eq!(
            std::fs::read_to_string(&doc_path).unwrap(),
            old_doc,
            "previous cache doc intact after failed fetch"
        );
        assert_eq!(
            std::fs::read_to_string(&lock_path).unwrap(),
            old_lock,
            "lock not rewritten by a failed fetch"
        );
    }
}
