use anyhow::Result;
use lazyspec::engine::config::Config;
use lazyspec::engine::document::Status;
use lazyspec::engine::gh::{
    test_support, GhAuthor, GhComment, GhFieldKind, GhFieldValueInput, GhFieldValueRepr, GhGraphql,
    GhIssue, GhIssueDependencyApi, GhIssueReader, GhIssueWriter, GhLabel, GqlVar,
    ProjectFieldValue, ProjectItem,
};
use lazyspec::engine::gh_schema::{GhSchemaSnapshot, OptionId, ProjectFieldId};
use lazyspec::engine::issue_body::TypeMatchRule;
use lazyspec::engine::issue_cache::IssueCache;
use lazyspec::engine::issue_map::IssueMap;
use lazyspec::engine::store::Store;
use lazyspec::engine::store_dispatch::{DocumentStore, GithubIssuesStore};
use lazyspec::engine::sync::{sync_all, GhMaps, GhRound, SyncContext, SyncOutcome, Syncers};
use lazyspec::tui::state::App;
use std::cell::RefCell;
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

// --- ITERATION-353: the doc's status comes from the authority board's cell ---

/// The board-bound `ticket` type plus the membership relationship its docs use
/// to hold board edges, so a second (non-authority) board can be a member-of
/// target.
const MEMBERSHIP_CONFIG: &str = r#"[naming]
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
name = "member-of"
inverse = "has-member"
github_native = "membership"

[[relationships]]
name = "related-to"
"#;

/// A gh fake serving issues whose remote open/closed state, body and per-issue
/// board items the test chooses, recording every board write so the read path can
/// be proven not to issue one and the authority-board add can be counted.
struct BoardGh {
    issues: Vec<GhIssue>,
    /// Board items per issue content node id.
    items: std::collections::HashMap<String, Vec<ProjectItem>>,
    /// Make every project-item read fail, as a token without the `project` scope
    /// does.
    items_unreadable: bool,
    /// Fail every `addProjectV2ItemById`, as a token without board write access
    /// does.
    add_fails: bool,
    /// Answer every `addProjectV2ItemById` with HTTP 200 plus an `errors` array
    /// and no item payload, as GitHub does for a token without the `project`
    /// scope.
    add_returns_errors: bool,
    /// The node id the live board lookup resolves board 7 to. `None`: the board
    /// resolves under neither the org nor the user root.
    board_node_id: Option<&'static str>,
    mutations: RefCell<Vec<String>>,
    /// `(projectId, contentId)` of every `addProjectV2ItemById` issued.
    adds: RefCell<Vec<(String, String)>>,
    /// One entry per live board-node-id lookup (`projectV2(number:) { id }`),
    /// which is what makes an unmemoized per-doc resolve visible.
    board_lookups: RefCell<Vec<u64>>,
    /// `(projectId, itemId, fieldId, value)` of every project-field write, so the
    /// board `Status` cell write can be inspected key by key.
    field_updates: RefCell<Vec<(String, String, String, GhFieldValueInput)>>,
    /// Every call against the remote, whatever its shape -- what an offline
    /// rejection has to leave empty.
    remote_calls: RefCell<Vec<String>>,
    /// Every issue body pushed through `issue_edit`, so what the remote issue ends
    /// up claiming about the doc's status can be read back.
    edited_bodies: RefCell<Vec<String>>,
}

fn board_issue(number: u64, state: &str, body: &str) -> GhIssue {
    GhIssue {
        number,
        id: format!("I_issue{}", number),
        url: format!("https://github.com/octo-org/repo/issues/{}", number),
        title: format!("Board bound issue {}", number),
        body: body.to_string(),
        labels: vec![GhLabel {
            name: "lazyspec:ticket".to_string(),
            color: String::new(),
        }],
        state: state.to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        author: Some(GhAuthor {
            login: "octocat".to_string(),
        }),
        issue_type: None,
        milestone: None,
        assignees: vec![],
    }
}

impl BoardGh {
    fn new(state: &'static str, body: &'static str, items: Vec<ProjectItem>) -> Self {
        BoardGh {
            issues: vec![board_issue(42, state, body)],
            items: [("I_issue42".to_string(), items)].into_iter().collect(),
            items_unreadable: false,
            add_fails: false,
            add_returns_errors: false,
            board_node_id: None,
            mutations: RefCell::new(Vec::new()),
            adds: RefCell::new(Vec::new()),
            board_lookups: RefCell::new(Vec::new()),
            field_updates: RefCell::new(Vec::new()),
            remote_calls: RefCell::new(Vec::new()),
            edited_bodies: RefCell::new(Vec::new()),
        }
    }

    fn with_unreadable_items(state: &'static str, body: &'static str) -> Self {
        BoardGh {
            items_unreadable: true,
            ..BoardGh::new(state, body, Vec::new())
        }
    }

    /// Two open tickets of the type: issue 42 with `items_42` and issue 43 with
    /// `items_43`, so one can be an item of the authority board and the other not.
    fn with_two_tickets(items_42: Vec<ProjectItem>, items_43: Vec<ProjectItem>) -> Self {
        BoardGh {
            issues: vec![
                board_issue(42, "OPEN", "plain body"),
                board_issue(43, "OPEN", "plain body"),
            ],
            items: [
                ("I_issue42".to_string(), items_42),
                ("I_issue43".to_string(), items_43),
            ]
            .into_iter()
            .collect(),
            ..BoardGh::new("OPEN", "plain body", Vec::new())
        }
    }

    fn failing_adds(self) -> Self {
        BoardGh {
            add_fails: true,
            ..self
        }
    }

    /// Adds that answer with an `errors` array and no item payload -- a GitHub
    /// "success" the process exit cannot distinguish from a real one.
    fn adds_rejected_by_scope(self) -> Self {
        BoardGh {
            add_returns_errors: true,
            ..self
        }
    }

    /// No `PROJECT-7` binding in the issue map, so the board's node id has to be
    /// looked up live -- the shape of a board created in the GitHub UI.
    fn resolving_board_live(self, node_id: &'static str) -> Self {
        BoardGh {
            board_node_id: Some(node_id),
            ..self
        }
    }
}

