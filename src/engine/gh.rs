use anyhow::{bail, Result};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::process::Command;

use crate::engine::document::AttrValue;

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

pub trait GhAuth {
    fn auth_status(&self) -> Result<AuthStatus>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum GqlVar {
    Str(String),
    Int(i64),
    Bool(bool),
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

pub trait GhGraphql {
    fn graphql(&self, query: &str, vars: &[(&str, GqlVar)]) -> Result<serde_json::Value>;

    /// Read every project field value set on the item for issue node
    /// `content_node_id`, across the boards the issue belongs to. Returns one
    /// [`ProjectFieldValue`] per set field (unset fields are omitted).
    fn project_item_fields(
        &self,
        repo: &str,
        content_node_id: &str,
    ) -> Result<Vec<ProjectFieldValue>>;

    /// Set one project field value on an item (`updateProjectV2ItemFieldValue`).
    /// All ids must already be resolved (project node id, item id, field id, and
    /// the single-select option / iteration id inside `value`).
    fn update_project_v2_item_field_value(
        &self,
        project_id: &str,
        item_id: &str,
        field_id: &str,
        value: &GhFieldValueInput,
    ) -> Result<()>;

    /// Clear one project field value on an item (`clearProjectV2ItemFieldValue`).
    /// A distinct mutation from the setter: GitHub rejects an empty-string text
    /// write as a "clear".
    fn clear_project_field(&self, project_id: &str, item_id: &str, field_id: &str) -> Result<()>;
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

    fn project_item_fields(
        &self,
        repo: &str,
        content_node_id: &str,
    ) -> Result<Vec<ProjectFieldValue>> {
        (**self).project_item_fields(repo, content_node_id)
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

const PROJECT_ITEM_FIELDS_QUERY: &str = "query($id: ID!) { node(id: $id) { ... on Issue { projectItems(first: 50) { nodes { project { number } fieldValues(first: 50) { nodes { __typename ... on ProjectV2ItemFieldSingleSelectValue { name field { ... on ProjectV2FieldCommon { name } } } ... on ProjectV2ItemFieldIterationValue { title field { ... on ProjectV2FieldCommon { name } } } ... on ProjectV2ItemFieldNumberValue { number field { ... on ProjectV2FieldCommon { name } } } ... on ProjectV2ItemFieldDateValue { date field { ... on ProjectV2FieldCommon { name } } } ... on ProjectV2ItemFieldTextValue { text field { ... on ProjectV2FieldCommon { name } } } } } } } } } }";

const UPDATE_PROJECT_FIELD_MUTATION: &str = "mutation($projectId: ID!, $itemId: ID!, $fieldId: ID!) { updateProjectV2ItemFieldValue(input: {projectId: $projectId, itemId: $itemId, fieldId: $fieldId, value: __VALUE__}) { projectV2Item { id } } }";

const CLEAR_PROJECT_FIELD_MUTATION: &str = "mutation($projectId: ID!, $itemId: ID!, $fieldId: ID!) { clearProjectV2ItemFieldValue(input: {projectId: $projectId, itemId: $itemId, fieldId: $fieldId}) { projectV2Item { id } } }";

/// Parse the `projectItems.nodes` of a [`PROJECT_ITEM_FIELDS_QUERY`] response
/// into one [`ProjectFieldValue`] per set field value. Unset fields and field
/// values with no resolvable name/typename are skipped.
pub fn parse_project_item_fields(resp: &serde_json::Value) -> Vec<ProjectFieldValue> {
    let mut out = Vec::new();
    let Some(items) = resp
        .pointer("/data/node/projectItems/nodes")
        .and_then(|v| v.as_array())
    else {
        return out;
    };

    for item in items {
        let Some(number) = item.pointer("/project/number").and_then(|v| v.as_u64()) else {
            continue;
        };
        let Some(values) = item
            .pointer("/fieldValues/nodes")
            .and_then(|v| v.as_array())
        else {
            continue;
        };
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
                out.push(ProjectFieldValue {
                    project_number: number,
                    field_name: field_name.to_string(),
                    kind,
                    value,
                });
            }
        }
    }
    out
}

/// `-f key=value` for GraphQL String vars, `-F key=value` for typed (Int/Boolean).
fn gql_var_flag(v: &GqlVar) -> (&'static str, String) {
    match v {
        GqlVar::Str(s) => ("-f", s.clone()),
        GqlVar::Int(n) => ("-F", n.to_string()),
        GqlVar::Bool(b) => ("-F", b.to_string()),
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
        let output = Command::new("gh").args(args).output();

        match output {
            Ok(o) => Ok(o),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                bail!(GhError::NotInstalled)
            }
            Err(e) => bail!("failed to execute gh: {}", e),
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
            "id,number,url,title,body,labels,state,updatedAt,createdAt,author".to_string()
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
            "id,number,url,title,body,labels,state,updatedAt,createdAt,author",
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
        let stdout = self.run_gh_checked(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
        serde_json::from_str(&stdout)
            .map_err(|e| anyhow::anyhow!("failed to parse graphql response: {}", e))
    }

    fn project_item_fields(
        &self,
        _repo: &str,
        content_node_id: &str,
    ) -> Result<Vec<ProjectFieldValue>> {
        let resp = self.graphql(
            PROJECT_ITEM_FIELDS_QUERY,
            &[("id", GqlVar::Str(content_node_id.to_string()))],
        )?;
        Ok(parse_project_item_fields(&resp))
    }

    fn update_project_v2_item_field_value(
        &self,
        project_id: &str,
        item_id: &str,
        field_id: &str,
        value: &GhFieldValueInput,
    ) -> Result<()> {
        // `gh` cannot pass a JSON-object GraphQL variable via -f/-F, so the
        // single-key value object is inlined into the mutation literally.
        let value_json = serde_json::to_string(value)?;
        let query = UPDATE_PROJECT_FIELD_MUTATION.replace("__VALUE__", &value_json);
        self.graphql(
            &query,
            &[
                ("projectId", GqlVar::Str(project_id.to_string())),
                ("itemId", GqlVar::Str(item_id.to_string())),
                ("fieldId", GqlVar::Str(field_id.to_string())),
            ],
        )?;
        Ok(())
    }

    fn clear_project_field(&self, project_id: &str, item_id: &str, field_id: &str) -> Result<()> {
        self.graphql(
            CLEAR_PROJECT_FIELD_MUTATION,
            &[
                ("projectId", GqlVar::Str(project_id.to_string())),
                ("itemId", GqlVar::Str(item_id.to_string())),
                ("fieldId", GqlVar::Str(field_id.to_string())),
            ],
        )?;
        Ok(())
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

    pub struct MockGhClient {
        pub auth: AuthStatus,
        pub list_result: Vec<GhIssue>,
        pub view_issue: RefCell<Option<GhIssue>>,
        pub create_result: Option<GhIssue>,
        pub label_create_fail: bool,
        pub closed: Cell<bool>,
        pub reopened: Cell<bool>,
        pub last_edit_title: RefCell<Option<String>>,
        pub last_edit_body: RefCell<Option<String>>,
        pub last_edit_labels_remove: RefCell<Vec<String>>,
        pub last_create_body: RefCell<Option<String>>,
        pub create_titles: RefCell<Vec<String>>,
        pub next_issue_number: Cell<u64>,
        pub graphql_responses: RefCell<Vec<serde_json::Value>>,
        pub graphql_calls: RefCell<Vec<(String, Vec<(String, GqlVar)>)>>,
        pub view_comments: RefCell<Vec<GhComment>>,
        pub comments_call_count: Cell<usize>,
        pub project_field_values: RefCell<Vec<ProjectFieldValue>>,
        pub project_field_calls: RefCell<Vec<String>>,
        pub field_updates: RefCell<Vec<(String, String, String, GhFieldValueInput)>>,
        pub field_clears: RefCell<Vec<(String, String, String)>>,
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
                closed: Cell::new(false),
                reopened: Cell::new(false),
                last_edit_title: RefCell::new(None),
                last_edit_body: RefCell::new(None),
                last_edit_labels_remove: RefCell::new(vec![]),
                last_create_body: RefCell::new(None),
                create_titles: RefCell::new(vec![]),
                next_issue_number: Cell::new(1),
                graphql_responses: RefCell::new(vec![]),
                graphql_calls: RefCell::new(vec![]),
                view_comments: RefCell::new(vec![]),
                comments_call_count: Cell::new(0),
                project_field_values: RefCell::new(vec![]),
                project_field_calls: RefCell::new(vec![]),
                field_updates: RefCell::new(vec![]),
                field_clears: RefCell::new(vec![]),
            }
        }

        pub fn with_project_field_values(mut self, values: Vec<ProjectFieldValue>) -> Self {
            self.project_field_values = RefCell::new(values);
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
            })
        }

        fn issue_edit(
            &self,
            _repo: &str,
            _number: u64,
            title: Option<&str>,
            body: Option<&str>,
            _labels_add: &[String],
            labels_remove: &[String],
        ) -> Result<()> {
            *self.last_edit_title.borrow_mut() = title.map(|s| s.to_string());
            *self.last_edit_body.borrow_mut() = body.map(|s| s.to_string());
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
            _name: &str,
            _description: &str,
            _color: &str,
        ) -> Result<()> {
            Ok(())
        }
    }

