//! One composed GraphQL document per fetch round (RFC-065).
//!
//! [`fetch_round`] builds a single query from the repo, issues it through the
//! existing [`GhGraphql`] seam, and returns everything the round learned as a
//! [`FetchSnapshot`]. It touches no cache and holds no state: the builder and
//! the parser are the whole module.
//!
//! Failure is per subtree, not per round. GitHub answers a partly-broken query
//! with `data` **plus** an `errors[]` array whose entries name the failed path,
//! and `GhCli::graphql` already returns that payload as `Ok`. So each subtree is
//! read independently and an `errors[].path` naming one turns that field into
//! `None` -- "not known this round", distinct from a known-empty `Some(vec![])`
//! -- so a consumer keeps its prior cache instead of overwriting it with
//! nothing. An entry with no `path` (a timeout or rate limit, which GitHub
//! reports against the whole response) fails every subtree, and so does a
//! non-null schema field that arrived null: emptiness must be something the
//! server said, never something a broken read left behind.

use std::collections::HashMap;

use anyhow::Result;
use serde_json::Value;

use crate::engine::gh::{GhGraphql, GhIssue, GhMilestone, GqlVar, ProjectItem};
use crate::engine::gh_schema::{self, IssueTypeId, IterationId, OptionId, ProjectFieldId};
use crate::engine::issue_cache::RefreshWarning;

/// One board's field schema, as `gh_schema::fetch_project_fields` returns it:
/// its fields, their single-select options, and its iterations.
pub type BoardFieldSchema = (Vec<ProjectFieldId>, Vec<OptionId>, Vec<IterationId>);

/// Everything one fetch round learned from GitHub, resolved in one request.
///
/// `Option` fields are three-valued on purpose: `Some(values)` is authoritative,
/// `Some(vec![])` means the repo genuinely has none, and `None` means the round
/// did not learn -- the subtree errored, or no round ran at all.
#[derive(Debug, Default)]
pub struct FetchSnapshot {
    /// Per type name: the issues matching that type's discovery rule.
    pub issues: HashMap<String, Vec<GhIssue>>,
    /// Per issue node id, in server order.
    pub sub_issues: HashMap<String, Vec<String>>,
    /// Per issue number, the numbers blocking it.
    pub blocked_by: HashMap<u64, Vec<u64>>,
    /// Per issue node id, its board memberships and field cells.
    pub project_items: HashMap<String, Vec<ProjectItem>>,
    pub milestones: Option<Vec<GhMilestone>>,
    /// Empty on a user-owned repo: issue types are an Organization-only field,
    /// so its absence from the response is an answer, not a failure.
    pub issue_types: Option<Vec<IssueTypeId>>,
    /// Per authority board number.
    pub board_fields: HashMap<u64, BoardFieldSchema>,
    /// Types whose issue list has more pages, with the cursor to resume from.
    pub next_pages: HashMap<String, String>,
    /// One per failed subtree.
    pub warnings: Vec<RefreshWarning>,
}

/// The milestone fields the parser reads back into [`GhMilestone`]. `state` is
/// GraphQL's `OPEN`/`CLOSED` and is lowercased to the REST spelling the cache
/// has always stored; the issue counts are `totalCount`s rather than node
/// connections, so they cost nothing against GitHub's node budget.
const MILESTONES_SELECTION: &str = "milestones(first: 100, states: [OPEN, CLOSED]) { nodes { \
     number title description dueOn state url \
     openIssues: issues(states: OPEN) { totalCount } \
     closedIssues: issues(states: CLOSED) { totalCount } } }";

/// `issueTypes` hangs off the Organization fragment because that is where
/// GitHub defines it; a user-owned repo returns no such key and no error. The
/// account kind is the selected `__typename`, never a failed probe.
const OWNER_SELECTION: &str = "owner { __typename \
     ... on Organization { issueTypes(first: 50) { nodes { id name } } } \
     ... on User { login } }";

/// Names the composed document so a test double can tell a round apart from the
/// other queries the same [`GhGraphql`] seam carries.
pub const ROUND_OPERATION: &str = "FetchRound";

pub fn is_round_query(query: &str) -> bool {
    query.contains(ROUND_OPERATION)
}

const MILESTONES_PATH: &str = "repository.milestones";
const OWNER_PATH: &str = "repository.owner";