fn int_var(vars: &[(&str, GqlVar)], key: &str) -> Option<u64> {
    vars.iter().find_map(|(k, v)| match v {
        GqlVar::Int(n) if *k == key => Some(*n as u64),
        _ => None,
    })
}

fn str_var<'a>(vars: &'a [(&str, GqlVar)], key: &str) -> Option<&'a str> {
    vars.iter().find_map(|(k, v)| match v {
        GqlVar::Str(s) if *k == key => Some(s.as_str()),
        _ => None,
    })
}

fn status_item(board: u64, status: &str) -> ProjectItem {
    ProjectItem {
        project_number: board,
        item_id: format!("PVTI_{}", board),
        fields: vec![ProjectFieldValue {
            project_number: board,
            field_name: "Status".to_string(),
            kind: GhFieldKind::SingleSelect,
            value: GhFieldValueRepr::OptionName(status.to_string()),
        }],
    }
}

/// `issue_list` panics: a fetch reads every type's issues off the composed
/// round now (RFC-065), so any REST list call is the regression this asserts
/// against. `issue_view` stays -- a mutation's read-back is correctly one read.
impl GhIssueReader for BoardGh {
    fn issue_list(
        &self,
        _repo: &str,
        _labels: &[String],
        _json_fields: &[String],
        _limit: Option<u64>,
    ) -> Result<Vec<GhIssue>> {
        unreachable!("a fetch reads issues off the composed round, never REST")
    }
    fn issue_view(&self, _repo: &str, number: u64) -> Result<GhIssue> {
        self.remote_calls
            .borrow_mut()
            .push(format!("issue_view:{number}"));
        self.issues
            .iter()
            .find(|i| i.number == number)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no such issue: {number}"))
    }
    fn issue_comments(&self, _repo: &str, _number: u64) -> Result<Vec<GhComment>> {
        Ok(vec![])
    }
}

/// The write half of the client seam, so the same fake can drive `update`. Only
/// the calls this story is about are recorded; the rest exist to satisfy the
/// trait and shout if the write path unexpectedly reaches them.
impl GhIssueWriter for BoardGh {
    fn issue_create(&self, _: &str, _: &str, _: &str, _: &[String]) -> Result<GhIssue> {
        unreachable!("the update path never creates an issue")
    }
    fn issue_edit(
        &self,
        _repo: &str,
        number: u64,
        _title: Option<&str>,
        body: Option<&str>,
        _labels_add: &[String],
        _labels_remove: &[String],
    ) -> Result<()> {
        self.remote_calls
            .borrow_mut()
            .push(format!("issue_edit:{number}"));
        self.edited_bodies
            .borrow_mut()
            .push(body.unwrap_or_default().to_string());
        Ok(())
    }
    fn issue_close(&self, _repo: &str, number: u64) -> Result<()> {
        self.remote_calls
            .borrow_mut()
            .push(format!("issue_close:{number}"));
        Ok(())
    }
    fn issue_reopen(&self, _repo: &str, number: u64) -> Result<()> {
        self.remote_calls
            .borrow_mut()
            .push(format!("issue_reopen:{number}"));
        Ok(())
    }
    fn issue_set_assignee(&self, _: &str, _: u64, _: &[String], _: &[String]) -> Result<()> {
        unreachable!("no assignee write in this story")
    }
    fn label_create(&self, _: &str, _: &str, _: &str, _: &str) -> Result<()> {
        Ok(())
    }
    fn label_ensure(&self, _: &str, _: &str, _: &str, _: &str) -> Result<()> {
        Ok(())
    }
}

impl GhGraphql for BoardGh {
    fn graphql(&self, query: &str, vars: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
        self.remote_calls.borrow_mut().push("graphql".to_string());
        if lazyspec::engine::gh_fetch::is_round_query(query) {
            let mut resp = test_support::with_issue_pages(
                query,
                test_support::round_response(&[], &[], &[]),
                &self.issues,
            );
            if self.items_unreadable {
                return Ok(test_support::without_project_items(
                    resp,
                    "your token has not been granted the required scopes: project",
                ));
            }
            for (node_id, items) in &self.items {
                resp = test_support::with_project_items_edge(
                    resp,
                    node_id,
                    test_support::project_items_edge(items),
                );
            }
            return Ok(resp);
        }
        if query.contains("mutation") {
            self.mutations.borrow_mut().push(query.to_string());
        }
        // The board-node-id resolve, distinguished from the schema snapshot's
        // field query by asking only for `id`.
        if query.contains("projectV2(number: $number) { id }") {
            self.board_lookups
                .borrow_mut()
                .push(int_var(vars, "number").unwrap_or_default());
            let root = if query.contains("organization") {
                "organization"
            } else {
                "user"
            };
            return Ok(match self.board_node_id {
                Some(id) => serde_json::json!({ "data": { root: { "projectV2": { "id": id } } } }),
                None => serde_json::json!({ "data": { root: serde_json::Value::Null } }),
            });
        }
        if query.contains("addProjectV2ItemById") {
            if self.add_fails {
                anyhow::bail!("Resource not accessible by integration");
            }
            if self.add_returns_errors {
                return Ok(serde_json::json!({
                    "data": { "addProjectV2ItemById": serde_json::Value::Null },
                    "errors": [{
                        "type": "INSUFFICIENT_SCOPES",
                        "message": "Your token has not been granted the required scopes to execute this query. The 'id' field requires one of the following scopes: ['project']."
                    }]
                }));
            }
            self.adds.borrow_mut().push((
                str_var(vars, "projectId").unwrap_or_default().to_string(),
                str_var(vars, "contentId").unwrap_or_default().to_string(),
            ));
            return Ok(serde_json::json!(
                { "data": { "addProjectV2ItemById": { "item": { "id": "PVTI_new" } } } }
            ));
        }
        Ok(serde_json::json!({ "data": { "nodes": [] } }))
    }
    /// The write path's read-back only: a fetch takes board memberships off the
    /// composed round, so a call here during a fetch is the regression.
    fn project_items(&self, _repo: &str, content_node_id: &str) -> Result<Vec<ProjectItem>> {
        self.remote_calls
            .borrow_mut()
            .push("project_items".to_string());
        if self.items_unreadable {
            anyhow::bail!("your token has not been granted the required scopes: project");
        }
        Ok(self.items.get(content_node_id).cloned().unwrap_or_default())
    }
    fn update_project_v2_item_field_value(
        &self,
        project_id: &str,
        item_id: &str,
        field_id: &str,
        value: &GhFieldValueInput,
    ) -> Result<()> {
        self.remote_calls
            .borrow_mut()
            .push("update_project_field".to_string());
        self.mutations
            .borrow_mut()
            .push(format!("update:{}:{}", item_id, field_id));
        self.field_updates.borrow_mut().push((
            project_id.to_string(),
            item_id.to_string(),
            field_id.to_string(),
            value.clone(),
        ));
        Ok(())
    }
    fn clear_project_field(&self, _project_id: &str, item_id: &str, field_id: &str) -> Result<()> {
        self.mutations
            .borrow_mut()
            .push(format!("clear:{}:{}", item_id, field_id));
        Ok(())
    }
}

