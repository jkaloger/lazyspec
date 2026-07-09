use crate::engine::clickup::ClickupClient;
use crate::engine::config::{Config, Lifecycle, StoreBackend};
use crate::engine::config_write::write_config_in_place;
use crate::engine::credentials::Token;
use crate::engine::gh::{GhGraphql, GhIssueReader, GhIssueWriter, GhMilestoneApi};
use crate::engine::git_ref::GitRefOps;
use crate::engine::github::resolve_repo;
use crate::engine::issue_body::TypeMatchRule;
use crate::engine::issue_map::IssueMap;
use crate::engine::status_colors::StatusColors;
use crate::engine::sync::{
    sync_all, ClickupMaps, ClickupSync, GhIssueSync, GhMaps, GhMilestoneSync, GitRefSync,
    SyncContext, Syncers,
};
use crate::engine::task_map::TaskMap;
use anyhow::{bail, Context, Result};
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

    // Which backends this run actually touches, after the `--type` filter. The
    // per-backend syncer (and the client/token it needs) is built only when its
    // backend has a type to fetch, so a github-only project never resolves a
    // ClickUp token and vice versa.
    let fetch_milestones = filter_types(milestone_types.clone(), type_filter);
    let fetch_gh = filter_types(gh_types.clone(), type_filter);
    let fetch_gitref = filter_types(git_ref_types.clone(), type_filter);
    let fetch_clickup = filter_types(clickup_types.clone(), type_filter);

    let gh_fetch = !fetch_milestones.is_empty() || !fetch_gh.is_empty();
    let clickup_fetch = !fetch_clickup.is_empty();

    // Token-absent / repo-unresolvable are hard errors raised HERE, before
    // sync_all writes any cache -- distinct from a per-type `SyncOutcome.error`.
    let repo = if gh_fetch {
        Some(resolve_repo(config, root).context(
            "Could not determine GitHub repo. Set [documents.github].repo in .lazyspec.toml",
        )?)
    } else {
        None
    };
    let clickup_token = if clickup_fetch {
        Some(
            clickup_token
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "no ClickUp token found; run `lazyspec setup clickup` before fetching \
                         clickup-tasks types"
                    )
                })?
                .expose()
                .to_string(),
        )
    } else {
        None
    };

    let type_rules: Vec<TypeMatchRule> = config
        .documents
        .types
        .iter()
        .map(TypeMatchRule::from)
        .collect();

    // Run-local sidecar maps, loaded only for the backends we touch and lent to
    // the syncers through the borrowed `SyncContext`; saved after `sync_all`.
    let mut issue_map = if gh_fetch {
        Some(IssueMap::load(root)?)
    } else {
        None
    };
    let mut task_map = if clickup_fetch {
        Some(TaskMap::load(root)?)
    } else {
        None
    };
    let mut status_colors = if clickup_fetch {
        Some(StatusColors::load(root)?)
    } else {
        None
    };

    let outcomes = {
        let mut ctx = SyncContext {
            gh: issue_map.as_mut().map(|m| GhMaps { issue_map: m }),
            clickup: match (task_map.as_mut(), status_colors.as_mut()) {
                (Some(t), Some(s)) => Some(ClickupMaps {
                    task_map: t,
                    status_colors: s,
                }),
                _ => None,
            },
        };

        let mut syncers = Syncers::default();
        if !fetch_milestones.is_empty() {
            syncers.milestone = Some(GhMilestoneSync {
                gh,
                repo: repo
                    .clone()
                    .expect("repo resolved when a milestone type fetches"),
            });
        }
        if !fetch_gh.is_empty() {
            syncers.issue = Some(GhIssueSync {
                reader: gh,
                graphql: gh,
                repo: repo
                    .clone()
                    .expect("repo resolved when an issue type fetches"),
                type_rules,
            });
        }
        if !fetch_gitref.is_empty() {
            syncers.git_ref = Some(GitRefSync {
                ops: git_ref_ops,
                remote: remote.to_string(),
            });
        }
        if clickup_fetch {
            syncers.clickup = Some(ClickupSync {
                client: clickup,
                token: clickup_token
                    .clone()
                    .expect("token present when a clickup type fetches"),
            });
        }

        sync_all(root, config, &mut ctx, &mut syncers, type_filter)
    };

    for o in &outcomes {
        for w in &o.warnings {
            eprintln!("warning: {}", w);
        }
    }

    if json {
        let json_out: Vec<serde_json::Value> = outcomes
            .iter()
            .map(|o| {
                let mut entry = serde_json::json!({
                    "type": o.type_name,
                    "fetched": o.fetched,
                    "new": o.new,
                    "removed": o.removed,
                });
                if let Some(err) = &o.error {
                    entry["error"] = serde_json::Value::String(err.clone());
                }
                entry
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_out)?);
    } else {
        for o in &outcomes {
            match &o.error {
                Some(err) => eprintln!("error: {}: {}", o.type_name, err),
                None => println!(
                    "{}: fetched {}, {} new, {} removed",
                    o.type_name, o.fetched, o.new, o.removed
                ),
            }
        }
    }

    // Persist every cache that succeeded, even when another type failed: the run
    // continued through every type, so there is no partial state to withhold.
    if let Some(m) = &issue_map {
        m.save(root)?;
    }
    if let Some(m) = &task_map {
        m.save(root)?;
    }
    if let Some(c) = &status_colors {
        c.save(root)?;
    }

    let lifecycles: Vec<(String, Lifecycle)> = outcomes
        .iter()
        .filter_map(|o| o.lifecycle.clone().map(|l| (o.type_name.clone(), l)))
        .collect();
    persist_clickup_lifecycles(root, &lifecycles)?;

    // Continue-then-exit-non-zero: a per-type failure fails the run, but only
    // after every other type refreshed and its cache was saved. A warnings-only
    // run has no `error` and exits zero.
    if outcomes.iter().any(|o| o.error.is_some()) {
        bail!("fetch failed for one or more types");
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::cache_lock::CacheLock;
    use crate::engine::clickup::{ClickupUser, FakeClickupClient};
    use crate::engine::config::{NumberingStrategy, StoreBackend, TypeDef};
    use crate::engine::gh::{
        GhComment, GhFieldValueInput, GhIssue, GhMilestone, GqlVar, ProjectFieldValue,
    };
    use crate::engine::git_ref::test_support::MockGitRefClient;
    use tempfile::TempDir;

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

    // The git-ref fetch logic lives in `engine::sync::fetch_git_ref` since
    // ITERATION-285; `GitRefSync` (driven by `sync_all`) is the CLI's only caller.
    // These exercise that relocated fn directly to keep its cache/lock mechanics
    // covered from the surface that depends on it.
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

        let counts =
            crate::engine::sync::fetch_git_ref(root, &mock, "origin", "iteration").unwrap();

        assert_eq!(counts.fetched, 2);
        assert_eq!(counts.new, 2);
        assert_eq!(counts.removed, 0);

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

        let counts =
            crate::engine::sync::fetch_git_ref(root, &mock, "origin", "iteration").unwrap();

        assert_eq!(counts.fetched, 0);
        assert_eq!(counts.new, 0);
        assert_eq!(counts.removed, 1);

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

        let counts =
            crate::engine::sync::fetch_git_ref(root, &mock, "origin", "iteration").unwrap();

        assert_eq!(counts.fetched, 0);
        assert_eq!(counts.new, 0);
        assert_eq!(counts.removed, 0);

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

        let counts =
            crate::engine::sync::fetch_git_ref(root, &mock, "origin", "iteration").unwrap();

        assert_eq!(counts.fetched, 0);
        assert_eq!(counts.new, 0);
        assert_eq!(counts.removed, 0);

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

        let counts =
            crate::engine::sync::fetch_git_ref(root, &mock, "origin", "iteration").unwrap();

        assert_eq!(counts.fetched, 1);
        assert_eq!(counts.new, 0); // existing doc updated, not new
        assert_eq!(counts.removed, 0);

        assert_eq!(
            std::fs::read_to_string(cache_dir.join("ITERATION-042.md")).unwrap(),
            "updated content"
        );

        let lock = CacheLock::load(root).unwrap();
        assert_eq!(lock.get("iteration/ITERATION-042"), Some("newsha"));
    }

    fn git_ref_type(name: &str, prefix: &str) -> TypeDef {
        TypeDef {
            name: name.to_string(),
            plural: format!("{}s", name),
            dir: format!("docs/{}", name),
            prefix: prefix.to_string(),
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
            clickup_custom_field_map: None,
        }
    }

    fn fake_clickup() -> FakeClickupClient {
        FakeClickupClient::valid(ClickupUser {
            id: 1,
            username: "fake".to_string(),
            email: "fake@example.com".to_string(),
        })
    }

    // AC (STORY-202): all types succeed -> every cache persisted, exit zero.
    #[test]
    fn run_persists_all_git_ref_caches_and_exits_ok_when_every_type_succeeds() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let mut config = Config::default();
        config.documents.types = vec![git_ref_type("alpha", "ALPHA"), git_ref_type("beta", "BETA")];

        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_list_result(Ok(vec![(
                "refs/lazyspec/alpha/ALPHA-1".to_string(),
                "sha1".to_string(),
            )]))
            .with_read_blob_result(Ok("# alpha".to_string()))
            .with_fetch_result(Ok(()))
            .with_list_result(Ok(vec![(
                "refs/lazyspec/beta/BETA-1".to_string(),
                "sha2".to_string(),
            )]))
            .with_read_blob_result(Ok("# beta".to_string()));

        let gh = StubGh;
        let clickup = fake_clickup();

        let result = run(
            root, &config, &gh, &mock, &clickup, None, "origin", None, false,
        );
        assert!(
            result.is_ok(),
            "all-succeed fetch must exit zero: {result:?}"
        );

        assert!(root.join(".lazyspec/cache/alpha/ALPHA-1.md").exists());
        assert!(root.join(".lazyspec/cache/beta/BETA-1.md").exists());
        let lock = CacheLock::load(root).unwrap();
        assert_eq!(lock.get("alpha/ALPHA-1"), Some("sha1"));
        assert_eq!(lock.get("beta/BETA-1"), Some("sha2"));
    }

    // AC (STORY-202): one type fails -> the rest still refresh, successes are
    // persisted, and the process exits non-zero.
    #[test]
    fn run_continues_past_a_failing_type_persists_successes_and_exits_non_zero() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let mut config = Config::default();
        config.documents.types = vec![git_ref_type("alpha", "ALPHA"), git_ref_type("beta", "BETA")];

        // alpha fetches cleanly; beta's fetch fails. sync_all fetches types in
        // config order, so alpha is fully written before beta errors.
        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_list_result(Ok(vec![(
                "refs/lazyspec/alpha/ALPHA-1".to_string(),
                "sha1".to_string(),
            )]))
            .with_read_blob_result(Ok("# alpha".to_string()))
            .with_fetch_result(Err(anyhow::anyhow!("beta remote unreachable")));

        let gh = StubGh;
        let clickup = fake_clickup();

        let result = run(
            root, &config, &gh, &mock, &clickup, None, "origin", None, false,
        );
        assert!(
            result.is_err(),
            "a failing type must make fetch exit non-zero"
        );

        // The type that succeeded is still persisted despite beta's failure.
        assert!(root.join(".lazyspec/cache/alpha/ALPHA-1.md").exists());
        assert_eq!(
            CacheLock::load(root).unwrap().get("alpha/ALPHA-1"),
            Some("sha1")
        );
    }

    /// Satisfies `run`'s combined GitHub trait bound so the git-ref-only fetch
    /// tests can call it; the GitHub path is never entered, so every method
    /// panics if reached.
    struct StubGh;

    impl GhIssueReader for StubGh {
        fn issue_list(
            &self,
            _: &str,
            _: &[String],
            _: &[String],
            _: Option<u64>,
        ) -> Result<Vec<GhIssue>> {
            unimplemented!("StubGh is unused in git-ref-only fetch tests")
        }
        fn issue_view(&self, _: &str, _: u64) -> Result<GhIssue> {
            unimplemented!()
        }
        fn issue_comments(&self, _: &str, _: u64) -> Result<Vec<GhComment>> {
            unimplemented!()
        }
    }

    impl GhIssueWriter for StubGh {
        fn issue_create(&self, _: &str, _: &str, _: &str, _: &[String]) -> Result<GhIssue> {
            unimplemented!()
        }
        fn issue_edit(
            &self,
            _: &str,
            _: u64,
            _: Option<&str>,
            _: Option<&str>,
            _: &[String],
            _: &[String],
        ) -> Result<()> {
            unimplemented!()
        }
        fn issue_close(&self, _: &str, _: u64) -> Result<()> {
            unimplemented!()
        }
        fn issue_reopen(&self, _: &str, _: u64) -> Result<()> {
            unimplemented!()
        }
        fn label_create(&self, _: &str, _: &str, _: &str, _: &str) -> Result<()> {
            unimplemented!()
        }
        fn label_ensure(&self, _: &str, _: &str, _: &str, _: &str) -> Result<()> {
            unimplemented!()
        }
    }

    impl GhGraphql for StubGh {
        fn graphql(&self, _: &str, _: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
            unimplemented!()
        }
        fn project_item_fields(&self, _: &str, _: &str) -> Result<Vec<ProjectFieldValue>> {
            unimplemented!()
        }
        fn update_project_v2_item_field_value(
            &self,
            _: &str,
            _: &str,
            _: &str,
            _: &GhFieldValueInput,
        ) -> Result<()> {
            unimplemented!()
        }
        fn clear_project_field(&self, _: &str, _: &str, _: &str) -> Result<()> {
            unimplemented!()
        }
    }

    impl GhMilestoneApi for StubGh {
        fn milestone_list(&self, _: &str) -> Result<Vec<GhMilestone>> {
            unimplemented!()
        }
        fn milestone_view(&self, _: &str, _: u64) -> Result<GhMilestone> {
            unimplemented!()
        }
        fn milestone_create(
            &self,
            _: &str,
            _: &str,
            _: &str,
            _: Option<&str>,
            _: &str,
        ) -> Result<GhMilestone> {
            unimplemented!()
        }
        fn milestone_edit(
            &self,
            _: &str,
            _: u64,
            _: Option<&str>,
            _: Option<&str>,
            _: Option<&str>,
            _: Option<&str>,
        ) -> Result<GhMilestone> {
            unimplemented!()
        }
        fn milestone_delete(&self, _: &str, _: u64) -> Result<()> {
            unimplemented!()
        }
        fn issue_set_milestone(&self, _: &str, _: u64, _: Option<u64>) -> Result<()> {
            unimplemented!()
        }
    }
}
