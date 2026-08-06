use anyhow::{bail, Result};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::process::Command;
use std::time::Duration;

use crate::engine::document::AttrValue;

// Wall-clock cap for a single `gh` invocation: every call is a network round-trip
// that must not hang the caller indefinitely (BUG-001).
const GH_TIMEOUT: Duration = Duration::from_secs(30);

// --- Data types ---

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GhLabel {
    pub name: String,
    #[serde(default)]
    pub color: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GhAuthor {
    pub login: String,
}

/// A single GitHub issue assignee, read from the `assignees` JSON field. Mirrors
/// [`GhAuthor`]: only the `login` is materialized. Surfaced as the native
/// `DocMeta.assignee` at fetch (first entry when multiple), and written through
/// via `gh issue edit --add-assignee/--remove-assignee`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GhAssignee {
    pub login: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GhIssue {
    pub number: u64,
    /// GraphQL node id (`I_*`), fetched via `--json id`. Empty when absent.
    /// Sub-issue mutations key off this rather than the REST `number`.
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub labels: Vec<GhLabel>,
    #[serde(default)]
    pub state: String,
    #[serde(default, rename = "updatedAt")]
    pub updated_at: String,
    #[serde(default, rename = "createdAt")]
    pub created_at: String,
    #[serde(default)]
    pub author: Option<GhAuthor>,
    /// Native GitHub issue-type name (`issueType.name`). `None` when the issue
    /// has no native type. Sourced only from the GraphQL `issueType` field,
    /// never from labels.
    #[serde(default, skip)]
    pub issue_type: Option<String>,
    /// Assigned GitHub milestone, read from the `milestone` JSON field. `None`
    /// when the issue has no milestone. Surfaced as a forward `targets`
    /// relation at fetch (the milestone's number resolves to its `MILESTONE-n`
    /// doc); the inverse `targeted-by` is derived virtually, never stored.
    #[serde(default)]
    pub milestone: Option<GhIssueMilestone>,
    /// Native GitHub issue assignees, read from the `assignees` JSON field. The
    /// first entry is inherited as `DocMeta.assignee` at fetch; the rest are out
    /// of scope (single assignee model). Empty when the issue is unassigned.
    #[serde(default)]
    pub assignees: Vec<GhAssignee>,
}

/// The slice of a milestone carried on an issue's `milestone` JSON field. Only
/// the `number` matters: it resolves to a `MILESTONE-n` doc via the issue map.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GhIssueMilestone {
    pub number: u64,
}

/// A GitHub milestone (REST shape). Field names mirror the REST API so a
/// `gh api repos/{repo}/milestones` response deserializes directly.
/// `open_issues`/`closed_issues` are read-only counts used to compute progress.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
pub struct GhMilestone {
    pub number: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub due_on: Option<String>,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub open_issues: u64,
    #[serde(default)]
    pub closed_issues: u64,
    #[serde(default, rename = "html_url")]
    pub url: String,
}

/// A single GitHub issue comment, flattened from the REST shape
/// (`author.login`, `body`, `created_at`). Read-only: comments are fetched on
/// read and never authored or round-tripped back to GitHub.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(from = "RawComment")]
pub struct GhComment {
    pub author: String,
    pub body: String,
    pub timestamp: String,
}

#[derive(Deserialize)]
struct RawComment {
    #[serde(default)]
    user: Option<GhAuthor>,
    #[serde(default)]
    body: String,
    #[serde(default)]
    created_at: String,
}

impl From<RawComment> for GhComment {
    fn from(raw: RawComment) -> Self {
        GhComment {
            author: raw.user.map(|u| u.login).unwrap_or_default(),
            body: raw.body,
            timestamp: raw.created_at,
        }
    }
}

// --- Error types ---

#[derive(Debug)]
pub enum GhError {
    NotInstalled,
    AuthFailed(String),
    ApiError { status: u16, message: String },
    RateLimited { retry_after: Option<u64> },
}

impl std::fmt::Display for GhError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GhError::NotInstalled => write!(f, "gh CLI is not installed"),
            GhError::AuthFailed(msg) => write!(f, "gh auth failed: {}", msg),
            GhError::ApiError { status, message } => {
                write!(f, "gh API error (HTTP {}): {}", status, message)
            }
            GhError::RateLimited { retry_after } => match retry_after {
                Some(secs) => write!(f, "gh API rate limited, retry after {}s", secs),
                None => write!(f, "gh API rate limited"),
            },
        }
    }
}

impl std::error::Error for GhError {}

// --- Auth ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthStatus {
    Authenticated { user: String, host: String },
    NotAuthenticated(String),
    GhNotInstalled,
}

// --- URL parsing ---

pub fn parse_issue_number_from_url(url: &str) -> Result<u64> {
    url.trim()
        .rsplit('/')
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| anyhow::anyhow!("failed to parse issue number from URL: {}", url))
}

// --- JSON parsing ---

pub fn parse_issue_json(stdout: &str) -> Result<GhIssue> {
    serde_json::from_str(stdout).map_err(|e| anyhow::anyhow!("failed to parse issue JSON: {}", e))
}

pub fn parse_issue_list_json(stdout: &str) -> Result<Vec<GhIssue>> {
    serde_json::from_str(stdout)
        .map_err(|e| anyhow::anyhow!("failed to parse issue list JSON: {}", e))
}

pub fn parse_comments_json(stdout: &str) -> Result<Vec<GhComment>> {
    serde_json::from_str(stdout)
        .map_err(|e| anyhow::anyhow!("failed to parse comments JSON: {}", e))
}

pub fn parse_milestone_json(stdout: &str) -> Result<GhMilestone> {
    serde_json::from_str(stdout)
        .map_err(|e| anyhow::anyhow!("failed to parse milestone JSON: {}", e))
}

pub fn parse_milestone_list_json(stdout: &str) -> Result<Vec<GhMilestone>> {
    serde_json::from_str(stdout)
        .map_err(|e| anyhow::anyhow!("failed to parse milestone list JSON: {}", e))
}

/// Pure argv builder for `issue_set_milestone`. `None` clears the milestone and
/// MUST emit `-F milestone=null` (a JSON null, not the string `"null"`);
/// `Some(n)` emits `-F milestone=<n>` (a typed int). `-F` (not `-f`) is what
/// makes `gh` send the value as raw JSON rather than a string.
pub fn build_set_milestone_args(
    repo: &str,
    issue_number: u64,
    milestone: Option<u64>,
) -> Vec<String> {
    let value = match milestone {
        Some(n) => n.to_string(),
        None => "null".to_string(),
    };
    vec![
        "api".to_string(),
        "-X".to_string(),
        "PATCH".to_string(),
        format!("repos/{}/issues/{}", repo, issue_number),
        "-F".to_string(),
        format!("milestone={}", value),
    ]
}

/// Pure argv builder for `issue_set_assignee`: `gh issue edit <n> --repo <repo>`
/// with one `--add-assignee <login>` per `add` and one `--remove-assignee
/// <login>` per `remove`. Follows the `build_set_milestone_args` precedent
/// (argv building kept pure so it is unit-testable without invoking `gh`).
pub fn build_set_assignee_args(
    repo: &str,
    issue_number: u64,
    add: &[String],
    remove: &[String],
) -> Vec<String> {
    let mut args = vec![
        "issue".to_string(),
        "edit".to_string(),
        issue_number.to_string(),
        "--repo".to_string(),
        repo.to_string(),
    ];
    for login in add {
        args.push("--add-assignee".to_string());
        args.push(login.clone());
    }
    for login in remove {
        args.push("--remove-assignee".to_string());
        args.push(login.clone());
    }
    args
}

/// Pure argv builder for adding a native issue-dependency
/// (`POST repos/{repo}/issues/{blocked_number}/dependencies/blocked_by`). The
/// blocking issue is named in the body by its REST *database id*
/// (`blocking_issue_id`), NOT its display number, and `-F` sends it as a typed
/// JSON int — both are what the dependencies API requires. Mirrors
/// [`build_set_milestone_args`]' REST-path discipline.
pub fn build_add_blocked_by_args(
    repo: &str,
    blocked_number: u64,
    blocking_issue_id: u64,
) -> Vec<String> {
    vec![
        "api".to_string(),
        "-X".to_string(),
        "POST".to_string(),
        format!(
            "repos/{}/issues/{}/dependencies/blocked_by",
            repo, blocked_number
        ),
        "-F".to_string(),
        format!("issue_id={}", blocking_issue_id),
    ]
}

/// Pure argv builder for removing a native issue-dependency
/// (`DELETE repos/{repo}/issues/{blocked_number}/dependencies/blocked_by/{blocking_issue_id}`).
/// The blocking issue's REST database id is the final path segment, not a body
/// field.
pub fn build_remove_blocked_by_args(
    repo: &str,
    blocked_number: u64,
    blocking_issue_id: u64,
) -> Vec<String> {
    vec![
        "api".to_string(),
        "-X".to_string(),
        "DELETE".to_string(),
        format!(
            "repos/{}/issues/{}/dependencies/blocked_by/{}",
            repo, blocked_number, blocking_issue_id
        ),
    ]
}

/// Pure argv builder for listing an issue's native blocked-by dependencies
/// (`GET repos/{repo}/issues/{n}/dependencies/blocked_by`). The response is an
/// array of issue objects; only their `number` is used downstream to resolve
/// each blocking issue to its doc.
pub fn build_list_blocked_by_args(repo: &str, blocked_number: u64) -> Vec<String> {
    vec![
        "api".to_string(),
        format!(
            "repos/{}/issues/{}/dependencies/blocked_by",
            repo, blocked_number
        ),
    ]
}

#[derive(Deserialize)]
struct BlockedByIssue {
    number: u64,
}

/// Parse a `dependencies/blocked_by` response (an array of issue objects) into
/// the blocking issues' display numbers. Only `number` is read; every other
/// field of the issue objects is ignored.
pub fn parse_blocked_by_numbers(stdout: &str) -> Result<Vec<u64>> {
    let issues: Vec<BlockedByIssue> = serde_json::from_str(stdout)
        .map_err(|e| anyhow::anyhow!("failed to parse blocked_by JSON: {}", e))?;
    Ok(issues.into_iter().map(|i| i.number).collect())
}

// --- Label helpers ---

pub fn type_label(type_name: &str) -> String {
    format!("lazyspec:{}", type_name)
}

pub fn deterministic_color(type_name: &str) -> String {
    let mut hasher = DefaultHasher::new();
    type_name.hash(&mut hasher);
    let hash = hasher.finish();
    format!("{:06x}", hash & 0xFFFFFF)
}

// --- Traits ---

pub trait GhIssueReader {
    fn issue_list(
        &self,
        repo: &str,
        labels: &[String],
        json_fields: &[String],
        limit: Option<u64>,
    ) -> Result<Vec<GhIssue>>;

    fn issue_view(&self, repo: &str, number: u64) -> Result<GhIssue>;

    /// Read-only fetch of an issue's comment thread (`GET
    /// repos/{repo}/issues/{number}/comments`). Lives on the reader only:
    /// comments are never authored, edited, or round-tripped back to GitHub.
    fn issue_comments(&self, repo: &str, number: u64) -> Result<Vec<GhComment>>;
}

pub trait GhIssueWriter {
    fn issue_create(
        &self,
        repo: &str,
        title: &str,
        body: &str,
        labels: &[String],
    ) -> Result<GhIssue>;

    fn issue_edit(
        &self,
        repo: &str,
        number: u64,
        title: Option<&str>,
        body: Option<&str>,
        labels_add: &[String],
        labels_remove: &[String],
    ) -> Result<()>;

    fn issue_close(&self, repo: &str, number: u64) -> Result<()>;

    fn issue_reopen(&self, repo: &str, number: u64) -> Result<()>;

    /// Set/clear an issue's native assignees (`gh issue edit
    /// --add-assignee/--remove-assignee`). `add` and `remove` are GitHub logins
    /// (no `@`); the caller diffs the desired assignee against the remote's
    /// current one and passes only the delta. A no-op when both are empty.
    fn issue_set_assignee(
        &self,
        repo: &str,
        number: u64,
        add: &[String],
        remove: &[String],
    ) -> Result<()>;

    fn label_create(&self, repo: &str, name: &str, description: &str, color: &str) -> Result<()>;

    fn label_ensure(&self, repo: &str, name: &str, description: &str, color: &str) -> Result<()>;
}

/// REST seam for GitHub milestones, kept separate from the issue traits so it
/// can be faked independently. Milestones use the REST API (`gh api
/// repos/{repo}/milestones`), not GraphQL.
pub trait GhMilestoneApi {
    fn milestone_list(&self, repo: &str) -> Result<Vec<GhMilestone>>;

    fn milestone_view(&self, repo: &str, number: u64) -> Result<GhMilestone>;

    fn milestone_create(
        &self,
        repo: &str,
        title: &str,
        description: &str,
        due_on: Option<&str>,
        state: &str,
    ) -> Result<GhMilestone>;

    fn milestone_edit(
        &self,
        repo: &str,
        number: u64,
        title: Option<&str>,
        description: Option<&str>,
        due_on: Option<&str>,
        state: Option<&str>,
    ) -> Result<GhMilestone>;

    fn milestone_delete(&self, repo: &str, number: u64) -> Result<()>;

    /// Set or clear the milestone association on an issue (`PATCH issues/{n}`,
    /// the GitHub edge of record). `None` clears it.
    fn issue_set_milestone(
        &self,
        repo: &str,
        issue_number: u64,
        milestone: Option<u64>,
    ) -> Result<()>;
}

/// REST seam for GitHub issue dependencies (the native blocked-by / blocking
/// graph), kept separate from [`GhMilestoneApi`] so it can be faked
/// independently. Dependencies are REST
/// (`gh api repos/{repo}/issues/{n}/dependencies/blocked_by`), not GraphQL.
///
/// Endpoints take a blocked issue by its display `number` (the path segment)
/// and identify the blocking issue by its REST *database id* — the real impl
/// resolves that id from the number, since the issue map carries only the
/// display number and GraphQL node id.
pub trait GhIssueDependencyApi {
    /// List the display numbers of the issues that block `blocked_number`
    /// (`GET issues/{blocked_number}/dependencies/blocked_by`, an array of issue
    /// objects). The read side of the native dependency graph; the caller maps
    /// each number to its doc via the issue map.
    fn list_blocked_by(&self, repo: &str, blocked_number: u64) -> Result<Vec<u64>>;

    /// Record that issue `blocked_number` is blocked by issue `blocking_number`
    /// (`POST issues/{blocked_number}/dependencies/blocked_by`, body
    /// `issue_id=<blocking issue database id>`). Idempotent on GitHub's side.
    fn add_blocked_by(&self, repo: &str, blocked_number: u64, blocking_number: u64) -> Result<()>;

    /// Drop the blocked-by edge [`add_blocked_by`] created
    /// (`DELETE issues/{blocked_number}/dependencies/blocked_by/{blocking issue database id}`).
    fn remove_blocked_by(
        &self,
        repo: &str,
        blocked_number: u64,
        blocking_number: u64,
    ) -> Result<()>;
}

pub trait GhAuth {
    fn auth_status(&self) -> Result<AuthStatus>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum GqlVar {
    Str(String),
    Int(i64),
    Bool(bool),
    /// A list var, emitted as repeated `-f key[]=value` flags (gh's array
    /// syntax). Binds to a `[String!]`/`[ID!]` GraphQL variable.
    StrList(Vec<String>),
}

/// The typed kind of a Projects v2 board field. Single-select and iteration are
/// option-backed (validated against the schema snapshot); number/date/text are
/// shape-checked only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhFieldKind {
    SingleSelect,
    Iteration,
    Number,
    Date,
    Text,
}

/// The read-side value of a project field item, in its native representation.
/// `SingleSelect`/`Iteration` carry the human-facing name/title (not the option
/// id); the id is resolved separately from the schema snapshot on write.
#[derive(Debug, Clone, PartialEq)]
pub enum GhFieldValueRepr {
    /// Single-select option *name*.
    OptionName(String),
    /// Iteration *title*.
    IterationTitle(String),
    Number(f64),
    Date(NaiveDate),
    Text(String),
}