fn round_query() -> String {
    format!(
        "query {ROUND_OPERATION}($owner: String!, $name: String!) {{ \
         repository(owner: $owner, name: $name) {{ {MILESTONES_SELECTION} {OWNER_SELECTION} }} }}"
    )
}

/// Issue the round and parse it. `Err` only for a transport failure or a repo
/// string that is not `owner/name`; a partly-failed response is an `Ok`
/// snapshot carrying warnings.
pub fn fetch_round(gh: &dyn GhGraphql, repo: &str) -> Result<FetchSnapshot> {
    let (owner, name) = gh_schema::split_repo(repo)?;
    let resp = gh.graphql(
        &round_query(),
        &[
            ("owner", GqlVar::Str(owner.to_string())),
            ("name", GqlVar::Str(name.to_string())),
        ],
    )?;
    Ok(parse_round(&resp))
}

/// [`fetch_round`] with a transport failure folded into the same shape a wholly
/// failed response produces: every subtree unknown, one warning. Callers that
/// must not abort a sync on an unreachable API use this and let each consumer
/// keep its prior cache.
pub fn fetch_round_best_effort(gh: &dyn GhGraphql, repo: &str) -> FetchSnapshot {
    match fetch_round(gh, repo) {
        Ok(snapshot) => snapshot,
        Err(e) => FetchSnapshot {
            warnings: vec![warning(format!(
                "github fetch round failed, caches unchanged: {}",
                e
            ))],
            ..Default::default()
        },
    }
}

fn parse_round(resp: &Value) -> FetchSnapshot {
    let errors = subtree_errors(resp);

    let Some(repo) = resp.pointer("/data/repository").filter(|v| !v.is_null()) else {
        let message = errors
            .first()
            .map(|e| e.message.as_str())
            .unwrap_or("the response carried no repository data");
        return FetchSnapshot {
            warnings: vec![milestones_warning(message), issue_types_warning(message)],
            ..Default::default()
        };
    };

    let mut warnings = Vec::new();
    let milestones = match resolve_subtree(&errors, MILESTONES_PATH, || parse_milestones(repo)) {
        Ok(milestones) => Some(milestones),
        Err(message) => {
            warnings.push(milestones_warning(&message));
            None
        }
    };
    let issue_types = match resolve_subtree(&errors, OWNER_PATH, || parse_issue_types(repo)) {
        Ok(issue_types) => Some(issue_types),
        Err(message) => {
            warnings.push(issue_types_warning(&message));
            None
        }
    };

    FetchSnapshot {
        milestones,
        issue_types,
        warnings,
        ..Default::default()
    }
}

/// `Repository.milestones` is `MilestoneConnection!`, so a missing or null value
/// is error propagation and never an answer -- a repo with no milestones sends
/// an empty `nodes` array. Unknown, therefore, not empty.
fn parse_milestones(repo: &Value) -> Option<Vec<GhMilestone>> {
    let nodes = repo.pointer("/milestones/nodes")?.as_array()?;
    Some(nodes.iter().filter_map(parse_milestone).collect())
}

fn parse_milestone(node: &Value) -> Option<GhMilestone> {
    Some(GhMilestone {
        number: node.get("number")?.as_u64()?,
        title: string_at(node, "title"),
        description: string_at(node, "description"),
        due_on: node
            .get("dueOn")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        state: string_at(node, "state").to_lowercase(),
        open_issues: total_count_at(node, "openIssues"),
        closed_issues: total_count_at(node, "closedIssues"),
        url: string_at(node, "url"),
    })
}

/// An absent `issueTypes` key is the answer on a user-owned repo -- the
/// Organization fragment simply did not apply -- so it resolves empty. A key
/// that is present but null is error propagation, so it resolves unknown.
fn parse_issue_types(repo: &Value) -> Option<Vec<IssueTypeId>> {
    let owner = repo.get("owner").filter(|v| !v.is_null())?;
    let Some(issue_types) = owner.get("issueTypes") else {
        return Some(Vec::new());
    };
    let nodes = issue_types.pointer("/nodes")?.as_array()?;
    Some(
        nodes
            .iter()
            .filter_map(|node| {
                Some(IssueTypeId {
                    name: node.get("name")?.as_str()?.to_string(),
                    id: node.get("id")?.as_str()?.to_string(),
                })
            })
            .collect(),
    )
}

