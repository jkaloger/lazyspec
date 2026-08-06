use chrono::{DateTime, Duration, Utc};
use std::fs;
use std::path::{Path, PathBuf};

use crate::engine::cache_lock::CacheLock;
use crate::engine::config::{AttrDef, Config, Lifecycle, TypeDef};
use crate::engine::document::{AttrValue, DocMeta, Relation, RelationType, Status};
use crate::engine::gh::{GhGraphql, GhIssue};
use crate::engine::gh_fetch::{self, FetchSnapshot};
use crate::engine::gh_schema;
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
/// parent's children in GitHub sub-issue order (doc ids). `fetch_all` builds it
/// from the round's inline `subIssues` edges and writes the nested layout from
/// it.
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

    /// Refresh stale cache entries for a given type from its own composed round.
    ///
    /// Returns early with zero API calls if all cached documents are fresh.
    /// On API failure, leaves stale cache in place and returns a warning.
    /// A corrupt cache lock is a hard `Err` before any API call.
    #[allow(clippy::too_many_arguments)]
    pub fn refresh_stale(
        &self,
        root: &Path,
        type_def: &TypeDef,
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

        let milestone_rel = config
            .relationship_by_github_native("milestone")
            .map(|r| r.name.as_str());

        // The TTL refresh is not driven by `sync_all`, so it composes its own
        // round -- this type's issues and the schema snapshot in one document.
        let round = gh_fetch::fetch_all_pages(
            gh_graphql,
            repo,
            &[TypeMatchRule::from(type_def)],
            &store_dispatch::authority_board_numbers(config),
        );
        // The round already named what it could not read, so the stale cache is
        // served on its warnings rather than a second one saying the same thing.
        let Some(issues) = round.issues.get(&type_def.name) else {
            return Ok(RefreshResult {
                refreshed: 0,
                unchanged: cached_ids.len(),
                warnings: round.warnings,
            });
        };

        let mut refreshed = 0usize;
        let mut unchanged = 0usize;
        let mut write_warnings = Vec::new();

        let lifecycle = type_def.effective_lifecycle();
        let (open_status, closed_status) = open_closed_statuses(type_def, &lifecycle);
        for issue in issues {
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
        warnings.extend(self.refresh_schema_snapshot(Some(&round), config));
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
    /// Every value comes from `fetch`, read once for the whole round rather than
    /// once per type and once per board; this pass issues no request of its own.
    /// A board the round did not resolve -- absent from `board_fields` -- is
    /// skipped, and so is everything when there was no round at all. The round
    /// already warned about each failure, so this pass stays silent.
    ///
    /// Boards are visited in `authority_board_numbers` order, not the map's, so
    /// the persisted file is byte-identical across runs on identical state.
    fn refresh_schema_snapshot(
        &self,
        fetch: Option<&FetchSnapshot>,
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
            let Some((fields, options, iterations)) =
                fetch.and_then(|f| f.board_fields.get(&number)).cloned()
            else {
                continue;
            };
            snapshot.replace_board_fields(number, fields, options, iterations);
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

    /// Full rebuild of a type's cache from the round's issue list, cleaning up
    /// docs the remote no longer has.
    ///
    /// The list is the round's, every page of it, so this is authoritative for
    /// the whole type directory. A round that did not resolve this type is an
    /// `Err`: an absent list is unknown, never empty, and treating it as empty
    /// would delete the cache the round failed to refresh.
    pub fn fetch_all(
        &self,
        root: &Path,
        type_def: &TypeDef,
        fetch: Option<&FetchSnapshot>,
        issue_map: &mut IssueMap,
        known_types: &[TypeMatchRule],
        config: &Config,
    ) -> anyhow::Result<FetchResult> {
        let Some((round, issues)) = fetch.and_then(|f| Some((f, f.issues.get(&type_def.name)?)))
        else {
            anyhow::bail!(
                "the github fetch round did not resolve issues for type '{}'; \
                 the cache was left unchanged",
                type_def.name
            );
        };

        let mut warnings: Vec<RefreshWarning> = Vec::new();

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
        for issue in issues {
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

        // A connection the round capped is a real loss of remote state, so name
        // the document and the edge rather than write the short list silently.
        warnings.extend(truncation_warnings(round, &node_to_doc));

        // The nested cache layout comes from the round's own sub-issue edges --
        // selected inline on each issue, so no request of their own.
        let parentage = if inject_subissue_relation {
            ParentageMap::new()
        } else {
            parentage_from(round, &node_to_doc)
        };

        // number -> doc id for the current fetch batch, so a same-type blocker
        // or parent resolves before the batch is written into the issue map
        // (cross-type ones resolve via the map once their type fetches, the same
        // ordering caveat milestones carry).
        let batch: std::collections::HashMap<u64, String> = issues
            .iter()
            .zip(parsed.iter())
            .map(|(issue, p)| (issue.number, p.id.clone()))
            .collect();

        // Inject the declared inverse relation (`blocked-by`) toward each
        // blocking issue's doc. Mirrors the milestone read-back -- the forward
        // `blocks` edge on the blocker is derived virtually in `build_links`,
        // never stored.
        if let Some(dep_rel) = config
            .relationship_by_github_native("dependency")
            .and_then(|r| r.inverse.as_deref())
        {
            for (issue, p) in issues.iter().zip(parsed.iter_mut()) {
                let Some(blockers) = round.blocked_by.get(&issue.number) else {
                    continue;
                };
                for &blocker in blockers {
                    let target = batch
                        .get(&blocker)
                        .cloned()
                        .or_else(|| issue_map.shorthand_for_number(blocker).map(String::from));
                    if let Some(target) = target {
                        p.meta.related.push(Relation {
                            rel_type: RelationType::new(dep_rel),
                            target,
                        });
                    }
                }
            }
        }

        // Flat-doc read-back: inject the sub-issue-native relation on each child
        // toward its remote parent (the forward name; the parent's inverse is
        // derived in the graph, never stored -- mirrors the dependency path). A
        // dropped remote edge simply yields no parent on re-fetch, so the
        // relation vanishes with the authoritative rebuild, no duplicates.
        if let Some(rel) = subissue_rel.filter(|_| inject_subissue_relation) {
            let parent_by_child = subissue_parent_numbers(round);
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

        warnings.extend(self.refresh_schema_snapshot(fetch, config));

        Ok(FetchResult {
            fetched: issues.len(),
            new: new_count,
            removed: removed.len(),
            warnings,
        })
    }
}

/// The round's sub-issue edges as a nested layout: parent doc id -> children doc
/// ids in GitHub's sub-issue order. A parent the round returned but this type did
/// not fetch has no doc to nest under, and a child outside the type's fetch has
/// no doc to nest -- both are dropped, so a parent left with no resolvable child
/// is omitted rather than written as a childless folder.
fn parentage_from(
    round: &FetchSnapshot,
    node_to_doc: &std::collections::HashMap<String, String>,
) -> ParentageMap {
    let mut map = ParentageMap::new();
    for (parent_node, child_nodes) in &round.sub_issues {
        let Some(parent_doc) = node_to_doc.get(parent_node) else {
            continue;
        };
        let children: Vec<String> = child_nodes
            .iter()
            .filter_map(|n| node_to_doc.get(n).cloned())
            .collect();
        if children.is_empty() {
            continue;
        }
        map.insert(parent_doc.clone(), children);
    }
    map
}

/// The same edges read the other way: child node id -> its parent's issue
/// number, which is what a flat child's relation targets. The parent's number
/// comes from the round's own issue lists, so a parent of another type resolves
/// as readily as a same-type one.
fn subissue_parent_numbers(round: &FetchSnapshot) -> std::collections::HashMap<String, u64> {
    let number_by_node: std::collections::HashMap<&str, u64> = round
        .issues
        .values()
        .flatten()
        .map(|issue| (issue.id.as_str(), issue.number))
        .collect();
    let mut map = std::collections::HashMap::new();
    for (parent_node, child_nodes) in &round.sub_issues {
        let Some(&parent_number) = number_by_node.get(parent_node.as_str()) else {
            continue;
        };
        for child in child_nodes {
            map.insert(child.clone(), parent_number);
        }
    }
    map
}

/// One warning per capped connection the round reported more of, naming the
/// document it belongs to and the edge that was cut short. An issue this type
/// did not fetch has no doc id here; the type that owns it warns instead.
fn truncation_warnings(
    round: &FetchSnapshot,
    node_to_doc: &std::collections::HashMap<String, String>,
) -> Vec<RefreshWarning> {
    round
        .truncations
        .iter()
        .filter_map(|t| {
            let doc = node_to_doc.get(&t.node_id)?;
            Some(RefreshWarning {
                message: format!(
                    "{}: `{}` truncated at {} on this fetch; the rest were not read",
                    doc,
                    t.connection,
                    t.connection.cap()
                ),
            })
        })
        .collect()
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
    use crate::engine::gh::{GhAuthor, GhGraphql, GhIssueDependencyApi, GhLabel, GqlVar};
    use anyhow::Result;
    use std::cell::RefCell;
    use std::collections::HashMap;
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

    /// What a composed round would have resolved for one type: the issues a
    /// double holds, under the key `fetch_all` looks them up by.
    trait RoundIssues {
        fn issues(&self) -> &[GhIssue];

        /// The sub-issue edges the round selected inline: parent node id ->
        /// child node ids, in server order. Empty for a double with none.
        fn sub_issues(&self) -> HashMap<String, Vec<String>> {
            HashMap::new()
        }

        fn round(&self, type_name: &str) -> FetchSnapshot {
            FetchSnapshot {
                issues: HashMap::from([(type_name.to_string(), self.issues().to_vec())]),
                sub_issues: self.sub_issues(),
                ..Default::default()
            }
        }
    }

    /// The GitHub seam a fetch actually reaches through, now that discovery is
    /// the composed round: every read is a `graphql` call, and the issues this
    /// double holds are what its round answers with on every alias.
    struct MockReader {
        issues: Vec<GhIssue>,
        fail: bool,
        graphql_responses: RefCell<Vec<serde_json::Value>>,
        graphql_call_count: AtomicUsize,
        round_issue_types: RefCell<Vec<gh_schema::IssueTypeId>>,
    }

    impl MockReader {
        fn new(issues: Vec<GhIssue>) -> Self {
            Self {
                issues,
                fail: false,
                graphql_responses: RefCell::new(vec![]),
                graphql_call_count: AtomicUsize::new(0),
                round_issue_types: RefCell::new(vec![]),
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

        fn graphql_call_count(&self) -> usize {
            self.graphql_call_count.load(Ordering::SeqCst)
        }
    }

    impl RoundIssues for MockReader {
        fn issues(&self) -> &[GhIssue] {
            &self.issues
        }
    }

    impl GhGraphql for MockReader {
        fn graphql(&self, query: &str, _vars: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
            self.graphql_call_count.fetch_add(1, Ordering::SeqCst);
            if gh_fetch::is_round_query(query) {
                if self.fail {
                    anyhow::bail!("API unreachable");
                }
                return Ok(crate::engine::gh::test_support::with_issue_pages(
                    query,
                    crate::engine::gh::test_support::round_response(
                        &[],
                        &self.round_issue_types.borrow(),
                        &[],
                    ),
                    &self.issues,
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
    fn test_refresh_stale_fetches_all_via_one_composed_round() {
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
                "owner/repo",
                &mut issue_map,
                ttl,
                &known_types,
                &Config::default(),
            )
            .unwrap();

        assert_eq!(
            gh.graphql_call_count(),
            1,
            "one composed round serves the whole refresh"
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

    /// A round that resolved one org issue type plus one board's `Status`
    /// column set, parsed from the connection nodes GitHub would have sent, so
    /// the fixture exercises the same reader the live round does.
    fn round_with_board(number: u64, field_id: &str, options: &[&str]) -> FetchSnapshot {
        let opts: Vec<serde_json::Value> = options
            .iter()
            .map(|name| {
                serde_json::json!({
                    "id": format!("opt_{}", name.to_lowercase().replace(' ', "_")),
                    "name": name
                })
            })
            .collect();
        let nodes = serde_json::json!([{
            "__typename": "ProjectV2SingleSelectField",
            "id": field_id,
            "name": "Status",
            "dataType": "SINGLE_SELECT",
            "options": opts
        }]);
        FetchSnapshot {
            board_fields: HashMap::from([(
                number,
                gh_schema::parse_project_fields(&nodes, number),
            )]),
            ..round_with_bug_issue_type()
        }
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

        let warnings = cache.refresh_schema_snapshot(
            Some(&round_with_board(
                7,
                "PVTSSF_b7",
                &["Ready To Start", "In Progress", "Review", "Done"],
            )),
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

    // The board a type nominates is the only one persisted: a round that
    // resolved a board no type nominates must not leak into the snapshot.
    #[test]
    fn refresh_schema_snapshot_ignores_a_board_no_type_nominates() {
        let (cache, tmp) = make_cache();

        let warnings = cache.refresh_schema_snapshot(
            Some(&round_with_board(9, "PVTSSF_b9", &["Triage"])),
            &config_with_status_authority(Some("PROJECT-7")),
        );

        assert!(warnings.is_empty(), "warnings: {:?}", warnings);
        let saved = gh_schema::GhSchemaSnapshot::load(tmp.path());
        assert!(saved.project_fields.is_empty());
    }

    #[test]
    fn refresh_schema_snapshot_persists_issue_types_without_a_status_authority() {
        let (cache, tmp) = make_cache();

        let warnings = cache.refresh_schema_snapshot(
            Some(&round_with_bug_issue_type()),
            &config_with_status_authority(None),
        );

        assert!(warnings.is_empty(), "warnings: {:?}", warnings);
        let saved = gh_schema::GhSchemaSnapshot::load(tmp.path());
        assert_eq!(saved.issue_type_id("Bug"), Some("IT_kwBug"));
        assert!(saved.project_fields.is_empty());
    }

    // A board the round could not resolve is absent from `board_fields`, which
    // is not the same as a board that answered with nothing: the prior ids must
    // survive. The round already warned about why, so this pass adds none.
    #[test]
    fn refresh_schema_snapshot_keeps_prior_board_fields_when_the_round_missed_the_board() {
        let (cache, tmp) = make_cache();
        write_board_7_snapshot(tmp.path(), "Review");

        let warnings = cache.refresh_schema_snapshot(
            Some(&round_with_bug_issue_type()),
            &config_with_status_authority(Some("PROJECT-7")),
        );

        assert!(warnings.is_empty(), "warnings: {:?}", warnings);
        let saved = gh_schema::GhSchemaSnapshot::load(tmp.path());
        assert_eq!(saved.issue_type_id("Bug"), Some("IT_kwBug"));
        assert_eq!(saved.field_id(7, "Status"), Some("PVTSSF_prior"));
        assert_eq!(saved.status_lifecycle(7).unwrap().states, vec!["review"]);
    }

    // The other side of that rule: a board that answered with an empty field set
    // is authoritative, so its prior ids are dropped rather than kept alive.
    #[test]
    fn refresh_schema_snapshot_clears_a_board_the_round_resolved_as_empty() {
        let (cache, tmp) = make_cache();
        write_board_7_snapshot(tmp.path(), "Review");

        let warnings = cache.refresh_schema_snapshot(
            Some(&FetchSnapshot {
                board_fields: HashMap::from([(7, Default::default())]),
                ..round_with_bug_issue_type()
            }),
            &config_with_status_authority(Some("PROJECT-7")),
        );

        assert!(warnings.is_empty(), "warnings: {:?}", warnings);
        let saved = gh_schema::GhSchemaSnapshot::load(tmp.path());
        assert!(saved.project_fields.is_empty());
        assert!(saved.single_select_options.is_empty());
    }

    #[test]
    fn refresh_schema_snapshot_replaces_a_boards_stale_options() {
        let (cache, tmp) = make_cache();
        write_board_7_snapshot(tmp.path(), "Retired Column");

        let warnings = cache.refresh_schema_snapshot(
            Some(&round_with_board(7, "PVTSSF_b7", &["Review", "Done"])),
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
                "owner/repo",
                &mut issue_map,
                ttl,
                &known_types,
                &Config::default(),
            )
            .unwrap();

        assert_eq!(
            gh.graphql_call_count(),
            0,
            "an all-fresh type composes no round at all"
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
        assert_eq!(
            result.warnings[0].message,
            "github fetch round failed, caches unchanged: API unreachable"
        );

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

    // The TTL refresh runs outside `sync_all`, so it composes its own round --
    // and a type classified by a native issue type is discovered on that page,
    // by `issueType`, rather than by a search resolved one `issue_view` at a
    // time. An issue carrying the tag but the wrong type is left stale.
    #[test]
    fn refresh_stale_classifies_its_own_round_on_the_native_issue_type() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def_signals(Some("Ticket"), Some("Bug"));
        let ttl = Duration::seconds(60);

        cache.write("STORY-10", "story", "old 10").unwrap();
        cache.write("STORY-11", "story", "old 11").unwrap();
        cache.write("STORY-12", "story", "old 12").unwrap();
        backdate_all(&cache, &["STORY-10", "STORY-11", "STORY-12"]);

        let typed = |number: u64, issue_type: &str| GhIssue {
            issue_type: Some(issue_type.to_string()),
            ..make_gh_issue(number, &format!("STORY-{:03}", number), "Body", &["Ticket"])
        };
        let gh = MockReader::new(vec![typed(10, "Task"), typed(11, "Bug"), typed(12, "Bug")]);

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let result = cache
            .refresh_stale(
                tmp.path(),
                &type_def,
                &gh,
                "owner/repo",
                &mut issue_map,
                ttl,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(
            gh.graphql_call_count(),
            1,
            "one composed round serves the whole refresh"
        );
        assert_eq!(result.refreshed, 2);
        assert!(cache.is_fresh("STORY-11", ttl));
        assert!(cache.is_fresh("STORY-12", ttl));
        assert!(
            !cache.is_fresh("STORY-10", ttl),
            "an issue of another native type is not this type's, and stays stale"
        );
    }

    // --- fetch_all tests ---

    // The 500-issue cap was an artifact of `gh issue list --limit`; the round
    // pages until GitHub stops offering more, so a type past the old ceiling
    // materializes in full and nothing warns about what was left behind.
    #[test]
    fn fetch_all_writes_every_issue_of_a_type_past_the_old_five_hundred_cap() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();

        let gh = MockReader::new(
            (1..=501)
                .map(|n| make_gh_issue(n, &format!("STORY-{}", n), "Body", &["lazyspec:story"]))
                .collect(),
        );

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let result = cache
            .fetch_all(
                tmp.path(),
                &type_def,
                Some(&gh.round(&type_def.name)),
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(result.fetched, 501);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert!(tmp
            .path()
            .join(".lazyspec/cache/story/STORY-501.md")
            .exists());
    }

    // A type the round could not read is unknown, never empty: rebuilding the
    // type directory from an absent list would delete every doc the failed read
    // was supposed to refresh, so the fetch fails and the cache stands.
    #[test]
    fn fetch_all_errors_without_emptying_the_cache_when_the_round_lacks_the_type() {
        let (cache, tmp) = make_cache();
        let type_def = story_type_def();
        let gh = MockReader::new(vec![make_gh_issue(
            10,
            "STORY-010",
            "Body",
            &["lazyspec:story"],
        )]);
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        cache
            .fetch_all(
                tmp.path(),
                &type_def,
                Some(&gh.round(&type_def.name)),
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        let err = cache
            .fetch_all(
                tmp.path(),
                &type_def,
                Some(&FetchSnapshot::default()),
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap_err();

        assert!(err.to_string().contains("did not resolve issues"), "{err}");
        assert!(
            tmp.path()
                .join(".lazyspec/cache/story/STORY-10.md")
                .exists(),
            "the prior cache must survive a round that could not read the type"
        );
    }

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
                Some(&gh.round(&type_def.name)),
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

        // The round answers every alias with this same issue #42 -- standing in
        // for both types' rules independently matching one GitHub issue.
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
                Some(&gh.round(&story_type.name)),
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        cache
            .fetch_all(
                tmp.path(),
                &ticket_type,
                Some(&gh.round(&ticket_type.name)),
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
                Some(&seed_gh.round(&story_type.name)),
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();
        cache
            .fetch_all(
                tmp.path(),
                &ticket_type,
                Some(&seed_gh.round(&ticket_type.name)),
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
                Some(&gh.round(&type_def.name)),
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
            ..gh.round(&type_def.name)
        };

        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        cache
            .fetch_all(
                tmp.path(),
                &type_def,
                Some(&round),
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
                Some(&initial_gh.round(&type_def.name)),
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
                Some(&updated_gh.round(&type_def.name)),
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
                Some(&gh.round(&type_def.name)),
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
                Some(&gh.round(&type_def.name)),
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
                Some(&gh.round(&type_def.name)),
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
                Some(&gh.round(&type_def.name)),
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
                Some(&gh.round(&type_def.name)),
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
                Some(&gh.round(&type_def.name)),
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
                Some(&gh.round(&type_def.name)),
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        assert_eq!(cached_status(&tmp, "STORY-10"), "in progress");
    }

    // The composed round can fail one board and nothing else. When it does, the
    // board-bound doc keeps its last known column AND the snapshot keeps that
    // board's prior ids -- the two halves of "a missing `project` scope costs a
    // warning, never data".
    #[test]
    fn a_failed_board_schema_costs_neither_the_docs_status_nor_the_boards_prior_ids() {
        let (cache, tmp) = make_cache();
        let type_def = authority_story_type_def();
        write_board_7_snapshot(tmp.path(), "In Progress");

        cache
            .write(
                "STORY-10",
                "story",
                "---\ntitle: \"Work\"\ntype: story\nstatus: in progress\nauthor: \"@octocat\"\ndate: 2026-01-01\ntags: []\n---\nplain body\n",
            )
            .unwrap();

        let gh = MockReader::new(vec![make_gh_issue(
            10,
            "Work",
            "plain body",
            &["lazyspec:story"],
        )]);
        // A round that resolved everything except board 7's schema.
        let round = FetchSnapshot {
            warnings: vec![RefreshWarning {
                message: "could not refresh field schema for board 7 (keeping prior, projects \
                          need `gh auth refresh -s project`): FORBIDDEN"
                    .to_string(),
            }],
            issues: gh.round(&type_def.name).issues,
            ..round_with_bug_issue_type()
        };
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        cache
            .fetch_all(
                tmp.path(),
                &type_def,
                Some(&round),
                &mut issue_map,
                &[story_match_rule()],
                &config_with_status_authority(Some("PROJECT-7")),
            )
            .unwrap();

        assert_eq!(cached_status(&tmp, "STORY-10"), "in progress");
        let saved = gh_schema::GhSchemaSnapshot::load(tmp.path());
        assert_eq!(saved.field_id(7, "Status"), Some("PVTSSF_prior"));
        assert_eq!(
            saved.option_id("PVTSSF_prior", "In Progress"),
            Some("opt_prior")
        );
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
                Some(&gh.round(&type_def.name)),
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

    // --- sub-issue edges off the composed round ---

    fn node_to_doc(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
        pairs
            .iter()
            .map(|(n, d)| (n.to_string(), d.to_string()))
            .collect()
    }

    /// A round that learned `sub_issues` and nothing else.
    fn round_with_sub_issues(edges: &[(&str, &[&str])]) -> FetchSnapshot {
        FetchSnapshot {
            sub_issues: edges
                .iter()
                .map(|(parent, kids)| {
                    (
                        parent.to_string(),
                        kids.iter().map(|k| k.to_string()).collect(),
                    )
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn parentage_resolves_children_in_subissue_order() {
        let round = round_with_sub_issues(&[("I_parent", &["I_b", "I_a"])]);
        let map = node_to_doc(&[
            ("I_parent", "STORY-1"),
            ("I_a", "STORY-2"),
            ("I_b", "STORY-3"),
        ]);

        let parentage = parentage_from(&round, &map);

        // Children preserve GitHub sub-issue order (I_b before I_a).
        assert_eq!(
            parentage.get("STORY-1"),
            Some(&vec!["STORY-3".to_string(), "STORY-2".to_string()])
        );
    }

    #[test]
    fn parentage_drops_unresolvable_child_nodes() {
        // I_unknown is not among the fetched issues.
        let round = round_with_sub_issues(&[("I_parent", &["I_a", "I_unknown"])]);
        let map = node_to_doc(&[("I_parent", "STORY-1"), ("I_a", "STORY-2")]);

        let parentage = parentage_from(&round, &map);

        assert_eq!(parentage.get("STORY-1"), Some(&vec!["STORY-2".to_string()]));
    }

    #[test]
    fn parentage_skips_a_parent_this_type_did_not_fetch() {
        // I_other is a parent of another type; its children are not this type's
        // to nest, so it contributes nothing rather than a phantom entry.
        let round = round_with_sub_issues(&[("I_other", &["I_c"]), ("I_p2", &["I_c"])]);
        let map = node_to_doc(&[("I_p2", "STORY-2"), ("I_c", "STORY-3")]);

        let parentage = parentage_from(&round, &map);

        assert_eq!(parentage.len(), 1);
        assert_eq!(parentage.get("STORY-2"), Some(&vec!["STORY-3".to_string()]));
    }

    #[test]
    fn parentage_omits_parents_with_no_resolvable_child() {
        let round = round_with_sub_issues(&[("I_parent", &["I_elsewhere"])]);
        let map = node_to_doc(&[("I_parent", "STORY-1")]);

        assert!(parentage_from(&round, &map).is_empty());
    }

    #[test]
    fn subissue_parent_numbers_invert_the_rounds_edges() {
        let mut parent = make_gh_issue(100, "STORY-100", "b", &[]);
        parent.id = "I_parent".to_string();
        let mut round = round_with_sub_issues(&[("I_parent", &["I_a", "I_b"])]);
        round
            .issues
            .insert("story".to_string(), vec![parent.clone()]);

        let by_child = subissue_parent_numbers(&round);

        assert_eq!(by_child.get("I_a"), Some(&100));
        assert_eq!(by_child.get("I_b"), Some(&100));
    }

    #[test]
    fn subissue_parent_numbers_resolve_a_parent_of_another_type() {
        // The parent came back on a different alias; its number is still the
        // round's to give, so the child's relation resolves.
        let mut parent = make_gh_issue(7, "EPIC-7", "b", &[]);
        parent.id = "I_epic".to_string();
        let mut round = round_with_sub_issues(&[("I_epic", &["I_child"])]);
        round.issues.insert("epic".to_string(), vec![parent]);

        assert_eq!(subissue_parent_numbers(&round).get("I_child"), Some(&7));
    }

    #[test]
    fn subissue_parent_numbers_skip_a_parent_the_round_never_returned() {
        let round = round_with_sub_issues(&[("I_unseen", &["I_child"])]);

        assert!(subissue_parent_numbers(&round).is_empty());
    }

    #[test]
    fn a_truncated_connection_warns_naming_the_document_and_the_edge() {
        let round = FetchSnapshot {
            truncations: vec![gh_fetch::Truncation {
                node_id: "I_parent".to_string(),
                connection: gh_fetch::Connection::SubIssues,
            }],
            ..Default::default()
        };
        let map = node_to_doc(&[("I_parent", "STORY-1")]);

        let warnings = truncation_warnings(&round, &map);

        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("STORY-1"), "{:?}", warnings[0]);
        assert!(
            warnings[0].message.contains("subIssues"),
            "{:?}",
            warnings[0]
        );
        assert!(warnings[0].message.contains("50"), "{:?}", warnings[0]);
    }

    /// A `story` type whose config declares the native dependency relationship,
    /// so `fetch_all` injects `blocked-by` from the round's edges.
    fn dependency_config() -> Config {
        let mut config = Config::default();
        config.documents.types = vec![story_type_def()];
        config.relationships = vec![crate::engine::config::RelationshipDef {
            name: "blocks".to_string(),
            inverse: Some("blocked-by".to_string()),
            github_native: Some("dependency".to_string()),
            traversal: None,
        }];
        config
    }

    fn round_of(issues: Vec<GhIssue>) -> FetchSnapshot {
        FetchSnapshot {
            issues: HashMap::from([("story".to_string(), issues)]),
            ..Default::default()
        }
    }

    #[test]
    fn a_blocker_fetched_in_the_same_round_resolves_to_its_doc() {
        let (cache, tmp) = make_cache();
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let mut round = round_of(vec![issue_with_node(42, "I_a"), issue_with_node(7, "I_b")]);
        round.blocked_by = HashMap::from([(42, vec![7])]);

        cache
            .fetch_all(
                tmp.path(),
                &story_type_def(),
                Some(&round),
                &mut issue_map,
                &[story_match_rule()],
                &dependency_config(),
            )
            .unwrap();

        let blocked =
            std::fs::read_to_string(tmp.path().join(".lazyspec/cache/story/STORY-42.md")).unwrap();
        assert!(blocked.contains("blocked-by: STORY-7"), "got:\n{blocked}");
        // `blocks` is derived in the graph, never stored on the blocker.
        let blocker =
            std::fs::read_to_string(tmp.path().join(".lazyspec/cache/story/STORY-7.md")).unwrap();
        assert!(!blocker.contains("blocks:"), "got:\n{blocker}");
    }

    #[test]
    fn a_blocker_of_another_type_resolves_through_the_issue_map() {
        let (cache, tmp) = make_cache();
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        // #7 belongs to a type fetched on an earlier pass, so only the map knows
        // its doc id.
        issue_map.insert("TICKET-7", 7, "2026-07-01T00:00:00Z", "I_b");
        let mut round = round_of(vec![issue_with_node(42, "I_a")]);
        round.blocked_by = HashMap::from([(42, vec![7])]);

        cache
            .fetch_all(
                tmp.path(),
                &story_type_def(),
                Some(&round),
                &mut issue_map,
                &[story_match_rule()],
                &dependency_config(),
            )
            .unwrap();

        let blocked =
            std::fs::read_to_string(tmp.path().join(".lazyspec/cache/story/STORY-42.md")).unwrap();
        assert!(blocked.contains("blocked-by: TICKET-7"), "got:\n{blocked}");
    }

    #[test]
    fn an_issue_with_more_children_than_the_cap_nests_what_arrived_and_warns() {
        let (cache, tmp) = make_cache();
        let mut issue_map = IssueMap::load(tmp.path()).unwrap();
        let cap = gh_fetch::Connection::SubIssues.cap();

        // GitHub has 51 children; the round read the first 50 and said so.
        let mut issues = vec![issue_with_node(100, "I_parent")];
        issues.extend((0..cap).map(|i| issue_with_node(200 + i as u64, &format!("I_c{i}"))));
        let mut round = round_of(issues);
        round.sub_issues = HashMap::from([(
            "I_parent".to_string(),
            (0..cap).map(|i| format!("I_c{i}")).collect(),
        )]);
        round.truncations = vec![gh_fetch::Truncation {
            node_id: "I_parent".to_string(),
            connection: gh_fetch::Connection::SubIssues,
        }];

        let result = cache
            .fetch_all(
                tmp.path(),
                &story_type_def(),
                Some(&round),
                &mut issue_map,
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap();

        let folder = tmp.path().join(".lazyspec/cache/story/STORY-100");
        let nested = std::fs::read_dir(&folder)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name() != "index.md")
            .count();
        assert_eq!(nested, cap, "every child the round did read is nested");

        let truncation: Vec<&str> = result
            .warnings
            .iter()
            .map(|w| w.message.as_str())
            .filter(|m| m.contains("subIssues"))
            .collect();
        assert_eq!(truncation.len(), 1, "{:?}", result.warnings);
        assert!(truncation[0].contains("STORY-100"), "{}", truncation[0]);
    }

    #[test]
    fn a_truncation_on_another_types_issue_is_left_to_that_type() {
        let round = FetchSnapshot {
            truncations: vec![gh_fetch::Truncation {
                node_id: "I_elsewhere".to_string(),
                connection: gh_fetch::Connection::BlockedBy,
            }],
            ..Default::default()
        };

        assert!(truncation_warnings(&round, &node_to_doc(&[])).is_empty());
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
                Some(&gh.round(&type_def.name)),
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

    /// The remote sub-issue shape a round answers with: the type's issues plus
    /// the parentage selected inline on them. No GraphQL seam -- a fetch reads
    /// both off the snapshot.
    struct NestingReader {
        issues: Vec<GhIssue>,
        sub_issues_by_node: std::collections::HashMap<String, Vec<String>>,
    }

    impl NestingReader {
        fn new(issues: Vec<GhIssue>, sub_issues_by_node: &[(&str, &[&str])]) -> Self {
            Self {
                issues,
                sub_issues_by_node: sub_issues_by_node
                    .iter()
                    .map(|(node, kids)| {
                        (
                            node.to_string(),
                            kids.iter().map(|k| k.to_string()).collect(),
                        )
                    })
                    .collect(),
            }
        }
    }

    impl RoundIssues for NestingReader {
        fn issues(&self) -> &[GhIssue] {
            &self.issues
        }

        fn sub_issues(&self) -> HashMap<String, Vec<String>> {
            self.sub_issues_by_node.clone()
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
                Some(&gh.round("story")),
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

        // STORY-11 deleted from GitHub: absent from the round's list entirely.
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
        R: RoundIssues,
    {
        cache
            .fetch_all(
                tmp.path(),
                &type_def,
                Some(&gh.round(&type_def.name)),
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
        let gh = NestingReader::new(
            vec![issue_with_node(100, "I_parent"), issue_with_node(11, "I_a")],
            &[("I_parent", &["I_a"])],
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
        let first = NestingReader::new(
            vec![issue_with_node(100, "I_parent"), issue_with_node(11, "I_a")],
            &[("I_parent", &["I_a"])],
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
        let second = NestingReader::new(
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
                "owner/repo",
                &mut issue_map,
                Duration::seconds(60),
                &[story_match_rule()],
                &Config::default(),
            )
            .unwrap_err();

        assert!(err.to_string().contains("corrupt"), "got: {err}");
        assert_eq!(
            gh.graphql_call_count(),
            0,
            "no API call before the lock error"
        );
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
                Some(&gh.round(&type_def.name)),
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
                Some(&gh.round(&type_def.name)),
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
            Some(&gh.round(&type_def.name)),
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