/// One field value read off a project item for a given board, ready to be
/// namespaced as `PROJECT-{project_number}.{field_name}`.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectFieldValue {
    pub project_number: u64,
    pub field_name: String,
    pub kind: GhFieldKind,
    pub value: GhFieldValueRepr,
}

/// One Projects v2 item: an issue's membership of a single board, plus whatever
/// field values are set on it. An item with an empty `fields` vec is still a
/// membership — "on the board with nothing filled in" is a different state from
/// "not on the board", and only this type can tell them apart.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectItem {
    pub project_number: u64,
    pub item_id: String,
    pub fields: Vec<ProjectFieldValue>,
}

/// Map a read field value to the typed [`AttrValue`] carried in `DocMeta`.
/// Single-select -> option name string, iteration -> title string, number ->
/// `Int` when integral else `Float`, date -> `Date`, text -> string. Always a
/// coerced value, never `AttrValue::Raw`.
pub fn gh_field_to_attr(value: &GhFieldValueRepr) -> AttrValue {
    match value {
        GhFieldValueRepr::OptionName(s) => AttrValue::Str(s.clone()),
        GhFieldValueRepr::IterationTitle(s) => AttrValue::Str(s.clone()),
        GhFieldValueRepr::Text(s) => AttrValue::Str(s.clone()),
        GhFieldValueRepr::Date(d) => AttrValue::Date(*d),
        GhFieldValueRepr::Number(n) => {
            if n.fract() == 0.0 {
                AttrValue::Int(*n as i64)
            } else {
                AttrValue::Float(*n)
            }
        }
    }
}

/// The write-side `value` payload for `updateProjectV2ItemFieldValue`. Each
/// variant serializes to a `value` object with EXACTLY ONE key (GitHub rejects
/// null sibling keys). Carries resolved ids, never names.
#[derive(Debug, Clone, PartialEq)]
pub enum GhFieldValueInput {
    SingleSelect(String),
    Iteration(String),
    Number(f64),
    Date(NaiveDate),
    Text(String),
}

impl Serialize for GhFieldValueInput {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            GhFieldValueInput::SingleSelect(id) => {
                map.serialize_entry("singleSelectOptionId", id)?;
            }
            GhFieldValueInput::Iteration(id) => {
                map.serialize_entry("iterationId", id)?;
            }
            GhFieldValueInput::Number(n) => {
                map.serialize_entry("number", n)?;
            }
            GhFieldValueInput::Date(d) => {
                map.serialize_entry("date", &d.format("%Y-%m-%d").to_string())?;
            }
            GhFieldValueInput::Text(s) => {
                map.serialize_entry("text", s)?;
            }
        }
        map.end()
    }
}

impl GhFieldValueInput {
    /// Render as a GraphQL input-object literal for splicing into a query
    /// string. Unlike `Serialize` (JSON, quoted keys), a GraphQL literal key is
    /// a bare name -- `{singleSelectOptionId: "opt_abc"}`, not
    /// `{"singleSelectOptionId": "opt_abc"}` -- so this cannot reuse
    /// `serde_json::to_string`. Values are still rendered through
    /// `serde_json` so string escaping matches GraphQL's JSON-compatible
    /// string literal syntax.
    fn to_graphql_literal(&self) -> String {
        let (key, value) = match self {
            GhFieldValueInput::SingleSelect(id) => ("singleSelectOptionId", json!(id)),
            GhFieldValueInput::Iteration(id) => ("iterationId", json!(id)),
            GhFieldValueInput::Number(n) => ("number", json!(n)),
            GhFieldValueInput::Date(d) => ("date", json!(d.format("%Y-%m-%d").to_string())),
            GhFieldValueInput::Text(s) => ("text", json!(s)),
        };
        format!("{{{}: {}}}", key, value)
    }
}

pub trait GhGraphql {
    fn graphql(&self, query: &str, vars: &[(&str, GqlVar)]) -> Result<serde_json::Value>;

    /// Read the project items for issue node `content_node_id`: one
    /// [`ProjectItem`] per board the issue belongs to, each carrying the set
    /// field values (unset fields are omitted).
    fn project_items(&self, repo: &str, content_node_id: &str) -> Result<Vec<ProjectItem>>;

    /// Set one project field value on an item (`updateProjectV2ItemFieldValue`).
    /// All ids must already be resolved (project node id, item id, field id, and
    /// the single-select option / iteration id inside `value`).
    ///
    /// Implemented once over [`GhGraphql::graphql`], so every client that speaks
    /// GraphQL inherits the payload check in [`require_project_item`].
    fn update_project_v2_item_field_value(
        &self,
        project_id: &str,
        item_id: &str,
        field_id: &str,
        value: &GhFieldValueInput,
    ) -> Result<()> {
        // `gh` cannot pass a JSON-object GraphQL variable via -f/-F, so the
        // single-key value object is inlined into the mutation literally --
        // as a GraphQL literal (bare key), not JSON (quoted key).
        let value_literal = value.to_graphql_literal();
        let query = UPDATE_PROJECT_FIELD_MUTATION.replace("__VALUE__", &value_literal);
        let resp = self.graphql(
            &query,
            &[
                ("projectId", GqlVar::Str(project_id.to_string())),
                ("itemId", GqlVar::Str(item_id.to_string())),
                ("fieldId", GqlVar::Str(field_id.to_string())),
            ],
        )?;
        require_project_item(&resp, "updateProjectV2ItemFieldValue")
    }

    /// Clear one project field value on an item (`clearProjectV2ItemFieldValue`).
    /// A distinct mutation from the setter: GitHub rejects an empty-string text
    /// write as a "clear".
    fn clear_project_field(&self, project_id: &str, item_id: &str, field_id: &str) -> Result<()> {
        let resp = self.graphql(
            CLEAR_PROJECT_FIELD_MUTATION,
            &[
                ("projectId", GqlVar::Str(project_id.to_string())),
                ("itemId", GqlVar::Str(item_id.to_string())),
                ("fieldId", GqlVar::Str(field_id.to_string())),
            ],
        )?;
        require_project_item(&resp, "clearProjectV2ItemFieldValue")
    }
}

/// A project-field mutation moved the cell only if GitHub echoed the item back.
/// `gh` exits zero on a response carrying an `errors` array and no data -- what a
/// token without the `project` scope gets -- so the payload the mutation is
/// supposed to return, never the process exit, is what says the write happened.
/// Both field mutations return `projectV2Item { id }`.
pub(crate) fn require_project_item(resp: &serde_json::Value, mutation: &str) -> Result<()> {
    if resp
        .pointer(&format!("/data/{}/projectV2Item/id", mutation))
        .is_some()
    {
        return Ok(());
    }
    if missing_project_scope(resp) {
        bail!(
            "writing a project field needs the `project` token scope; run `gh auth refresh -s project`"
        );
    }
    let detail = graphql_error_messages(resp);
    if detail.is_empty() {
        bail!("{} returned no project item", mutation);
    }
    bail!("{} returned no project item: {}", mutation, detail)
}

/// True when a GraphQL response signals the `project` token scope is missing:
/// a top-level `errors[]` entry whose type or message names insufficient
/// scopes, the `project` scope, an inaccessible resource, or a missing
/// permission. Everything Projects v2 needs that scope, which `repo` does not
/// grant.
pub(crate) fn missing_project_scope(resp: &serde_json::Value) -> bool {
    let Some(errors) = resp.pointer("/errors").and_then(|v| v.as_array()) else {
        return false;
    };
    errors.iter().any(|e| {
        let kind = e.get("type").and_then(|v| v.as_str()).unwrap_or_default();
        let msg = e
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_lowercase();
        kind == "INSUFFICIENT_SCOPES"
            || msg.contains("`project` scope")
            || msg.contains("project scope")
            || msg.contains("resource not accessible")
            || msg.contains("does not have permission")
    })
}

