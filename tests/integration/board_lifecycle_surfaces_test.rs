use anyhow::Result;
use lazyspec::engine::config::Config;
use lazyspec::engine::document::Status;
use lazyspec::engine::gh::{
    GhAuthor, GhComment, GhFieldValueInput, GhGraphql, GhIssue, GhIssueDependencyApi,
    GhIssueReader, GhLabel, GqlVar, ProjectFieldValue,
};
use lazyspec::engine::issue_body::TypeMatchRule;
use lazyspec::engine::issue_cache::IssueCache;
use lazyspec::engine::issue_map::IssueMap;
use lazyspec::engine::store::Store;
use lazyspec::tui::state::App;
use std::fs;
use tempfile::TempDir;

/// The state a fetch leaves behind for a board-bound type (STORY-248): board 7's
/// `Status` columns persisted as the type's declared lifecycle -- lowercased, in
/// board order, no edges. Every surface reads this through
/// `TypeDef::effective_lifecycle`'s declared-states branch.
const BOARD_BOUND_CONFIG: &str = r#"[naming]
pattern = "{type}-{n:03}-{title}.md"

[templates]
dir = ".lazyspec/templates"

[github]
repo = "octo-org/repo"

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

/// The same board-bound type, but with hand-declared transition `edges` -- a
/// shape the nominated board cannot produce, since a board carries column order
/// and no transition rules.
const DECLARED_EDGES_CONFIG: &str = r#"[naming]
pattern = "{type}-{n:03}-{title}.md"

[templates]
dir = ".lazyspec/templates"

[github]
repo = "octo-org/repo"

[[types]]
name = "ticket"
plural = "tickets"
dir = "docs/tickets"
prefix = "TICKET"
store = "github-issues"
status_authority = "PROJECT-7"
lifecycle = { states = ["review", "done"], edges = [{ from = "review", to = "done" }] }

[[relationships]]
name = "related-to"
"#;

const BOARD_STATES: [&str; 4] = ["ready to start", "in progress", "review", "done"];

/// A project whose config is post-fetch, plus one cached ticket sitting at the
/// board column `in progress`. github-issues docs live under
/// `.lazyspec/cache/<type>/`, which is where the store reads them from.
fn board_bound_project() -> TempDir {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".lazyspec.toml"), BOARD_BOUND_CONFIG).unwrap();

    let cache = tmp.path().join(".lazyspec/cache/ticket");
    fs::create_dir_all(&cache).unwrap();
    fs::write(
        cache.join("TICKET-001-board-bound.md"),
        "---\ntitle: \"Board bound\"\ntype: ticket\nstatus: in progress\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\nbody\n",
    )
    .unwrap();

    tmp
}

// DICTUM-006: `config --json` reports the board-derived states, in board order.
#[test]
fn config_json_reports_the_board_derived_lifecycle_in_board_order() {
    let config = Config::parse(BOARD_BOUND_CONFIG).unwrap();

    let json = lazyspec::cli::config::run_show_json(&config).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let ticket = parsed["types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "ticket")
        .expect("config --json reports the ticket type");
    let states: Vec<&str> = ticket["lifecycle"]["states"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();

    assert_eq!(states, BOARD_STATES);
    assert!(ticket["lifecycle"]["edges"].as_array().unwrap().is_empty());
}

#[test]
fn validate_json_reports_no_error_for_a_board_bound_type() {
    let tmp = board_bound_project();
    let config = Config::parse(BOARD_BOUND_CONFIG).unwrap();
    let store = Store::load(tmp.path(), &config).unwrap();

    let json = lazyspec::cli::validate::run_json(&store, &config, &[]);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(
        parsed["errors"].as_array().unwrap().is_empty(),
        "got: {json}"
    );
    assert!(
        parsed["parse_errors"].as_array().unwrap().is_empty(),
        "got: {json}"
    );
}

// DICTUM-006: the conflict reaches `--json` consumers, not just the human render.
#[test]
fn validate_json_reports_a_lifecycle_the_nominated_board_cannot_own() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".lazyspec.toml"), DECLARED_EDGES_CONFIG).unwrap();
    let config = Config::parse(DECLARED_EDGES_CONFIG).unwrap();
    let store = Store::load(tmp.path(), &config).unwrap();

    let json = lazyspec::cli::validate::run_json(&store, &config, &[]);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let errors = parsed["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1, "got: {json}");
    let message = errors[0].as_str().unwrap();
    assert!(message.contains("status_authority"), "got: {json}");
    assert!(message.contains("lifecycle"), "got: {json}");
    assert!(message.contains("ticket"), "got: {json}");
    assert!(message.contains("PROJECT-7"), "got: {json}");
}

// The reason lowercasing the board's column names is forced, not cosmetic: a
// doc's status is lowercased on parse, so `In Progress` would never match.
#[test]
fn a_doc_at_a_board_derived_status_is_accepted() {
    let tmp = board_bound_project();
    let config = Config::parse(BOARD_BOUND_CONFIG).unwrap();
    let store = Store::load(tmp.path(), &config).unwrap();

    let ticket = config.type_by_name("ticket").unwrap();
    assert!(ticket.accepts_status(&Status::new("in progress")));

    let doc = store
        .all_docs()
        .into_iter()
        .find(|d| d.doc_type.as_str() == "ticket")
        .expect("the cached ticket loads");
    assert_eq!(doc.status, Status::new("in progress"));
    assert!(ticket.accepts_status(&doc.status));
}