impl GhIssueDependencyApi for BoardGh {
    fn add_blocked_by(&self, _repo: &str, _blocked: u64, _blocking: u64) -> Result<()> {
        unreachable!("no dependency writes on the fetch read path")
    }
    fn remove_blocked_by(&self, _repo: &str, _blocked: u64, _blocking: u64) -> Result<()> {
        unreachable!("no dependency writes on the fetch read path")
    }
}

/// Drive the real fetch pipeline for the `ticket` type: `fetch_all` followed by
/// the project-field injection pass, exactly as `lazyspec fetch` does.
fn sync_tickets(tmp: &TempDir, config: &Config, gh: &BoardGh) -> Vec<SyncOutcome> {
    let mut issue_map = IssueMap::load(tmp.path()).unwrap();
    let mut ctx = SyncContext {
        gh: Some(GhMaps {
            issue_map: &mut issue_map,
        }),
        clickup: None,
        fetch: None,
    };
    let round = GhRound {
        gh,
        repo: "octo-org/repo".to_string(),
    };
    let mut syncers = Syncers {
        issue: Some(
            round.issue_sync(
                config
                    .documents
                    .types
                    .iter()
                    .map(TypeMatchRule::from)
                    .collect(),
            ),
        ),
        round: Some(round),
        ..Default::default()
    };
    sync_all(tmp.path(), config, &mut ctx, &mut syncers, None)
}

fn cached_doc(tmp: &TempDir, config: &Config, id: &str) -> lazyspec::engine::document::DocMeta {
    let store = Store::load(tmp.path(), config).unwrap();
    store
        .all_docs()
        .into_iter()
        .find(|d| d.id == id)
        .unwrap_or_else(|| panic!("the fetched ticket {id} is cached"))
        .clone()
}

fn cached_ticket(tmp: &TempDir, config: &Config) -> lazyspec::engine::document::DocMeta {
    cached_doc(tmp, config, "TICKET-42")
}

/// The authority-status warnings only. The offline fake cannot resolve a repo
/// owner, so every fetch also carries a schema-snapshot warning unrelated to the
/// Status cell.
fn status_warnings(warnings: &[String]) -> Vec<&String> {
    warnings
        .iter()
        .filter(|w| w.contains("authority board"))
        .collect()
}

fn membership_project(config_src: &str) -> (TempDir, Config) {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".lazyspec.toml"), config_src).unwrap();
    let config = Config::parse(config_src).unwrap();
    (tmp, config)
}

/// Bind `PROJECT-{number}` to a board node id in the issue map, the shape a prior
/// board create or membership sync leaves behind, so fetch resolves the board's
/// project id with no live lookup.
fn bind_authority_board(tmp: &TempDir, number: u64, node_id: &str) {
    let mut issue_map = IssueMap::load(tmp.path()).unwrap();
    issue_map.insert_kind(
        format!("PROJECT-{}", number),
        number,
        "",
        node_id,
        lazyspec::engine::issue_map::EntryKind::Project,
    );
    issue_map.save(tmp.path()).unwrap();
}

/// Seed the cache with the ticket the fake serves, sitting at `status` -- the
/// state a previous successful fetch would have left behind.
fn seed_cached_ticket(tmp: &TempDir, status: &str) {
    let cache = tmp.path().join(".lazyspec/cache/ticket");
    fs::create_dir_all(&cache).unwrap();
    fs::write(
        cache.join("TICKET-42.md"),
        format!("---\ntitle: \"Board bound issue\"\ntype: ticket\nstatus: {status}\nauthor: \"@octocat\"\ndate: 2026-01-01\ntags: []\n---\nplain body\n"),
    )
    .unwrap();
}

// AC2, the drift this story exists to remove: the issue is CLOSED on GitHub but
// its authority-board cell says `In Progress`, and the cell wins. Nothing about
// the doc's status is derived from open/closed.
#[test]
fn a_closed_issue_takes_its_status_from_the_authority_board_cell_not_open_closed() {
    let (tmp, config) = membership_project(MEMBERSHIP_CONFIG);
    let gh = BoardGh::new("CLOSED", "plain body", vec![status_item(7, "In Progress")]);

    let outcomes = sync_tickets(&tmp, &config, &gh);

    assert_eq!(
        cached_ticket(&tmp, &config).status,
        Status::new("in progress")
    );
    assert!(
        status_warnings(&outcomes[0].warnings).is_empty(),
        "got: {:?}",
        outcomes[0]
    );
    assert!(outcomes[0].error.is_none(), "got: {:?}", outcomes[0]);
}

