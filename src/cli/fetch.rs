use crate::engine::clickup::ClickupClient;
use crate::engine::config::{Config, Lifecycle, StoreBackend};
use crate::engine::config_write::write_config_in_place;
use crate::engine::credentials::Token;
use crate::engine::gh::{GhGraphql, GhIssueReader, GhIssueWriter, GhMilestoneApi};
use crate::engine::gh_schema::GhSchemaSnapshot;
use crate::engine::git_ref::GitRefOps;
use crate::engine::github::resolve_repo;
use crate::engine::issue_body::TypeMatchRule;
use crate::engine::issue_map::IssueMap;
use crate::engine::status_colors::StatusColors;
use crate::engine::store_dispatch;
use crate::engine::sync::{
    sync_all, ClickupMaps, ClickupSync, GhIssueSync, GhMaps, GhMilestoneSync, GhRound, GitRefSync,
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

    let pb = crate::cli::spinner::op_spinner("fetching from remotes", json);
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
            fetch: None,
        };

        let mut syncers = Syncers::default();
        if let Some(repo) = repo.clone() {
            syncers.round = Some(GhRound { gh, repo });
        }
        if !fetch_milestones.is_empty() {
            syncers.milestone = Some(GhMilestoneSync);
        }
        if !fetch_gh.is_empty() {
            syncers.issue = Some(GhIssueSync {
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
                remote: config.git_ref.remote.clone(),
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

    if outcomes.iter().any(|o| o.error.is_some()) {
        crate::cli::spinner::finish_err(pb, "fetch completed with errors");
    } else {
        crate::cli::spinner::finish_ok(pb, "fetch complete");
    }

    for o in &outcomes {
        for w in &o.warnings {
            eprintln!("warning: {}", w);
        }
    }

    if json {
        println!("{}", outcomes_json(&outcomes)?);
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
    persist_board_lifecycles(root, config)?;

    // Continue-then-exit-non-zero: a per-type failure fails the run, but only
    // after every other type refreshed and its cache was saved. A warnings-only
    // run has no `error` and exits zero.
    if outcomes.iter().any(|o| o.error.is_some()) {
        bail!("fetch failed for one or more types");
    }

    Ok(())
}

/// One JSON entry per fetched type. `warnings` carries the same messages the
/// human run prints to stderr (a doc with no `Status` on its authority board, a
/// stale-cache fallback, a truncated search) and is present only when the type
/// produced some, mirroring the mutation commands' `warnings` array. `error` is
/// present only for a type whose fetch failed.
pub fn outcomes_json(outcomes: &[crate::engine::sync::SyncOutcome]) -> Result<String> {
    let entries: Vec<serde_json::Value> = outcomes
        .iter()
        .map(|o| {
            let mut entry = serde_json::json!({
                "type": o.type_name,
                "fetched": o.fetched,
                "new": o.new,
                "removed": o.removed,
            });
            if !o.warnings.is_empty() {
                entry["warnings"] = serde_json::json!(o.warnings);
            }
            if let Some(err) = &o.error {
                entry["error"] = serde_json::Value::String(err.clone());
            }
            entry
        })
        .collect();
    Ok(serde_json::to_string_pretty(&entries)?)
}

/// Write each `(type, lifecycle)` derived from a bound List's status set back
/// into `.lazyspec.toml`, so the type's effective lifecycle reflects the live
/// List.
fn persist_clickup_lifecycles(root: &Path, lifecycles: &[(String, Lifecycle)]) -> Result<()> {
    rewrite_lifecycles(root, lifecycles)
}

/// Write the lifecycle each `status_authority`-bound type derives from its
/// board's `Status` column set back into `.lazyspec.toml`, so
/// [`TypeDef::effective_lifecycle`](crate::engine::config::TypeDef::effective_lifecycle)
/// serves the board's columns through its declared-states branch and every
/// surface picks them up unchanged.
///
/// Reads the schema snapshot from disk, so it must run after the sync phase that
/// writes it.
fn persist_board_lifecycles(root: &Path, config: &Config) -> Result<()> {
    let snapshot = GhSchemaSnapshot::load(root);
    let lifecycles: Vec<(String, Lifecycle)> = config
        .documents
        .types
        .iter()
        .filter_map(|type_def| {
            let number =
                store_dispatch::board_number(type_def.status_authority.as_deref()?).ok()?;
            // A board with no resolvable `Status` column yields None, which is
            // dropped rather than persisted as an empty lifecycle: writing one
            // would wipe the states the type already declares.
            let lifecycle = snapshot.status_lifecycle(number)?;
            Some((type_def.name.clone(), lifecycle))
        })
        .collect();
    rewrite_lifecycles(root, &lifecycles)
}

/// Apply each `(type, lifecycle)` to `.lazyspec.toml`, rewriting the file only
/// when a lifecycle actually changed so an unchanged fetch leaves it untouched.
///
/// `write_config_in_place` is mandatory here, not a nicety: `[github]` and
/// `[[types]]` are `serde(skip)`/`skip_deserializing`, so serializing the whole
/// `Config` would drop them and leave the file unparseable. The in-place writer
/// is the only lossless path, and it preserves comments and formatting.
fn rewrite_lifecycles(root: &Path, lifecycles: &[(String, Lifecycle)]) -> Result<()> {
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
        test_support::GhRequestCounter, GhComment, GhFieldValueInput, GhIssue, GhMilestone, GqlVar,
        ProjectItem,
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

    /// A `github-issues` type nominating board 7 as its status authority, with no
    /// `lifecycle` key of its own. The comment above the type block makes decor
    /// preservation provable, and `[github]` is required by strict parse.
    const GH_AUTHORITY_CONFIG_SRC: &str = r#"[naming]
pattern = "{type}-{n:03}-{title}.md"

[templates]
dir = ".lazyspec/templates"

[github]
repo = "octo-org/repo"

# ticket type follows
[[types]]
name = "ticket"
plural = "tickets"
dir = "docs/tickets"
prefix = "TICKET"
store = "github-issues"
status_authority = "PROJECT-7"

[[relationships]]
name = "related-to"
"#;

    /// The same type, already declaring exactly the lifecycle board 7 derives.
    const GH_AUTHORITY_CONFIG_IN_SYNC_SRC: &str = r#"[naming]
pattern = "{type}-{n:03}-{title}.md"

[templates]
dir = ".lazyspec/templates"

[github]
repo = "octo-org/repo"

# ticket type follows
[[types]]
name = "ticket"
plural = "tickets"
dir = "docs/tickets"
prefix = "TICKET"
store = "github-issues"
status_authority = "PROJECT-7"
lifecycle = { states = ["ready to start", "in progress", "review", "done"], edges = [] }

[[relationships]]
name = "related-to"
"#;

    /// A `github-issues` type that nominates no authority board (STORY-224's
    /// shape), so nothing about it may change.
    const GH_NO_AUTHORITY_CONFIG_SRC: &str = r#"[naming]
pattern = "{type}-{n:03}-{title}.md"

[templates]
dir = ".lazyspec/templates"

[github]
repo = "octo-org/repo"

# ticket type follows
[[types]]
name = "ticket"
plural = "tickets"
dir = "docs/tickets"
prefix = "TICKET"
store = "github-issues"

[[relationships]]
name = "related-to"
"#;

    /// Write a schema snapshot holding one board whose single field is
    /// `field_name`, carrying `options` in the given order.
    fn write_board_snapshot(root: &Path, project_number: u64, field_name: &str, options: &[&str]) {
        use crate::engine::gh_schema::{GhSchemaSnapshot, OptionId, ProjectFieldId};
        let field_id = format!("PVTSSF_b{}", project_number);
        let snapshot = GhSchemaSnapshot {
            project_fields: vec![ProjectFieldId {
                project_number,
                field_name: field_name.to_string(),
                id: field_id.clone(),
                data_type: "SINGLE_SELECT".to_string(),
            }],
            single_select_options: options
                .iter()
                .map(|name| OptionId {
                    field_id: field_id.clone(),
                    name: (*name).to_string(),
                    id: format!("opt_{}", name.to_lowercase().replace(' ', "_")),
                })
                .collect(),
            ..Default::default()
        };
        snapshot.save(root).unwrap();
    }

    /// The shape RFC-065 is measured against: ten `github-issues` types, one
    /// `github-milestones` type, and one board nominated as a status authority.
    fn many_types_config_src() -> String {
        let mut src = String::from(
            "[naming]\npattern = \"{type}-{n:03}-{title}.md\"\n\n\
             [templates]\ndir = \".lazyspec/templates\"\n\n\
             [github]\nrepo = \"octo-org/repo\"\n\n\
             [[types]]\nname = \"release\"\nplural = \"releases\"\n\
             dir = \"docs/releases\"\nprefix = \"RELEASE\"\nstore = \"github-milestones\"\n\n",
        );
        // All four discovery rules across the ten types -- plain label, tag,
        // native issue type, and both -- so the round composes every alias
        // shape, not ten copies of one.
        // Letter-suffixed prefixes: a digit in a prefix stops `extract_id_from
        // _name` at the prefix itself, so `T0-1` would resolve as `T0` and the
        // cached doc would never match its issue-map entry.
        for (n, prefix) in ('A'..='J').enumerate() {
            let authority = if n == 0 {
                "status_authority = \"PROJECT-7\"\n"
            } else {
                ""
            };
            let rule = match n % 4 {
                1 => "github_issue_tag = \"triage\"\n",
                2 => "github_issue_type = \"Bug\"\n",
                3 => "github_issue_tag = \"triage\"\ngithub_issue_type = \"Bug\"\n",
                _ => "",
            };
            src.push_str(&format!(
                "[[types]]\nname = \"t{n}\"\nplural = \"t{n}s\"\ndir = \"docs/t{n}\"\n\
                 prefix = \"T{prefix}\"\nstore = \"github-issues\"\n{authority}{rule}\n"
            ));
        }
        src.push_str("[[relationships]]\nname = \"related-to\"\n\n");
        // Declared so the round's inline `blockedBy` edge is actually consumed:
        // the point of the count is that enrichment costs no request, which only
        // means something when the enrichment happens.
        src.push_str(
            "[[relationships]]\nname = \"blocks\"\ninverse = \"blocked-by\"\n\
             github_native = \"dependency\"\n",
        );
        src
    }

    // STORY-249 AC1/AC2: milestones, org issue types and every authority board's
    // field schema arrive together. Twelve types' worth of that work costs one
    // composed request, and no other GraphQL document is issued at all.
    //
    #[test]
    fn milestone_issue_type_and_board_schema_work_costs_one_composed_request() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let src = many_types_config_src();
        std::fs::write(root.join(".lazyspec.toml"), &src).unwrap();
        let config = Config::parse(&src).unwrap();

        let gh = GhRequestCounter::with_board(7, &["Review", "Done"]).with_enriched_issues();
        run(
            root,
            &config,
            &gh,
            &MockGitRefClient::new(),
            &fake_clickup(),
            None,
            None,
            true,
        )
        .unwrap();

        assert_eq!(
            gh.round_queries.borrow().len(),
            1,
            "one composed round for the whole fetch"
        );
        assert!(
            gh.other_queries.borrow().is_empty(),
            "no board-schema probe survives the round: {:?}",
            gh.other_queries.borrow()
        );
        assert_eq!(gh.milestone_list_calls.get(), 0);

        // That one request also carried the enrichment: #2 nests under #1 from
        // the round's `subIssues`, and #1 carries `blocked-by` from its
        // `blockedBy` -- both without a second query.
        assert!(
            root.join(".lazyspec/cache/t0/TA-1/00-TA-2.md").is_file(),
            "sub-issue parentage must materialize off the round"
        );
        let parent =
            std::fs::read_to_string(root.join(".lazyspec/cache/t0/TA-1/index.md")).unwrap();
        assert!(
            parent.contains("blocked-by: TA-2"),
            "dependency edges must come off the round, got:\n{parent}"
        );

        // And that one request is what carried the board's schema.
        let saved = GhSchemaSnapshot::load(root);
        assert_eq!(saved.field_id(7, "Status"), Some("PVTSSF_b7"));
        assert_eq!(
            saved.status_lifecycle(7).unwrap().states,
            vec!["review", "done"]
        );

        // STORY-251: and the board *memberships* too. `t0` hands its lifecycle
        // to board 7, so a status read off that board's `Status` cell is proof
        // the membership arrived on the same request as everything else.
        assert!(
            parent.contains("status: review"),
            "the authority board's cell must come off the round, got:\n{parent}"
        );
    }

    #[test]
    fn persist_board_lifecycles_writes_derived_states_into_config() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join(".lazyspec.toml"), GH_AUTHORITY_CONFIG_SRC).unwrap();
        write_board_snapshot(
            root,
            7,
            "Status",
            &["Ready To Start", "In Progress", "Review", "Done"],
        );

        let config = Config::parse(GH_AUTHORITY_CONFIG_SRC).unwrap();
        persist_board_lifecycles(root, &config).unwrap();

        let out = std::fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
        assert!(out.contains("# ticket type follows"), "got:\n{out}");
        let reparsed = Config::parse(&out).unwrap();
        let td = reparsed.type_by_name("ticket").unwrap();
        assert_eq!(
            td.lifecycle.states,
            vec!["ready to start", "in progress", "review", "done"]
        );
        assert!(td.lifecycle.edges.is_empty());
        assert_eq!(td.status_authority.as_deref(), Some("PROJECT-7"));
    }

    #[test]
    fn persist_board_lifecycles_leaves_config_untouched_when_unchanged() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join(".lazyspec.toml"), GH_AUTHORITY_CONFIG_IN_SYNC_SRC).unwrap();
        write_board_snapshot(
            root,
            7,
            "Status",
            &["Ready To Start", "In Progress", "Review", "Done"],
        );

        let config = Config::parse(GH_AUTHORITY_CONFIG_IN_SYNC_SRC).unwrap();
        persist_board_lifecycles(root, &config).unwrap();

        let out = std::fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
        assert_eq!(out, GH_AUTHORITY_CONFIG_IN_SYNC_SRC);
    }

    // An unresolvable board must never wipe the states the type already has, so
    // the subject here is the config that declares four of them.
    #[test]
    fn persist_board_lifecycles_skips_a_board_with_no_status_field() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join(".lazyspec.toml"), GH_AUTHORITY_CONFIG_IN_SYNC_SRC).unwrap();
        write_board_snapshot(root, 7, "Sprint", &[]);

        let config = Config::parse(GH_AUTHORITY_CONFIG_IN_SYNC_SRC).unwrap();
        persist_board_lifecycles(root, &config).unwrap();

        let out = std::fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
        assert_eq!(out, GH_AUTHORITY_CONFIG_IN_SYNC_SRC);
    }

    // STORY-248 AC10: STORY-224's canonical open/closed path is untouched -- a
    // type nominating no authority board gets no derived lifecycle, even with a
    // populated snapshot on disk.
    #[test]
    fn persist_board_lifecycles_ignores_types_without_status_authority() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join(".lazyspec.toml"), GH_NO_AUTHORITY_CONFIG_SRC).unwrap();
        write_board_snapshot(root, 7, "Status", &["Review", "Done"]);

        let config = Config::parse(GH_NO_AUTHORITY_CONFIG_SRC).unwrap();
        persist_board_lifecycles(root, &config).unwrap();

        let out = std::fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
        assert_eq!(out, GH_NO_AUTHORITY_CONFIG_SRC);
        let reparsed = Config::parse(&out).unwrap();
        assert!(reparsed
            .type_by_name("ticket")
            .unwrap()
            .lifecycle
            .states
            .is_empty());
    }

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
            status_authority: None,
            clickup_list_id: None,
            clickup_task_type: None,
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

        let result = run(root, &config, &gh, &mock, &clickup, None, None, false);
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

        let result = run(root, &config, &gh, &mock, &clickup, None, None, false);
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

    // STORY-218 AC1: fetch targets the remote from `[git-ref]`, not a hardcoded
    // `origin`. A config override reaches the git client's fetch call.
    #[test]
    fn run_fetches_git_ref_from_configured_remote() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let mut config = Config::default();
        config.documents.types = vec![git_ref_type("alpha", "ALPHA")];
        config.git_ref.remote = "upstream".to_string();

        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_list_result(Ok(vec![(
                "refs/lazyspec/alpha/ALPHA-1".to_string(),
                "sha1".to_string(),
            )]))
            .with_read_blob_result(Ok("# alpha".to_string()));

        let gh = StubGh;
        let clickup = fake_clickup();

        let result = run(root, &config, &gh, &mock, &clickup, None, None, false);
        assert!(result.is_ok(), "fetch must exit zero: {result:?}");

        let calls = mock.calls.borrow();
        assert!(
            calls
                .iter()
                .any(|c| c == "fetch_refs:upstream:refs/lazyspec/alpha/*"),
            "fetch should target the configured remote, got: {calls:?}"
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
        fn issue_set_assignee(&self, _: &str, _: u64, _: &[String], _: &[String]) -> Result<()> {
            Ok(())
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
        fn project_items(&self, _: &str, _: &str) -> Result<Vec<ProjectItem>> {
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

    impl crate::engine::gh::GhIssueDependencyApi for StubGh {
        fn list_blocked_by(&self, _: &str, _: u64) -> Result<Vec<u64>> {
            unimplemented!()
        }
        fn add_blocked_by(&self, _: &str, _: u64, _: u64) -> Result<()> {
            unimplemented!()
        }
        fn remove_blocked_by(&self, _: &str, _: u64, _: u64) -> Result<()> {
            unimplemented!()
        }
    }
}
