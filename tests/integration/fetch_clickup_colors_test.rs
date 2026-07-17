//! ITERATION-283 task 4: a clickup-tasks fetch captures the bound List's
//! per-status colours into `.lazyspec/status-colors.json` (cache artifact) and
//! never writes colour content into `.lazyspec.toml`.

use anyhow::Result;
use lazyspec::engine::clickup::{ClickupStatus, FakeClickupClient};
use lazyspec::engine::config::Config;
use lazyspec::engine::credentials::Token;
use lazyspec::engine::gh::{
    GhGraphql, GhIssue, GhIssueReader, GhIssueWriter, GhMilestone, GhMilestoneApi, GqlVar,
};
use lazyspec::engine::git_ref::GitCli;
use lazyspec::engine::status_colors::StatusColors;
use tempfile::TempDir;

struct NoopGh;
impl GhIssueReader for NoopGh {
    fn issue_list(
        &self,
        _repo: &str,
        _labels: &[String],
        _json_fields: &[String],
        _limit: Option<u64>,
    ) -> Result<Vec<GhIssue>> {
        unreachable!("not used in this test (clickup types only)")
    }
    fn issue_view(&self, _repo: &str, _number: u64) -> Result<GhIssue> {
        unreachable!("not used in this test (clickup types only)")
    }
    fn issue_comments(
        &self,
        _repo: &str,
        _number: u64,
    ) -> Result<Vec<lazyspec::engine::gh::GhComment>> {
        unreachable!("not used in this test (clickup types only)")
    }
}
impl GhGraphql for NoopGh {
    fn graphql(&self, _query: &str, _vars: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
        unreachable!("not used in this test (clickup types only)")
    }
    fn project_item_fields(
        &self,
        _repo: &str,
        _content_node_id: &str,
    ) -> Result<Vec<lazyspec::engine::gh::ProjectFieldValue>> {
        unreachable!("not used in this test (clickup types only)")
    }
    fn update_project_v2_item_field_value(
        &self,
        _project_id: &str,
        _item_id: &str,
        _field_id: &str,
        _value: &lazyspec::engine::gh::GhFieldValueInput,
    ) -> Result<()> {
        unreachable!("not used in this test (clickup types only)")
    }
    fn clear_project_field(
        &self,
        _project_id: &str,
        _item_id: &str,
        _field_id: &str,
    ) -> Result<()> {
        unreachable!("not used in this test (clickup types only)")
    }
}
impl GhIssueWriter for NoopGh {
    fn issue_create(
        &self,
        _repo: &str,
        _title: &str,
        _body: &str,
        _labels: &[String],
    ) -> Result<GhIssue> {
        unreachable!("not used in this test (clickup types only)")
    }
    fn issue_edit(
        &self,
        _repo: &str,
        _number: u64,
        _title: Option<&str>,
        _body: Option<&str>,
        _labels_add: &[String],
        _labels_remove: &[String],
    ) -> Result<()> {
        unreachable!("not used in this test (clickup types only)")
    }
    fn issue_close(&self, _repo: &str, _number: u64) -> Result<()> {
        unreachable!("not used in this test (clickup types only)")
    }
    fn issue_reopen(&self, _repo: &str, _number: u64) -> Result<()> {
        unreachable!("not used in this test (clickup types only)")
    }
    fn label_create(
        &self,
        _repo: &str,
        _name: &str,
        _description: &str,
        _color: &str,
    ) -> Result<()> {
        unreachable!("not used in this test (clickup types only)")
    }
    fn label_ensure(
        &self,
        _repo: &str,
        _name: &str,
        _description: &str,
        _color: &str,
    ) -> Result<()> {
        unreachable!("not used in this test (clickup types only)")
    }
}
impl GhMilestoneApi for NoopGh {
    fn milestone_list(&self, _repo: &str) -> Result<Vec<GhMilestone>> {
        unreachable!("not used in this test (clickup types only)")
    }
    fn milestone_view(&self, _repo: &str, _number: u64) -> Result<GhMilestone> {
        unreachable!("not used in this test (clickup types only)")
    }
    fn milestone_create(
        &self,
        _repo: &str,
        _title: &str,
        _description: &str,
        _due_on: Option<&str>,
        _state: &str,
    ) -> Result<GhMilestone> {
        unreachable!("not used in this test (clickup types only)")
    }
    fn milestone_edit(
        &self,
        _repo: &str,
        _number: u64,
        _title: Option<&str>,
        _description: Option<&str>,
        _due_on: Option<&str>,
        _state: Option<&str>,
    ) -> Result<GhMilestone> {
        unreachable!("not used in this test (clickup types only)")
    }
    fn milestone_delete(&self, _repo: &str, _number: u64) -> Result<()> {
        unreachable!("not used in this test (clickup types only)")
    }
    fn issue_set_milestone(
        &self,
        _repo: &str,
        _issue_number: u64,
        _milestone: Option<u64>,
    ) -> Result<()> {
        unreachable!("not used in this test (clickup types only)")
    }
}

const CONFIG_SRC: &str = r#"[naming]
pattern = "{type}-{n:03}-{title}.md"

[templates]
dir = ".lazyspec/templates"

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

fn status(name: &str, orderindex: i64, color: &str) -> ClickupStatus {
    ClickupStatus {
        status: name.to_string(),
        orderindex,
        status_type: String::new(),
        color: color.to_string(),
    }
}

#[test]
fn fetch_captures_status_colors_into_cache_not_config() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::write(root.join(".lazyspec.toml"), CONFIG_SRC).unwrap();
    let config = Config::parse(CONFIG_SRC).unwrap();

    let clickup = FakeClickupClient::with_tasks(vec![]).with_statuses(vec![
        status("to do", 0, "#87909e"),
        status("in progress", 1, "#4194f6"),
        status("done", 2, ""),
    ]);
    let token = Token::new("pk_test");

    lazyspec::cli::fetch::run(
        root,
        &config,
        &NoopGh,
        &GitCli,
        &clickup,
        Some(&token),
        None,
        true,
    )
    .expect("fetch");

    let colors = StatusColors::load(root).expect("load status colors");
    assert_eq!(colors.get("task", "to do"), Some("#87909e"));
    assert_eq!(colors.get("task", "in progress"), Some("#4194f6"));
    assert!(
        colors.get("task", "done").is_none(),
        "empty colour must be omitted"
    );
    assert!(root.join(".lazyspec/status-colors.json").exists());

    let toml_out = std::fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
    assert!(
        !toml_out.contains("#87909e") && !toml_out.contains("#4194f6"),
        "colours must never land in .lazyspec.toml, got:\n{toml_out}"
    );
    assert!(
        !toml_out.contains("color"),
        "no colour keys in config, got:\n{toml_out}"
    );
}