fn string_at(node: &Value, field: &str) -> String {
    node.get(field)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn total_count_at(node: &Value, field: &str) -> u64 {
    node.pointer(&format!("/{}/totalCount", field))
        .and_then(|v| v.as_u64())
        .unwrap_or_default()
}

/// One `errors[]` entry reduced to the dotted path it names and why it failed.
struct SubtreeError {
    /// `None` when the entry names no path. GitHub reports a timed-out or
    /// rate-limited query that way, and it condemns the whole response.
    path: Option<String>,
    message: String,
}

fn subtree_errors(resp: &Value) -> Vec<SubtreeError> {
    let Some(errors) = resp.get("errors").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    errors
        .iter()
        .map(|e| SubtreeError {
            path: dotted_path(e.get("path")),
            message: e
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unspecified GraphQL error")
                .to_string(),
        })
        .collect()
}

fn dotted_path(path: Option<&Value>) -> Option<String> {
    let segments = path?.as_array()?;
    let dotted = segments
        .iter()
        .map(|s| match s.as_str() {
            Some(name) => name.to_string(),
            None => s.to_string(),
        })
        .collect::<Vec<_>>()
        .join(".");
    Some(dotted).filter(|p| !p.is_empty())
}

/// What one subtree resolved to, or why it did not: an error naming it, or a
/// parse that found no value where the schema promises one.
fn resolve_subtree<T>(
    errors: &[SubtreeError],
    subtree: &str,
    parse: impl FnOnce() -> Option<T>,
) -> Result<T, String> {
    if let Some(message) = failure_at(errors, subtree) {
        return Err(message.to_string());
    }
    parse().ok_or_else(|| format!("the response carried no {}", subtree))
}

/// Why `subtree` did not resolve, if an error names it -- either at or below it
/// (`repository.owner.issueTypes`), or above it and so taking it down too
/// (`repository`).
fn failure_at<'a>(errors: &'a [SubtreeError], subtree: &str) -> Option<&'a str> {
    errors
        .iter()
        .find(|e| covers(e.path.as_deref(), subtree))
        .map(|e| e.message.as_str())
}

fn covers(error_path: Option<&str>, subtree: &str) -> bool {
    let Some(error_path) = error_path else {
        return true;
    };
    error_path == subtree
        || error_path.starts_with(&format!("{}.", subtree))
        || subtree.starts_with(&format!("{}.", error_path))
}

fn warning(message: String) -> RefreshWarning {
    RefreshWarning { message }
}

/// Verbatim the string the per-request schema fetch emitted before the round,
/// so `--json` consumers see the same `warnings` entry they always did.
fn issue_types_warning(message: &str) -> RefreshWarning {
    warning(format!(
        "could not refresh gh schema snapshot (keeping prior issue types, projects need `gh auth refresh -s project`): {}",
        message
    ))
}