fn board_bound_app(tmp: &TempDir, config: &Config) -> App {
    let store = Store::load(tmp.path(), config).unwrap();
    App::new(
        store,
        config,
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    )
}

// AC1: the TUI status DAG reports the board's columns, in board order -- neither
// alphabetical nor the github canonical open/closed pair.
#[test]
fn tui_status_dag_reports_board_derived_states_in_board_order() {
    let tmp = board_bound_project();
    let config = Config::parse(BOARD_BOUND_CONFIG).unwrap();

    let app = board_bound_app(&tmp, &config);

    assert_eq!(app.available_statuses, BOARD_STATES);
}

// AC1: an edgeless board lifecycle leaves the DAG unconstrained, so any column
// moves to any other. The picker leads with the current status (a no-op) and then
// offers every other column -- `targets_from` excludes the current one.
#[test]
fn tui_status_picker_offers_every_board_column_from_any_column() {
    let tmp = board_bound_project();
    let config = Config::parse(BOARD_BOUND_CONFIG).unwrap();
    let mut app = board_bound_app(&tmp, &config);

    app.selected_type = 0;
    app.selected_doc = 0;
    app.open_status_picker(&config);

    assert!(app.status_picker.active);
    assert_eq!(
        app.status_picker.states,
        ["in progress", "ready to start", "review", "done"]
    );

    let ticket = config.type_by_name("ticket").unwrap();
    assert_eq!(
        ticket.effective_lifecycle().targets_from("in progress"),
        ["ready to start", "review", "done"]
    );
}

/// A gh fake at the `GhIssueReader`/`GhGraphql` seam serving one OPEN issue that
/// carries the ticket type's identity label and no lazyspec body, so its status
/// comes purely from the type's lifecycle. No sub-issue parentage, no
/// dependencies, no network.
struct OneOpenIssueGh;

impl GhIssueReader for OneOpenIssueGh {
    fn issue_list(
        &self,
        _repo: &str,
        _labels: &[String],
        _json_fields: &[String],
        _limit: Option<u64>,
    ) -> Result<Vec<GhIssue>> {
        Ok(vec![GhIssue {
            number: 42,
            id: String::new(),
            url: "https://github.com/octo-org/repo/issues/42".to_string(),
            title: "Board bound issue".to_string(),
            body: "plain body, no lazyspec frontmatter".to_string(),
            labels: vec![GhLabel {
                name: "lazyspec:ticket".to_string(),
                color: String::new(),
            }],
            state: "OPEN".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            author: Some(GhAuthor {
                login: "octocat".to_string(),
            }),
            issue_type: None,
            milestone: None,
            assignees: vec![],
        }])
    }
    fn issue_view(&self, _repo: &str, _number: u64) -> Result<GhIssue> {
        unreachable!("label discovery never views a single issue")
    }
    fn issue_comments(&self, _repo: &str, _number: u64) -> Result<Vec<GhComment>> {
        Ok(vec![])
    }
}

impl GhGraphql for OneOpenIssueGh {
    fn graphql(&self, _query: &str, _vars: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
        Ok(serde_json::json!({ "data": { "nodes": [] } }))
    }
    fn project_item_fields(
        &self,
        _repo: &str,
        _content_node_id: &str,
    ) -> Result<Vec<ProjectFieldValue>> {
        Ok(vec![])
    }
    fn update_project_v2_item_field_value(
        &self,
        _project_id: &str,
        _item_id: &str,
        _field_id: &str,
        _value: &GhFieldValueInput,
    ) -> Result<()> {
        unreachable!("no write-through on the fetch read path")
    }
    fn clear_project_field(
        &self,
        _project_id: &str,
        _item_id: &str,
        _field_id: &str,
    ) -> Result<()> {
        unreachable!("no write-through on the fetch read path")
    }
}

impl GhIssueDependencyApi for OneOpenIssueGh {
    fn list_blocked_by(&self, _repo: &str, _blocked_number: u64) -> Result<Vec<u64>> {
        Ok(vec![])
    }
    fn add_blocked_by(&self, _repo: &str, _blocked: u64, _blocking: u64) -> Result<()> {
        unreachable!("no dependency writes on the fetch read path")
    }
    fn remove_blocked_by(&self, _repo: &str, _blocked: u64, _blocking: u64) -> Result<()> {
        unreachable!("no dependency writes on the fetch read path")
    }
}

// AC1, DICTUM-006: `list` reports the board-derived status one hop removed -- an
// open issue caches at the board's first column (`ready to start`), not the
// github canonical `open`, and that is the status `list --json` prints.
#[test]
fn list_reports_a_board_derived_status_for_a_cached_doc() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".lazyspec.toml"), BOARD_BOUND_CONFIG).unwrap();
    let config = Config::parse(BOARD_BOUND_CONFIG).unwrap();
    let ticket = config.type_by_name("ticket").unwrap();

    let gh = OneOpenIssueGh;
    let mut issue_map = IssueMap::load(tmp.path()).unwrap();
    IssueCache::new(tmp.path())
        .fetch_all(
            tmp.path(),
            ticket,
            &gh,
            &gh,
            &gh,
            "octo-org/repo",
            &mut issue_map,
            &[TypeMatchRule::from(ticket)],
            &config,
        )
        .expect("fetch_all writes the cache");

    let store = Store::load(tmp.path(), &config).unwrap();
    let json = lazyspec::cli::list::run_json(&store, Some("ticket"), None);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let docs = parsed.as_array().unwrap();
    assert_eq!(docs.len(), 1, "got: {json}");
    assert_eq!(docs[0]["status"], "ready to start", "got: {json}");
}