// AC3: an item on the authority board with an EMPTY Status cell leaves the doc's
// status unset, writes nothing back to the board, and warns naming the doc.
#[test]
fn an_empty_authority_status_cell_leaves_the_status_unset_and_warns() {
    let (tmp, config) = membership_project(MEMBERSHIP_CONFIG);
    let gh = BoardGh::new(
        "OPEN",
        "plain body",
        vec![ProjectItem {
            project_number: 7,
            item_id: "PVTI_7".to_string(),
            fields: vec![],
        }],
    );

    let outcomes = sync_tickets(&tmp, &config, &gh);

    assert_eq!(cached_ticket(&tmp, &config).status, Status::new(""));
    let status_warnings = status_warnings(&outcomes[0].warnings);
    assert_eq!(status_warnings.len(), 1, "got: {:?}", outcomes[0].warnings);
    assert_eq!(
        status_warnings[0],
        "TICKET-42 has an empty Status cell on authority board PROJECT-7; status left unset"
    );
    // ITERATION-354: a blank cell is already-a-member, so no value is written to
    // it AND no add is issued -- adding it again would be non-idempotent.
    assert!(
        gh.mutations.borrow().is_empty(),
        "the read path must not write to the board, got: {:?}",
        gh.mutations.borrow()
    );
    assert!(gh.adds.borrow().is_empty());
}

// AC4 (was ITERATION-353's "not an item warns distinctly"): a doc absent from the
// authority board is now put ON it, and the freshly added item's empty Status cell
// leaves the status unset -- a distinct message from an existing empty cell,
// because the fix differs.
#[test]
fn a_doc_absent_from_the_authority_board_is_added_and_warns_distinctly() {
    let (tmp, config) = membership_project(MEMBERSHIP_CONFIG);
    bind_authority_board(&tmp, 7, "PVT_board7");
    let gh = BoardGh::new("OPEN", "plain body", vec![status_item(9, "Triage")]);

    let outcomes = sync_tickets(&tmp, &config, &gh);

    assert_eq!(cached_ticket(&tmp, &config).status, Status::new(""));
    let status_warnings = status_warnings(&outcomes[0].warnings);
    assert_eq!(status_warnings.len(), 1, "got: {:?}", outcomes[0].warnings);
    assert_eq!(
        status_warnings[0],
        "TICKET-42 was added to authority board PROJECT-7 with an empty Status cell; status left unset"
    );
    assert_eq!(
        gh.adds.borrow().as_slice(),
        [("PVT_board7".to_string(), "I_issue42".to_string())]
    );
}

// A token without the `project` scope cannot read the board at all. That warns
// and exits zero, so a fetch must not take the chance to blank every board-bound
// doc's status: each keeps what it last read off the board.
#[test]
fn an_unreadable_board_keeps_the_last_known_status_and_warns() {
    let (tmp, config) = membership_project(MEMBERSHIP_CONFIG);
    seed_cached_ticket(&tmp, "in progress");
    let gh = BoardGh::with_unreadable_items("OPEN", "plain body");

    let outcomes = sync_tickets(&tmp, &config, &gh);

    assert_eq!(
        cached_ticket(&tmp, &config).status,
        Status::new("in progress")
    );
    // The memberships ride the round, so the warning is the round's -- one for
    // the whole read, naming the scope that withheld it.
    assert!(
        outcomes[0]
            .warnings
            .iter()
            .any(|w| w.contains("could not read project fields")
                && w.contains("gh auth refresh -s project")),
        "got: {:?}",
        outcomes[0].warnings
    );
    assert!(outcomes[0].error.is_none(), "got: {:?}", outcomes[0]);
}

// AC11: the same warning reaches `fetch --json`, alongside the per-type counts.
#[test]
fn fetch_json_reports_the_unset_status_cell_warning() {
    let (tmp, config) = membership_project(MEMBERSHIP_CONFIG);
    let gh = BoardGh::new(
        "OPEN",
        "plain body",
        vec![ProjectItem {
            project_number: 7,
            item_id: "PVTI_7".to_string(),
            fields: vec![],
        }],
    );

    let outcomes = sync_tickets(&tmp, &config, &gh);
    let json = lazyspec::cli::fetch::outcomes_json(&outcomes).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let entry = &parsed.as_array().unwrap()[0];
    assert_eq!(entry["type"], "ticket", "got: {json}");
    let warnings: Vec<&str> = entry["warnings"]
        .as_array()
        .expect("warnings array")
        .iter()
        .map(|w| w.as_str().unwrap())
        .filter(|w| w.contains("authority board"))
        .collect();
    assert_eq!(warnings.len(), 1, "got: {json}");
    assert!(warnings[0].contains("TICKET-42"), "got: {json}");
    assert!(warnings[0].contains("PROJECT-7"), "got: {json}");
}

// AC9: a doc on the authority board AND a second board that also has a `Status`
// field -- only board 7 drives the lifecycle; board 9's stays a plain attribute.
#[test]
fn a_second_boards_status_stays_a_plain_attribute() {
    let (tmp, config) = membership_project(MEMBERSHIP_CONFIG);
    let body = "<!-- lazyspec\n---\ndate: 2026-01-01\nrelated:\n- member-of: PROJECT-7\n- member-of: PROJECT-9\n---\n-->\n\nplain body";
    let gh = BoardGh::new(
        "OPEN",
        body,
        vec![status_item(7, "Review"), status_item(9, "Triage")],
    );

    sync_tickets(&tmp, &config, &gh);

    let doc = cached_ticket(&tmp, &config);
    assert_eq!(doc.status, Status::new("review"));
    let board_9 = doc
        .attributes
        .get("PROJECT-9.Status")
        .expect("board 9's Status survives as a plain attribute");
    assert_eq!(
        serde_json::to_value(board_9).unwrap(),
        serde_json::json!("Triage"),
        "got: {:?}",
        doc.attributes
    );
}

