//! The engine fetch seam (RFC-057). One orchestrator, [`sync_all`], refreshes
//! every configured type's cache by dispatching each to its per-backend syncer
//! through an exhaustive `match StoreBackend`. Each syncer implements the static
//! [`TypeSync`] contract; the sidecar maps stay owned by the caller and are lent
//! in through a borrowed [`SyncContext`]. Adding a backend or a fetch step then
//! happens in one place, so the CLI and TUI stay in lockstep by construction.

use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;

use crate::engine::cache_lock::CacheLock;
use crate::engine::clickup::ClickupClient;
use crate::engine::clickup_cache;
use crate::engine::config::{Config, Lifecycle, StoreBackend, TypeDef};
use crate::engine::gh::{GhGraphql, GhIssueReader, GhMilestoneApi};
use crate::engine::git_ref::GitRefOps;
use crate::engine::issue_body::TypeMatchRule;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::status_colors::StatusColors;
use crate::engine::store_dispatch;
use crate::engine::task_map::TaskMap;

/// The GitHub sidecar map a caller lends to the two GitHub syncers. Borrowed,
/// never owned: the TUI's `issue_map` must stay the single field inside
/// `GithubIssuesStore` so the poll and the edit-push path read one map.
pub struct GhMaps<'a> {
    pub issue_map: &'a mut IssueMap,
}

/// The ClickUp sidecar maps a caller lends to [`ClickupSync`].
pub struct ClickupMaps<'a> {
    pub task_map: &'a mut TaskMap,
    pub status_colors: &'a mut StatusColors,
}

/// The maps [`sync_all`] lends to the syncers, borrowed from whoever owns them.
/// `gh` is `Some` when any GitHub backend is configured; `clickup` is `Some`
/// when a clickup-tasks type is configured.
pub struct SyncContext<'a> {
    pub gh: Option<GhMaps<'a>>,
    pub clickup: Option<ClickupMaps<'a>>,
}

/// The result of syncing one type. Never a `Result`: a per-type failure is an
/// `Ok`-shaped outcome with [`error`](SyncOutcome::error) set, so one bad type
/// cannot sink the run or the other types. Severity is a caller decision.
#[derive(Debug, Default)]
pub struct SyncOutcome {
    pub type_name: String,
    pub fetched: usize,
    pub new: usize,
    pub removed: usize,
    pub warnings: Vec<String>,
    /// Set when this type's fetch failed; the run still continued.
    pub error: Option<String>,
    /// The derived lifecycle for backends that produce one (ClickUp). The caller
    /// decides whether to persist it (only the CLI does).
    pub lifecycle: Option<Lifecycle>,
}

impl SyncOutcome {
    fn failed(type_name: &str, message: impl Into<String>) -> Self {
        SyncOutcome {
            type_name: type_name.to_string(),
            error: Some(message.into()),
            ..Default::default()
        }
    }
}

/// The contract every syncer shares: refresh one type's cache plus this
/// backend's sidecar maps (borrowed via `ctx`) and return the outcome. A
/// **static** contract, not a `dyn` trait -- [`sync_all`] dispatches through a
/// `match StoreBackend`, never a vtable. Never returns `Result`: every failure
/// has exactly one home, [`SyncOutcome::error`].
pub trait TypeSync {
    fn sync(
        &mut self,
        ctx: &mut SyncContext,
        root: &Path,
        td: &TypeDef,
        cfg: &Config,
    ) -> SyncOutcome;
}

/// Refreshes a `github-milestones` type. Milestones must be synced before
/// issues so an issue's native milestone resolves to its `MILESTONE-n` doc.
pub struct GhMilestoneSync<'c> {
    pub gh: &'c dyn GhMilestoneApi,
    pub repo: String,
}

