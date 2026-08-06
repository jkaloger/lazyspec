//! Regression: a fresh `fetch` (empty issue-map) must surface an issue's native
//! GitHub milestone as a forward `targets: MILESTONE-n` relation on the cached
//! issue doc. The forward edge resolves the milestone number through the
//! issue-map, so the milestone must be fetched (and mapped) BEFORE issues. The
//! bug was the reverse fetch order: on the first fetch the milestone was not yet
//! in the map, the lookup returned `None`, and the relation was silently dropped.

use anyhow::Result;
use lazyspec::engine::config::{
    Config, GithubConfig, NumberingStrategy, RelationshipDef, StoreBackend, TypeDef,
};
use lazyspec::engine::gh::{
    test_support, GhComment, GhFieldValueInput, GhGraphql, GhIssue, GhIssueDependencyApi,
    GhIssueMilestone, GhIssueReader, GhIssueWriter, GhMilestone, GhMilestoneApi, GqlVar,
    ProjectItem,
};
use lazyspec::engine::git_ref::GitCli;
use tempfile::TempDir;

/// A gh fake exposing one issue carrying milestone #1 and one milestone #1.
/// graphql answers the schema-snapshot and parentage queries with empties so the
/// fetch path settles a flat cache layout; writes are unused.
struct MilestoneGh {
    issues: Vec<GhIssue>,
    milestones: Vec<GhMilestone>,
}

impl GhIssueReader for MilestoneGh {
    fn issue_list(
        &self,
        _repo: &str,
        _labels: &[String],
        _json_fields: &[String],
        _limit: Option<u64>,
    ) -> Result<Vec<GhIssue>> {
        unreachable!("a fetch reads issues off the composed round, never REST")
    }
    fn issue_view(&self, _repo: &str, _number: u64) -> Result<GhIssue> {
        unreachable!("issue_view not used")
    }
    fn issue_comments(&self, _repo: &str, _number: u64) -> Result<Vec<GhComment>> {
        Ok(vec![])
    }
}

impl GhGraphql for MilestoneGh {
    fn graphql(&self, query: &str, _vars: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
        if lazyspec::engine::gh_fetch::is_round_query(query) {
            return Ok(test_support::with_issue_pages(
                query,
                test_support::round_response(&self.milestones, &[], &[]),
                &self.issues,
            ));
        }
        Ok(serde_json::json!({
            "data": { "organization": { "issueTypes": { "nodes": [] } } }
        }))
    }
    fn project_items(&self, _repo: &str, _content_node_id: &str) -> Result<Vec<ProjectItem>> {
        Ok(vec![])
    }
    fn update_project_v2_item_field_value(
        &self,
        _project_id: &str,
        _item_id: &str,
        _field_id: &str,
        _value: &GhFieldValueInput,
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

impl GhIssueWriter for MilestoneGh {
    fn issue_create(&self, _: &str, _: &str, _: &str, _: &[String]) -> Result<GhIssue> {
        unreachable!()
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
        unreachable!()
    }
    fn issue_close(&self, _: &str, _: u64) -> Result<()> {
        unreachable!()
    }
    fn issue_reopen(&self, _: &str, _: u64) -> Result<()> {
        unreachable!()
    }
    fn issue_set_assignee(&self, _: &str, _: u64, _: &[String], _: &[String]) -> Result<()> {
        Ok(())
    }

    fn label_create(&self, _: &str, _: &str, _: &str, _: &str) -> Result<()> {
        unreachable!()
    }
    fn label_ensure(&self, _: &str, _: &str, _: &str, _: &str) -> Result<()> {
        unreachable!()
    }
}

impl GhMilestoneApi for MilestoneGh {
    fn milestone_view(&self, _repo: &str, number: u64) -> Result<GhMilestone> {
        self.milestones
            .iter()
            .find(|m| m.number == number)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("milestone {} not found", number))
    }
    fn milestone_create(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: Option<&str>,
        _: &str,
    ) -> Result<GhMilestone> {
        unreachable!()
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
        unreachable!()
    }
    fn milestone_delete(&self, _: &str, _: u64) -> Result<()> {
        unreachable!()
    }
    fn issue_set_milestone(&self, _: &str, _: u64, _: Option<u64>) -> Result<()> {
        unreachable!()
    }
}

impl GhIssueDependencyApi for MilestoneGh {
    fn add_blocked_by(&self, _: &str, _: u64, _: u64) -> Result<()> {
        unreachable!()
    }
    fn remove_blocked_by(&self, _: &str, _: u64, _: u64) -> Result<()> {
        unreachable!()
    }
}

fn ticket_type() -> TypeDef {
    TypeDef {
        name: "ticket".to_string(),
        plural: "tickets".to_string(),
        dir: "docs/tickets".to_string(),
        prefix: "TICKET".to_string(),
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
        status_authority: None,
        clickup_list_id: None,
        clickup_task_type: None,
        clickup_custom_field_map: None,
    }
}

fn milestone_type() -> TypeDef {
    TypeDef {
        name: "milestone".to_string(),
        plural: "milestones".to_string(),
        dir: "docs/milestones".to_string(),
        prefix: "MILESTONE".to_string(),
        store: StoreBackend::GithubMilestones,
        ..ticket_type()
    }
}

fn config() -> Config {
    let mut config = Config::default();
    config.documents.types = vec![ticket_type(), milestone_type()];
    config.documents.github = Some(GithubConfig {
        repo: Some("owner/repo".to_string()),
        cache_ttl: 60,
    });
    config.relationships = vec![RelationshipDef {
        name: "targets".to_string(),
        inverse: Some("targeted-by".to_string()),
        github_native: Some("milestone".to_string()),
        traversal: None,
    }];
    config
}

#[test]
fn fresh_fetch_surfaces_issue_milestone_as_targets_relation() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    let gh = MilestoneGh {
        issues: vec![GhIssue {
            number: 64,
            id: "I_node64".to_string(),
            url: "https://github.com/owner/repo/issues/64".to_string(),
            title: "Exercise attr round-trip".to_string(),
            body: "body".to_string(),
            labels: vec![],
            state: "OPEN".to_string(),
            updated_at: "2026-06-26T10:00:00Z".to_string(),
            created_at: "2026-06-26T10:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: Some(GhIssueMilestone { number: 1 }),
            assignees: vec![],
        }],
        milestones: vec![GhMilestone {
            number: 1,
            title: "v0.1".to_string(),
            description: "first".to_string(),
            due_on: None,
            state: "open".to_string(),
            open_issues: 1,
            closed_issues: 0,
            url: String::new(),
        }],
    };

    // Fresh root: the issue-map starts empty, so the milestone must be fetched
    // and mapped before the issue for the forward relation to resolve.
    lazyspec::cli::fetch::run(
        root,
        &config(),
        &gh,
        &GitCli,
        &lazyspec::engine::clickup::FakeClickupClient::with_tasks(vec![]),
        None,
        None,
        true,
    )
    .unwrap();

    let ticket = std::fs::read_to_string(root.join(".lazyspec/cache/ticket/TICKET-64.md"))
        .expect("TICKET-64 cache doc written");
    assert!(
        ticket.contains("targets: MILESTONE-1"),
        "issue's native milestone must surface as a forward targets relation:\n{ticket}"
    );
}