// AC1/AC2, DICTUM-006: `list` reports the board-derived status one hop removed
// -- the doc caches at the board column its `Status` cell names (`ready to
// start`), not the github canonical `open`, and that is what `list --json`
// prints.
#[test]
fn list_reports_a_board_derived_status_for_a_cached_doc() {
    let (tmp, config) = membership_project(MEMBERSHIP_CONFIG);
    let gh = BoardGh::new("OPEN", "plain body", vec![status_item(7, "Ready To Start")]);

    sync_tickets(&tmp, &config, &gh);

    let store = Store::load(tmp.path(), &config).unwrap();
    let json = lazyspec::cli::list::run_json(&store, Some("ticket"), None);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    let docs = parsed.as_array().unwrap();
    assert_eq!(docs.len(), 1, "got: {json}");
    assert_eq!(docs[0]["status"], "ready to start", "got: {json}");
}

// --- ITERATION-354: fetch puts non-members on the authority board ---

// AC4: two docs of the type, one already an item of board 7 and one not. Fetch
// issues exactly ONE `addProjectV2ItemById`, carrying the non-member's content
// node id and the board's project node id, and that doc's status stays unset with
// a warning naming it -- the added item's Status cell is empty.
#[test]
fn fetch_adds_only_the_doc_that_is_not_an_item_of_the_authority_board() {
    let (tmp, config) = membership_project(MEMBERSHIP_CONFIG);
    bind_authority_board(&tmp, 7, "PVT_board7");
    let gh = BoardGh::with_two_tickets(vec![status_item(7, "Review")], vec![]);

    let outcomes = sync_tickets(&tmp, &config, &gh);

    assert_eq!(
        gh.adds.borrow().as_slice(),
        [("PVT_board7".to_string(), "I_issue43".to_string())],
        "exactly one add, for the non-member"
    );
    assert_eq!(
        cached_doc(&tmp, &config, "TICKET-43").status,
        Status::new("")
    );
    let status_warnings = status_warnings(&outcomes[0].warnings);
    assert_eq!(status_warnings.len(), 1, "got: {:?}", outcomes[0].warnings);
    assert_eq!(
        status_warnings[0],
        "TICKET-43 was added to authority board PROJECT-7 with an empty Status cell; status left unset"
    );
    assert!(outcomes[0].error.is_none(), "got: {:?}", outcomes[0]);
}

// AC4: the point of the add -- after it, the type reports one lifecycle. Neither
// the member nor the freshly added doc ever falls back to `open`/`closed`.
#[test]
fn no_doc_reports_open_or_closed_after_the_authority_board_add() {
    let (tmp, config) = membership_project(MEMBERSHIP_CONFIG);
    bind_authority_board(&tmp, 7, "PVT_board7");
    let gh = BoardGh::with_two_tickets(vec![status_item(7, "Review")], vec![]);

    sync_tickets(&tmp, &config, &gh);

    assert_eq!(gh.adds.borrow().len(), 1, "the non-member was added");
    assert_eq!(
        cached_doc(&tmp, &config, "TICKET-42").status,
        Status::new("review")
    );
    assert_eq!(
        cached_doc(&tmp, &config, "TICKET-43").status,
        Status::new("")
    );
    let store = Store::load(tmp.path(), &config).unwrap();
    for doc in store.all_docs() {
        assert!(
            doc.status != Status::new("open") && doc.status != Status::new("closed"),
            "{} fell back to the canonical github lifecycle: {:?}",
            doc.id,
            doc.status
        );
    }
}

// AC4 idempotence: the next fetch sees both docs as items, so it issues no add at
// all. Membership is repaired once, not re-asserted every fetch.
#[test]
fn a_second_fetch_with_both_docs_on_the_board_issues_no_add() {
    let (tmp, config) = membership_project(MEMBERSHIP_CONFIG);
    bind_authority_board(&tmp, 7, "PVT_board7");
    let first = BoardGh::with_two_tickets(vec![status_item(7, "Review")], vec![]);
    sync_tickets(&tmp, &config, &first);
    assert_eq!(first.adds.borrow().len(), 1);

    // The board now carries both items, and someone has moved the freshly added
    // one into `Review`. Reading that column back proves membership was genuinely
    // re-read on the second pass, so "no add" means idempotent rather than skipped.
    let second = BoardGh::with_two_tickets(
        vec![status_item(7, "Review")],
        vec![status_item(7, "Review")],
    );
    sync_tickets(&tmp, &config, &second);

    assert!(
        second.adds.borrow().is_empty(),
        "got: {:?}",
        second.adds.borrow()
    );
    assert_eq!(
        cached_doc(&tmp, &config, "TICKET-43").status,
        Status::new("review"),
        "the second pass must re-read the board, not skip it"
    );
}

// AC4 cost: the authority board's node id is resolved ONCE per fetch, however
// many docs have to be added to it. A board created in the GitHub UI has no
// `PROJECT-n` binding to resolve offline, so without memoization a type with N
// non-members costs N live lookups.
#[test]
fn one_fetch_resolves_the_authority_board_once_for_every_non_member() {
    let (tmp, config) = membership_project(MEMBERSHIP_CONFIG);
    let gh = BoardGh::with_two_tickets(vec![], vec![]).resolving_board_live("PVT_live7");

    let outcomes = sync_tickets(&tmp, &config, &gh);

    assert_eq!(
        gh.board_lookups.borrow().as_slice(),
        [7],
        "the board must be resolved once for the whole pass"
    );
    assert_eq!(
        gh.adds.borrow().as_slice(),
        [
            ("PVT_live7".to_string(), "I_issue42".to_string()),
            ("PVT_live7".to_string(), "I_issue43".to_string())
        ],
        "both non-members are still added"
    );
    assert!(outcomes[0].error.is_none(), "got: {:?}", outcomes[0]);
}