/// The `message` of every top-level GraphQL error, joined -- empty when the
/// response carries no `errors` array.
fn graphql_error_messages(resp: &serde_json::Value) -> String {
    resp.pointer("/errors")
        .and_then(|v| v.as_array())
        .map(|errors| {
            errors
                .iter()
                .filter_map(|e| e.get("message").and_then(|m| m.as_str()))
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_default()
}

// Forward the issue/graphql seams through shared references so callers holding a
// borrowed client (e.g. the fetch loop) can construct a `GithubIssuesStore`
// over `&G` without owning the client.
impl<T: GhIssueReader + ?Sized> GhIssueReader for &T {
    fn issue_list(
        &self,
        repo: &str,
        labels: &[String],
        json_fields: &[String],
        limit: Option<u64>,
    ) -> Result<Vec<GhIssue>> {
        (**self).issue_list(repo, labels, json_fields, limit)
    }

    fn issue_view(&self, repo: &str, number: u64) -> Result<GhIssue> {
        (**self).issue_view(repo, number)
    }

    fn issue_comments(&self, repo: &str, number: u64) -> Result<Vec<GhComment>> {
        (**self).issue_comments(repo, number)
    }
}

impl<T: GhIssueWriter + ?Sized> GhIssueWriter for &T {
    fn issue_create(
        &self,
        repo: &str,
        title: &str,
        body: &str,
        labels: &[String],
    ) -> Result<GhIssue> {
        (**self).issue_create(repo, title, body, labels)
    }

    fn issue_edit(
        &self,
        repo: &str,
        number: u64,
        title: Option<&str>,
        body: Option<&str>,
        labels_add: &[String],
        labels_remove: &[String],
    ) -> Result<()> {
        (**self).issue_edit(repo, number, title, body, labels_add, labels_remove)
    }

    fn issue_close(&self, repo: &str, number: u64) -> Result<()> {
        (**self).issue_close(repo, number)
    }

    fn issue_reopen(&self, repo: &str, number: u64) -> Result<()> {
        (**self).issue_reopen(repo, number)
    }

    fn issue_set_assignee(
        &self,
        repo: &str,
        number: u64,
        add: &[String],
        remove: &[String],
    ) -> Result<()> {
        (**self).issue_set_assignee(repo, number, add, remove)
    }

    fn label_create(&self, repo: &str, name: &str, description: &str, color: &str) -> Result<()> {
        (**self).label_create(repo, name, description, color)
    }

    fn label_ensure(&self, repo: &str, name: &str, description: &str, color: &str) -> Result<()> {
        (**self).label_ensure(repo, name, description, color)
    }
}

impl<T: GhGraphql + ?Sized> GhGraphql for &T {
    fn graphql(&self, query: &str, vars: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
        (**self).graphql(query, vars)
    }

    fn project_items(&self, repo: &str, content_node_id: &str) -> Result<Vec<ProjectItem>> {
        (**self).project_items(repo, content_node_id)
    }

    fn update_project_v2_item_field_value(
        &self,
        project_id: &str,
        item_id: &str,
        field_id: &str,
        value: &GhFieldValueInput,
    ) -> Result<()> {
        (**self).update_project_v2_item_field_value(project_id, item_id, field_id, value)
    }

    fn clear_project_field(&self, project_id: &str, item_id: &str, field_id: &str) -> Result<()> {
        (**self).clear_project_field(project_id, item_id, field_id)
    }
}

impl<T: GhMilestoneApi + ?Sized> GhMilestoneApi for &T {
    fn milestone_list(&self, repo: &str) -> Result<Vec<GhMilestone>> {
        (**self).milestone_list(repo)
    }

    fn milestone_view(&self, repo: &str, number: u64) -> Result<GhMilestone> {
        (**self).milestone_view(repo, number)
    }

    fn milestone_create(
        &self,
        repo: &str,
        title: &str,
        description: &str,
        due_on: Option<&str>,
        state: &str,
    ) -> Result<GhMilestone> {
        (**self).milestone_create(repo, title, description, due_on, state)
    }

    fn milestone_edit(
        &self,
        repo: &str,
        number: u64,
        title: Option<&str>,
        description: Option<&str>,
        due_on: Option<&str>,
        state: Option<&str>,
    ) -> Result<GhMilestone> {
        (**self).milestone_edit(repo, number, title, description, due_on, state)
    }

    fn milestone_delete(&self, repo: &str, number: u64) -> Result<()> {
        (**self).milestone_delete(repo, number)
    }

    fn issue_set_milestone(
        &self,
        repo: &str,
        issue_number: u64,
        milestone: Option<u64>,
    ) -> Result<()> {
        (**self).issue_set_milestone(repo, issue_number, milestone)
    }
}

impl<T: GhIssueDependencyApi + ?Sized> GhIssueDependencyApi for &T {
    fn list_blocked_by(&self, repo: &str, blocked_number: u64) -> Result<Vec<u64>> {
        (**self).list_blocked_by(repo, blocked_number)
    }

    fn add_blocked_by(&self, repo: &str, blocked_number: u64, blocking_number: u64) -> Result<()> {
        (**self).add_blocked_by(repo, blocked_number, blocking_number)
    }

    fn remove_blocked_by(
        &self,
        repo: &str,
        blocked_number: u64,
        blocking_number: u64,
    ) -> Result<()> {
        (**self).remove_blocked_by(repo, blocked_number, blocking_number)
    }
}

/// Upcast to `dyn Any` so a boxed store client can be downcast back to its
/// concrete type (used by tests to inspect a mock's captured state after it has
/// been moved behind a `Box<dyn GhClient>` etc.). Blanket-implemented for every
/// `'static` type, so no client needs to implement it by hand.
pub trait AsAny {
    fn as_any(&self) -> &dyn std::any::Any;
}

impl<T: std::any::Any> AsAny for T {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// The full GitHub-issues client seam as a single object-safe trait, so
/// `GithubIssuesStore` can hold a `Box<dyn GhClient>` instead of being generic
/// over its client type. Blanket-implemented for anything that satisfies the
/// three underlying seams.
pub trait GhClient: GhIssueReader + GhIssueWriter + GhGraphql + AsAny + Send {
    /// View this client as `&dyn GhGraphql` for helpers that take the graphql
    /// seam directly (trait-object upcasting is not relied on).
    fn as_graphql(&self) -> &dyn GhGraphql;
}

impl<T: GhIssueReader + GhIssueWriter + GhGraphql + AsAny + Send> GhClient for T {
    fn as_graphql(&self) -> &dyn GhGraphql {
        self
    }
}

/// The milestone client seam as an object-safe trait for
/// `GithubMilestonesStore`'s boxed client.
pub trait GhMilestoneClient: GhMilestoneApi + AsAny {}

impl<T: GhMilestoneApi + AsAny> GhMilestoneClient for T {}

/// The graphql client seam as an object-safe trait for `GithubProjectsStore`'s
/// boxed client.
pub trait GhProjectsClient: GhGraphql + AsAny {
    fn as_graphql(&self) -> &dyn GhGraphql;
}

impl<T: GhGraphql + AsAny> GhProjectsClient for T {
    fn as_graphql(&self) -> &dyn GhGraphql {
        self
    }
}

/// One inline fragment per project-field value type, naming the field it belongs
/// to. This is the exact shape [`parse_project_items_array`] reads, so every
/// query that wants project field values -- the single-issue read below and the
/// composed fetch round's inline `fieldValues` (`gh_fetch`) -- selects it from
/// here rather than spelling it out a second time.
pub(crate) const PROJECT_FIELD_VALUE_SELECTION: &str = "__typename \
     ... on ProjectV2ItemFieldSingleSelectValue { name field { ... on ProjectV2FieldCommon { name } } } \
     ... on ProjectV2ItemFieldIterationValue { title field { ... on ProjectV2FieldCommon { name } } } \
     ... on ProjectV2ItemFieldNumberValue { number field { ... on ProjectV2FieldCommon { name } } } \
     ... on ProjectV2ItemFieldDateValue { date field { ... on ProjectV2FieldCommon { name } } } \
     ... on ProjectV2ItemFieldTextValue { text field { ... on ProjectV2FieldCommon { name } } }";

/// One issue's board memberships and field cells. The read-back after a
/// project-field write, where one issue is genuinely one request; a fetch reads
/// the same shape inline on the composed round instead.
fn project_item_fields_query() -> String {
    format!(
        "query($id: ID!) {{ node(id: $id) {{ ... on Issue {{ \
         projectItems(first: 50) {{ nodes {{ id project {{ number }} \
         fieldValues(first: 50) {{ nodes {{ {PROJECT_FIELD_VALUE_SELECTION} }} }} }} }} }} }} }}"
    )
}

const UPDATE_PROJECT_FIELD_MUTATION: &str = "mutation($projectId: ID!, $itemId: ID!, $fieldId: ID!) { updateProjectV2ItemFieldValue(input: {projectId: $projectId, itemId: $itemId, fieldId: $fieldId, value: __VALUE__}) { projectV2Item { id } } }";

const CLEAR_PROJECT_FIELD_MUTATION: &str = "mutation($projectId: ID!, $itemId: ID!, $fieldId: ID!) { clearProjectV2ItemFieldValue(input: {projectId: $projectId, itemId: $itemId, fieldId: $fieldId}) { projectV2Item { id } } }";

/// Parse the `projectItems.nodes` of a [`project_item_fields_query`] response
/// into one [`ProjectItem`] per board membership, each carrying one
/// [`ProjectFieldValue`] per set field value. Unset fields and field values with
/// no resolvable name/typename are skipped; an item whose board number is
/// missing is skipped entirely, but an item with no field values is kept.
pub fn parse_project_items(resp: &serde_json::Value) -> Vec<ProjectItem> {
    let Some(items) = resp
        .pointer("/data/node/projectItems/nodes")
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };
    parse_project_items_array(items)
}

/// The per-item parse, over the `projectItems.nodes` array wherever it was
/// selected -- the single-issue read back or the composed round's inline
/// connection. One [`ProjectItem`] per board membership, each carrying one
/// [`ProjectFieldValue`] per set field value. Unset fields and field values with
/// no resolvable name/typename are skipped; an item whose board number is
/// missing is skipped entirely, but an item with no field values is kept.
pub(crate) fn parse_project_items_array(items: &[serde_json::Value]) -> Vec<ProjectItem> {
    let mut out: Vec<ProjectItem> = Vec::new();
    for item in items {
        let Some(number) = item.pointer("/project/number").and_then(|v| v.as_u64()) else {
            continue;
        };
        let item_id = item
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let mut fields = Vec::new();
        let values = item
            .pointer("/fieldValues/nodes")
            .and_then(|v| v.as_array())
            .map(|v| v.as_slice())
            .unwrap_or_default();
        for fv in values {
            let Some(field_name) = fv.pointer("/field/name").and_then(|v| v.as_str()) else {
                continue;
            };
            let typename = fv.get("__typename").and_then(|v| v.as_str()).unwrap_or("");
            let parsed = match typename {
                "ProjectV2ItemFieldSingleSelectValue" => {
                    fv.get("name").and_then(|v| v.as_str()).map(|s| {
                        (
                            GhFieldKind::SingleSelect,
                            GhFieldValueRepr::OptionName(s.to_string()),
                        )
                    })
                }
                "ProjectV2ItemFieldIterationValue" => {
                    fv.get("title").and_then(|v| v.as_str()).map(|s| {
                        (
                            GhFieldKind::Iteration,
                            GhFieldValueRepr::IterationTitle(s.to_string()),
                        )
                    })
                }
                "ProjectV2ItemFieldNumberValue" => fv
                    .get("number")
                    .and_then(|v| v.as_f64())
                    .map(|n| (GhFieldKind::Number, GhFieldValueRepr::Number(n))),
                "ProjectV2ItemFieldDateValue" => fv
                    .get("date")
                    .and_then(|v| v.as_str())
                    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                    .map(|d| (GhFieldKind::Date, GhFieldValueRepr::Date(d))),
                "ProjectV2ItemFieldTextValue" => fv
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(|s| (GhFieldKind::Text, GhFieldValueRepr::Text(s.to_string()))),
                _ => None,
            };
            if let Some((kind, value)) = parsed {
                fields.push(ProjectFieldValue {
                    project_number: number,
                    field_name: field_name.to_string(),
                    kind,
                    value,
                });
            }
        }
        out.push(ProjectItem {
            project_number: number,
            item_id,
            fields,
        });
    }
    out
}

/// `-f key=value` for GraphQL String vars, `-F key=value` for typed (Int/Boolean).
/// `StrList` is handled in `build_graphql_args` (it expands to multiple flags).
fn gql_var_flag(v: &GqlVar) -> (&'static str, String) {
    match v {
        GqlVar::Str(s) => ("-f", s.clone()),
        GqlVar::Int(n) => ("-F", n.to_string()),
        GqlVar::Bool(b) => ("-F", b.to_string()),
        GqlVar::StrList(_) => unreachable!("StrList expanded in build_graphql_args"),
    }
}

/// Pure argv builder for `gh api graphql`. Vars flatten to repeated
/// `-f`/`-F` flags; never a single `variables=` JSON blob.
pub fn build_graphql_args(query: &str, vars: &[(&str, GqlVar)]) -> Vec<String> {
    let mut args = vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={}", query),
    ];
    for (key, var) in vars {
        if let GqlVar::StrList(items) = var {
            for item in items {
                args.push("-f".to_string());
                args.push(format!("{}[]={}", key, item));
            }
            continue;
        }
        let (flag, value) = gql_var_flag(var);
        args.push(flag.to_string());
        args.push(format!("{}={}", key, value));
    }
    args
}

// --- Implementation ---

pub struct GhCli;

impl Default for GhCli {
    fn default() -> Self {
        Self::new()
    }
}

impl GhCli {
    pub fn new() -> Self {
        GhCli
    }

    fn run_gh(&self, args: &[&str]) -> Result<std::process::Output> {
        // Every gh call is a network round-trip; cap it so a hung remote can't
        // wedge the caller (e.g. the background poll thread -- BUG-001).
        let mut cmd = Command::new("gh");
        cmd.args(args);

        match crate::engine::subprocess::output_with_timeout(cmd, GH_TIMEOUT) {
            Ok(o) => Ok(o),
            Err(e) => match e.downcast_ref::<std::io::Error>() {
                Some(io) if io.kind() == std::io::ErrorKind::NotFound => {
                    bail!(GhError::NotInstalled)
                }
                _ => bail!("failed to execute gh: {}", e),
            },
        }
    }

    fn run_gh_checked(&self, args: &[&str]) -> Result<String> {
        let output = self.run_gh(args)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let msg = stderr.trim().to_string();
            bail!(classify_gh_error(&msg));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

impl GhIssueReader for GhCli {
    fn issue_list(
        &self,
        repo: &str,
        labels: &[String],
        json_fields: &[String],
        limit: Option<u64>,
    ) -> Result<Vec<GhIssue>> {
        let label_filter = labels.join(",");
        let fields = if json_fields.is_empty() {
            "id,number,url,title,body,labels,state,updatedAt,createdAt,author,milestone,assignees"
                .to_string()
        } else {
            json_fields.join(",")
        };

        let limit_str = limit.map(|l| l.to_string());
        let mut args = vec![
            "issue", "list", "--repo", repo, "--state", "all", "--json", &fields,
        ];

        if !labels.is_empty() {
            args.push("--label");
            args.push(&label_filter);
        }

        if let Some(ref l) = limit_str {
            args.push("--limit");
            args.push(l);
        }

        let stdout = self.run_gh_checked(&args)?;
        parse_issue_list_json(&stdout)
    }

    fn issue_view(&self, repo: &str, number: u64) -> Result<GhIssue> {
        let num_str = number.to_string();
        let args = [
            "issue",
            "view",
            &num_str,
            "--repo",
            repo,
            "--json",
            "id,number,url,title,body,labels,state,updatedAt,createdAt,author,milestone,assignees",
        ];

        let stdout = self.run_gh_checked(&args)?;
        let mut issue = parse_issue_json(&stdout)?;

        // `gh issue view` does not expose the native issue type; fetch it over
        // GraphQL. Native issue-types are GA so no preview Accept header is sent.
        let (owner, name) = split_owner_repo(repo)?;
        let resp = self.graphql(
            ISSUE_TYPE_QUERY,
            &[
                ("owner", GqlVar::Str(owner.to_string())),
                ("name", GqlVar::Str(name.to_string())),
                ("number", GqlVar::Int(number as i64)),
            ],
        )?;
        issue.issue_type = parse_issue_type_name(&resp);

        Ok(issue)
    }

    fn issue_comments(&self, repo: &str, number: u64) -> Result<Vec<GhComment>> {
        let endpoint = format!("repos/{}/issues/{}/comments", repo, number);
        let stdout = self.run_gh_checked(&["api", &endpoint])?;
        parse_comments_json(&stdout)
    }
}

const ISSUE_TYPE_QUERY: &str = "query($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) { issue(number: $number) { id issueType { id name } } } }";

fn split_owner_repo(repo: &str) -> Result<(&str, &str)> {
    repo.split_once('/')
        .filter(|(o, n)| !o.is_empty() && !n.is_empty())
        .ok_or_else(|| anyhow::anyhow!("repo '{}' must be in owner/name form", repo))
}

fn parse_issue_type_name(resp: &serde_json::Value) -> Option<String> {
    resp.pointer("/data/repository/issue/issueType/name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

impl GhIssueWriter for GhCli {
    fn issue_create(
        &self,
        repo: &str,
        title: &str,
        body: &str,
        labels: &[String],
    ) -> Result<GhIssue> {
        let mut args = vec![
            "issue", "create", "--repo", repo, "--title", title, "--body", body,
        ];
        for label in labels {
            args.push("--label");
            args.push(label);
        }

        let stdout = self.run_gh_checked(&args)?;
        let number = parse_issue_number_from_url(&stdout)?;
        self.issue_view(repo, number)
    }

    fn issue_edit(
        &self,
        repo: &str,
        number: u64,
        title: Option<&str>,
        body: Option<&str>,
        labels_add: &[String],
        labels_remove: &[String],
    ) -> Result<()> {
        let num_str = number.to_string();
        let mut args = vec!["issue", "edit", &num_str, "--repo", repo];

        if let Some(t) = title {
            args.push("--title");
            args.push(t);
        }
        if let Some(b) = body {
            args.push("--body");
            args.push(b);
        }
        for label in labels_add {
            args.push("--add-label");
            args.push(label);
        }
        for label in labels_remove {
            args.push("--remove-label");
            args.push(label);
        }

        self.run_gh_checked(&args)?;
        Ok(())
    }

    fn issue_close(&self, repo: &str, number: u64) -> Result<()> {
        let num_str = number.to_string();
        self.run_gh_checked(&["issue", "close", &num_str, "--repo", repo])?;
        Ok(())
    }

    fn issue_reopen(&self, repo: &str, number: u64) -> Result<()> {
        let num_str = number.to_string();
        self.run_gh_checked(&["issue", "reopen", &num_str, "--repo", repo])?;
        Ok(())
    }

    fn issue_set_assignee(
        &self,
        repo: &str,
        number: u64,
        add: &[String],
        remove: &[String],
    ) -> Result<()> {
        if add.is_empty() && remove.is_empty() {
            return Ok(());
        }
        let args = build_set_assignee_args(repo, number, add, remove);
        self.run_gh_checked(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
        Ok(())
    }

    fn label_create(&self, repo: &str, name: &str, description: &str, color: &str) -> Result<()> {
        self.run_gh_checked(&[
            "label",
            "create",
            name,
            "--repo",
            repo,
            "--description",
            description,
            "--color",
            color,
        ])?;
        Ok(())
    }

    fn label_ensure(&self, repo: &str, name: &str, description: &str, color: &str) -> Result<()> {
        self.run_gh_checked(&[
            "label",
            "create",
            name,
            "--repo",
            repo,
            "--description",
            description,
            "--color",
            color,
            "--force",
        ])?;
        Ok(())
    }
}

impl GhMilestoneApi for GhCli {
    fn milestone_list(&self, repo: &str) -> Result<Vec<GhMilestone>> {
        let endpoint = format!("repos/{}/milestones?state=all", repo);
        let stdout = self.run_gh_checked(&["api", &endpoint])?;
        parse_milestone_list_json(&stdout)
    }

    fn milestone_view(&self, repo: &str, number: u64) -> Result<GhMilestone> {
        let endpoint = format!("repos/{}/milestones/{}", repo, number);
        let stdout = self.run_gh_checked(&["api", &endpoint])?;
        parse_milestone_json(&stdout)
    }

    fn milestone_create(
        &self,
        repo: &str,
        title: &str,
        description: &str,
        due_on: Option<&str>,
        state: &str,
    ) -> Result<GhMilestone> {
        let endpoint = format!("repos/{}/milestones", repo);
        let title_arg = format!("title={}", title);
        let desc_arg = format!("description={}", description);
        let state_arg = format!("state={}", state);
        let mut args = vec![
            "api", "-X", "POST", &endpoint, "-f", &title_arg, "-f", &desc_arg, "-f", &state_arg,
        ];
        let due_arg;
        if let Some(due) = due_on {
            due_arg = format!("due_on={}", due);
            args.push("-f");
            args.push(&due_arg);
        }
        let stdout = self.run_gh_checked(&args)?;
        parse_milestone_json(&stdout)
    }

    fn milestone_edit(
        &self,
        repo: &str,
        number: u64,
        title: Option<&str>,
        description: Option<&str>,
        due_on: Option<&str>,
        state: Option<&str>,
    ) -> Result<GhMilestone> {
        let endpoint = format!("repos/{}/milestones/{}", repo, number);
        let mut args = vec![
            "api".to_string(),
            "-X".to_string(),
            "PATCH".to_string(),
            endpoint,
        ];
        if let Some(t) = title {
            args.push("-f".to_string());
            args.push(format!("title={}", t));
        }
        if let Some(d) = description {
            args.push("-f".to_string());
            args.push(format!("description={}", d));
        }
        if let Some(d) = due_on {
            args.push("-f".to_string());
            args.push(format!("due_on={}", d));
        }
        if let Some(s) = state {
            args.push("-f".to_string());
            args.push(format!("state={}", s));
        }
        let stdout = self.run_gh_checked(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
        parse_milestone_json(&stdout)
    }

    fn milestone_delete(&self, repo: &str, number: u64) -> Result<()> {
        let endpoint = format!("repos/{}/milestones/{}", repo, number);
        self.run_gh_checked(&["api", "-X", "DELETE", &endpoint])?;
        Ok(())
    }

    fn issue_set_milestone(
        &self,
        repo: &str,
        issue_number: u64,
        milestone: Option<u64>,
    ) -> Result<()> {
        let args = build_set_milestone_args(repo, issue_number, milestone);
        self.run_gh_checked(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
        Ok(())
    }
}

impl GhCli {
    /// Resolve an issue's REST database id (distinct from its display number)
    /// via `gh api repos/{repo}/issues/{n} --jq .id`. The dependencies API
    /// identifies the blocking issue by this id, not its number.
    fn issue_database_id(&self, repo: &str, number: u64) -> Result<u64> {
        let endpoint = format!("repos/{}/issues/{}", repo, number);
        let stdout = self.run_gh_checked(&["api", &endpoint, "--jq", ".id"])?;
        stdout.trim().parse::<u64>().map_err(|e| {
            anyhow::anyhow!(
                "failed to parse issue database id from '{}': {}",
                stdout.trim(),
                e
            )
        })
    }
}

impl GhIssueDependencyApi for GhCli {
    fn list_blocked_by(&self, repo: &str, blocked_number: u64) -> Result<Vec<u64>> {
        let args = build_list_blocked_by_args(repo, blocked_number);
        let stdout = self.run_gh_checked(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
        parse_blocked_by_numbers(&stdout)
    }

    fn add_blocked_by(&self, repo: &str, blocked_number: u64, blocking_number: u64) -> Result<()> {
        let blocking_id = self.issue_database_id(repo, blocking_number)?;
        let args = build_add_blocked_by_args(repo, blocked_number, blocking_id);
        self.run_gh_checked(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
        Ok(())
    }

    fn remove_blocked_by(
        &self,
        repo: &str,
        blocked_number: u64,
        blocking_number: u64,
    ) -> Result<()> {
        let blocking_id = self.issue_database_id(repo, blocking_number)?;
        let args = build_remove_blocked_by_args(repo, blocked_number, blocking_id);
        self.run_gh_checked(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
        Ok(())
    }
}

impl GhAuth for GhCli {
    fn auth_status(&self) -> Result<AuthStatus> {
        let output = match Command::new("gh").args(["auth", "status"]).output() {
            Ok(o) => o,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(AuthStatus::GhNotInstalled);
            }
            Err(e) => bail!("failed to execute gh: {}", e),
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}{}", stdout, stderr);

        if !output.status.success() {
            let msg = combined.trim().to_string();
            let lower = msg.to_lowercase();
            if lower.contains("not logged in")
                || lower.contains("authentication")
                || lower.contains("auth")
            {
                bail!(GhError::AuthFailed(msg.clone()));
            }
            return Ok(AuthStatus::NotAuthenticated(msg));
        }

        let user = extract_field(&combined, "Logged in to")
            .and_then(|_| extract_field(&combined, "account"))
            .or_else(|| extract_after(&combined, "account "))
            .unwrap_or_default();

        let host = extract_field(&combined, "Logged in to").unwrap_or_default();

        Ok(AuthStatus::Authenticated { user, host })
    }
}

impl GhGraphql for GhCli {
    fn graphql(&self, query: &str, vars: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
        let args = build_graphql_args(query, vars);
        let output = self.run_gh(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        // `gh api graphql` exits non-zero whenever the response carries a
        // GraphQL `errors` array -- e.g. an org-rooted query against a user
        // account, or one failed subtree of a composed round -- even though the
        // body's `data` still carries everything that did resolve. Parse stdout
        // first so that signal survives; only fall back to the exit-code failure
        // path when stdout is not a GraphQL response at all.
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
            if json.get("data").is_some() {
                return Ok(json);
            }
        }

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(classify_gh_error(stderr.trim()));
        }

        serde_json::from_str(&stdout)
            .map_err(|e| anyhow::anyhow!("failed to parse graphql response: {}", e))
    }

    fn project_items(&self, _repo: &str, content_node_id: &str) -> Result<Vec<ProjectItem>> {
        let resp = self.graphql(
            &project_item_fields_query(),
            &[("id", GqlVar::Str(content_node_id.to_string()))],
        )?;
        Ok(parse_project_items(&resp))
    }
}

fn classify_gh_error(stderr: &str) -> GhError {
    let lower = stderr.to_lowercase();

    if lower.contains("rate limit") || lower.contains("api rate limit") {
        let retry_after = lower.find("retry after").and_then(|idx| {
            lower[idx..]
                .split_whitespace()
                .find_map(|token| token.trim_end_matches('s').parse::<u64>().ok())
        });
        return GhError::RateLimited { retry_after };
    }

    if lower.contains("not logged in")
        || lower.contains("authentication")
        || lower.contains("auth token")
    {
        return GhError::AuthFailed(stderr.to_string());
    }

    // Try to extract HTTP status from gh stderr (e.g., "HTTP 404", "422 Validation Failed")
    let status = extract_http_status(&lower);
    GhError::ApiError {
        status: status.unwrap_or(0),
        message: stderr.to_string(),
    }
}

fn extract_http_status(lower: &str) -> Option<u16> {
    if let Some(idx) = lower.find("http ") {
        let rest = &lower[idx + 5..];
        if let Some(code) = rest
            .split_whitespace()
            .next()
            .and_then(|s| s.parse::<u16>().ok())
        {
            return Some(code);
        }
    }
    // Also match bare "404:" or "422 " patterns
    for token in lower.split_whitespace() {
        if let Ok(code) = token
            .trim_matches(|c: char| !c.is_ascii_digit())
            .parse::<u16>()
        {
            if (400..=599).contains(&code) {
                return Some(code);
            }
        }
    }
    None
}

fn extract_field(text: &str, prefix: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let value = rest
                .trim()
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '-');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn extract_after(text: &str, needle: &str) -> Option<String> {
    let idx = text.find(needle)?;
    let rest = &text[idx + needle.len()..];
    let token = rest.split_whitespace().next()?;
    let cleaned = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '.');
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned.to_string())
    }
}

#[cfg(test)]
pub mod test_support {
    use super::*;
    use std::cell::{Cell, RefCell};

    type GraphqlCall = (String, Vec<(String, GqlVar)>);

    /// A composed fetch-round response ([`crate::engine::gh_fetch`]) in the shape
    /// GitHub returns it, so a double answering a round drives the real parser
    /// rather than a hand-built [`crate::engine::gh_fetch::FetchSnapshot`].
    /// Org-owned: `issueTypes` only ever resolves under the Organization
    /// fragment.
    pub fn round_response(
        milestones: &[GhMilestone],
        issue_types: &[crate::engine::gh_schema::IssueTypeId],
        boards: &[(u64, Vec<String>)],
    ) -> serde_json::Value {
        let milestone_nodes: Vec<_> = milestones
            .iter()
            .map(|m| {
                serde_json::json!({
                    "number": m.number,
                    "title": m.title,
                    "description": m.description,
                    "dueOn": m.due_on,
                    "state": m.state.to_uppercase(),
                    "url": m.url,
                    "openIssues": {"totalCount": m.open_issues},
                    "closedIssues": {"totalCount": m.closed_issues}
                })
            })
            .collect();
        let issue_type_nodes: Vec<_> = issue_types
            .iter()
            .map(|t| serde_json::json!({"id": t.id, "name": t.name}))
            .collect();
        let mut owner = serde_json::json!({
            "__typename": "Organization",
            "issueTypes": {"nodes": issue_type_nodes}
        });
        for (number, columns) in boards {
            let field_id = format!("PVTSSF_b{}", number);
            let options: Vec<_> = columns
                .iter()
                .map(|name| {
                    serde_json::json!({
                        "id": format!("{}_{}", field_id, name.to_lowercase()),
                        "name": name
                    })
                })
                .collect();
            owner[format!("b{}", number)] = serde_json::json!({"fields": {"nodes": [{
                "__typename": "ProjectV2SingleSelectField",
                "id": field_id,
                "name": "Status",
                "dataType": "SINGLE_SELECT",
                "options": options
            }]}});
        }
        serde_json::json!({"data": {"repository": {
            "milestones": {"nodes": milestone_nodes},
            "owner": owner
        }}})
    }

    /// Answer every issue alias `query` composed with one finished page holding
    /// `issues`. `Repository.issues` is non-null, so a double that leaves an
    /// alias out is telling the parser that type's list failed; an empty page
    /// says "this type has none", which is what an empty `issues` gives.
    pub fn with_issue_pages(
        query: &str,
        mut resp: serde_json::Value,
        issues: &[GhIssue],
    ) -> serde_json::Value {
        let nodes: Vec<serde_json::Value> = issues.iter().map(issue_node).collect();
        let mut index = 0;
        while query.contains(&format!("t{}: issues(", index)) {
            resp["data"]["repository"][format!("t{}", index)] = serde_json::json!({
                "pageInfo": {"hasNextPage": false, "endCursor": serde_json::Value::Null},
                "nodes": nodes
            });
            index += 1;
        }
        resp
    }

    /// An issue's sub-issue children as the round selects them inline. Use
    /// [`with_sub_issues`] to attach one to a round response.
    pub fn sub_issue_edge(children: &[&str]) -> serde_json::Value {
        edge(
            children
                .iter()
                .map(|id| serde_json::json!({"id": id}))
                .collect(),
            false,
        )
    }

    /// The numbers blocking an issue, as the round selects them inline.
    pub fn blocked_by_edge(blockers: &[u64]) -> serde_json::Value {
        edge(
            blockers
                .iter()
                .map(|number| serde_json::json!({"number": number}))
                .collect(),
            false,
        )
    }

    /// An issue's board memberships and field cells, rendered back into the
    /// GraphQL shape the round selects, so a double drives the real parser
    /// rather than handing [`ProjectItem`]s straight to the snapshot.
    pub fn project_items_edge(items: &[ProjectItem]) -> serde_json::Value {
        edge(items.iter().map(project_item_node).collect(), false)
    }

    fn project_item_node(item: &ProjectItem) -> serde_json::Value {
        serde_json::json!({
            "id": item.item_id,
            "project": {"number": item.project_number},
            "fieldValues": edge(item.fields.iter().map(field_value_node).collect(), false)
        })
    }

    fn field_value_node(value: &ProjectFieldValue) -> serde_json::Value {
        let field = serde_json::json!({"name": value.field_name});
        match &value.value {
            GhFieldValueRepr::OptionName(name) => serde_json::json!({
                "__typename": "ProjectV2ItemFieldSingleSelectValue", "name": name, "field": field
            }),
            GhFieldValueRepr::IterationTitle(title) => serde_json::json!({
                "__typename": "ProjectV2ItemFieldIterationValue", "title": title, "field": field
            }),
            GhFieldValueRepr::Number(number) => serde_json::json!({
                "__typename": "ProjectV2ItemFieldNumberValue", "number": number, "field": field
            }),
            GhFieldValueRepr::Date(date) => serde_json::json!({
                "__typename": "ProjectV2ItemFieldDateValue",
                "date": date.format("%Y-%m-%d").to_string(),
                "field": field
            }),
            GhFieldValueRepr::Text(text) => serde_json::json!({
                "__typename": "ProjectV2ItemFieldTextValue", "text": text, "field": field
            }),
        }
    }

    fn edge(nodes: Vec<serde_json::Value>, has_next_page: bool) -> serde_json::Value {
        serde_json::json!({"pageInfo": {"hasNextPage": has_next_page}, "nodes": nodes})
    }

    /// Replace the `subIssues` connection of the issue with node id `node_id` on
    /// every alias of a round response. Doubles answer the round's inline
    /// connections this way rather than the retired `nodes(ids:)` batch.
    pub fn with_sub_issues(
        resp: serde_json::Value,
        node_id: &str,
        edge: serde_json::Value,
    ) -> serde_json::Value {
        with_issue_edge(resp, node_id, "subIssues", edge)
    }

    /// As [`with_sub_issues`], for the `blockedBy` connection.
    pub fn with_blocked_by(
        resp: serde_json::Value,
        node_id: &str,
        edge: serde_json::Value,
    ) -> serde_json::Value {
        with_issue_edge(resp, node_id, "blockedBy", edge)
    }

    /// The round as a token without the `project` scope gets it: every issue's
    /// `projectItems` null, with one `errors[]` entry naming the path. The issue
    /// lists and the other inline connections come back intact beside it, which
    /// is the partial-failure shape the round is built to survive.
    pub fn without_project_items(mut resp: serde_json::Value, message: &str) -> serde_json::Value {
        let mut paths = Vec::new();
        if let Some(repo) = resp
            .pointer_mut("/data/repository")
            .and_then(|v| v.as_object_mut())
        {
            for (alias, value) in repo.iter_mut() {
                let Some(nodes) = value.get_mut("nodes").and_then(|v| v.as_array_mut()) else {
                    continue;
                };
                for (index, node) in nodes.iter_mut().enumerate() {
                    node["projectItems"] = serde_json::Value::Null;
                    paths.push(serde_json::json!([
                        "repository",
                        alias,
                        "nodes",
                        index,
                        "projectItems"
                    ]));
                }
            }
        }
        resp["errors"] = paths
            .into_iter()
            .map(|path| serde_json::json!({"type": "INSUFFICIENT_SCOPES", "message": message, "path": path}))
            .collect();
        resp
    }

    /// As [`with_sub_issues`], for the `projectItems` connection.
    pub fn with_project_items_edge(
        resp: serde_json::Value,
        node_id: &str,
        edge: serde_json::Value,
    ) -> serde_json::Value {
        with_issue_edge(resp, node_id, "projectItems", edge)
    }

    fn with_issue_edge(
        mut resp: serde_json::Value,
        node_id: &str,
        field: &str,
        edge: serde_json::Value,
    ) -> serde_json::Value {
        let Some(repo) = resp
            .pointer_mut("/data/repository")
            .and_then(|v| v.as_object_mut())
        else {
            return resp;
        };
        for (_, alias) in repo.iter_mut() {
            let Some(nodes) = alias.get_mut("nodes").and_then(|v| v.as_array_mut()) else {
                continue;
            };
            for node in nodes {
                if node.get("id").and_then(|v| v.as_str()) == Some(node_id) {
                    node[field] = edge.clone();
                }
            }
        }
        resp
    }

    /// One issue as `repository.issues.nodes` carries it: REST's fields, with
    /// `labels` and `assignees` in the `nodes` connection GraphQL wraps them in,
    /// and the inline edge connections empty until a test attaches one.
    pub fn issue_node(issue: &GhIssue) -> serde_json::Value {
        serde_json::json!({
            "id": issue.id,
            "number": issue.number,
            "url": issue.url,
            "title": issue.title,
            "body": issue.body,
            "state": issue.state,
            "updatedAt": issue.updated_at,
            "createdAt": issue.created_at,
            "author": issue.author.as_ref().map(|a| serde_json::json!({"login": a.login})),
            "issueType": issue.issue_type.as_ref().map(|t| serde_json::json!({"name": t})),
            "milestone": issue.milestone.as_ref().map(|m| serde_json::json!({"number": m.number})),
            "labels": {"nodes": issue.labels.iter()
                .map(|l| serde_json::json!({"name": l.name}))
                .collect::<Vec<_>>()},
            "assignees": {"nodes": issue.assignees.iter()
                .map(|a| serde_json::json!({"login": a.login}))
                .collect::<Vec<_>>()},
            "subIssues": sub_issue_edge(&[]),
            "blockedBy": blocked_by_edge(&[]),
            "projectItems": project_items_edge(&[])
        })
    }

    /// What one fetch costs at the GitHub seams, counted rather than stubbed.
    ///
    /// Every trait a fetch reaches through is implemented here so one double can
    /// drive both surfaces -- `cli::fetch::run` and the TUI's `poll_sync` -- and
    /// they can be held to the same count. Reads answer with the configured
    /// round and nothing else: what is under test is how many requests a fetch
    /// makes, not what comes back.
    #[derive(Default)]
    pub struct GhRequestCounter {
        pub round_queries: RefCell<Vec<String>>,
        /// Every GraphQL document that was not a composed round. The point of
        /// the round is that this stays empty.
        pub other_queries: RefCell<Vec<String>>,
        pub milestone_list_calls: Cell<usize>,
        pub board_columns: Vec<(u64, Vec<String>)>,
        /// Whether the round answers with issues carrying sub-issue and
        /// blocked-by edges. See [`GhRequestCounter::with_enriched_issues`].
        enriched: bool,
    }

    impl GhRequestCounter {
        /// A counter whose round answers `board` with the given `Status` columns.
        pub fn with_board(number: u64, columns: &[&str]) -> Self {
            Self {
                board_columns: vec![(
                    number,
                    columns.iter().map(|c| c.to_string()).collect::<Vec<_>>(),
                )],
                ..Default::default()
            }
        }

        /// Answer every alias with a parent issue `#1`, its sub-issue child `#2`,
        /// a `blockedBy` edge from `#1` to `#2`, and membership of every
        /// configured board with its `Status` cell set -- so a fetch that still
        /// read any of that through a query of its own, or wrote a membership it
        /// wrongly read as missing, would land in `other_queries`.
        pub fn with_enriched_issues(mut self) -> Self {
            self.enriched = true;
            self
        }

        /// The `Status` cell the counter's boards answer with, which must be one
        /// of the columns the same board's schema declares.
        pub const COUNTED_STATUS: &'static str = "Review";

        fn round_project_items(&self) -> Vec<ProjectItem> {
            self.board_columns
                .iter()
                .map(|(number, _)| ProjectItem {
                    project_number: *number,
                    item_id: format!("PVTI_b{}", number),
                    fields: vec![ProjectFieldValue {
                        project_number: *number,
                        field_name: "Status".to_string(),
                        kind: GhFieldKind::SingleSelect,
                        value: GhFieldValueRepr::OptionName(Self::COUNTED_STATUS.to_string()),
                    }],
                })
                .collect()
        }

        fn round_issues(&self) -> Vec<GhIssue> {
            if !self.enriched {
                return Vec::new();
            }
            [1u64, 2]
                .iter()
                .map(|&number| counted_issue(number))
                .collect()
        }
    }

    fn counted_issue(number: u64) -> GhIssue {
        GhIssue {
            number,
            id: format!("I_{}", number),
            url: String::new(),
            title: format!("issue {}", number),
            body: String::new(),
            labels: Vec::new(),
            state: "OPEN".to_string(),
            updated_at: "2026-08-01T00:00:00Z".to_string(),
            created_at: "2026-08-01T00:00:00Z".to_string(),
            author: None,
            issue_type: None,
            milestone: None,
            assignees: Vec::new(),
        }
    }

    impl GhGraphql for GhRequestCounter {
        fn graphql(&self, query: &str, _vars: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
            if !crate::engine::gh_fetch::is_round_query(query) {
                self.other_queries.borrow_mut().push(query.to_string());
                bail!("GhRequestCounter answers composed rounds only");
            }
            self.round_queries.borrow_mut().push(query.to_string());
            let mut resp = with_issue_pages(
                query,
                round_response(&[], &[], &self.board_columns),
                &self.round_issues(),
            );
            if self.enriched {
                resp = with_sub_issues(resp, "I_1", sub_issue_edge(&["I_2"]));
                resp = with_blocked_by(resp, "I_1", blocked_by_edge(&[2]));
                let items = project_items_edge(&self.round_project_items());
                for issue in self.round_issues() {
                    resp = with_project_items_edge(resp, &issue.id, items.clone());
                }
            }
            Ok(resp)
        }

        fn project_items(&self, _repo: &str, _content_node_id: &str) -> Result<Vec<ProjectItem>> {
            Ok(Vec::new())
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

        fn clear_project_field(&self, _: &str, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
    }

    /// Both REST issue reads panic rather than count: a fetch resolves every
    /// type's list from the composed round, so reaching this seam at all is the
    /// regression, and the trait says so louder than an assertion downstream.
    impl GhIssueReader for GhRequestCounter {
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
            unreachable!("a fetch reads issues off the composed round, never REST")
        }

        fn issue_comments(&self, _repo: &str, _number: u64) -> Result<Vec<GhComment>> {
            Ok(Vec::new())
        }
    }

    impl GhMilestoneApi for GhRequestCounter {
        fn milestone_list(&self, _repo: &str) -> Result<Vec<GhMilestone>> {
            self.milestone_list_calls
                .set(self.milestone_list_calls.get() + 1);
            Ok(Vec::new())
        }

        fn milestone_view(&self, _repo: &str, _number: u64) -> Result<GhMilestone> {
            bail!("no milestone reads under test")
        }

        fn milestone_create(
            &self,
            _repo: &str,
            _title: &str,
            _description: &str,
            _due_on: Option<&str>,
            _state: &str,
        ) -> Result<GhMilestone> {
            bail!("no milestone writes under test")
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
            bail!("no milestone writes under test")
        }

        fn milestone_delete(&self, _repo: &str, _number: u64) -> Result<()> {
            bail!("no milestone writes under test")
        }

        fn issue_set_milestone(&self, _: &str, _: u64, _: Option<u64>) -> Result<()> {
            Ok(())
        }
    }

    impl GhIssueDependencyApi for GhRequestCounter {
        fn list_blocked_by(&self, _repo: &str, _blocked_number: u64) -> Result<Vec<u64>> {
            Ok(Vec::new())
        }
        fn add_blocked_by(&self, _repo: &str, _blocked: u64, _blocking: u64) -> Result<()> {
            Ok(())
        }
        fn remove_blocked_by(&self, _repo: &str, _blocked: u64, _blocking: u64) -> Result<()> {
            Ok(())
        }
    }

    /// Only here to satisfy the bound `cli::fetch::run` declares. A fetch writes
    /// nothing, so every method failing loudly is the assertion.
    impl GhIssueWriter for GhRequestCounter {
        fn issue_create(&self, _: &str, _: &str, _: &str, _: &[String]) -> Result<GhIssue> {
            bail!("a fetch must not write issues")
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
            bail!("a fetch must not write issues")
        }
        fn issue_close(&self, _: &str, _: u64) -> Result<()> {
            bail!("a fetch must not write issues")
        }
        fn issue_reopen(&self, _: &str, _: u64) -> Result<()> {
            bail!("a fetch must not write issues")
        }
        fn issue_set_assignee(&self, _: &str, _: u64, _: &[String], _: &[String]) -> Result<()> {
            bail!("a fetch must not write issues")
        }
        fn label_create(&self, _: &str, _: &str, _: &str, _: &str) -> Result<()> {
            bail!("a fetch must not write labels")
        }
        fn label_ensure(&self, _: &str, _: &str, _: &str, _: &str) -> Result<()> {
            bail!("a fetch must not write labels")
        }
    }

    pub struct MockGhClient {
        pub auth: AuthStatus,
        pub list_result: Vec<GhIssue>,
        pub view_issue: RefCell<Option<GhIssue>>,
        pub create_result: Option<GhIssue>,
        pub label_create_fail: bool,
        pub edit_fail: bool,
        pub closed: Cell<bool>,
        pub reopened: Cell<bool>,
        pub last_set_assignee: RefCell<Option<(Vec<String>, Vec<String>)>>,
        pub last_edit_title: RefCell<Option<String>>,
        pub last_edit_body: RefCell<Option<String>>,
        pub last_edit_labels_add: RefCell<Vec<String>>,
        pub last_edit_labels_remove: RefCell<Vec<String>>,
        pub last_ensure_label_names: RefCell<Vec<String>>,
        pub last_create_body: RefCell<Option<String>>,
        pub last_create_labels: RefCell<Vec<String>>,
        pub create_titles: RefCell<Vec<String>>,
        pub next_issue_number: Cell<u64>,
        pub graphql_responses: RefCell<Vec<serde_json::Value>>,
        pub graphql_calls: RefCell<Vec<GraphqlCall>>,
        /// What a composed fetch round resolves. Answered off to the side of
        /// `graphql_responses` so a test seeding a specific query's response
        /// does not have to account for the round the fetch path now runs.
        pub round_milestones: RefCell<Vec<GhMilestone>>,
        pub round_issue_types: RefCell<Vec<crate::engine::gh_schema::IssueTypeId>>,
        /// The `blockedBy` edges a composed round answers inline, per issue node
        /// id. Empty on every issue unless a test sets one.
        pub round_blocked_by: RefCell<Vec<(String, Vec<u64>)>>,
        pub view_comments: RefCell<Vec<GhComment>>,
        pub comments_call_count: Cell<usize>,
        pub project_items: RefCell<Vec<ProjectItem>>,
        pub project_field_calls: RefCell<Vec<String>>,
        pub field_updates: RefCell<Vec<(String, String, String, GhFieldValueInput)>>,
        pub field_clears: RefCell<Vec<(String, String, String)>>,
        /// A canned GraphQL response for project-field writes, handed to the same
        /// [`require_project_item`] check the graphql-backed clients apply -- so a
        /// test can drive an in-band GitHub rejection (HTTP 200 plus an `errors`
        /// array and no payload). `None`: the write succeeds.
        pub project_write_response: RefCell<Option<serde_json::Value>>,
    }

    impl Default for MockGhClient {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MockGhClient {
        pub fn new() -> Self {
            Self {
                auth: AuthStatus::Authenticated {
                    user: "testuser".to_string(),
                    host: "github.com".to_string(),
                },
                list_result: vec![],
                view_issue: RefCell::new(None),
                create_result: None,
                label_create_fail: false,
                edit_fail: false,
                closed: Cell::new(false),
                reopened: Cell::new(false),
                last_set_assignee: RefCell::new(None),
                last_edit_title: RefCell::new(None),
                last_edit_body: RefCell::new(None),
                last_edit_labels_add: RefCell::new(vec![]),
                last_edit_labels_remove: RefCell::new(vec![]),
                last_ensure_label_names: RefCell::new(vec![]),
                last_create_body: RefCell::new(None),
                last_create_labels: RefCell::new(vec![]),
                create_titles: RefCell::new(vec![]),
                next_issue_number: Cell::new(1),
                graphql_responses: RefCell::new(vec![]),
                graphql_calls: RefCell::new(vec![]),
                round_milestones: RefCell::new(vec![]),
                round_issue_types: RefCell::new(vec![]),
                round_blocked_by: RefCell::new(vec![]),
                view_comments: RefCell::new(vec![]),
                comments_call_count: Cell::new(0),
                project_items: RefCell::new(vec![]),
                project_field_calls: RefCell::new(vec![]),
                field_updates: RefCell::new(vec![]),
                field_clears: RefCell::new(vec![]),
                project_write_response: RefCell::new(None),
            }
        }

        /// Answer every project-field write with `resp` instead of an implicit
        /// success, so a test can reproduce what GitHub returns for a write it
        /// rejected.
        pub fn with_project_write_response(self, resp: serde_json::Value) -> Self {
            *self.project_write_response.borrow_mut() = Some(resp);
            self
        }

        /// Group loose field values into one item per board (in first-seen board
        /// order) with a synthetic item id, for tests that only care about the
        /// values and not about membership or item ids.
        pub fn with_project_field_values(mut self, values: Vec<ProjectFieldValue>) -> Self {
            let mut items: Vec<ProjectItem> = Vec::new();
            for value in values {
                match items
                    .iter_mut()
                    .find(|i| i.project_number == value.project_number)
                {
                    Some(item) => item.fields.push(value),
                    None => items.push(ProjectItem {
                        project_number: value.project_number,
                        item_id: format!("PVTI_{}", value.project_number),
                        fields: vec![value],
                    }),
                }
            }
            self.project_items = RefCell::new(items);
            self
        }

        pub fn with_project_items(mut self, items: Vec<ProjectItem>) -> Self {
            self.project_items = RefCell::new(items);
            self
        }

        pub fn with_comments(mut self, comments: Vec<GhComment>) -> Self {
            self.view_comments = RefCell::new(comments);
            self
        }

        pub fn with_graphql_responses(mut self, responses: Vec<serde_json::Value>) -> Self {
            self.graphql_responses = RefCell::new(responses);
            self
        }

        /// The milestones a composed fetch round resolves for this client.
        pub fn with_milestones(mut self, milestones: Vec<GhMilestone>) -> Self {
            self.round_milestones = RefCell::new(milestones);
            self
        }

        /// The org issue types a composed fetch round resolves for this client.
        pub fn with_issue_types(
            mut self,
            issue_types: Vec<crate::engine::gh_schema::IssueTypeId>,
        ) -> Self {
            self.round_issue_types = RefCell::new(issue_types);
            self
        }

        /// The numbers blocking the issue with node id `node_id`, as the round
        /// selects them inline on that issue.
        pub fn with_round_blocked_by(self, node_id: &str, blockers: Vec<u64>) -> Self {
            self.round_blocked_by
                .borrow_mut()
                .push((node_id.to_string(), blockers));
            self
        }

        pub fn with_auth(mut self, auth: AuthStatus) -> Self {
            self.auth = auth;
            self
        }

        pub fn with_list_result(mut self, issues: Vec<GhIssue>) -> Self {
            self.list_result = issues;
            self
        }

        pub fn with_view_issue(mut self, issue: GhIssue) -> Self {
            self.view_issue = RefCell::new(Some(issue));
            self
        }

        pub fn with_create_result(mut self, issue: GhIssue) -> Self {
            self.create_result = Some(issue);
            self
        }

        pub fn with_label_create_fail(mut self) -> Self {
            self.label_create_fail = true;
            self
        }

        pub fn with_edit_fail(mut self) -> Self {
            self.edit_fail = true;
            self
        }
    }

    impl GhIssueReader for MockGhClient {
        fn issue_list(
            &self,
            _repo: &str,
            _labels: &[String],
            _json_fields: &[String],
            _limit: Option<u64>,
        ) -> Result<Vec<GhIssue>> {
            Ok(self.list_result.clone())
        }

        fn issue_view(&self, _repo: &str, number: u64) -> Result<GhIssue> {
            if let Some(issue) = self.view_issue.borrow().as_ref() {
                return Ok(issue.clone());
            }
            Ok(GhIssue {
                number,
                id: format!("I_node{}", number),
                url: format!("https://github.com/test/repo/issues/{}", number),
                title: "Viewed issue".to_string(),
                body: String::new(),
                labels: vec![],
                state: "OPEN".to_string(),
                updated_at: String::new(),
                created_at: String::new(),
                author: None,
                issue_type: None,
                milestone: None,
                assignees: vec![],
            })
        }

        fn issue_comments(&self, _repo: &str, _number: u64) -> Result<Vec<GhComment>> {
            self.comments_call_count
                .set(self.comments_call_count.get() + 1);
            Ok(self.view_comments.borrow().clone())
        }
    }

    impl GhIssueWriter for MockGhClient {
        fn issue_create(
            &self,
            _repo: &str,
            title: &str,
            body: &str,
            labels: &[String],
        ) -> Result<GhIssue> {
            *self.last_create_body.borrow_mut() = Some(body.to_string());
            *self.last_create_labels.borrow_mut() = labels.to_vec();
            self.create_titles.borrow_mut().push(title.to_string());
            if let Some(ref issue) = self.create_result {
                return Ok(issue.clone());
            }
            let number = self.next_issue_number.get();
            self.next_issue_number.set(number + 1);
            Ok(GhIssue {
                number,
                id: format!("I_node{}", number),
                url: format!("https://github.com/test/repo/issues/{}", number),
                title: title.to_string(),
                body: body.to_string(),
                labels: labels
                    .iter()
                    .map(|l| GhLabel {
                        name: l.clone(),
                        color: String::new(),
                    })
                    .collect(),
                state: "OPEN".to_string(),
                updated_at: "2026-03-27T00:00:00Z".to_string(),
                created_at: String::new(),
                author: None,
                issue_type: None,
                milestone: None,
                assignees: vec![],
            })
        }

        fn issue_edit(
            &self,
            _repo: &str,
            _number: u64,
            title: Option<&str>,
            body: Option<&str>,
            labels_add: &[String],
            labels_remove: &[String],
        ) -> Result<()> {
            if self.edit_fail {
                bail!("simulated issue_edit failure");
            }
            *self.last_edit_title.borrow_mut() = title.map(|s| s.to_string());
            *self.last_edit_body.borrow_mut() = body.map(|s| s.to_string());
            *self.last_edit_labels_add.borrow_mut() = labels_add.to_vec();
            *self.last_edit_labels_remove.borrow_mut() = labels_remove.to_vec();
            Ok(())
        }

        fn issue_close(&self, _repo: &str, _number: u64) -> Result<()> {
            self.closed.set(true);
            Ok(())
        }

        fn issue_reopen(&self, _repo: &str, _number: u64) -> Result<()> {
            self.reopened.set(true);
            Ok(())
        }

        fn issue_set_assignee(
            &self,
            _repo: &str,
            _number: u64,
            add: &[String],
            remove: &[String],
        ) -> Result<()> {
            *self.last_set_assignee.borrow_mut() = Some((add.to_vec(), remove.to_vec()));
            Ok(())
        }

        fn label_create(
            &self,
            _repo: &str,
            _name: &str,
            _description: &str,
            _color: &str,
        ) -> Result<()> {
            if self.label_create_fail {
                bail!("label already exists");
            }
            Ok(())
        }

        fn label_ensure(
            &self,
            _repo: &str,
            name: &str,
            _description: &str,
            _color: &str,
        ) -> Result<()> {
            self.last_ensure_label_names
                .borrow_mut()
                .push(name.to_string());
            Ok(())
        }
    }

    impl GhAuth for MockGhClient {
        fn auth_status(&self) -> Result<AuthStatus> {
            Ok(self.auth.clone())
        }
    }

    /// No native dependencies: lets a `MockGhClient` double as the dependency
    /// reader on fetch paths that thread a single client. Dependency-specific
    /// behaviour is faked with [`MockGhDependencyClient`] instead.
    impl GhIssueDependencyApi for MockGhClient {
        fn list_blocked_by(&self, _repo: &str, _blocked_number: u64) -> Result<Vec<u64>> {
            Ok(vec![])
        }
        fn add_blocked_by(&self, _repo: &str, _blocked: u64, _blocking: u64) -> Result<()> {
            Ok(())
        }
        fn remove_blocked_by(&self, _repo: &str, _blocked: u64, _blocking: u64) -> Result<()> {
            Ok(())
        }
    }

    /// In-memory fake for [`GhMilestoneApi`]. create/edit mutate the backing vec
    /// so a subsequent view round-trips the change; `last_set_milestone` records
    /// the most recent issue->milestone association write. Zero network.
    pub struct MockGhMilestoneClient {
        pub milestones: RefCell<Vec<GhMilestone>>,
        pub next_number: Cell<u64>,
        pub last_set_milestone: RefCell<Option<(u64, Option<u64>)>>,
        pub last_edit: RefCell<Option<MilestoneEdit>>,
        pub create_calls: Cell<usize>,
    }

    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct MilestoneEdit {
        pub number: u64,
        pub title: Option<String>,
        pub description: Option<String>,
        pub due_on: Option<String>,
        pub state: Option<String>,
    }

    impl Default for MockGhMilestoneClient {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MockGhMilestoneClient {
        pub fn new() -> Self {
            Self {
                milestones: RefCell::new(vec![]),
                next_number: Cell::new(1),
                last_set_milestone: RefCell::new(None),
                last_edit: RefCell::new(None),
                create_calls: Cell::new(0),
            }
        }

        pub fn with_milestones(milestones: Vec<GhMilestone>) -> Self {
            let next = milestones.iter().map(|m| m.number).max().unwrap_or(0) + 1;
            let me = Self::new();
            *me.milestones.borrow_mut() = milestones;
            me.next_number.set(next);
            me
        }
    }

    impl GhMilestoneApi for MockGhMilestoneClient {
        fn milestone_list(&self, _repo: &str) -> Result<Vec<GhMilestone>> {
            Ok(self.milestones.borrow().clone())
        }

        fn milestone_view(&self, _repo: &str, number: u64) -> Result<GhMilestone> {
            self.milestones
                .borrow()
                .iter()
                .find(|m| m.number == number)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("milestone {} not found", number))
        }

        fn milestone_create(
            &self,
            _repo: &str,
            title: &str,
            description: &str,
            due_on: Option<&str>,
            state: &str,
        ) -> Result<GhMilestone> {
            self.create_calls.set(self.create_calls.get() + 1);
            let number = self.next_number.get();
            self.next_number.set(number + 1);
            let milestone = GhMilestone {
                number,
                title: title.to_string(),
                description: description.to_string(),
                due_on: due_on.map(|s| s.to_string()),
                state: state.to_string(),
                open_issues: 0,
                closed_issues: 0,
                url: format!("https://github.com/test/repo/milestone/{}", number),
            };
            self.milestones.borrow_mut().push(milestone.clone());
            Ok(milestone)
        }

        fn milestone_edit(
            &self,
            _repo: &str,
            number: u64,
            title: Option<&str>,
            description: Option<&str>,
            due_on: Option<&str>,
            state: Option<&str>,
        ) -> Result<GhMilestone> {
            *self.last_edit.borrow_mut() = Some(MilestoneEdit {
                number,
                title: title.map(|s| s.to_string()),
                description: description.map(|s| s.to_string()),
                due_on: due_on.map(|s| s.to_string()),
                state: state.map(|s| s.to_string()),
            });
            let mut milestones = self.milestones.borrow_mut();
            let m = milestones
                .iter_mut()
                .find(|m| m.number == number)
                .ok_or_else(|| anyhow::anyhow!("milestone {} not found", number))?;
            if let Some(t) = title {
                m.title = t.to_string();
            }
            if let Some(d) = description {
                m.description = d.to_string();
            }
            if let Some(d) = due_on {
                m.due_on = Some(d.to_string());
            }
            if let Some(s) = state {
                m.state = s.to_string();
            }
            Ok(m.clone())
        }

        fn milestone_delete(&self, _repo: &str, number: u64) -> Result<()> {
            self.milestones.borrow_mut().retain(|m| m.number != number);
            Ok(())
        }

        fn issue_set_milestone(
            &self,
            _repo: &str,
            issue_number: u64,
            milestone: Option<u64>,
        ) -> Result<()> {
            *self.last_set_milestone.borrow_mut() = Some((issue_number, milestone));
            Ok(())
        }
    }

    /// Delegating impl so a shared `Rc<MockGhMilestoneClient>` can be moved into
    /// an `FnOnce` factory while the original handle remains inspectable after.
    impl GhMilestoneApi for std::rc::Rc<MockGhMilestoneClient> {
        fn milestone_list(&self, repo: &str) -> Result<Vec<GhMilestone>> {
            (**self).milestone_list(repo)
        }
        fn milestone_view(&self, repo: &str, number: u64) -> Result<GhMilestone> {
            (**self).milestone_view(repo, number)
        }
        fn milestone_create(
            &self,
            repo: &str,
            title: &str,
            description: &str,
            due_on: Option<&str>,
            state: &str,
        ) -> Result<GhMilestone> {
            (**self).milestone_create(repo, title, description, due_on, state)
        }
        fn milestone_edit(
            &self,
            repo: &str,
            number: u64,
            title: Option<&str>,
            description: Option<&str>,
            due_on: Option<&str>,
            state: Option<&str>,
        ) -> Result<GhMilestone> {
            (**self).milestone_edit(repo, number, title, description, due_on, state)
        }
        fn milestone_delete(&self, repo: &str, number: u64) -> Result<()> {
            (**self).milestone_delete(repo, number)
        }
        fn issue_set_milestone(
            &self,
            repo: &str,
            issue_number: u64,
            milestone: Option<u64>,
        ) -> Result<()> {
            (**self).issue_set_milestone(repo, issue_number, milestone)
        }
    }

    /// In-memory fake for [`GhIssueDependencyApi`]. Records each add/remove as a
    /// `(blocked_number, blocking_number)` pair so a test can assert the native
    /// edge (and its direction) without touching GitHub. `blocked_by` holds the
    /// canned read-back set: `blocked_by[n]` is the list of issue numbers that
    /// block issue `n` (returned by `list_blocked_by`). Zero network.
    #[derive(Default)]
    pub struct MockGhDependencyClient {
        pub blocked_by: RefCell<std::collections::HashMap<u64, Vec<u64>>>,
        pub added: RefCell<Vec<(u64, u64)>>,
        pub removed: RefCell<Vec<(u64, u64)>>,
    }

    impl MockGhDependencyClient {
        pub fn new() -> Self {
            Self::default()
        }

        /// Seed the canned read-back set: issue `blocked_number` is blocked by
        /// each number in `blocking_numbers`.
        pub fn with_blocked_by(self, blocked_number: u64, blocking_numbers: Vec<u64>) -> Self {
            self.blocked_by
                .borrow_mut()
                .insert(blocked_number, blocking_numbers);
            self
        }
    }

    impl GhIssueDependencyApi for MockGhDependencyClient {
        fn list_blocked_by(&self, _repo: &str, blocked_number: u64) -> Result<Vec<u64>> {
            Ok(self
                .blocked_by
                .borrow()
                .get(&blocked_number)
                .cloned()
                .unwrap_or_default())
        }

        fn add_blocked_by(
            &self,
            _repo: &str,
            blocked_number: u64,
            blocking_number: u64,
        ) -> Result<()> {
            self.added
                .borrow_mut()
                .push((blocked_number, blocking_number));
            Ok(())
        }

        fn remove_blocked_by(
            &self,
            _repo: &str,
            blocked_number: u64,
            blocking_number: u64,
        ) -> Result<()> {
            self.removed
                .borrow_mut()
                .push((blocked_number, blocking_number));
            Ok(())
        }
    }

    /// Delegating impl so a shared `Rc<MockGhDependencyClient>` can be moved into
    /// an `FnOnce` factory while the original handle remains inspectable after
    /// (mirrors the milestone-client Rc impl).
    impl GhIssueDependencyApi for std::rc::Rc<MockGhDependencyClient> {
        fn list_blocked_by(&self, repo: &str, blocked_number: u64) -> Result<Vec<u64>> {
            (**self).list_blocked_by(repo, blocked_number)
        }

        fn add_blocked_by(
            &self,
            repo: &str,
            blocked_number: u64,
            blocking_number: u64,
        ) -> Result<()> {
            (**self).add_blocked_by(repo, blocked_number, blocking_number)
        }

        fn remove_blocked_by(
            &self,
            repo: &str,
            blocked_number: u64,
            blocking_number: u64,
        ) -> Result<()> {
            (**self).remove_blocked_by(repo, blocked_number, blocking_number)
        }
    }

    /// Delegating impls so a shared `Rc<MockGhClient>` can be moved into an
    /// `FnOnce` factory while the original handle remains inspectable after
    /// (mirrors the milestone-client Rc impl). The issue reader/writer impls let
    /// ordinary-relation tests inspect the recorded `issue_edit` body once the
    /// client has been consumed by `GithubIssuesStore`.
    impl GhIssueReader for std::rc::Rc<MockGhClient> {
        fn issue_list(
            &self,
            repo: &str,
            labels: &[String],
            json_fields: &[String],
            limit: Option<u64>,
        ) -> Result<Vec<GhIssue>> {
            (**self).issue_list(repo, labels, json_fields, limit)
        }
        fn issue_view(&self, repo: &str, number: u64) -> Result<GhIssue> {
            (**self).issue_view(repo, number)
        }
        fn issue_comments(&self, repo: &str, number: u64) -> Result<Vec<GhComment>> {
            (**self).issue_comments(repo, number)
        }
    }

    impl GhIssueWriter for std::rc::Rc<MockGhClient> {
        fn issue_create(
            &self,
            repo: &str,
            title: &str,
            body: &str,
            labels: &[String],
        ) -> Result<GhIssue> {
            (**self).issue_create(repo, title, body, labels)
        }
        fn issue_edit(
            &self,
            repo: &str,
            number: u64,
            title: Option<&str>,
            body: Option<&str>,
            labels_add: &[String],
            labels_remove: &[String],
        ) -> Result<()> {
            (**self).issue_edit(repo, number, title, body, labels_add, labels_remove)
        }
        fn issue_close(&self, repo: &str, number: u64) -> Result<()> {
            (**self).issue_close(repo, number)
        }
        fn issue_reopen(&self, repo: &str, number: u64) -> Result<()> {
            (**self).issue_reopen(repo, number)
        }
        fn issue_set_assignee(
            &self,
            repo: &str,
            number: u64,
            add: &[String],
            remove: &[String],
        ) -> Result<()> {
            (**self).issue_set_assignee(repo, number, add, remove)
        }
        fn label_create(
            &self,
            repo: &str,
            name: &str,
            description: &str,
            color: &str,
        ) -> Result<()> {
            (**self).label_create(repo, name, description, color)
        }
        fn label_ensure(
            &self,
            repo: &str,
            name: &str,
            description: &str,
            color: &str,
        ) -> Result<()> {
            (**self).label_ensure(repo, name, description, color)
        }
    }

    impl GhGraphql for std::rc::Rc<MockGhClient> {
        fn graphql(&self, query: &str, vars: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
            (**self).graphql(query, vars)
        }

        fn project_items(&self, repo: &str, content_node_id: &str) -> Result<Vec<ProjectItem>> {
            (**self).project_items(repo, content_node_id)
        }

        fn update_project_v2_item_field_value(
            &self,
            project_id: &str,
            item_id: &str,
            field_id: &str,
            value: &GhFieldValueInput,
        ) -> Result<()> {
            (**self).update_project_v2_item_field_value(project_id, item_id, field_id, value)
        }

        fn clear_project_field(
            &self,
            project_id: &str,
            item_id: &str,
            field_id: &str,
        ) -> Result<()> {
            (**self).clear_project_field(project_id, item_id, field_id)
        }
    }

    // Delegating impls over `Arc<Mutex<MockGhClient>>` so a shared handle can be
    // moved into an `FnOnce` factory that produces a `Send` client (required by
    // `GhClient`, since `GithubIssuesStore` is shared across threads by the TUI),
    // while the original handle stays inspectable after. `Rc` cannot be used here
    // because it is not `Send`; `Mutex` also makes the handle `Sync`.
    impl GhIssueReader for std::sync::Arc<std::sync::Mutex<MockGhClient>> {
        fn issue_list(
            &self,
            repo: &str,
            labels: &[String],
            json_fields: &[String],
            limit: Option<u64>,
        ) -> Result<Vec<GhIssue>> {
            self.lock()
                .unwrap()
                .issue_list(repo, labels, json_fields, limit)
        }
        fn issue_view(&self, repo: &str, number: u64) -> Result<GhIssue> {
            self.lock().unwrap().issue_view(repo, number)
        }
        fn issue_comments(&self, repo: &str, number: u64) -> Result<Vec<GhComment>> {
            self.lock().unwrap().issue_comments(repo, number)
        }
    }

    impl GhIssueWriter for std::sync::Arc<std::sync::Mutex<MockGhClient>> {
        fn issue_create(
            &self,
            repo: &str,
            title: &str,
            body: &str,
            labels: &[String],
        ) -> Result<GhIssue> {
            self.lock().unwrap().issue_create(repo, title, body, labels)
        }
        fn issue_edit(
            &self,
            repo: &str,
            number: u64,
            title: Option<&str>,
            body: Option<&str>,
            labels_add: &[String],
            labels_remove: &[String],
        ) -> Result<()> {
            self.lock()
                .unwrap()
                .issue_edit(repo, number, title, body, labels_add, labels_remove)
        }
        fn issue_close(&self, repo: &str, number: u64) -> Result<()> {
            self.lock().unwrap().issue_close(repo, number)
        }
        fn issue_reopen(&self, repo: &str, number: u64) -> Result<()> {
            self.lock().unwrap().issue_reopen(repo, number)
        }
        fn issue_set_assignee(
            &self,
            repo: &str,
            number: u64,
            add: &[String],
            remove: &[String],
        ) -> Result<()> {
            self.lock()
                .unwrap()
                .issue_set_assignee(repo, number, add, remove)
        }
        fn label_create(
            &self,
            repo: &str,
            name: &str,
            description: &str,
            color: &str,
        ) -> Result<()> {
            self.lock()
                .unwrap()
                .label_create(repo, name, description, color)
        }
        fn label_ensure(
            &self,
            repo: &str,
            name: &str,
            description: &str,
            color: &str,
        ) -> Result<()> {
            self.lock()
                .unwrap()
                .label_ensure(repo, name, description, color)
        }
    }

    impl GhGraphql for std::sync::Arc<std::sync::Mutex<MockGhClient>> {
        fn graphql(&self, query: &str, vars: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
            self.lock().unwrap().graphql(query, vars)
        }
        fn project_items(&self, repo: &str, content_node_id: &str) -> Result<Vec<ProjectItem>> {
            self.lock().unwrap().project_items(repo, content_node_id)
        }
        fn update_project_v2_item_field_value(
            &self,
            project_id: &str,
            item_id: &str,
            field_id: &str,
            value: &GhFieldValueInput,
        ) -> Result<()> {
            self.lock()
                .unwrap()
                .update_project_v2_item_field_value(project_id, item_id, field_id, value)
        }
        fn clear_project_field(
            &self,
            project_id: &str,
            item_id: &str,
            field_id: &str,
        ) -> Result<()> {
            self.lock()
                .unwrap()
                .clear_project_field(project_id, item_id, field_id)
        }
    }

    impl GhGraphql for MockGhClient {
        fn graphql(&self, query: &str, vars: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
            let recorded = vars
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect();
            self.graphql_calls
                .borrow_mut()
                .push((query.to_string(), recorded));

            if crate::engine::gh_fetch::is_round_query(query) {
                let mut resp = with_issue_pages(
                    query,
                    round_response(
                        &self.round_milestones.borrow(),
                        &self.round_issue_types.borrow(),
                        &[],
                    ),
                    &self.list_result,
                );
                for (node_id, blockers) in self.round_blocked_by.borrow().iter() {
                    resp = with_blocked_by(resp, node_id, blocked_by_edge(blockers));
                }
                // Board memberships ride the round now, so the items this mock
                // was given answer there rather than through `project_items`.
                // Every issue carries the same set: a test that wants them apart
                // reaches for `with_project_items_edge`.
                let items = self.project_items.borrow();
                if !items.is_empty() {
                    for issue in &self.list_result {
                        resp = with_project_items_edge(resp, &issue.id, project_items_edge(&items));
                    }
                }
                return Ok(resp);
            }

            let mut responses = self.graphql_responses.borrow_mut();
            if responses.is_empty() {
                bail!("no canned graphql response available");
            }
            Ok(responses.remove(0))
        }

        fn project_items(&self, _repo: &str, content_node_id: &str) -> Result<Vec<ProjectItem>> {
            self.project_field_calls
                .borrow_mut()
                .push(content_node_id.to_string());
            Ok(self.project_items.borrow().clone())
        }

        fn update_project_v2_item_field_value(
            &self,
            project_id: &str,
            item_id: &str,
            field_id: &str,
            value: &GhFieldValueInput,
        ) -> Result<()> {
            self.field_updates.borrow_mut().push((
                project_id.to_string(),
                item_id.to_string(),
                field_id.to_string(),
                value.clone(),
            ));
            match self.project_write_response.borrow().as_ref() {
                Some(resp) => require_project_item(resp, "updateProjectV2ItemFieldValue"),
                None => Ok(()),
            }
        }

        fn clear_project_field(
            &self,
            project_id: &str,
            item_id: &str,
            field_id: &str,
        ) -> Result<()> {
            self.field_clears.borrow_mut().push((
                project_id.to_string(),
                item_id.to_string(),
                field_id.to_string(),
            ));
            match self.project_write_response.borrow().as_ref() {
                Some(resp) => require_project_item(resp, "clearProjectV2ItemFieldValue"),
                None => Ok(()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{MockGhClient, MockGhDependencyClient, MockGhMilestoneClient};
    use super::*;

    // --- JSON parsing tests ---

    #[test]
    fn parse_single_issue() {
        let json = r#"{
            "number": 42,
            "url": "https://github.com/owner/repo/issues/42",
            "title": "Test issue",
            "body": "Some body text",
            "labels": [{"name": "bug", "color": "d73a4a"}],
            "state": "OPEN",
            "updatedAt": "2026-03-27T00:00:00Z"
        }"#;

        let issue = parse_issue_json(json).unwrap();
        assert_eq!(issue.number, 42);
        assert_eq!(issue.url, "https://github.com/owner/repo/issues/42");
        assert_eq!(issue.title, "Test issue");
        assert_eq!(issue.body, "Some body text");
        assert_eq!(issue.labels.len(), 1);
        assert_eq!(issue.labels[0].name, "bug");
        assert_eq!(issue.labels[0].color, "d73a4a");
        assert_eq!(issue.state, "OPEN");
        assert_eq!(issue.updated_at, "2026-03-27T00:00:00Z");
    }

    #[test]
    fn parse_issue_list() {
        let json = r#"[
            {"number": 1, "title": "First"},
            {"number": 2, "title": "Second"}
        ]"#;

        let issues = parse_issue_list_json(json).unwrap();
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].number, 1);
        assert_eq!(issues[0].title, "First");
        assert_eq!(issues[1].number, 2);
    }

    #[test]
    fn parse_empty_list() {
        let issues = parse_issue_list_json("[]").unwrap();
        assert!(issues.is_empty());
    }

    #[test]
    fn parse_issue_json_with_author() {
        let json = r#"{
            "number": 5,
            "title": "Authored issue",
            "author": {"login": "jkaloger"}
        }"#;

        let issue = parse_issue_json(json).unwrap();
        assert_eq!(
            issue.author,
            Some(GhAuthor {
                login: "jkaloger".to_string()
            })
        );
    }

    #[test]
    fn parse_partial_json_fields() {
        let json = r#"{"number": 10, "title": "Partial"}"#;
        let issue = parse_issue_json(json).unwrap();
        assert_eq!(issue.number, 10);
        assert_eq!(issue.title, "Partial");
        assert_eq!(issue.url, "");
        assert_eq!(issue.body, "");
        assert!(issue.labels.is_empty());
        assert_eq!(issue.state, "");
        assert_eq!(issue.updated_at, "");
    }

    // --- comment parsing tests ---

    #[test]
    fn parse_comments_rest_shape() {
        let json = r#"[
            {"user": {"login": "alice"}, "body": "first", "created_at": "2026-06-01T00:00:00Z"},
            {"user": {"login": "bob"}, "body": "second", "created_at": "2026-06-02T00:00:00Z"}
        ]"#;
        let comments = parse_comments_json(json).unwrap();
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].author, "alice");
        assert_eq!(comments[0].body, "first");
        assert_eq!(comments[0].timestamp, "2026-06-01T00:00:00Z");
        assert_eq!(comments[1].author, "bob");
    }

    #[test]
    fn parse_comments_empty_list() {
        assert!(parse_comments_json("[]").unwrap().is_empty());
    }

    #[test]
    fn mock_issue_comments_returns_canned_and_counts() {
        let c = GhComment {
            author: "alice".to_string(),
            body: "hi".to_string(),
            timestamp: "2026-06-01T00:00:00Z".to_string(),
        };
        let client = MockGhClient::new().with_comments(vec![c.clone()]);
        assert_eq!(client.comments_call_count.get(), 0);
        let got = client.issue_comments("owner/repo", 1).unwrap();
        assert_eq!(got, vec![c]);
        assert_eq!(client.comments_call_count.get(), 1);
    }

    // --- type_label tests ---

    #[test]
    fn type_label_format() {
        assert_eq!(type_label("RFC"), "lazyspec:RFC");
        assert_eq!(type_label("ADR"), "lazyspec:ADR");
        assert_eq!(type_label("story"), "lazyspec:story");
    }

    // --- deterministic_color tests ---

    #[test]
    fn deterministic_color_stability() {
        let c1 = deterministic_color("RFC");
        let c2 = deterministic_color("RFC");
        assert_eq!(c1, c2);
        assert_eq!(c1.len(), 6);
        assert!(c1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn deterministic_color_varies_by_input() {
        let c1 = deterministic_color("RFC");
        let c2 = deterministic_color("ADR");
        assert_ne!(c1, c2);
    }

    // --- Mock-based tests ---

    #[test]
    fn mock_issue_create() {
        let client = MockGhClient::new();
        let issue = client
            .issue_create("owner/repo", "title", "body", &["bug".to_string()])
            .unwrap();
        assert_eq!(issue.number, 1);
        assert_eq!(issue.title, "title");
        assert_eq!(issue.labels[0].name, "bug");
    }

    #[test]
    fn mock_issue_list_empty() {
        let client = MockGhClient::new();
        let issues = client.issue_list("owner/repo", &[], &[], None).unwrap();
        assert!(issues.is_empty());
    }

    #[test]
    fn mock_issue_list_with_results() {
        let client = MockGhClient::new().with_list_result(vec![
            GhIssue {
                number: 1,
                id: String::new(),
                url: String::new(),
                title: "First".to_string(),
                body: String::new(),
                labels: vec![],
                state: "OPEN".to_string(),
                updated_at: String::new(),
                created_at: String::new(),
                author: None,
                issue_type: None,
                milestone: None,
                assignees: vec![],
            },
            GhIssue {
                number: 2,
                id: String::new(),
                url: String::new(),
                title: "Second".to_string(),
                body: String::new(),
                labels: vec![],
                state: "OPEN".to_string(),
                updated_at: String::new(),
                created_at: String::new(),
                author: None,
                issue_type: None,
                milestone: None,
                assignees: vec![],
            },
        ]);
        let issues = client.issue_list("owner/repo", &[], &[], None).unwrap();
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn mock_label_ensure_succeeds_on_existing() {
        let client = MockGhClient::new().with_label_create_fail();

        // label_create fails
        assert!(client
            .label_create("owner/repo", "bug", "desc", "ff0000")
            .is_err());
        // label_ensure still succeeds
        assert!(client
            .label_ensure("owner/repo", "bug", "desc", "ff0000")
            .is_ok());
    }

    #[test]
    fn mock_auth_status() {
        let client = MockGhClient::new();
        let status = client.auth_status().unwrap();
        assert_eq!(
            status,
            AuthStatus::Authenticated {
                user: "testuser".to_string(),
                host: "github.com".to_string(),
            }
        );
    }

    #[test]
    fn mock_issue_view() {
        let client = MockGhClient::new();
        let issue = client.issue_view("owner/repo", 42).unwrap();
        assert_eq!(issue.number, 42);
    }

    #[test]
    fn mock_issue_edit() {
        let client = MockGhClient::new();
        let result = client.issue_edit(
            "owner/repo",
            42,
            None,
            Some("updated body"),
            &["new-label".to_string()],
            &["old-label".to_string()],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn mock_issue_close_reopen() {
        let client = MockGhClient::new();
        assert!(client.issue_close("owner/repo", 1).is_ok());
        assert!(client.issue_reopen("owner/repo", 1).is_ok());
    }

    // --- parse_issue_number_from_url tests ---

    #[test]
    fn parse_issue_number_from_valid_url() {
        let num = parse_issue_number_from_url("https://github.com/owner/repo/issues/42").unwrap();
        assert_eq!(num, 42);
    }

    #[test]
    fn parse_issue_number_from_url_with_trailing_newline() {
        let num = parse_issue_number_from_url("https://github.com/owner/repo/issues/99\n").unwrap();
        assert_eq!(num, 99);
    }

    #[test]
    fn parse_issue_number_from_invalid_url() {
        let result = parse_issue_number_from_url("not-a-url");
        assert!(result.is_err());
    }

    // --- classify_gh_error tests ---

    #[test]
    fn classify_rate_limit_error() {
        let err = classify_gh_error("API rate limit exceeded for user");
        assert!(matches!(err, GhError::RateLimited { retry_after: None }));
    }

    #[test]
    fn classify_rate_limit_with_retry_after() {
        let err = classify_gh_error("API rate limit exceeded. Retry after 60s");
        match err {
            GhError::RateLimited { retry_after } => assert_eq!(retry_after, Some(60)),
            other => panic!("expected RateLimited, got {:?}", other),
        }
    }

    #[test]
    fn classify_auth_failure() {
        let err = classify_gh_error("not logged in to any github hosts");
        assert!(matches!(err, GhError::AuthFailed(_)));
    }

    #[test]
    fn classify_auth_token_error() {
        let err = classify_gh_error("auth token not found");
        assert!(matches!(err, GhError::AuthFailed(_)));
    }

    #[test]
    fn classify_api_error_with_http_status() {
        let err = classify_gh_error("HTTP 404: Not Found");
        match err {
            GhError::ApiError { status, message } => {
                assert_eq!(status, 404);
                assert_eq!(message, "HTTP 404: Not Found");
            }
            other => panic!("expected ApiError, got {:?}", other),
        }
    }

    #[test]
    fn classify_api_error_with_422() {
        let err = classify_gh_error("422 Validation Failed");
        match err {
            GhError::ApiError { status, .. } => assert_eq!(status, 422),
            other => panic!("expected ApiError, got {:?}", other),
        }
    }

    #[test]
    fn classify_unknown_error_as_api_error() {
        let err = classify_gh_error("something went wrong");
        match err {
            GhError::ApiError { status, message } => {
                assert_eq!(status, 0);
                assert_eq!(message, "something went wrong");
            }
            other => panic!("expected ApiError with status 0, got {:?}", other),
        }
    }

    #[test]
    fn gh_error_display_variants() {
        let not_installed = GhError::NotInstalled;
        assert_eq!(format!("{}", not_installed), "gh CLI is not installed");

        let auth = GhError::AuthFailed("bad token".to_string());
        assert_eq!(format!("{}", auth), "gh auth failed: bad token");

        let api = GhError::ApiError {
            status: 404,
            message: "not found".to_string(),
        };
        assert_eq!(format!("{}", api), "gh API error (HTTP 404): not found");

        let rate = GhError::RateLimited {
            retry_after: Some(30),
        };
        assert_eq!(format!("{}", rate), "gh API rate limited, retry after 30s");

        let rate_none = GhError::RateLimited { retry_after: None };
        assert_eq!(format!("{}", rate_none), "gh API rate limited");
    }

    // --- GraphQL argv builder tests (AC1, AC3) ---

    #[test]
    fn build_graphql_args_has_api_graphql_query_prefix() {
        let args = build_graphql_args("query { viewer { login } }", &[]);
        assert_eq!(&args[0], "api");
        assert_eq!(&args[1], "graphql");
        assert_eq!(&args[2], "-f");
        assert_eq!(&args[3], "query=query { viewer { login } }");
    }

    #[test]
    fn build_graphql_args_string_var_uses_dash_f() {
        let args = build_graphql_args("q", &[("owner", GqlVar::Str("foo".to_string()))]);
        let pos = args.iter().position(|a| a == "owner=foo").unwrap();
        assert_eq!(args[pos - 1], "-f");
    }

    #[test]
    fn build_graphql_args_typed_vars_use_dash_capital_f() {
        let args = build_graphql_args(
            "q",
            &[("number", GqlVar::Int(5)), ("flag", GqlVar::Bool(true))],
        );
        let num_pos = args.iter().position(|a| a == "number=5").unwrap();
        assert_eq!(args[num_pos - 1], "-F");
        let flag_pos = args.iter().position(|a| a == "flag=true").unwrap();
        assert_eq!(args[flag_pos - 1], "-F");
    }

    #[test]
    fn build_graphql_args_never_emits_variables_blob() {
        let args = build_graphql_args(
            "q",
            &[
                ("owner", GqlVar::Str("foo".to_string())),
                ("number", GqlVar::Int(5)),
                ("flag", GqlVar::Bool(true)),
            ],
        );
        assert!(
            !args.iter().any(|a| a.contains("variables=")),
            "must not serialize a variables JSON blob: {:?}",
            args
        );
    }

    // --- Mock graphql seam tests (AC2) ---

    #[test]
    fn mock_graphql_returns_canned_response_and_records_call() {
        let client = MockGhClient::new()
            .with_graphql_responses(vec![serde_json::json!({"data": {"ok": true}})]);
        let result = client
            .graphql("query { x }", &[("owner", GqlVar::Str("foo".to_string()))])
            .unwrap();
        assert_eq!(result["data"]["ok"], serde_json::json!(true));

        let calls = client.graphql_calls.borrow();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "query { x }");
        assert_eq!(
            calls[0].1,
            vec![("owner".to_string(), GqlVar::Str("foo".to_string()))]
        );
    }

    // --- Milestone parsing + argv tests ---

    #[test]
    fn parse_single_milestone_rest_fields() {
        let json = r#"{
            "number": 3,
            "title": "v1.0",
            "description": "first release",
            "due_on": "2026-09-01T00:00:00Z",
            "state": "open",
            "open_issues": 7,
            "closed_issues": 3,
            "html_url": "https://github.com/o/r/milestone/3"
        }"#;
        let m = parse_milestone_json(json).unwrap();
        assert_eq!(m.number, 3);
        assert_eq!(m.title, "v1.0");
        assert_eq!(m.description, "first release");
        assert_eq!(m.due_on.as_deref(), Some("2026-09-01T00:00:00Z"));
        assert_eq!(m.state, "open");
        assert_eq!(m.open_issues, 7);
        assert_eq!(m.closed_issues, 3);
        assert_eq!(m.url, "https://github.com/o/r/milestone/3");
    }

    #[test]
    fn parse_milestone_list_and_null_due_on() {
        let json = r#"[
            {"number": 1, "title": "a", "due_on": null, "state": "closed"},
            {"number": 2, "title": "b", "state": "open"}
        ]"#;
        let list = parse_milestone_list_json(json).unwrap();
        assert_eq!(list.len(), 2);
        assert!(list[0].due_on.is_none());
        assert_eq!(list[0].state, "closed");
    }

    // AC4 (real-client edge): clearing the milestone emits `-F milestone=null`
    // (a JSON null), not the string "null"; -F is required so gh sends raw JSON.
    #[test]
    fn build_set_milestone_args_none_emits_json_null() {
        let args = build_set_milestone_args("o/r", 12, None);
        let pos = args.iter().position(|a| a == "milestone=null").unwrap();
        assert_eq!(args[pos - 1], "-F", "must use -F so value is raw JSON null");
        assert!(!args.iter().any(|a| a == "-f"), "must not use -f for null");
        assert!(args.contains(&"PATCH".to_string()));
        assert!(args.contains(&"repos/o/r/issues/12".to_string()));
    }

    #[test]
    fn build_set_milestone_args_some_emits_typed_int() {
        let args = build_set_milestone_args("o/r", 12, Some(5));
        let pos = args.iter().position(|a| a == "milestone=5").unwrap();
        assert_eq!(args[pos - 1], "-F", "must use -F so value is a typed int");
    }

    // --- Issue-dependency argv builders (STORY-244 AC1/AC2) ---

    // The add POST targets the blocked issue's number in the path and carries
    // the blocking issue's REST database id in the body as a typed int (`-F`),
    // never its display number.
    #[test]
    fn build_add_blocked_by_args_posts_issue_id_as_typed_int() {
        let args = build_add_blocked_by_args("o/r", 12, 9876543);
        assert!(args.contains(&"POST".to_string()));
        assert!(args.contains(&"repos/o/r/issues/12/dependencies/blocked_by".to_string()));
        let pos = args.iter().position(|a| a == "issue_id=9876543").unwrap();
        assert_eq!(
            args[pos - 1],
            "-F",
            "issue_id must be sent with -F as a typed JSON int"
        );
        assert!(
            !args.iter().any(|a| a == "-f"),
            "must not use -f (would send issue_id as a string)"
        );
    }

    // The remove DELETE puts the blocking issue's database id in the path, not a
    // body field.
    #[test]
    fn build_remove_blocked_by_args_deletes_with_id_path_segment() {
        let args = build_remove_blocked_by_args("o/r", 12, 9876543);
        assert!(args.contains(&"DELETE".to_string()));
        assert!(
            args.contains(&"repos/o/r/issues/12/dependencies/blocked_by/9876543".to_string()),
            "database id must be the final path segment, got: {:?}",
            args
        );
        assert!(
            !args.iter().any(|a| a.starts_with("issue_id=")),
            "delete carries no body field"
        );
    }

    #[test]
    fn mock_dependency_records_add_and_remove_pairs() {
        let client = MockGhDependencyClient::new();
        client.add_blocked_by("o/r", 12, 7).unwrap();
        client.remove_blocked_by("o/r", 12, 7).unwrap();
        assert_eq!(*client.added.borrow(), vec![(12, 7)]);
        assert_eq!(*client.removed.borrow(), vec![(12, 7)]);
    }

    // --- Issue-dependency read-back (STORY-244 AC3/AC6) ---

    // The list GET targets the blocked issue's number under the blocked_by
    // collection, with no method flag (a plain read).
    #[test]
    fn build_list_blocked_by_args_gets_the_blocked_by_collection() {
        let args = build_list_blocked_by_args("o/r", 12);
        assert_eq!(
            args,
            vec![
                "api".to_string(),
                "repos/o/r/issues/12/dependencies/blocked_by".to_string(),
            ]
        );
    }

    // The response is an array of issue objects; only their `number` is read.
    #[test]
    fn parse_blocked_by_numbers_extracts_numbers_ignoring_other_fields() {
        let json = r#"[
            {"number": 7, "title": "blocker", "state": "open"},
            {"number": 9, "title": "other blocker", "state": "closed"}
        ]"#;
        assert_eq!(parse_blocked_by_numbers(json).unwrap(), vec![7, 9]);
    }

    #[test]
    fn parse_blocked_by_numbers_empty_array_is_empty() {
        assert_eq!(parse_blocked_by_numbers("[]").unwrap(), Vec::<u64>::new());
    }

    #[test]
    fn mock_dependency_returns_canned_blocked_by_set() {
        let client = MockGhDependencyClient::new().with_blocked_by(12, vec![7]);
        assert_eq!(client.list_blocked_by("o/r", 12).unwrap(), vec![7]);
        // An issue with no seeded blockers reads back empty, never errors.
        assert_eq!(
            client.list_blocked_by("o/r", 99).unwrap(),
            Vec::<u64>::new()
        );
    }

    #[test]
    fn mock_milestone_create_then_view_round_trips() {
        let client = MockGhMilestoneClient::new();
        let created = client
            .milestone_create("o/r", "v1", "desc", Some("2026-09-01T00:00:00Z"), "open")
            .unwrap();
        assert_eq!(created.number, 1);
        let viewed = client.milestone_view("o/r", 1).unwrap();
        assert_eq!(viewed.title, "v1");
        assert_eq!(viewed.due_on.as_deref(), Some("2026-09-01T00:00:00Z"));
    }

    #[test]
    fn mock_milestone_edit_mutates_backing_vec() {
        let client = MockGhMilestoneClient::new();
        client
            .milestone_create("o/r", "v1", "d", None, "open")
            .unwrap();
        client
            .milestone_edit(
                "o/r",
                1,
                Some("v2"),
                None,
                Some("2027-01-01T00:00:00Z"),
                None,
            )
            .unwrap();
        let viewed = client.milestone_view("o/r", 1).unwrap();
        assert_eq!(viewed.title, "v2");
        assert_eq!(viewed.due_on.as_deref(), Some("2027-01-01T00:00:00Z"));
        let edit = client.last_edit.borrow();
        assert_eq!(edit.as_ref().unwrap().title.as_deref(), Some("v2"));
    }

    #[test]
    fn mock_issue_set_milestone_records_call() {
        let client = MockGhMilestoneClient::new();
        client.issue_set_milestone("o/r", 9, Some(2)).unwrap();
        assert_eq!(*client.last_set_milestone.borrow(), Some((9, Some(2))));
        client.issue_set_milestone("o/r", 9, None).unwrap();
        assert_eq!(*client.last_set_milestone.borrow(), Some((9, None)));
    }

    // --- Project field type mapping (AC1) ---

    #[test]
    fn gh_field_to_attr_single_select_is_str() {
        let v = GhFieldValueRepr::OptionName("In Progress".to_string());
        assert_eq!(
            gh_field_to_attr(&v),
            AttrValue::Str("In Progress".to_string())
        );
    }

    #[test]
    fn gh_field_to_attr_iteration_is_str_title() {
        let v = GhFieldValueRepr::IterationTitle("Sprint 4".to_string());
        assert_eq!(gh_field_to_attr(&v), AttrValue::Str("Sprint 4".to_string()));
    }

    #[test]
    fn gh_field_to_attr_text_is_str() {
        let v = GhFieldValueRepr::Text("freeform".to_string());
        assert_eq!(gh_field_to_attr(&v), AttrValue::Str("freeform".to_string()));
    }

    #[test]
    fn gh_field_to_attr_date_is_date() {
        let d = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        let v = GhFieldValueRepr::Date(d);
        assert_eq!(gh_field_to_attr(&v), AttrValue::Date(d));
    }

    #[test]
    fn gh_field_to_attr_integral_number_is_int() {
        let v = GhFieldValueRepr::Number(3.0);
        assert_eq!(gh_field_to_attr(&v), AttrValue::Int(3));
    }

    #[test]
    fn gh_field_to_attr_fractional_number_is_float() {
        let v = GhFieldValueRepr::Number(2.5);
        assert_eq!(gh_field_to_attr(&v), AttrValue::Float(2.5));
    }

    // --- GhFieldValueInput single-key serialization (AC3, AC4, AC7) ---

    #[test]
    fn field_value_input_single_select_emits_only_option_id_key() {
        let v = GhFieldValueInput::SingleSelect("opt_abc".to_string());
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json, serde_json::json!({"singleSelectOptionId": "opt_abc"}));
        assert_eq!(json.as_object().unwrap().len(), 1);
    }

    #[test]
    fn field_value_input_iteration_emits_only_iteration_id_key() {
        let v = GhFieldValueInput::Iteration("iter_1".to_string());
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json, serde_json::json!({"iterationId": "iter_1"}));
        assert_eq!(json.as_object().unwrap().len(), 1);
    }

    #[test]
    fn field_value_input_number_emits_only_number_key() {
        let v = GhFieldValueInput::Number(7.0);
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json, serde_json::json!({"number": 7.0}));
        assert_eq!(json.as_object().unwrap().len(), 1);
    }

    #[test]
    fn field_value_input_date_emits_only_date_key() {
        let v = GhFieldValueInput::Date(NaiveDate::from_ymd_opt(2026, 6, 25).unwrap());
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json, serde_json::json!({"date": "2026-06-25"}));
        assert_eq!(json.as_object().unwrap().len(), 1);
    }

    #[test]
    fn field_value_input_text_emits_only_text_key() {
        let v = GhFieldValueInput::Text("hi".to_string());
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json, serde_json::json!({"text": "hi"}));
        assert_eq!(json.as_object().unwrap().len(), 1);
    }

    // --- project item fields parse ---

    #[test]
    fn parse_project_items_all_kinds() {
        let resp = serde_json::json!({
            "data": {"node": {"projectItems": {"nodes": [
                {"id": "PVTI_abc", "project": {"number": 1}, "fieldValues": {"nodes": [
                    {"__typename": "ProjectV2ItemFieldSingleSelectValue", "name": "In Progress", "field": {"name": "Status"}},
                    {"__typename": "ProjectV2ItemFieldNumberValue", "number": 5.0, "field": {"name": "Estimate"}},
                    {"__typename": "ProjectV2ItemFieldDateValue", "date": "2026-06-25", "field": {"name": "Due"}},
                    {"__typename": "ProjectV2ItemFieldTextValue", "text": "note", "field": {"name": "Notes"}},
                    {"__typename": "ProjectV2ItemFieldIterationValue", "title": "Sprint 1", "field": {"name": "Sprint"}},
                    {"__typename": "ProjectV2ItemFieldLabelValue", "field": {"name": "Ignored"}}
                ]}}
            ]}}}
        });
        let items = parse_project_items(&resp);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].project_number, 1);
        assert_eq!(items[0].item_id, "PVTI_abc");
        let vals = &items[0].fields;
        assert_eq!(vals.len(), 5, "label value should be skipped: {:?}", vals);
        let status = vals.iter().find(|v| v.field_name == "Status").unwrap();
        assert_eq!(status.project_number, 1);
        assert_eq!(status.kind, GhFieldKind::SingleSelect);
        assert_eq!(
            status.value,
            GhFieldValueRepr::OptionName("In Progress".to_string())
        );
    }

    #[test]
    fn parse_project_items_keeps_membership_with_no_field_values() {
        let resp = serde_json::json!({
            "data": {"node": {"projectItems": {"nodes": [
                {"id": "PVTI_empty", "project": {"number": 7}, "fieldValues": {"nodes": []}},
                {"project": {"number": 8}, "fieldValues": {"nodes": []}},
                {"id": "PVTI_orphan", "fieldValues": {"nodes": []}}
            ]}}}
        });
        let items = parse_project_items(&resp);
        assert_eq!(
            items.len(),
            2,
            "item without a project number is skipped: {:?}",
            items
        );
        assert_eq!(items[0].project_number, 7);
        assert_eq!(items[0].item_id, "PVTI_empty");
        assert!(items[0].fields.is_empty());
        assert_eq!(items[1].project_number, 8);
        assert_eq!(items[1].item_id, "", "missing id leaves item_id empty");
    }

    #[test]
    fn mock_project_items_returns_canned_and_records_node() {
        let client = MockGhClient::new().with_project_field_values(vec![ProjectFieldValue {
            project_number: 1,
            field_name: "Status".to_string(),
            kind: GhFieldKind::SingleSelect,
            value: GhFieldValueRepr::OptionName("Todo".to_string()),
        }]);
        let got = client.project_items("o/r", "I_node1").unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].project_number, 1);
        assert_eq!(got[0].fields.len(), 1);
        assert_eq!(
            *client.project_field_calls.borrow(),
            vec!["I_node1".to_string()]
        );
    }

    #[test]
    fn mock_project_items_builder_carries_explicit_membership() {
        let client = MockGhClient::new().with_project_items(vec![ProjectItem {
            project_number: 4,
            item_id: "PVTI_explicit".to_string(),
            fields: vec![],
        }]);
        let got = client.project_items("o/r", "I_node1").unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].item_id, "PVTI_explicit");
        assert!(got[0].fields.is_empty());
    }

    #[test]
    fn mock_update_and_clear_record_calls() {
        let client = MockGhClient::new();
        client
            .update_project_v2_item_field_value(
                "PVT_1",
                "PVTI_1",
                "F_1",
                &GhFieldValueInput::SingleSelect("opt_1".to_string()),
            )
            .unwrap();
        client
            .clear_project_field("PVT_1", "PVTI_1", "F_1")
            .unwrap();
        assert_eq!(client.field_updates.borrow().len(), 1);
        assert_eq!(client.field_clears.borrow().len(), 1);
    }

    #[test]
    fn mock_graphql_pops_responses_fifo() {
        let client = MockGhClient::new().with_graphql_responses(vec![
            serde_json::json!({"n": 1}),
            serde_json::json!({"n": 2}),
        ]);
        assert_eq!(client.graphql("q", &[]).unwrap()["n"], 1);
        assert_eq!(client.graphql("q", &[]).unwrap()["n"], 2);
    }

    /// A graphql-only client: it answers every query with one canned response and
    /// inherits the project-field writes from [`GhGraphql`]'s defaults, which is
    /// the seam under test.
    struct CannedGraphql(serde_json::Value);

    impl GhGraphql for CannedGraphql {
        fn graphql(&self, _query: &str, _vars: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
            Ok(self.0.clone())
        }
        fn project_items(&self, _repo: &str, _content_node_id: &str) -> Result<Vec<ProjectItem>> {
            unreachable!("the field-write path reads no memberships")
        }
    }

    fn set_status(client: &dyn GhGraphql) -> Result<()> {
        client.update_project_v2_item_field_value(
            "PVT_7",
            "PVTI_7",
            "F_status7",
            &GhFieldValueInput::SingleSelect("opt_done".to_string()),
        )
    }

    // A token without the `project` scope makes GitHub answer the mutation with
    // HTTP 200 plus an `errors` array and no payload, which `gh` exits zero on.
    // That is a rejected write, and reporting it as a success would claim a card
    // moved that never did.
    #[test]
    fn a_scopeless_project_field_write_is_not_a_success() {
        let client = CannedGraphql(serde_json::json!({
            "data": { "updateProjectV2ItemFieldValue": serde_json::Value::Null },
            "errors": [{
                "type": "INSUFFICIENT_SCOPES",
                "message": "Your token has not been granted the required scopes to execute this query."
            }]
        }));

        let err = set_status(&client).unwrap_err().to_string();

        assert!(err.contains("`project` token scope"), "got: {err}");
        assert!(err.contains("gh auth refresh -s project"), "got: {err}");

        let err = client
            .clear_project_field("PVT_7", "PVTI_7", "F_status7")
            .unwrap_err()
            .to_string();
        assert!(err.contains("gh auth refresh -s project"), "got: {err}");
    }

    // Any other in-band rejection names the mutation and carries GitHub's own
    // message, so the failure is diagnosable.
    #[test]
    fn an_in_band_project_field_error_names_the_mutation_and_the_reason() {
        let client = CannedGraphql(serde_json::json!({
            "errors": [{ "message": "Could not resolve to a node with the global id of 'PVTI_7'." }]
        }));

        let err = set_status(&client).unwrap_err().to_string();

        assert!(err.contains("updateProjectV2ItemFieldValue"), "got: {err}");
        assert!(err.contains("Could not resolve to a node"), "got: {err}");
    }

    // The success shape: the item the mutation echoes back is what says the cell
    // moved.
    #[test]
    fn a_project_field_write_echoing_the_item_succeeds() {
        let update = CannedGraphql(serde_json::json!({
            "data": { "updateProjectV2ItemFieldValue": { "projectV2Item": { "id": "PVTI_7" } } }
        }));
        set_status(&update).unwrap();

        let clear = CannedGraphql(serde_json::json!({
            "data": { "clearProjectV2ItemFieldValue": { "projectV2Item": { "id": "PVTI_7" } } }
        }));
        clear
            .clear_project_field("PVT_7", "PVTI_7", "F_status7")
            .unwrap();
    }
}