impl TypeSync for GhMilestoneSync<'_> {
    fn sync(
        &mut self,
        ctx: &mut SyncContext,
        root: &Path,
        td: &TypeDef,
        _cfg: &Config,
    ) -> SyncOutcome {
        let Some(maps) = ctx.gh.as_mut() else {
            return SyncOutcome::failed(&td.name, "github maps not provided in SyncContext");
        };
        match crate::engine::milestone_cache::fetch_milestones(
            root,
            td,
            &self.gh,
            &self.repo,
            maps.issue_map,
        ) {
            Ok(r) => SyncOutcome {
                type_name: td.name.clone(),
                fetched: r.fetched,
                new: r.new,
                removed: r.removed,
                warnings: r.warnings.into_iter().map(|w| w.message).collect(),
                ..Default::default()
            },
            Err(e) => SyncOutcome::failed(&td.name, e.to_string()),
        }
    }
}

/// Refreshes a `github-issues` type, then folds in the per-item project-field
/// injection (best-effort) so both surfaces inject identically.
pub struct GhIssueSync<'c> {
    pub reader: &'c dyn GhIssueReader,
    pub graphql: &'c dyn GhGraphql,
    pub repo: String,
    pub type_rules: Vec<TypeMatchRule>,
}

impl TypeSync for GhIssueSync<'_> {
    fn sync(
        &mut self,
        ctx: &mut SyncContext,
        root: &Path,
        td: &TypeDef,
        cfg: &Config,
    ) -> SyncOutcome {
        let Some(maps) = ctx.gh.as_mut() else {
            return SyncOutcome::failed(&td.name, "github maps not provided in SyncContext");
        };
        let cache = IssueCache::new(root);
        let result = match cache.fetch_all(
            root,
            td,
            self.reader,
            self.graphql,
            &self.repo,
            maps.issue_map,
            &self.type_rules,
            cfg,
        ) {
            Ok(r) => r,
            Err(e) => return SyncOutcome::failed(&td.name, e.to_string()),
        };

        let mut warnings: Vec<String> = result.warnings.into_iter().map(|w| w.message).collect();
        warnings.extend(inject_project_fields_into_cache(
            root,
            self.graphql,
            &self.repo,
            maps.issue_map,
            cfg,
            td,
        ));

        SyncOutcome {
            type_name: td.name.clone(),
            fetched: result.fetched,
            new: result.new,
            removed: result.removed,
            warnings,
            ..Default::default()
        }
    }
}

/// Refreshes a `git-ref` type by fetching its refs into the local cache.
pub struct GitRefSync<'c> {
    pub ops: &'c dyn GitRefOps,
    pub remote: String,
}

impl TypeSync for GitRefSync<'_> {
    fn sync(
        &mut self,
        _ctx: &mut SyncContext,
        root: &Path,
        td: &TypeDef,
        _cfg: &Config,
    ) -> SyncOutcome {
        match fetch_git_ref(root, self.ops, &self.remote, &td.name) {
            Ok(c) => SyncOutcome {
                type_name: td.name.clone(),
                fetched: c.fetched,
                new: c.new,
                removed: c.removed,
                ..Default::default()
            },
            Err(e) => SyncOutcome::failed(&td.name, e.to_string()),
        }
    }
}

/// Refreshes a `clickup-tasks` type: task cache, then the bound List's status
/// colours (the previously TUI-missed capture) and the derived lifecycle.
pub struct ClickupSync<'c> {
    pub client: &'c dyn ClickupClient,
    pub token: String,
}

impl TypeSync for ClickupSync<'_> {
    fn sync(
        &mut self,
        ctx: &mut SyncContext,
        root: &Path,
        td: &TypeDef,
        _cfg: &Config,
    ) -> SyncOutcome {
        let Some(maps) = ctx.clickup.as_mut() else {
            return SyncOutcome::failed(&td.name, "clickup maps not provided in SyncContext");
        };
        let result =
            match clickup_cache::fetch_tasks(root, td, self.client, &self.token, maps.task_map) {
                Ok(r) => r,
                Err(e) => return SyncOutcome::failed(&td.name, e.to_string()),
            };

        let Some(list_id) = td.clickup_list_id.as_deref() else {
            return SyncOutcome::failed(
                &td.name,
                format!(
                    "type '{}' is clickup-tasks but has no clickup_list_id configured",
                    td.name
                ),
            );
        };
        let (lifecycle, colors) =
            match clickup_cache::fetch_lifecycle_and_colors(self.client, &self.token, list_id) {
                Ok(v) => v,
                Err(e) => return SyncOutcome::failed(&td.name, e.to_string()),
            };
        maps.status_colors.set_type(&td.name, colors);

        SyncOutcome {
            type_name: td.name.clone(),
            fetched: result.fetched,
            new: result.new,
            removed: result.removed,
            lifecycle: Some(lifecycle),
            ..Default::default()
        }
    }
}