// A token without the `project` scope makes GitHub answer the add with HTTP 200
// plus an `errors` array, which `gh` exits zero on. That is a failed add, so it
// must warn as one -- reporting a repair that did not happen would leave the doc
// silently off the board.
#[test]
fn an_add_answered_with_an_errors_array_warns_that_the_add_failed() {
    let (tmp, config) = membership_project(MEMBERSHIP_CONFIG);
    bind_authority_board(&tmp, 7, "PVT_board7");
    let gh = BoardGh::new("OPEN", "plain body", vec![]).adds_rejected_by_scope();

    let outcomes = sync_tickets(&tmp, &config, &gh);

    let status_warnings = status_warnings(&outcomes[0].warnings);
    assert_eq!(status_warnings.len(), 1, "got: {:?}", outcomes[0].warnings);
    assert!(
        status_warnings[0].starts_with("could not add TICKET-42 to authority board PROJECT-7"),
        "got: {}",
        status_warnings[0]
    );
    assert!(
        status_warnings[0].contains("gh auth refresh -s project"),
        "the actionable scope hint must survive, got: {}",
        status_warnings[0]
    );
    assert!(outcomes[0].error.is_none(), "got: {:?}", outcomes[0]);
}

// A failed add says nothing about the doc's lifecycle, so it must not blank the
// status the board last reported -- exactly as a failed board READ preserves it.
#[test]
fn a_failed_authority_board_add_keeps_the_last_known_status() {
    let (tmp, config) = membership_project(MEMBERSHIP_CONFIG);
    bind_authority_board(&tmp, 7, "PVT_board7");
    seed_cached_ticket(&tmp, "in progress");
    let gh = BoardGh::new("OPEN", "plain body", vec![]).failing_adds();

    let outcomes = sync_tickets(&tmp, &config, &gh);

    assert_eq!(
        cached_ticket(&tmp, &config).status,
        Status::new("in progress"),
        "a failed add must not wipe a previously good board status"
    );
    assert!(
        status_warnings(&outcomes[0].warnings)[0]
            .starts_with("could not add TICKET-42 to authority board PROJECT-7"),
        "got: {:?}",
        outcomes[0].warnings
    );
}

// --- ITERATION-355: update --status moves the card on the authority board ---

const BOARD_COLUMNS: [&str; 4] = ["Ready To Start", "In Progress", "Review", "Done"];

/// A doc body carrying the lazyspec comment, which is what the update path parses
/// out of the remote issue.
const LAZYSPEC_BODY: &str =
    "<!-- lazyspec\n---\nauthor: \"@octocat\"\ndate: 2026-01-01\n---\n-->\n\nplain body";

/// Board 7's `Status` columns as `fetch` caches them: the offline source of the
/// field id and the option ids the write path uses.
fn write_board_snapshot(tmp: &TempDir) {
    GhSchemaSnapshot {
        project_fields: vec![ProjectFieldId {
            project_number: 7,
            field_name: "Status".to_string(),
            id: "F_status7".to_string(),
            data_type: "SINGLE_SELECT".to_string(),
        }],
        single_select_options: BOARD_COLUMNS
            .iter()
            .map(|name| OptionId {
                field_id: "F_status7".to_string(),
                name: (*name).to_string(),
                id: format!("opt_{}", name.to_lowercase().replace(' ', "_")),
            })
            .collect(),
        ..Default::default()
    }
    .save(tmp.path())
    .unwrap();
}

/// A board-bound project ready for a status write: the config, board 7's columns
/// cached, TICKET-42 cached at `status`, and the issue plus the board bound in the
/// issue map so nothing has to be looked up live.
fn writable_board_project(status: &str) -> (TempDir, Config) {
    let (tmp, config) = membership_project(MEMBERSHIP_CONFIG);
    write_board_snapshot(&tmp);
    seed_cached_ticket(&tmp, status);
    bind_authority_board(&tmp, 7, "PVT_board7");
    let mut issue_map = IssueMap::load(tmp.path()).unwrap();
    issue_map.insert("TICKET-42", 42, "2026-01-01T00:00:00Z", "I_issue42");
    issue_map.save(tmp.path()).unwrap();
    (tmp, config)
}

fn board_store(tmp: &TempDir, config: &Config, gh: BoardGh) -> GithubIssuesStore {
    GithubIssuesStore {
        client: Box::new(gh),
        root: tmp.path().to_path_buf(),
        repo: "octo-org/repo".to_string(),
        config: config.clone(),
        issue_map: IssueMap::load(tmp.path()).unwrap(),
        issue_cache: IssueCache::new(tmp.path()),
    }
}

/// The fake back out of the store, to read what the write path did to it.
fn store_gh(store: &GithubIssuesStore) -> &BoardGh {
    (*store.client)
        .as_any()
        .downcast_ref::<BoardGh>()
        .expect("the store holds the BoardGh fake")
}

fn board_gh_at(column: &str, issue_state: &'static str) -> BoardGh {
    BoardGh {
        issues: vec![board_issue(42, issue_state, LAZYSPEC_BODY)],
        ..BoardGh::new(issue_state, "plain body", vec![status_item(7, column)])
    }
}

// AC5: the requested column becomes one `updateProjectV2ItemFieldValue` against
// the authority board's `Status` field, with a value object carrying exactly
// `singleSelectOptionId`; the cached status becomes the column lowercased. The
// display-cased and lowercased spellings are one column, so both resolve the same
// option id.
#[test]
fn update_status_moves_the_card_on_the_authority_board() {
    for requested in ["In Progress", "in progress"] {
        let (tmp, config) = writable_board_project("ready to start");
        let mut store = board_store(&tmp, &config, board_gh_at("Ready To Start", "OPEN"));
        let ticket = config.type_by_name("ticket").unwrap();

        store
            .update(ticket, "TICKET-42", &[("status", requested)])
            .unwrap();

        let gh = store_gh(&store);
        let updates = gh.field_updates.borrow();
        assert_eq!(updates.len(), 1, "one board write, got: {updates:?}");
        let (project_id, item_id, field_id, value) = &updates[0];
        assert_eq!(project_id, "PVT_board7");
        assert_eq!(item_id, "PVTI_7");
        assert_eq!(field_id, "F_status7");
        assert_eq!(
            serde_json::to_value(value).unwrap(),
            serde_json::json!({"singleSelectOptionId": "opt_in_progress"}),
            "exactly one key in the value object"
        );
        assert_eq!(
            cached_ticket(&tmp, &config).status,
            Status::new("in progress")
        );
    }
}