fn milestones_warning(message: &str) -> RefreshWarning {
    warning(format!(
        "could not refresh milestones (keeping prior): {}",
        message
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::gh::test_support::MockGhClient;

    fn milestone_node(number: u64, title: &str, state: &str) -> Value {
        serde_json::json!({
            "number": number,
            "title": title,
            "description": "the description",
            "dueOn": "2026-09-01T00:00:00Z",
            "state": state,
            "url": format!("https://github.com/octo-org/repo/milestone/{}", number),
            "openIssues": {"totalCount": 7},
            "closedIssues": {"totalCount": 3}
        })
    }

    fn org_owned_response() -> Value {
        serde_json::json!({
            "data": {"repository": {
                "milestones": {"nodes": [
                    milestone_node(3, "v1.0", "OPEN"),
                    milestone_node(4, "v2.0", "CLOSED")
                ]},
                "owner": {
                    "__typename": "Organization",
                    "issueTypes": {"nodes": [
                        {"id": "IT_kwABC", "name": "Bug"},
                        {"id": "IT_kwDEF", "name": "Feature"}
                    ]}
                }
            }}
        })
    }

    fn user_owned_response() -> Value {
        serde_json::json!({
            "data": {"repository": {
                "milestones": {"nodes": [milestone_node(3, "v1.0", "OPEN")]},
                "owner": {"__typename": "User", "login": "jkaloger"}
            }}
        })
    }

    #[test]
    fn org_owned_round_yields_milestones_and_issue_types() {
        let snapshot = parse_round(&org_owned_response());

        let milestones = snapshot.milestones.expect("milestones resolved");
        assert_eq!(milestones.len(), 2);
        assert_eq!(milestones[0].number, 3);
        assert_eq!(milestones[0].title, "v1.0");
        assert_eq!(milestones[0].description, "the description");
        assert_eq!(
            milestones[0].due_on.as_deref(),
            Some("2026-09-01T00:00:00Z")
        );
        assert_eq!(milestones[0].open_issues, 7);
        assert_eq!(milestones[0].closed_issues, 3);
        assert_eq!(
            milestones[0].url,
            "https://github.com/octo-org/repo/milestone/3"
        );
        assert_eq!(
            snapshot.issue_types.expect("issue types resolved"),
            vec![
                IssueTypeId {
                    name: "Bug".into(),
                    id: "IT_kwABC".into()
                },
                IssueTypeId {
                    name: "Feature".into(),
                    id: "IT_kwDEF".into()
                }
            ]
        );
        assert!(snapshot.warnings.is_empty());
    }

    // The cache has always stored the REST spelling of a milestone's state, and
    // `gh-schema`-adjacent consumers compare it case-sensitively; GraphQL's
    // `OPEN`/`CLOSED` must arrive as `open`/`closed` or every cached milestone
    // doc changes on the first fetch after the cut-over.
    #[test]
    fn milestone_state_arrives_in_the_rest_spelling() {
        let snapshot = parse_round(&org_owned_response());
        let milestones = snapshot.milestones.unwrap();
        assert_eq!(milestones[0].state, "open");
        assert_eq!(milestones[1].state, "closed");
    }

    // A user-owned repo has no `issueTypes` key at all. That is an answer --
    // "this account has none" -- not a failure, so it must resolve to an empty
    // set with no warning, never to `None` (which would keep stale ids alive).
    #[test]
    fn user_owned_round_yields_no_issue_types_and_no_warning() {
        let snapshot = parse_round(&user_owned_response());

        assert_eq!(snapshot.issue_types, Some(Vec::new()));
        assert_eq!(snapshot.milestones.expect("milestones resolved").len(), 1);
        assert!(snapshot.warnings.is_empty());
    }

    // Partial response: `data` intact for one subtree, `errors[].path` naming
    // the other. The intact subtree still lands; the failed one is unknown, so
    // its consumer keeps what it had.
    #[test]
    fn partial_response_keeps_the_intact_subtree_and_warns_for_the_failed_one() {
        let mut resp = org_owned_response();
        resp["data"]["repository"]["owner"] = Value::Null;
        resp["errors"] = serde_json::json!([{
            "type": "FORBIDDEN",
            "message": "Resource not accessible by integration",
            "path": ["repository", "owner", "issueTypes"]
        }]);

        let snapshot = parse_round(&resp);

        assert_eq!(snapshot.milestones.expect("milestones resolved").len(), 2);
        assert_eq!(snapshot.issue_types, None);
        assert_eq!(snapshot.warnings.len(), 1);
        assert_eq!(
            snapshot.warnings[0].message,
            "could not refresh gh schema snapshot (keeping prior issue types, projects need \
             `gh auth refresh -s project`): Resource not accessible by integration"
        );
    }

    #[test]
    fn a_failed_milestone_subtree_leaves_issue_types_intact() {
        let mut resp = org_owned_response();
        resp["data"]["repository"]["milestones"] = Value::Null;
        resp["errors"] = serde_json::json!([{
            "message": "Something went wrong",
            "path": ["repository", "milestones"]
        }]);

        let snapshot = parse_round(&resp);

        assert_eq!(snapshot.milestones, None);
        assert_eq!(snapshot.issue_types.expect("issue types resolved").len(), 2);
        assert_eq!(
            snapshot.warnings[0].message,
            "could not refresh milestones (keeping prior): Something went wrong"
        );
    }

    // A timed-out or rate-limited query comes back as `data` with the failed
    // connections nulled and an `errors[]` entry that names no path at all.
    // Nothing in the response is trustworthy, so nothing may be learned from it
    // -- least of all that the repo has no milestones.
    #[test]
    fn a_pathless_error_leaves_every_subtree_unknown() {
        let mut resp = org_owned_response();
        resp["data"]["repository"]["milestones"] = Value::Null;
        resp["errors"] = serde_json::json!([{
            "message": "Something went wrong while executing your query. \
                        This may be the result of a timeout, or it could be a GitHub bug."
        }]);

        let snapshot = parse_round(&resp);

        assert_eq!(snapshot.milestones, None);
        assert_eq!(snapshot.issue_types, None);
        assert_eq!(snapshot.warnings.len(), 2);
        assert!(snapshot
            .warnings
            .iter()
            .all(|w| w.message.contains("This may be the result of a timeout")));
    }

    // `Repository.milestones` is non-null in GitHub's schema, so a null one is
    // error propagation whether or not an `errors[]` entry survived to name it.
    #[test]
    fn a_null_milestone_connection_is_unknown_even_with_no_error_entry() {
        let mut resp = org_owned_response();
        resp["data"]["repository"]["milestones"] = Value::Null;

        let snapshot = parse_round(&resp);

        assert_eq!(snapshot.milestones, None);
        assert_eq!(snapshot.issue_types.expect("issue types resolved").len(), 2);
        assert_eq!(snapshot.warnings.len(), 1);
        assert!(
            snapshot.warnings[0]
                .message
                .starts_with("could not refresh milestones (keeping prior):"),
            "{}",
            snapshot.warnings[0].message
        );
    }

    // The other side of that rule: a repo that really has no milestones answers
    // with an empty `nodes` array, and that is authoritative -- the consumer
    // must prune, not keep stale docs alive.
    #[test]
    fn an_empty_milestone_connection_is_a_known_empty_set() {
        let mut resp = org_owned_response();
        resp["data"]["repository"]["milestones"] = serde_json::json!({"nodes": []});

        let snapshot = parse_round(&resp);

        assert_eq!(snapshot.milestones, Some(Vec::new()));
        assert!(snapshot.warnings.is_empty());
    }

    // An error above both subtrees takes both down: neither is known, so
    // neither consumer may overwrite its cache.
    #[test]
    fn a_null_repository_leaves_every_subtree_unknown() {
        let resp = serde_json::json!({
            "data": {"repository": Value::Null},
            "errors": [{
                "type": "NOT_FOUND",
                "message": "Could not resolve to a Repository with the name 'octo-org/nope'.",
                "path": ["repository"]
            }]
        });

        let snapshot = parse_round(&resp);

        assert_eq!(snapshot.milestones, None);
        assert_eq!(snapshot.issue_types, None);
        assert_eq!(snapshot.warnings.len(), 2);
        assert!(snapshot
            .warnings
            .iter()
            .all(|w| w.message.contains("Could not resolve to a Repository")));
    }

    /// The transport itself failing (timeout, auth, rate limit) -- distinct
    /// from GitHub answering with partial data.
    struct UnreachableGh;

    impl GhGraphql for UnreachableGh {
        fn graphql(&self, _query: &str, _vars: &[(&str, GqlVar)]) -> Result<Value> {
            anyhow::bail!("gh: connection reset")
        }
        fn project_items(&self, _repo: &str, _content_node_id: &str) -> Result<Vec<ProjectItem>> {
            unreachable!("the round only speaks graphql")
        }
    }

    #[test]
    fn the_round_reads_one_repository_in_one_request() {
        let gh = MockGhClient::new()
            .with_milestones(vec![GhMilestone {
                number: 3,
                ..Default::default()
            }])
            .with_issue_types(vec![IssueTypeId {
                name: "Bug".into(),
                id: "IT_kwABC".into(),
            }]);

        let snapshot = fetch_round(&gh, "octo-org/repo").unwrap();

        assert_eq!(snapshot.milestones.unwrap().len(), 1);
        assert_eq!(snapshot.issue_types.unwrap().len(), 1);
        let calls = gh.graphql_calls.borrow();
        assert_eq!(calls.len(), 1);
        assert!(is_round_query(&calls[0].0), "got: {}", calls[0].0);
        assert_eq!(
            calls[0].1,
            vec![
                ("owner".to_string(), GqlVar::Str("octo-org".to_string())),
                ("name".to_string(), GqlVar::Str("repo".to_string()))
            ]
        );
    }

    #[test]
    fn a_transport_failure_is_a_round_with_nothing_known() {
        let snapshot = fetch_round_best_effort(&UnreachableGh, "octo-org/repo");

        assert_eq!(snapshot.milestones, None);
        assert_eq!(snapshot.issue_types, None);
        assert_eq!(snapshot.warnings.len(), 1);
        assert!(snapshot.warnings[0]
            .message
            .starts_with("github fetch round failed, caches unchanged:"));
    }

    #[test]
    fn a_repo_that_is_not_owner_slash_name_never_reaches_the_transport() {
        let gh = MockGhClient::new();
        assert!(fetch_round(&gh, "no-slash").is_err());
        assert!(gh.graphql_calls.borrow().is_empty());
    }
}