    impl GhAuth for MockGhClient {
        fn auth_status(&self) -> Result<AuthStatus> {
            Ok(self.auth.clone())
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

    /// Delegating impl so a shared `Rc<MockGhClient>` can be moved into an
    /// `FnOnce` factory while the original handle remains inspectable after
    /// (mirrors the milestone-client Rc impl).
    impl GhGraphql for std::rc::Rc<MockGhClient> {
        fn graphql(&self, query: &str, vars: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
            (**self).graphql(query, vars)
        }

        fn project_item_fields(
            &self,
            repo: &str,
            content_node_id: &str,
        ) -> Result<Vec<ProjectFieldValue>> {
            (**self).project_item_fields(repo, content_node_id)
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

    impl GhGraphql for MockGhClient {
        fn graphql(&self, query: &str, vars: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
            let recorded = vars
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect();
            self.graphql_calls
                .borrow_mut()
                .push((query.to_string(), recorded));

            let mut responses = self.graphql_responses.borrow_mut();
            if responses.is_empty() {
                bail!("no canned graphql response available");
            }
            Ok(responses.remove(0))
        }

        fn project_item_fields(
            &self,
            _repo: &str,
            content_node_id: &str,
        ) -> Result<Vec<ProjectFieldValue>> {
            self.project_field_calls
                .borrow_mut()
                .push(content_node_id.to_string());
            Ok(self.project_field_values.borrow().clone())
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
            Ok(())
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
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{MockGhClient, MockGhMilestoneClient};
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
    fn parse_project_item_fields_all_kinds() {
        let resp = serde_json::json!({
            "data": {"node": {"projectItems": {"nodes": [
                {"project": {"number": 1}, "fieldValues": {"nodes": [
                    {"__typename": "ProjectV2ItemFieldSingleSelectValue", "name": "In Progress", "field": {"name": "Status"}},
                    {"__typename": "ProjectV2ItemFieldNumberValue", "number": 5.0, "field": {"name": "Estimate"}},
                    {"__typename": "ProjectV2ItemFieldDateValue", "date": "2026-06-25", "field": {"name": "Due"}},
                    {"__typename": "ProjectV2ItemFieldTextValue", "text": "note", "field": {"name": "Notes"}},
                    {"__typename": "ProjectV2ItemFieldIterationValue", "title": "Sprint 1", "field": {"name": "Sprint"}},
                    {"__typename": "ProjectV2ItemFieldLabelValue", "field": {"name": "Ignored"}}
                ]}}
            ]}}}
        });
        let vals = parse_project_item_fields(&resp);
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
    fn mock_project_item_fields_returns_canned_and_records_node() {
        let client = MockGhClient::new().with_project_field_values(vec![ProjectFieldValue {
            project_number: 1,
            field_name: "Status".to_string(),
            kind: GhFieldKind::SingleSelect,
            value: GhFieldValueRepr::OptionName("Todo".to_string()),
        }]);
        let got = client.project_item_fields("o/r", "I_node1").unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(
            *client.project_field_calls.borrow(),
            vec!["I_node1".to_string()]
        );
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
}