// AC7: the last column in board order is the lifecycle's terminal status, so the
// ordinary write-through would close the issue on arrival. A board-bound type must
// not: the coupling is the board's own Projects automation to express.
#[test]
fn a_move_to_the_last_column_never_closes_the_issue() {
    let (tmp, config) = writable_board_project("review");
    let mut store = board_store(&tmp, &config, board_gh_at("Review", "OPEN"));
    let ticket = config.type_by_name("ticket").unwrap();

    store
        .update(ticket, "TICKET-42", &[("status", "Done")])
        .unwrap();

    let gh = store_gh(&store);
    assert!(
        !gh.remote_calls
            .borrow()
            .iter()
            .any(|c| c.starts_with("issue_close")),
        "got: {:?}",
        gh.remote_calls.borrow()
    );
    assert_eq!(cached_ticket(&tmp, &config).status, Status::new("done"));
}

// AC7, the other direction: leaving the last column does not reopen a closed
// issue. The board and the issue's open/closed state stay independent both ways.
#[test]
fn a_move_out_of_the_last_column_never_reopens_the_issue() {
    let (tmp, config) = writable_board_project("done");
    let mut store = board_store(&tmp, &config, board_gh_at("Done", "CLOSED"));
    let ticket = config.type_by_name("ticket").unwrap();

    store
        .update(ticket, "TICKET-42", &[("status", "Review")])
        .unwrap();

    let gh = store_gh(&store);
    assert!(
        !gh.remote_calls
            .borrow()
            .iter()
            .any(|c| c.starts_with("issue_reopen")),
        "got: {:?}",
        gh.remote_calls.borrow()
    );
    assert_eq!(cached_ticket(&tmp, &config).status, Status::new("review"));
}

// AC6: a value naming no column on the authority board is rejected by the update
// op itself, which runs before any store -- and so any client -- is built, naming
// the valid columns. Nothing in this test can reach the network: no fake, no
// client, no `gh` invocation, because the bail precedes their construction.
#[test]
fn update_status_rejects_a_value_the_authority_board_has_no_column_for() {
    let (tmp, config) = writable_board_project("ready to start");
    let store = Store::load(tmp.path(), &config).unwrap();

    let err = lazyspec::engine::ops::update::run_with_config(
        tmp.path(),
        &store,
        "TICKET-42",
        &[("status", "Blocked")],
        Some(&config),
    )
    .unwrap_err()
    .to_string();

    assert!(err.contains("Blocked"), "got: {err}");
    assert!(err.contains("ticket"), "got: {err}");
    assert!(err.contains("PROJECT-7"), "got: {err}");
    for column in BOARD_COLUMNS {
        assert!(err.contains(column), "got: {err}");
    }
    assert_eq!(
        cached_ticket(&tmp, &config).status,
        Status::new("ready to start"),
        "a rejected move leaves the doc where it was"
    );
}

// AC6 as the store sees it: the same rejection with a client in hand makes ZERO
// calls against the remote -- not the board mutation, not even the pre-flight
// issue view the update path opens with.
#[test]
fn a_rejected_status_makes_no_remote_call_at_all() {
    let (tmp, config) = writable_board_project("ready to start");
    let mut store = board_store(&tmp, &config, board_gh_at("Ready To Start", "OPEN"));
    let ticket = config.type_by_name("ticket").unwrap();

    let err = store
        .update(ticket, "TICKET-42", &[("status", "Blocked")])
        .unwrap_err()
        .to_string();

    let gh = store_gh(&store);
    assert!(err.contains("Blocked"), "got: {err}");
    assert!(
        gh.remote_calls.borrow().is_empty(),
        "got: {:?}",
        gh.remote_calls.borrow()
    );
}

// The TUI needs no per-surface work here, and this is why: the picker offers the
// states of `effective_lifecycle` (the board's columns) and applies them through
// the same `ops::update`, so every state it can offer resolves to a column of the
// authority board. The picker cannot produce a value the offline gate rejects.
#[test]
fn every_status_the_tui_picker_offers_resolves_to_a_board_column() {
    let (tmp, config) = writable_board_project("ready to start");
    let mut app = board_bound_app(&tmp, &config);
    app.selected_type = 0;
    app.selected_doc = 0;
    app.open_status_picker(&config);

    let snapshot = GhSchemaSnapshot::load(tmp.path());
    assert!(!app.status_picker.states.is_empty());
    for state in &app.status_picker.states {
        assert!(
            snapshot.status_option(7, state).is_some(),
            "the picker offers \"{state}\", which board 7 has no column for"
        );
    }
}

// AC11: the move reaches `update --json`, which reports the doc as it stands after
// the write.
#[test]
fn update_json_reports_the_board_column_the_doc_moved_to() {
    let (tmp, config) = writable_board_project("ready to start");
    let mut store = board_store(&tmp, &config, board_gh_at("Ready To Start", "OPEN"));
    let ticket = config.type_by_name("ticket").unwrap();

    let outcome = store
        .update(ticket, "TICKET-42", &[("status", "In Progress")])
        .unwrap();

    let doc = cached_ticket(&tmp, &config);
    let mut json = lazyspec::cli::json::doc_to_json(&doc);
    lazyspec::cli::json::merge_push_outcome(&mut json, &outcome);

    assert_eq!(json["id"], "TICKET-42");
    assert_eq!(json["status"], "in progress");
    assert_eq!(json["synced"], true);
}

// AC4: the add is best-effort. It fails, the run still reports no error, the
// warning names the doc and the board, and the other doc still processes.
#[test]
fn a_failed_authority_board_add_warns_and_the_fetch_carries_on() {
    let (tmp, config) = membership_project(MEMBERSHIP_CONFIG);
    bind_authority_board(&tmp, 7, "PVT_board7");
    let gh = BoardGh::with_two_tickets(vec![status_item(7, "Review")], vec![]).failing_adds();

    let outcomes = sync_tickets(&tmp, &config, &gh);

    let status_warnings = status_warnings(&outcomes[0].warnings);
    assert_eq!(status_warnings.len(), 1, "got: {:?}", outcomes[0].warnings);
    assert!(
        status_warnings[0].starts_with("could not add TICKET-43 to authority board PROJECT-7"),
        "got: {}",
        status_warnings[0]
    );
    assert!(outcomes[0].error.is_none(), "got: {:?}", outcomes[0]);
    assert_eq!(
        cached_doc(&tmp, &config, "TICKET-42").status,
        Status::new("review")
    );
    assert_eq!(
        cached_doc(&tmp, &config, "TICKET-43").status,
        Status::new("")
    );
}