/// A caller's per-backend syncers. Typed `Option` fields (not a slice) so
/// "backend not configured" is a `None` the dispatch reads directly rather than
/// a runtime scan that could silently lack a needed syncer.
#[derive(Default)]
pub struct Syncers<'c> {
    pub milestone: Option<GhMilestoneSync<'c>>,
    pub issue: Option<GhIssueSync<'c>>,
    pub git_ref: Option<GitRefSync<'c>>,
    pub clickup: Option<ClickupSync<'c>>,
}

/// Refresh every configured type's cache, in fixed backend order (milestones,
/// issues, git-ref, clickup), collecting one [`SyncOutcome`] per type. Never
/// aborts: a per-type fetch failure -- or a missing syncer for a configured
/// backend -- is recorded in that type's `error` and the run continues. The
/// ordering rule (milestones before issues) lives here, in one place.
pub fn sync_all(
    root: &Path,
    config: &Config,
    ctx: &mut SyncContext,
    syncers: &mut Syncers,
    filter: Option<&str>,
) -> Vec<SyncOutcome> {
    let order = [
        StoreBackend::GithubMilestones,
        StoreBackend::GithubIssues,
        StoreBackend::GitRef,
        StoreBackend::ClickupTasks,
    ];

    let mut outcomes = Vec::new();
    for backend in order {
        for td in config.documents.types.iter().filter(|t| t.store == backend) {
            if filter.is_some_and(|f| f != td.name) {
                continue;
            }
            if let Some(outcome) = dispatch(syncers, ctx, root, td, config) {
                outcomes.push(outcome);
            }
        }
    }
    outcomes
}

/// The one site the compiler forces open when a `StoreBackend` variant is added.
/// Filesystem has no remote and github-projects fields are pulled within
/// `GhIssueSync`, so both are explicit skip arms (`None`), not omissions.
fn dispatch(
    syncers: &mut Syncers,
    ctx: &mut SyncContext,
    root: &Path,
    td: &TypeDef,
    cfg: &Config,
) -> Option<SyncOutcome> {
    match td.store {
        StoreBackend::GithubMilestones => Some(run_syncer(
            syncers.milestone.as_mut(),
            ctx,
            root,
            td,
            cfg,
            "github-milestones",
        )),
        StoreBackend::GithubIssues => Some(run_syncer(
            syncers.issue.as_mut(),
            ctx,
            root,
            td,
            cfg,
            "github-issues",
        )),
        StoreBackend::GitRef => Some(run_syncer(
            syncers.git_ref.as_mut(),
            ctx,
            root,
            td,
            cfg,
            "git-ref",
        )),
        StoreBackend::ClickupTasks => Some(run_syncer(
            syncers.clickup.as_mut(),
            ctx,
            root,
            td,
            cfg,
            "clickup-tasks",
        )),
        StoreBackend::Filesystem => None,
        StoreBackend::GithubProjects => None,
    }
}

/// Run a syncer if present, else record a missing-syncer error on the outcome
/// (never a panic) so a configured backend without a syncer fails just its own
/// type.
fn run_syncer<S: TypeSync>(
    syncer: Option<&mut S>,
    ctx: &mut SyncContext,
    root: &Path,
    td: &TypeDef,
    cfg: &Config,
    backend: &str,
) -> SyncOutcome {
    match syncer {
        Some(s) => s.sync(ctx, root, td, cfg),
        None => SyncOutcome::failed(
            &td.name,
            format!("no syncer configured for backend '{}'", backend),
        ),
    }
}