// A board-bound doc's status is the board's alone, on the write path too: an
// update carrying no status of its own must leave the doc where the cache (the
// board's last word) holds it. The hazard is the issue's open/closed bit -- the
// issue here is CLOSED while the board has the card in `In Progress`, so the
// open/closed fallback would write the terminal status `done` into the cache AND
// into the body pushed back to the issue.
#[test]
fn a_non_status_update_leaves_a_board_bound_doc_at_its_board_status() {
    let (tmp, config) = writable_board_project("in progress");
    let mut store = board_store(&tmp, &config, board_gh_at("In Progress", "CLOSED"));
    let ticket = config.type_by_name("ticket").unwrap();

    store
        .update(ticket, "TICKET-42", &[("title", "Renamed")])
        .unwrap();

    assert_eq!(
        cached_ticket(&tmp, &config).status,
        Status::new("in progress")
    );
    let gh = store_gh(&store);
    let bodies = gh.edited_bodies.borrow();
    assert_eq!(bodies.len(), 1, "one body push, got: {bodies:?}");
    assert!(
        bodies[0].contains("status: in progress"),
        "the pushed body must carry the board's status, got: {}",
        bodies[0]
    );
    assert!(
        !bodies[0].contains("done"),
        "the pushed body must not claim the terminal status, got: {}",
        bodies[0]
    );
    assert!(
        gh.field_updates.borrow().is_empty(),
        "no status change means no board write, got: {:?}",
        gh.field_updates.borrow()
    );
}

/// The same board-bound ticket, but its issue is an item of board 9 only -- so it
/// has no `Status` cell on the authority board to write.
fn board_gh_off_the_authority_board() -> BoardGh {
    BoardGh {
        issues: vec![board_issue(42, "OPEN", LAZYSPEC_BODY)],
        ..BoardGh::new("OPEN", "plain body", vec![status_item(9, "Triage")])
    }
}

// A doc that is not an item of the authority board has no cell to move, and the
// rejection must cost nothing remotely: pushing the issue body first would leave
// the remote carrying a `status:` line for a card that never moved, with no local
// cache write to match it.
#[test]
fn a_status_move_on_a_non_member_rejects_before_the_body_is_pushed() {
    let (tmp, config) = writable_board_project("ready to start");
    let mut store = board_store(&tmp, &config, board_gh_off_the_authority_board());
    let ticket = config.type_by_name("ticket").unwrap();

    let err = store
        .update(ticket, "TICKET-42", &[("status", "Review")])
        .unwrap_err()
        .to_string();

    assert!(err.contains("not an item"), "got: {err}");
    assert!(err.contains("PROJECT-7"), "got: {err}");
    assert!(err.contains("lazyspec fetch"), "got: {err}");
    let gh = store_gh(&store);
    assert!(
        gh.edited_bodies.borrow().is_empty(),
        "the remote body must not be rewritten, got: {:?}",
        gh.edited_bodies.borrow()
    );
    assert!(
        !gh.remote_calls
            .borrow()
            .iter()
            .any(|c| c.starts_with("issue_edit")),
        "got: {:?}",
        gh.remote_calls.borrow()
    );
    assert!(gh.field_updates.borrow().is_empty());
    assert_eq!(
        cached_ticket(&tmp, &config).status,
        Status::new("ready to start")
    );
}

/// A `status_authority` on a type whose store has no github issue to be a board
/// item: nothing of this type could ever reach a Projects v2 board.
const FILESYSTEM_AUTHORITY_CONFIG: &str = r#"[naming]
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
store = "filesystem"
status_authority = "PROJECT-7"

[[relationships]]
name = "related-to"
"#;

/// A `status_authority` naming no board number, which silently behaves as no
/// authority at all.
const NOT_A_BOARD_AUTHORITY_CONFIG: &str = r#"[naming]
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
status_authority = "PROJECT-seven"

[[relationships]]
name = "related-to"
"#;

/// The `errors` array `validate --json` reports for `config_src`.
fn validate_json_errors(config_src: &str) -> Vec<String> {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".lazyspec.toml"), config_src).unwrap();
    let config = Config::parse(config_src).unwrap();
    let store = Store::load(tmp.path(), &config).unwrap();

    let json = lazyspec::cli::validate::run_json(&store, &config, &[]);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    parsed["errors"]
        .as_array()
        .expect("errors array")
        .iter()
        .map(|e| e.as_str().unwrap().to_string())
        .collect()
}

// DICTUM-006: an unsatisfiable `status_authority` reaches `--json` consumers.
#[test]
fn validate_json_reports_status_authority_on_a_type_that_is_not_github_issues() {
    let errors = validate_json_errors(FILESYSTEM_AUTHORITY_CONFIG);

    assert_eq!(errors.len(), 1, "got: {errors:?}");
    assert!(errors[0].contains("status_authority"), "got: {}", errors[0]);
    assert!(errors[0].contains("ticket"), "got: {}", errors[0]);
    assert!(errors[0].contains("PROJECT-7"), "got: {}", errors[0]);
    assert!(errors[0].contains("filesystem"), "got: {}", errors[0]);
}

#[test]
fn validate_json_reports_a_status_authority_that_names_no_board() {
    let errors = validate_json_errors(NOT_A_BOARD_AUTHORITY_CONFIG);

    assert_eq!(errors.len(), 1, "got: {errors:?}");
    assert!(errors[0].contains("status_authority"), "got: {}", errors[0]);
    assert!(errors[0].contains("ticket"), "got: {}", errors[0]);
    assert!(errors[0].contains("PROJECT-seven"), "got: {}", errors[0]);
}