/// Counts from a git-ref fetch, mirroring the github-issues fetch summary.
pub(crate) struct GitRefCounts {
    pub fetched: usize,
    pub new: usize,
    pub removed: usize,
}

/// Fetch a `git-ref` type's refs and rewrite its cache from them, keyed on the
/// cache lock's per-doc SHA: unchanged refs are skipped, new/changed ones are
/// written, and docs gone from the remote are removed. Relocated from
/// `cli::fetch` (RFC-057 Interfaces); the CLI now adapts this in ITERATION-286.
pub(crate) fn fetch_git_ref(
    root: &Path,
    git_ref_ops: &dyn GitRefOps,
    remote: &str,
    type_name: &str,
) -> Result<GitRefCounts> {
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
        crate::engine::fs::atomic_write(&cache_file, &content)?;

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

    Ok(GitRefCounts {
        fetched,
        new: new_count,
        removed,
    })
}

/// For every cached doc of `type_def`, inject the per-item project field values
/// as `PROJECT-n.<field>` attributes, rewriting the cache file. Best-effort: a
/// per-doc failure is returned as a warning and the rest still process (the
/// engine emits no stderr, so warnings flow back to the caller via the outcome).
/// Relocated from `cli::fetch::inject_project_fields_into_cache` (RFC-057).
fn inject_project_fields_into_cache(
    root: &Path,
    client: &dyn GhGraphql,
    repo: &str,
    issue_map: &IssueMap,
    config: &Config,
    type_def: &TypeDef,
) -> Vec<String> {
    let mut warnings = Vec::new();
    let cache_dir = root.join(".lazyspec/cache").join(&type_def.name);
    let entries = match std::fs::read_dir(&cache_dir) {
        Ok(e) => e,
        Err(_) => return warnings,
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
        if let Err(e) = store_dispatch::inject_project_fields_for_meta(
            client, repo, issue_map, config, &mut meta,
        ) {
            warnings.push(format!(
                "could not read project fields for {}: {}",
                meta.id, e
            ));
            continue;
        }
        if let Err(e) = store_dispatch::write_cache_file(root, type_def, &meta, &body) {
            warnings.push(format!("could not rewrite cache for {}: {}", meta.id, e));
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{
        Config, NumberingStrategy, RelationshipDef, StoreBackend, TypeDef,
    };
    use crate::engine::gh::test_support::{MockGhClient, MockGhMilestoneClient};
    use crate::engine::gh::{
        GhAuthor, GhIssue, GhIssueMilestone, GhLabel, GhMilestone, ProjectFieldValue,
    };
    use crate::engine::gh::{GhFieldKind, GhFieldValueRepr};
    use tempfile::TempDir;

    fn type_def(name: &str, prefix: &str, store: StoreBackend) -> TypeDef {
        TypeDef {
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
        }
    }

    fn milestone_relationship() -> RelationshipDef {
        RelationshipDef {
            name: "targets".to_string(),
            inverse: Some("targeted-by".to_string()),
            github_native: Some("milestone".to_string()),
            traversal: None,
        }
    }

    fn gh_issue_with_milestone(number: u64, milestone_number: u64) -> GhIssue {
        GhIssue {
            number,
            id: format!("I_node{}", number),
            url: String::new(),
            title: "An issue".to_string(),
            body: String::new(),
            labels: vec![GhLabel {
                name: "lazyspec:story".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-07-01T00:00:00Z".to_string(),
            created_at: "2026-07-01T00:00:00Z".to_string(),
            author: Some(GhAuthor {
                login: "octocat".to_string(),
            }),
            issue_type: None,
            milestone: Some(GhIssueMilestone {
                number: milestone_number,
            }),
        }
    }

    // AC (STORY-202): milestones are fetched before issues, so an issue whose
    // native milestone points at a just-synced milestone resolves its forward
    // `targets` relation. The ordering rule lives in sync_all; if issue fetch
    // ran first, the map would lack the milestone and the relation would drop.
    #[test]
    fn sync_all_orders_milestones_before_issues_so_cross_relation_resolves() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let mut config = Config::default();
        config.documents.types = vec![
            // Deliberately list issues before milestones to prove sync_all, not
            // config order, establishes the ordering.
            type_def("story", "STORY", StoreBackend::GithubIssues),
            type_def("milestone", "MILESTONE", StoreBackend::GithubMilestones),
        ];
        config.relationships = vec![milestone_relationship()];

        let milestone_client = MockGhMilestoneClient::with_milestones(vec![GhMilestone {
            number: 7,
            title: "v1".to_string(),
            description: "first".to_string(),
            due_on: None,
            state: "open".to_string(),
            open_issues: 1,
            closed_issues: 0,
            url: String::new(),
        }]);
        let issue_client =
            MockGhClient::new().with_list_result(vec![gh_issue_with_milestone(42, 7)]);

        let mut issue_map = IssueMap::load(root).unwrap();
        let mut ctx = SyncContext {
            gh: Some(GhMaps {
                issue_map: &mut issue_map,
            }),
            clickup: None,
        };
        let mut syncers = Syncers {
            milestone: Some(GhMilestoneSync {
                gh: &milestone_client,
                repo: "owner/repo".to_string(),
            }),
            issue: Some(GhIssueSync {
                reader: &issue_client,
                graphql: &issue_client,
                repo: "owner/repo".to_string(),
                type_rules: config
                    .documents
                    .types
                    .iter()
                    .map(TypeMatchRule::from)
                    .collect(),
            }),
            ..Default::default()
        };

        let outcomes = sync_all(root, &config, &mut ctx, &mut syncers, None);

        assert_eq!(outcomes.len(), 2);
        // Milestone outcome comes first regardless of config order.
        assert_eq!(outcomes[0].type_name, "milestone");
        assert_eq!(outcomes[1].type_name, "story");
        assert!(outcomes.iter().all(|o| o.error.is_none()), "{:?}", outcomes);

        let story =
            std::fs::read_to_string(root.join(".lazyspec/cache/story/STORY-42.md")).unwrap();
        assert!(
            story.contains("targets: MILESTONE-7"),
            "issue must resolve its milestone relation after ordered fetch, got:\n{story}"
        );
    }

    /// A GitHub client fake whose project-field GraphQL always errors, to drive
    /// the injection-failure path (MockGhClient's `project_item_fields` cannot
    /// fail). Everything else is inert.
    struct FailingInjectClient {
        issues: Vec<GhIssue>,
    }

    impl GhIssueReader for FailingInjectClient {
        fn issue_list(
            &self,
            _repo: &str,
            _labels: &[String],
            _fields: &[String],
            _limit: Option<u64>,
        ) -> Result<Vec<GhIssue>> {
            Ok(self.issues.clone())
        }
        fn issue_view(&self, _repo: &str, number: u64) -> Result<GhIssue> {
            Ok(gh_issue_no_milestone(number))
        }
        fn issue_comments(
            &self,
            _repo: &str,
            _number: u64,
        ) -> Result<Vec<crate::engine::gh::GhComment>> {
            Ok(vec![])
        }
    }

    impl GhGraphql for FailingInjectClient {
        fn graphql(
            &self,
            _query: &str,
            _vars: &[(&str, crate::engine::gh::GqlVar)],
        ) -> Result<serde_json::Value> {
            anyhow::bail!("graphql unreachable")
        }
        fn project_item_fields(
            &self,
            _repo: &str,
            _content_node_id: &str,
        ) -> Result<Vec<ProjectFieldValue>> {
            anyhow::bail!("project fields unreachable")
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

    // AC (STORY-202): a project-field GraphQL injection failure is a warning, not
    // an error -- the cached doc keeps its other fields and the outcome carries
    // no `error` (so the CLI exits zero on warnings alone in ITERATION-286).
    #[test]
    fn gh_issue_injection_failure_is_warning_not_error() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let mut config = Config::default();
        config.documents.types = vec![type_def("story", "STORY", StoreBackend::GithubIssues)];
        config.relationships = vec![RelationshipDef {
            name: "member-of".to_string(),
            inverse: Some("has-member".to_string()),
            github_native: Some("membership".to_string()),
            traversal: None,
        }];

        // An issue whose body declares a board membership, so injection attempts
        // the project-field GraphQL -- which this client fails -> a warning.
        let issue = GhIssue {
            body: issue_body_with_membership(),
            ..gh_issue_no_milestone(42)
        };
        let issue_client = FailingInjectClient {
            issues: vec![issue],
        };

        let mut issue_map = IssueMap::load(root).unwrap();
        issue_map.insert("STORY-42", 42, "", "I_node42");
        let mut ctx = SyncContext {
            gh: Some(GhMaps {
                issue_map: &mut issue_map,
            }),
            clickup: None,
        };
        let mut syncers = Syncers {
            issue: Some(GhIssueSync {
                reader: &issue_client,
                graphql: &issue_client,
                repo: "owner/repo".to_string(),
                type_rules: config
                    .documents
                    .types
                    .iter()
                    .map(TypeMatchRule::from)
                    .collect(),
            }),
            ..Default::default()
        };

        let outcomes = sync_all(root, &config, &mut ctx, &mut syncers, None);
        assert_eq!(outcomes.len(), 1);
        assert!(
            outcomes[0].error.is_none(),
            "injection failure must not set error: {:?}",
            outcomes[0]
        );
        assert!(
            outcomes[0]
                .warnings
                .iter()
                .any(|w| w.contains("project fields")),
            "expected an injection warning, got: {:?}",
            outcomes[0].warnings
        );
        // The cached doc still exists with its other fields intact.
        assert!(root.join(".lazyspec/cache/story/STORY-42.md").exists());
    }

    fn gh_issue_no_milestone(number: u64) -> GhIssue {
        GhIssue {
            milestone: None,
            ..gh_issue_with_milestone(number, 0)
        }
    }

    /// A github-issues body carrying a `member-of: PROJECT-1` relation, encoded
    /// the way GitHub bodies actually carry lazyspec metadata (an HTML comment
    /// frontmatter block), so `fetch_all` parses the membership back out.
    fn issue_body_with_membership() -> String {
        use crate::engine::document::{DocMeta, DocType, Relation, RelationType, Status};
        let meta = DocMeta {
            path: Default::default(),
            title: "An issue".to_string(),
            doc_type: DocType::new("story"),
            status: Status::new("draft"),
            author: "@octocat".to_string(),
            date: chrono::Utc::now().date_naive(),
            tags: vec![],
            provenance: vec![],
            related: vec![Relation {
                rel_type: RelationType::new("member-of"),
                target: "PROJECT-1".to_string(),
            }],
            validate_ignore: false,
            virtual_doc: false,
            attributes: Default::default(),
            id: String::new(),
        };
        crate::engine::issue_body::serialize(&meta, "body")
    }

    // AC (STORY-202): a configured backend with no syncer in Syncers yields an
    // error-bearing outcome, never a panic.
    #[test]
    fn missing_syncer_for_configured_backend_is_error_not_panic() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let mut config = Config::default();
        config.documents.types = vec![type_def(
            "milestone",
            "MILESTONE",
            StoreBackend::GithubMilestones,
        )];

        let mut issue_map = IssueMap::load(root).unwrap();
        let mut ctx = SyncContext {
            gh: Some(GhMaps {
                issue_map: &mut issue_map,
            }),
            clickup: None,
        };
        // No milestone syncer configured.
        let mut syncers = Syncers::default();

        let outcomes = sync_all(root, &config, &mut ctx, &mut syncers, None);
        assert_eq!(outcomes.len(), 1);
        let err = outcomes[0]
            .error
            .as_ref()
            .expect("missing syncer must set error");
        assert!(err.contains("no syncer configured"), "got: {err}");
    }

    // AC (STORY-202): filesystem and github-projects types are explicit skip arms
    // -- they produce no outcome and no fetch is attempted.
    #[test]
    fn filesystem_and_github_projects_types_are_skipped() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let mut config = Config::default();
        config.documents.types = vec![
            type_def("rfc", "RFC", StoreBackend::Filesystem),
            type_def("board", "PROJECT", StoreBackend::GithubProjects),
        ];

        let mut issue_map = IssueMap::load(root).unwrap();
        let mut ctx = SyncContext {
            gh: Some(GhMaps {
                issue_map: &mut issue_map,
            }),
            clickup: None,
        };
        let mut syncers = Syncers::default();

        let outcomes = sync_all(root, &config, &mut ctx, &mut syncers, None);
        assert!(
            outcomes.is_empty(),
            "skip arms must produce no outcome, got: {:?}",
            outcomes
        );
    }

    // The single-type filter refreshes only the named type.
    #[test]
    fn filter_refreshes_only_the_named_type() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let mut config = Config::default();
        config.documents.types = vec![
            type_def("milestone", "MILESTONE", StoreBackend::GithubMilestones),
            type_def("story", "STORY", StoreBackend::GithubIssues),
        ];

        let milestone_client = MockGhMilestoneClient::with_milestones(vec![]);
        let issue_client = MockGhClient::new();

        let mut issue_map = IssueMap::load(root).unwrap();
        let mut ctx = SyncContext {
            gh: Some(GhMaps {
                issue_map: &mut issue_map,
            }),
            clickup: None,
        };
        let mut syncers = Syncers {
            milestone: Some(GhMilestoneSync {
                gh: &milestone_client,
                repo: "owner/repo".to_string(),
            }),
            issue: Some(GhIssueSync {
                reader: &issue_client,
                graphql: &issue_client,
                repo: "owner/repo".to_string(),
                type_rules: vec![],
            }),
            ..Default::default()
        };

        let outcomes = sync_all(root, &config, &mut ctx, &mut syncers, Some("story"));
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].type_name, "story");
    }

    // The injection path proves it wires the real store_dispatch: a mapped node
    // with a project field value lands as a PROJECT-n attribute on the doc.
    #[test]
    fn gh_issue_injection_writes_project_field_attribute() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let mut config = Config::default();
        config.documents.types = vec![type_def("story", "STORY", StoreBackend::GithubIssues)];
        config.relationships = vec![RelationshipDef {
            name: "member-of".to_string(),
            inverse: Some("has-member".to_string()),
            github_native: Some("membership".to_string()),
            traversal: None,
        }];

        let issue = GhIssue {
            body: issue_body_with_membership(),
            ..gh_issue_no_milestone(7)
        };
        let issue_client = MockGhClient::new()
            .with_list_result(vec![issue])
            .with_project_field_values(vec![ProjectFieldValue {
                project_number: 1,
                field_name: "Status".into(),
                kind: GhFieldKind::SingleSelect,
                value: GhFieldValueRepr::OptionName("In Progress".into()),
            }]);

        let mut issue_map = IssueMap::load(root).unwrap();
        issue_map.insert("STORY-7", 7, "", "I_node7");
        let mut ctx = SyncContext {
            gh: Some(GhMaps {
                issue_map: &mut issue_map,
            }),
            clickup: None,
        };
        let mut syncers = Syncers {
            issue: Some(GhIssueSync {
                reader: &issue_client,
                graphql: &issue_client,
                repo: "owner/repo".to_string(),
                type_rules: config
                    .documents
                    .types
                    .iter()
                    .map(TypeMatchRule::from)
                    .collect(),
            }),
            ..Default::default()
        };

        let outcomes = sync_all(root, &config, &mut ctx, &mut syncers, None);
        assert!(outcomes[0].error.is_none(), "{:?}", outcomes[0]);
        let doc = std::fs::read_to_string(root.join(".lazyspec/cache/story/STORY-7.md")).unwrap();
        assert!(
            doc.contains("PROJECT-1.Status: In Progress"),
            "expected injected project field, got:\n{doc}"
        );
    }
}
