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

use crate::engine::config::{Config, StoreBackend};
use crate::engine::gh::{
    GhAssignee, GhAuthor, GhGraphql, GhIssue, GhIssueMilestone, GhLabel, GhMilestone, GqlVar,
    ProjectItem,
};
use crate::engine::gh_schema::{self, IssueTypeId, IterationId, OptionId, ProjectFieldId};
use crate::engine::issue_body::TypeMatchRule;
use crate::engine::issue_cache::RefreshWarning;

/// One board's field schema: its fields, their single-select options, and its
/// iterations, in the shape [`gh_schema::parse_project_fields`] produces.
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
    /// Per issue node id, its children's node ids in server order. An issue with
    /// no sub-issues is simply absent.
    pub sub_issues: HashMap<String, Vec<String>>,
    /// Per issue number, the numbers blocking it. An issue nothing blocks is
    /// simply absent.
    pub blocked_by: HashMap<u64, Vec<u64>>,
    /// Every capped connection that reported another page, so a consumer can
    /// name what it did not get instead of writing a partial edge set silently.
    pub truncations: Vec<Truncation>,
    /// Per issue node id, its board memberships and field cells.
    pub project_items: HashMap<String, Vec<ProjectItem>>,
    pub milestones: Option<Vec<GhMilestone>>,
    /// Empty on a user-owned repo: issue types are an Organization-only field,
    /// so its absence from the response is an answer, not a failure.
    pub issue_types: Option<Vec<IssueTypeId>>,
    /// Per authority board number. Absent means the round did not resolve that
    /// board, so its prior schema stands; present with empty vectors means the
    /// board answered and genuinely has no fields.
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

/// The `ProjectV2.fields` selection, named once and spliced into both owner
/// fragments. GraphQL merges two selections that share an alias only while they
/// are identical, so the two copies must stay one constant.
const PROJECT_FIELDS_SELECTION: &str = "fields(first: 50) { nodes { __typename \
     ... on ProjectV2FieldCommon { id name dataType } \
     ... on ProjectV2SingleSelectField { id name dataType options { id name } } \
     ... on ProjectV2IterationField { id name dataType \
     configuration { iterations { id title } } } } }";

/// One issue's flat fields. `issueType { name }` riding along is what lets a
/// type classified by native issue type be discovered by filtering this page
/// instead of by one `issue_view` per number.
const ISSUE_NODE_SELECTION: &str = "id number url title body state updatedAt createdAt \
     author { login } issueType { name } milestone { number } \
     labels(first: 20) { nodes { name } } assignees(first: 10) { nodes { login } }";

/// A nested connection the round selects inline on every issue. Selecting them
/// here is what makes sub-issue parentage and dependency edges cost no request
/// of their own; the cap is what keeps a document of them inside GitHub's node
/// budget, and [`FetchSnapshot::truncations`] is what keeps the cap honest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connection {
    SubIssues,
    BlockedBy,
}

impl Connection {
    /// How many entries of this connection one issue's page carries.
    pub fn cap(self) -> usize {
        match self {
            Connection::SubIssues => 50,
            Connection::BlockedBy => 50,
        }
    }

    /// The GraphQL field, so a warning names the edge a reader can go look at.
    pub fn field(self) -> &'static str {
        match self {
            Connection::SubIssues => "subIssues",
            Connection::BlockedBy => "blockedBy",
        }
    }
}

impl std::fmt::Display for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.field())
    }
}

/// One issue's connection that had more entries than its cap, so the snapshot
/// holds only the first page of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Truncation {
    /// The issue's node id: what a consumer maps back to the document it wrote.
    pub node_id: String,
    pub connection: Connection,
}

/// The sub-issue and dependency edges, selected on the same node as the issue's
/// own fields. `pageInfo { hasNextPage }` on each is what turns a cap into a
/// reported truncation rather than a silently short edge list.
fn issue_edges_selection() -> String {
    format!(
        "subIssues(first: {}) {{ pageInfo {{ hasNextPage }} nodes {{ id }} }} \
         blockedBy(first: {}) {{ pageInfo {{ hasNextPage }} nodes {{ number }} }}",
        Connection::SubIssues.cap(),
        Connection::BlockedBy.cap()
    )
}

/// GraphQL's ceiling on a connection page, and so the round's page size.
const ISSUE_PAGE_SIZE: usize = 100;

/// Names the composed document so a test double can tell a round apart from the
/// other queries the same [`GhGraphql`] seam carries.
pub const ROUND_OPERATION: &str = "FetchRound";

pub fn is_round_query(query: &str) -> bool {
    query.contains(ROUND_OPERATION)
}

const MILESTONES_PATH: &str = "repository.milestones";
const OWNER_PATH: &str = "repository.owner";
const ISSUE_TYPES_PATH: &str = "repository.owner.issueTypes";

/// The response key one type's issue list arrives under. Aliasing by index keeps
/// every configured type in one `repository` selection instead of one list each.
fn issue_alias(index: usize) -> String {
    format!("t{}", index)
}

/// The variable one type's resume cursor binds to, paired with its alias.
fn cursor_var(index: usize) -> String {
    format!("c{}", index)
}

fn issues_path(index: usize) -> String {
    format!("repository.{}", issue_alias(index))
}

/// The response key one board's schema arrives under. Aliasing by number keeps
/// every board in one flat owner selection instead of one request each.
fn board_alias(number: u64) -> String {
    format!("b{}", number)
}

fn board_path(number: u64) -> String {
    format!("{}.{}", OWNER_PATH, board_alias(number))
}

/// `issueTypes` hangs off the Organization fragment because that is where GitHub
/// defines it; a user-owned repo returns no such key and no error. The account
/// kind is the selected `__typename`, never a failed probe.
///
/// Each board is aliased into **both** fragments. `projectV2` resolves the same
/// `ProjectV2` type either way, so the two selections merge legally and one
/// response carries the board whichever kind of account owns the repo. `login`
/// keeps the User fragment non-empty when no board is requested.
fn owner_selection(boards: &[u64]) -> String {
    let aliased: String = boards
        .iter()
        .map(|&n| {
            format!(
                "{}: projectV2(number: {}) {{ {PROJECT_FIELDS_SELECTION} }} ",
                board_alias(n),
                n
            )
        })
        .collect();
    format!(
        "owner {{ __typename \
         ... on Organization {{ issueTypes(first: 50) {{ nodes {{ id name }} }} {aliased}}} \
         ... on User {{ login {aliased}}} }}"
    )
}

/// The GitHub label a type's alias narrows on, or `None` when the rule cannot be
/// expressed as one: an issue-type-only rule lists everything and is classified
/// against [`matches_rule`] instead. A `tag` supersedes the default `label`.
fn labels_filter(rule: &TypeMatchRule) -> Option<&str> {
    match (&rule.tag, &rule.issue_type) {
        (Some(tag), _) => Some(tag),
        (None, Some(_)) => None,
        (None, None) => Some(&rule.label),
    }
}

/// Whether a node the server already narrowed by [`labels_filter`] belongs to
/// this type. Only a native issue type is left to check: a `tag` or a `label`
/// was applied by the `labels:` argument, so the arms carrying one need no
/// second look at the node's labels.
fn matches_rule(rule: &TypeMatchRule, issue: &GhIssue) -> bool {
    match (&rule.tag, &rule.issue_type) {
        (_, None) => true,
        (None, Some(issue_type)) | (Some(_), Some(issue_type)) => {
            issue.issue_type.as_deref() == Some(issue_type.as_str())
        }
    }
}

fn issues_selection(types: &[TypeMatchRule]) -> String {
    let node = format!("{ISSUE_NODE_SELECTION} {}", issue_edges_selection());
    types
        .iter()
        .enumerate()
        .map(|(index, rule)| {
            let labels = match labels_filter(rule) {
                Some(label) => format!("labels: [{}], ", Value::from(label)),
                None => String::new(),
            };
            format!(
                "{}: issues(first: {ISSUE_PAGE_SIZE}, states: [OPEN, CLOSED], {labels}after: ${}) \
                 {{ pageInfo {{ hasNextPage endCursor }} nodes {{ {node} }} }} ",
                issue_alias(index),
                cursor_var(index)
            )
        })
        .collect()
}

/// How much of the repository a round asks for. The first round of a fetch
/// composes everything; a continuation round re-composes only the aliases still
/// paging, because milestones and the owner subtree already answered and
/// re-reading them would cost nodes for values the snapshot holds.
#[derive(Clone, Copy)]
enum RoundScope {
    Everything,
    IssuePages,
}

fn round_query(types: &[TypeMatchRule], boards: &[u64], scope: RoundScope) -> String {
    let issues = issues_selection(types);
    let rest = match scope {
        RoundScope::Everything => format!("{MILESTONES_SELECTION} {}", owner_selection(boards)),
        RoundScope::IssuePages => String::new(),
    };
    let cursors: String = (0..types.len())
        .map(|index| format!(", ${}: String", cursor_var(index)))
        .collect();
    format!(
        "query {ROUND_OPERATION}($owner: String!, $name: String!{cursors}) {{ \
         repository(owner: $owner, name: $name) {{ {issues}{rest} }} }}"
    )
}

/// The discovery rules of every `github-issues` type, in config order -- one
/// alias each on the round, and the order their cursors bind in.
pub fn issue_rules(config: &Config) -> Vec<TypeMatchRule> {
    config
        .documents
        .types
        .iter()
        .filter(|td| td.store == StoreBackend::GithubIssues)
        .map(TypeMatchRule::from)
        .collect()
}

/// Every page of every type, in one snapshot. The first round composes one alias
/// per type alongside the repository-wide subtrees; each round after it
/// re-composes only the aliases still reporting another page. So the request
/// count is the largest type's page count rather than the sum across types --
/// one 300-issue type beside nine short ones costs three rounds, not twelve.
///
/// Never `Err`: a round that could not be issued leaves its subtrees unknown and
/// warns, so every consumer keeps the cache it already had.
pub fn fetch_all_pages(
    gh: &dyn GhGraphql,
    repo: &str,
    types: &[TypeMatchRule],
    boards: &[u64],
) -> FetchSnapshot {
    let mut snapshot = fetch_round_best_effort(gh, repo, types, boards, &HashMap::new());
    let mut pending = still_paging(types, &snapshot.next_pages);
    while !pending.is_empty() {
        let cursors = std::mem::take(&mut snapshot.next_pages);
        let page = fetch_issue_page_best_effort(gh, repo, &pending, &cursors);
        merge_issue_page(&mut snapshot, &pending, &cursors, page);
        pending = still_paging(&pending, &snapshot.next_pages);
    }
    snapshot
}

fn still_paging(
    types: &[TypeMatchRule],
    next_pages: &HashMap<String, String>,
) -> Vec<TypeMatchRule> {
    types
        .iter()
        .filter(|rule| next_pages.contains_key(&rule.name))
        .cloned()
        .collect()
}

/// Fold a continuation round into the snapshot the first round opened. A type
/// the round did not answer is dropped outright rather than left holding the
/// pages that did arrive: a half-read list must not overwrite a whole cache.
fn merge_issue_page(
    snapshot: &mut FetchSnapshot,
    requested: &[TypeMatchRule],
    sent: &HashMap<String, String>,
    mut page: FetchSnapshot,
) {
    for rule in requested {
        match page.issues.remove(&rule.name) {
            Some(issues) => snapshot
                .issues
                .entry(rule.name.clone())
                .or_default()
                .extend(issues),
            None => {
                snapshot.issues.remove(&rule.name);
            }
        }
    }
    // Edges are keyed by issue, not by type, so a later page only ever adds
    // issues the earlier rounds had not reached.
    snapshot.sub_issues.extend(page.sub_issues);
    snapshot.blocked_by.extend(page.blocked_by);
    snapshot.truncations.extend(page.truncations);
    // A cursor handed back unchanged would resume the same page for ever, so
    // only one that moved counts as another page.
    snapshot.next_pages.extend(
        page.next_pages
            .into_iter()
            .filter(|(name, cursor)| sent.get(name) != Some(cursor)),
    );
    snapshot.warnings.extend(page.warnings);
}

/// Issue the round and parse it. `types` are the `github-issues` types whose
/// lists this round should resolve, one alias each; `boards` are the Projects v2
/// board numbers whose field schema it should resolve; `cursors` resumes a
/// type's list from where a prior round left off, keyed by type name, and a type
/// absent from it starts at the first page. `Err` only for a transport failure
/// or a repo string that is not `owner/name`; a partly-failed response is an
/// `Ok` snapshot carrying warnings.
fn fetch_round(
    gh: &dyn GhGraphql,
    repo: &str,
    types: &[TypeMatchRule],
    boards: &[u64],
    cursors: &HashMap<String, String>,
) -> Result<FetchSnapshot> {
    run_round(gh, repo, types, boards, cursors, RoundScope::Everything)
}

fn run_round(
    gh: &dyn GhGraphql,
    repo: &str,
    types: &[TypeMatchRule],
    boards: &[u64],
    cursors: &HashMap<String, String>,
    scope: RoundScope,
) -> Result<FetchSnapshot> {
    let (owner, name) = gh_schema::split_repo(repo)?;
    let cursor_vars: Vec<String> = (0..types.len()).map(cursor_var).collect();
    let mut vars = vec![
        ("owner", GqlVar::Str(owner.to_string())),
        ("name", GqlVar::Str(name.to_string())),
    ];
    for (index, rule) in types.iter().enumerate() {
        if let Some(cursor) = cursors.get(&rule.name) {
            vars.push((cursor_vars[index].as_str(), GqlVar::Str(cursor.clone())));
        }
    }
    let resp = gh.graphql(&round_query(types, boards, scope), &vars)?;
    Ok(parse_round(&resp, types, boards, scope))
}

/// [`fetch_round`] with a transport failure folded into the same shape a wholly
/// failed response produces: every subtree unknown, one warning. Callers that
/// must not abort a sync on an unreachable API use this and let each consumer
/// keep its prior cache.
fn fetch_round_best_effort(
    gh: &dyn GhGraphql,
    repo: &str,
    types: &[TypeMatchRule],
    boards: &[u64],
    cursors: &HashMap<String, String>,
) -> FetchSnapshot {
    best_effort(fetch_round(gh, repo, types, boards, cursors))
}

/// The continuation of a round already begun: one alias per type still reporting
/// `hasNextPage`, resumed from its `endCursor`, and nothing else. `milestones`
/// and `issue_types` come back unknown because this round never asked -- a
/// caller merges the issues into the snapshot the first round produced.
fn fetch_issue_page_best_effort(
    gh: &dyn GhGraphql,
    repo: &str,
    types: &[TypeMatchRule],
    cursors: &HashMap<String, String>,
) -> FetchSnapshot {
    best_effort(run_round(
        gh,
        repo,
        types,
        &[],
        cursors,
        RoundScope::IssuePages,
    ))
}

fn best_effort(round: Result<FetchSnapshot>) -> FetchSnapshot {
    match round {
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

fn parse_round(
    resp: &Value,
    types: &[TypeMatchRule],
    boards: &[u64],
    scope: RoundScope,
) -> FetchSnapshot {
    let errors = subtree_errors(resp);

    let Some(repo) = resp.pointer("/data/repository").filter(|v| !v.is_null()) else {
        let message = errors
            .first()
            .map(|e| e.message.as_str())
            .unwrap_or("the response carried no repository data");
        let mut warnings = match scope {
            RoundScope::Everything => {
                vec![milestones_warning(message), issue_types_warning(message)]
            }
            RoundScope::IssuePages => Vec::new(),
        };
        warnings.extend(types.iter().map(|t| issues_warning(&t.name, message)));
        warnings.extend(boards.iter().map(|&n| board_fields_warning(n, message)));
        return FetchSnapshot {
            warnings,
            ..Default::default()
        };
    };

    let mut warnings = Vec::new();
    let mut issues = HashMap::new();
    let mut next_pages = HashMap::new();
    let mut edges = IssueEdges::default();
    for (index, rule) in types.iter().enumerate() {
        match resolve_subtree(&errors, &issues_path(index), || {
            parse_issue_page(repo, index)
        }) {
            Ok(page) => {
                if let Some(cursor) = page.next_cursor {
                    next_pages.insert(rule.name.clone(), cursor);
                }
                edges.absorb(page.edges);
                issues.insert(
                    rule.name.clone(),
                    page.nodes
                        .into_iter()
                        .filter(|issue| matches_rule(rule, issue))
                        .collect(),
                );
            }
            Err(message) => warnings.push(issues_warning(&rule.name, &message)),
        }
    }

    let (milestones, issue_types) = match scope {
        RoundScope::Everything => repo_subtrees(&errors, repo, &mut warnings),
        RoundScope::IssuePages => (None, None),
    };

    let mut board_fields = HashMap::new();
    for &number in boards {
        match resolve_subtree(&errors, &board_path(number), || {
            parse_board_fields(repo, number)
        }) {
            Ok(schema) => {
                board_fields.insert(number, schema);
            }
            Err(message) => warnings.push(board_fields_warning(number, &message)),
        }
    }

    FetchSnapshot {
        issues,
        sub_issues: edges.sub_issues,
        blocked_by: edges.blocked_by,
        truncations: edges.truncations,
        next_pages,
        milestones,
        issue_types,
        board_fields,
        warnings,
        ..Default::default()
    }
}

/// The two repository-wide subtrees, resolved independently of each other and
/// of every alias, appending a warning for whichever did not answer.
fn repo_subtrees(
    errors: &[SubtreeError],
    repo: &Value,
    warnings: &mut Vec<RefreshWarning>,
) -> (Option<Vec<GhMilestone>>, Option<Vec<IssueTypeId>>) {
    let milestones = match resolve_subtree(errors, MILESTONES_PATH, || parse_milestones(repo)) {
        Ok(milestones) => Some(milestones),
        Err(message) => {
            warnings.push(milestones_warning(&message));
            None
        }
    };
    let issue_types = match resolve_subtree(errors, ISSUE_TYPES_PATH, || parse_issue_types(repo)) {
        Ok(issue_types) => Some(issue_types),
        Err(message) => {
            warnings.push(issue_types_warning(&message));
            None
        }
    };
    (milestones, issue_types)
}

/// One type's alias as it came back: the page's issues, the edges selected on
/// them, and the cursor to resume from when the connection says there is more.
struct IssuePage {
    nodes: Vec<GhIssue>,
    edges: IssueEdges,
    next_cursor: Option<String>,
}

/// The inline connections, accumulated across every alias of a round. They are
/// repo-wide rather than per type on purpose: an issue's blocker or its
/// sub-issue parent is often a document of another type, and the alias that
/// returned it is an accident of which type's list it matched.
#[derive(Default)]
struct IssueEdges {
    sub_issues: HashMap<String, Vec<String>>,
    blocked_by: HashMap<u64, Vec<u64>>,
    truncations: Vec<Truncation>,
}

impl IssueEdges {
    fn absorb(&mut self, other: IssueEdges) {
        self.sub_issues.extend(other.sub_issues);
        self.blocked_by.extend(other.blocked_by);
        self.truncations.extend(other.truncations);
    }

    /// An issue that carries no edge at all is left out of both maps: absence is
    /// "nothing blocks it, nothing hangs off it", which is what the round said.
    fn read(&mut self, node: &Value) {
        let Some(node_id) = node.get("id").and_then(|v| v.as_str()) else {
            return;
        };
        for connection in [Connection::SubIssues, Connection::BlockedBy] {
            if has_next_page(node, connection) {
                self.truncations.push(Truncation {
                    node_id: node_id.to_string(),
                    connection,
                });
            }
        }
        let children: Vec<String> = connection_nodes(node, Connection::SubIssues.field())
            .iter()
            .filter_map(|child| child.get("id").and_then(|v| v.as_str()))
            .map(str::to_string)
            .collect();
        if !children.is_empty() {
            self.sub_issues.insert(node_id.to_string(), children);
        }
        let blockers: Vec<u64> = connection_nodes(node, Connection::BlockedBy.field())
            .iter()
            .filter_map(|blocker| blocker.get("number").and_then(|v| v.as_u64()))
            .collect();
        if blockers.is_empty() {
            return;
        }
        let Some(number) = node.get("number").and_then(|v| v.as_u64()) else {
            return;
        };
        self.blocked_by.insert(number, blockers);
    }
}

fn has_next_page(node: &Value, connection: Connection) -> bool {
    node.pointer(&format!("/{}/pageInfo/hasNextPage", connection.field()))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// `Repository.issues` is `IssueConnection!`, so a missing or null alias is error
/// propagation and never an answer -- a type with no issues sends an empty
/// `nodes` array. Unknown, therefore, not empty.
fn parse_issue_page(repo: &Value, index: usize) -> Option<IssuePage> {
    let page = repo.get(issue_alias(index)).filter(|v| !v.is_null())?;
    let nodes = page.get("nodes")?.as_array()?;
    let mut edges = IssueEdges::default();
    for node in nodes {
        edges.read(node);
    }
    Some(IssuePage {
        nodes: nodes.iter().filter_map(issue_from_node).collect(),
        edges,
        next_cursor: next_cursor(page),
    })
}

fn next_cursor(page: &Value) -> Option<String> {
    if !page.pointer("/pageInfo/hasNextPage")?.as_bool()? {
        return None;
    }
    page.pointer("/pageInfo/endCursor")?
        .as_str()
        .map(str::to_string)
}

/// The one place the GraphQL and REST shapes of an issue differ: `labels` and
/// `assignees` arrive as `{nodes: [...]}` connections rather than bare arrays,
/// and `issueType` is `#[serde(skip)]` on [`GhIssue`] so it is set here. Every
/// consumer of `GhIssue` stays on the REST shape it already knows.
fn issue_from_node(node: &Value) -> Option<GhIssue> {
    Some(GhIssue {
        number: node.get("number")?.as_u64()?,
        id: string_at(node, "id"),
        url: string_at(node, "url"),
        title: string_at(node, "title"),
        body: string_at(node, "body"),
        labels: connection_nodes(node, "labels")
            .iter()
            .map(|label| GhLabel {
                name: string_at(label, "name"),
                color: String::new(),
            })
            .collect(),
        state: string_at(node, "state"),
        updated_at: string_at(node, "updatedAt"),
        created_at: string_at(node, "createdAt"),
        author: node
            .pointer("/author/login")
            .and_then(|v| v.as_str())
            .map(|login| GhAuthor {
                login: login.to_string(),
            }),
        issue_type: node
            .pointer("/issueType/name")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        milestone: node
            .pointer("/milestone/number")
            .and_then(|v| v.as_u64())
            .map(|number| GhIssueMilestone { number }),
        assignees: connection_nodes(node, "assignees")
            .iter()
            .map(|assignee| GhAssignee {
                login: string_at(assignee, "login"),
            })
            .collect(),
    })
}

fn connection_nodes<'a>(node: &'a Value, field: &str) -> &'a [Value] {
    node.pointer(&format!("/{}/nodes", field))
        .and_then(|v| v.as_array())
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// `projectV2(number:)` is nullable, so a null board is indistinguishable from
/// one whose read failed -- both leave the alias with nothing to parse and both
/// must keep the prior schema. Only a real `fields` connection is an answer.
fn parse_board_fields(repo: &Value, number: u64) -> Option<BoardFieldSchema> {
    let nodes = repo
        .pointer(&format!("/owner/{}/fields/nodes", board_alias(number)))
        .filter(|nodes| nodes.is_array())?;
    Some(gh_schema::parse_project_fields(nodes, number))
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

fn issues_warning(type_name: &str, message: &str) -> RefreshWarning {
    warning(format!(
        "could not refresh issues for type '{}' (keeping prior): {}",
        type_name, message
    ))
}

fn milestones_warning(message: &str) -> RefreshWarning {
    warning(format!(
        "could not refresh milestones (keeping prior): {}",
        message
    ))
}

/// Verbatim the string the per-board schema fetch emitted before the round, so
/// a user who has seen it once reads the same sentence after the cut-over.
fn board_fields_warning(number: u64, message: &str) -> RefreshWarning {
    warning(format!(
        "could not refresh field schema for board {} (keeping prior, projects need `gh auth refresh -s project`): {}",
        number, message
    ))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::engine::gh::test_support::{round_response, MockGhClient};

    fn rule(name: &str, tag: Option<&str>, issue_type: Option<&str>) -> TypeMatchRule {
        TypeMatchRule {
            name: name.to_string(),
            label: format!("lazyspec:{}", name),
            tag: tag.map(str::to_string),
            issue_type: issue_type.map(str::to_string),
        }
    }

    fn issue_node(number: u64, labels: &[&str], issue_type: Option<&str>) -> Value {
        serde_json::json!({
            "id": format!("I_kw{}", number),
            "number": number,
            "url": format!("https://github.com/octo-org/repo/issues/{}", number),
            "title": format!("issue {}", number),
            "body": "the body",
            "state": "OPEN",
            "updatedAt": "2026-08-01T00:00:00Z",
            "createdAt": "2026-07-01T00:00:00Z",
            "author": {"login": "jkaloger"},
            "issueType": issue_type.map(|name| serde_json::json!({"name": name})),
            "milestone": {"number": 3},
            "labels": {"nodes": labels
                .iter()
                .map(|name| serde_json::json!({"name": name}))
                .collect::<Vec<_>>()},
            "assignees": {"nodes": [{"login": "octocat"}]}
        })
    }

    /// A finished connection as GitHub actually sends one: `endCursor` still
    /// names the last node it returned, and only `hasNextPage` says to stop.
    fn last_page(nodes: Vec<Value>) -> Value {
        serde_json::json!({
            "pageInfo": {"hasNextPage": false, "endCursor": "Y3Vyc29yOmxhc3Q="},
            "nodes": nodes
        })
    }

    fn with_issues(mut resp: Value, pages: &[(usize, Value)]) -> Value {
        for (index, page) in pages {
            resp["data"]["repository"][issue_alias(*index)] = page.clone();
        }
        resp
    }

    fn numbers(snapshot: &FetchSnapshot, type_name: &str) -> Vec<u64> {
        snapshot.issues[type_name]
            .iter()
            .map(|i| i.number)
            .collect()
    }

    fn no_cursors() -> HashMap<String, String> {
        HashMap::new()
    }

    #[test]
    fn a_label_rule_filters_its_alias_on_the_types_default_label() {
        let types = [rule("story", None, None)];
        let query = round_query(&types, &[], RoundScope::Everything);
        assert!(
            query.contains(
                "t0: issues(first: 100, states: [OPEN, CLOSED], \
                 labels: [\"lazyspec:story\"], after: $c0)"
            ),
            "{}",
            query
        );

        let resp = with_issues(
            org_owned_response(),
            &[(0, last_page(vec![issue_node(1, &["lazyspec:story"], None)]))],
        );

        assert_eq!(
            numbers(
                &parse_round(&resp, &types, &[], RoundScope::Everything),
                "story"
            ),
            vec![1]
        );
    }

    #[test]
    fn a_tag_rule_filters_its_alias_on_the_tag_rather_than_the_label() {
        let types = [rule("bug", Some("triage"), None)];
        let query = round_query(&types, &[], RoundScope::Everything);
        assert!(query.contains("labels: [\"triage\"]"), "{}", query);
        assert!(
            !query.contains("lazyspec:bug"),
            "a tag supersedes the default label: {}",
            query
        );

        let resp = with_issues(
            org_owned_response(),
            &[(0, last_page(vec![issue_node(2, &["triage"], None)]))],
        );

        assert_eq!(
            numbers(
                &parse_round(&resp, &types, &[], RoundScope::Everything),
                "bug"
            ),
            vec![2]
        );
    }

    // The N+1 deletion: an issue-type rule cannot be expressed as a `labels:`
    // filter, so its alias lists everything and the classification happens
    // against the `issueType { name }` the same response already carries.
    #[test]
    fn an_issue_type_rule_lists_unfiltered_and_classifies_on_issue_type() {
        let types = [rule("epic", None, Some("Epic"))];
        let query = round_query(&types, &[], RoundScope::Everything);
        assert!(
            query.contains("t0: issues(first: 100, states: [OPEN, CLOSED], after: $c0)"),
            "{}",
            query
        );
        assert!(!query.contains("labels: ["), "{}", query);

        let resp = with_issues(
            org_owned_response(),
            &[(
                0,
                last_page(vec![
                    issue_node(1, &[], Some("Epic")),
                    issue_node(2, &[], Some("Task")),
                    issue_node(3, &[], None),
                ]),
            )],
        );

        assert_eq!(
            numbers(
                &parse_round(&resp, &types, &[], RoundScope::Everything),
                "epic"
            ),
            vec![1]
        );
    }

    #[test]
    fn a_tag_plus_issue_type_rule_ands_both_fields_of_one_result_set() {
        let types = [rule("spike", Some("research"), Some("Task"))];
        let query = round_query(&types, &[], RoundScope::Everything);
        assert!(query.contains("labels: [\"research\"]"), "{}", query);

        let resp = with_issues(
            org_owned_response(),
            &[(
                0,
                last_page(vec![
                    issue_node(1, &["research"], Some("Task")),
                    issue_node(2, &["research"], Some("Epic")),
                    issue_node(3, &["research"], None),
                ]),
            )],
        );

        assert_eq!(
            numbers(
                &parse_round(&resp, &types, &[], RoundScope::Everything),
                "spike"
            ),
            vec![1]
        );
    }

    #[test]
    fn every_alias_selects_the_issue_node_fields_and_its_own_cursor_variable() {
        let types = [rule("story", None, None), rule("epic", None, Some("Epic"))];
        let query = round_query(&types, &[], RoundScope::Everything);

        assert!(
            query.contains(
                "query FetchRound($owner: String!, $name: String!, $c0: String, $c1: String)"
            ),
            "{}",
            query
        );
        assert!(query.contains("t1: issues"), "{}", query);
        assert!(query.contains("after: $c1)"), "{}", query);

        for selection in [
            "pageInfo { hasNextPage endCursor }",
            "id number url title body state updatedAt createdAt",
            "author { login }",
            "issueType { name }",
            "milestone { number }",
            "labels(first: 20) { nodes { name } }",
            "assignees(first: 10) { nodes { login } }",
        ] {
            assert_eq!(
                query.matches(selection).count(),
                2,
                "every alias selects `{}`: {}",
                selection,
                query
            );
        }
    }

    // GraphQL wraps `labels` and `assignees` in `{nodes: [...]}` where REST hands
    // the array over directly, and `issueType` is `#[serde(skip)]` on `GhIssue`.
    // The helper absorbs both so no consumer of `GhIssue` has to know.
    #[test]
    fn a_node_becomes_a_rest_shaped_issue_with_its_native_issue_type() {
        let types = [rule("story", None, None)];
        let mut node = issue_node(7, &["lazyspec:story", "urgent"], Some("Task"));
        node["assignees"] = serde_json::json!({"nodes": [
            {"login": "octocat"}, {"login": "hubot"}
        ]});

        let snapshot = parse_round(
            &with_issues(org_owned_response(), &[(0, last_page(vec![node]))]),
            &types,
            &[],
            RoundScope::Everything,
        );

        assert_eq!(
            snapshot.issues["story"],
            vec![GhIssue {
                number: 7,
                id: "I_kw7".into(),
                url: "https://github.com/octo-org/repo/issues/7".into(),
                title: "issue 7".into(),
                body: "the body".into(),
                labels: vec![
                    GhLabel {
                        name: "lazyspec:story".into(),
                        color: String::new()
                    },
                    GhLabel {
                        name: "urgent".into(),
                        color: String::new()
                    }
                ],
                state: "OPEN".into(),
                updated_at: "2026-08-01T00:00:00Z".into(),
                created_at: "2026-07-01T00:00:00Z".into(),
                author: Some(GhAuthor {
                    login: "jkaloger".into()
                }),
                issue_type: Some("Task".into()),
                milestone: Some(GhIssueMilestone { number: 3 }),
                assignees: vec![
                    GhAssignee {
                        login: "octocat".into()
                    },
                    GhAssignee {
                        login: "hubot".into()
                    }
                ],
            }]
        );
    }

    #[test]
    fn an_unassigned_issue_with_no_milestone_and_no_author_still_parses() {
        let types = [rule("story", None, None)];
        let mut node = issue_node(8, &[], None);
        node["author"] = Value::Null;
        node["milestone"] = Value::Null;
        node["assignees"] = serde_json::json!({"nodes": []});

        let snapshot = parse_round(
            &with_issues(org_owned_response(), &[(0, last_page(vec![node]))]),
            &types,
            &[],
            RoundScope::Everything,
        );

        let issue = &snapshot.issues["story"][0];
        assert_eq!(issue.author, None);
        assert_eq!(issue.milestone, None);
        assert_eq!(issue.issue_type, None);
        assert!(issue.assignees.is_empty());
        assert!(issue.labels.is_empty());
    }

    // Same per-subtree posture the milestone and board aliases already have: one
    // type's list failing must not empty that type's cache nor touch the others.
    #[test]
    fn a_failed_issue_alias_leaves_the_other_types_intact_and_warns() {
        let types = [rule("story", None, None), rule("epic", None, Some("Epic"))];
        let mut resp = with_issues(
            org_owned_response(),
            &[
                (0, Value::Null),
                (1, last_page(vec![issue_node(1, &[], Some("Epic"))])),
            ],
        );
        resp["errors"] = serde_json::json!([{
            "type": "FORBIDDEN",
            "message": "Resource not accessible by integration",
            "path": ["repository", "t0"]
        }]);

        let snapshot = parse_round(&resp, &types, &[], RoundScope::Everything);

        assert!(
            !snapshot.issues.contains_key("story"),
            "an unresolved type must be absent, never an empty list"
        );
        assert_eq!(numbers(&snapshot, "epic"), vec![1]);
        assert_eq!(snapshot.warnings.len(), 1, "{:?}", snapshot.warnings);
        assert_eq!(
            snapshot.warnings[0].message,
            "could not refresh issues for type 'story' (keeping prior): \
             Resource not accessible by integration"
        );
    }

    #[test]
    fn a_type_whose_list_has_another_page_reports_the_cursor_to_resume_from() {
        let types = [rule("story", None, None), rule("epic", None, Some("Epic"))];
        let resp = with_issues(
            org_owned_response(),
            &[
                (
                    0,
                    serde_json::json!({
                        "pageInfo": {"hasNextPage": true, "endCursor": "Y3Vyc29yOjEwMA=="},
                        "nodes": [issue_node(1, &["lazyspec:story"], None)]
                    }),
                ),
                (1, last_page(vec![issue_node(2, &[], Some("Epic"))])),
            ],
        );

        let snapshot = parse_round(&resp, &types, &[], RoundScope::Everything);

        assert_eq!(
            snapshot.next_pages.get("story").map(String::as_str),
            Some("Y3Vyc29yOjEwMA==")
        );
        assert!(!snapshot.next_pages.contains_key("epic"));
    }

    #[test]
    fn a_supplied_cursor_binds_only_to_that_types_alias_variable() {
        let gh = MockGhClient::new();
        let types = [rule("story", None, None), rule("epic", None, Some("Epic"))];
        let cursors = HashMap::from([("epic".to_string(), "Y3Vyc29yOjEwMA==".to_string())]);

        fetch_round(&gh, "octo-org/repo", &types, &[], &cursors).unwrap();

        let calls = gh.graphql_calls.borrow();
        assert_eq!(
            calls[0].1,
            vec![
                ("owner".to_string(), GqlVar::Str("octo-org".to_string())),
                ("name".to_string(), GqlVar::Str("repo".to_string())),
                (
                    "c1".to_string(),
                    GqlVar::Str("Y3Vyc29yOjEwMA==".to_string())
                )
            ]
        );
    }

    // Milestones and the owner answered in the first round, so re-reading them
    // on a continuation would spend nodes on values the snapshot already holds.
    #[test]
    fn a_continuation_round_composes_the_issue_aliases_and_nothing_else() {
        let types = [rule("story", None, None)];
        let query = round_query(&types, &[], RoundScope::IssuePages);

        assert!(query.contains("t0: issues(first: 100"), "{}", query);
        assert!(query.contains("after: $c0)"), "{}", query);
        assert!(!query.contains("milestones("), "{}", query);
        assert!(!query.contains("owner { __typename"), "{}", query);
    }

    // A continuation round asked for nothing but issues, so it must report
    // nothing about what it left out -- neither a value nor a warning, or every
    // extra page would look like the milestone cache failing.
    #[test]
    fn a_continuation_round_says_nothing_about_the_subtrees_it_omitted() {
        let types = [rule("story", None, None)];
        let resp = with_issues(
            serde_json::json!({"data": {"repository": {}}}),
            &[(0, last_page(vec![issue_node(9, &["lazyspec:story"], None)]))],
        );

        let snapshot = parse_round(&resp, &types, &[], RoundScope::IssuePages);

        assert_eq!(numbers(&snapshot, "story"), vec![9]);
        assert_eq!(snapshot.milestones, None);
        assert_eq!(snapshot.issue_types, None);
        assert!(snapshot.warnings.is_empty(), "{:?}", snapshot.warnings);
    }

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

    /// One board's `fields` connection: a `Status` single-select whose option
    /// ids are derived from `field_id`, so two boards in one response never
    /// share an id.
    fn board_node(field_id: &str, options: &[&str]) -> Value {
        let opts: Vec<Value> = options
            .iter()
            .map(|name| {
                serde_json::json!({
                    "id": format!("{}_{}", field_id, name.to_lowercase()),
                    "name": name
                })
            })
            .collect();
        serde_json::json!({"fields": {"nodes": [{
            "__typename": "ProjectV2SingleSelectField",
            "id": field_id,
            "name": "Status",
            "dataType": "SINGLE_SELECT",
            "options": opts
        }]}})
    }

    fn with_boards(mut resp: Value) -> Value {
        resp["data"]["repository"]["owner"]["b7"] = board_node("PVTSSF_b7", &["Review", "Done"]);
        resp["data"]["repository"]["owner"]["b9"] = board_node("PVTSSF_b9", &["Triage"]);
        resp
    }

    fn status_options(snapshot: &FetchSnapshot, board: u64) -> Vec<&str> {
        let (_, options, _) = &snapshot.board_fields[&board];
        options.iter().map(|o| o.name.as_str()).collect()
    }

    // The alias trick RFC-065 turns on: one selection per board, spliced into
    // both owner fragments, so the request count does not grow with the number
    // of boards and does not depend on the account kind.
    #[test]
    fn every_board_is_aliased_into_both_owner_fragments_from_one_selection() {
        let query = round_query(&[], &[7, 9], RoundScope::Everything);
        let (organization, user) = query
            .split_once("... on User")
            .expect("the query carries a User fragment");

        for alias in ["b7: projectV2(number: 7)", "b9: projectV2(number: 9)"] {
            assert!(organization.contains(alias), "Organization: {}", query);
            assert!(user.contains(alias), "User: {}", query);
        }
        assert_eq!(
            query.matches(PROJECT_FIELDS_SELECTION).count(),
            4,
            "both fragments must splice the one shared fields selection: {}",
            query
        );
    }

    #[test]
    fn an_org_owned_round_yields_every_boards_field_schema() {
        let snapshot = parse_round(
            &with_boards(org_owned_response()),
            &[],
            &[7, 9],
            RoundScope::Everything,
        );

        assert_eq!(
            snapshot
                .issue_types
                .as_ref()
                .expect("issue types resolved")
                .len(),
            2
        );
        assert_eq!(status_options(&snapshot, 7), vec!["Review", "Done"]);
        assert_eq!(status_options(&snapshot, 9), vec!["Triage"]);
        let (fields, _, _) = &snapshot.board_fields[&7];
        assert_eq!(fields[0].project_number, 7);
        assert_eq!(fields[0].id, "PVTSSF_b7");
        assert!(snapshot.warnings.is_empty(), "{:?}", snapshot.warnings);
    }

    // The point of aliasing into both fragments: a user-owned repo answers every
    // board through the `User` fragment in the same one response. `issueTypes`
    // is simply absent (an Organization-only field), which is an answer, not a
    // failure, so nothing warns.
    #[test]
    fn a_user_owned_round_resolves_every_board_through_the_user_fragment() {
        let resp = with_boards(user_owned_response());
        assert_eq!(resp["data"]["repository"]["owner"]["__typename"], "User");
        assert!(resp["data"]["repository"]["owner"]
            .get("issueTypes")
            .is_none());

        let snapshot = parse_round(&resp, &[], &[7, 9], RoundScope::Everything);

        assert_eq!(status_options(&snapshot, 7), vec!["Review", "Done"]);
        assert_eq!(status_options(&snapshot, 9), vec!["Triage"]);
        assert_eq!(snapshot.issue_types, Some(Vec::new()));
        assert!(snapshot.warnings.is_empty(), "{:?}", snapshot.warnings);
    }

    // A token without the `project` scope fails one board and nothing else.
    // Every other subtree -- milestones, issue types, the other board -- still
    // lands, and the one warning names the board so its prior ids are kept.
    #[test]
    fn a_partial_response_fails_only_the_board_its_error_path_names() {
        let mut resp = with_boards(org_owned_response());
        resp["data"]["repository"]["owner"]["b7"] = Value::Null;
        resp["errors"] = serde_json::json!([{
            "type": "FORBIDDEN",
            "message": "Your token has not been granted the required scopes",
            "path": ["repository", "owner", "b7"]
        }]);

        let snapshot = parse_round(&resp, &[], &[7, 9], RoundScope::Everything);

        assert_eq!(
            snapshot
                .milestones
                .as_ref()
                .expect("milestones resolved")
                .len(),
            2
        );
        assert_eq!(
            snapshot
                .issue_types
                .as_ref()
                .expect("issue types resolved")
                .len(),
            2
        );
        assert_eq!(status_options(&snapshot, 9), vec!["Triage"]);
        assert!(
            !snapshot.board_fields.contains_key(&7),
            "an unresolved board must be absent, never an empty schema"
        );
        assert_eq!(snapshot.warnings.len(), 1, "{:?}", snapshot.warnings);
        assert_eq!(
            snapshot.warnings[0].message,
            "could not refresh field schema for board 7 (keeping prior, projects need \
             `gh auth refresh -s project`): Your token has not been granted the required scopes"
        );
    }

    // `projectV2(number:)` is nullable, so GitHub can null a board without an
    // `errors[]` entry -- a board number that resolves to nothing. Unknown, not
    // an empty schema, or the next save would wipe that board's ids.
    #[test]
    fn a_null_board_with_no_error_entry_is_unknown_not_empty() {
        let mut resp = with_boards(org_owned_response());
        resp["data"]["repository"]["owner"]["b7"] = Value::Null;

        let snapshot = parse_round(&resp, &[], &[7, 9], RoundScope::Everything);

        assert!(!snapshot.board_fields.contains_key(&7));
        assert_eq!(status_options(&snapshot, 9), vec!["Triage"]);
        assert_eq!(snapshot.warnings.len(), 1, "{:?}", snapshot.warnings);
        assert!(
            snapshot.warnings[0]
                .message
                .starts_with("could not refresh field schema for board 7 (keeping prior,"),
            "{}",
            snapshot.warnings[0].message
        );
    }

    // A board with no fields at all answered, so it is a known empty set: the
    // consumer must drop that board's stale ids rather than keep them.
    #[test]
    fn a_board_with_an_empty_field_connection_is_a_known_empty_schema() {
        let mut resp = with_boards(org_owned_response());
        resp["data"]["repository"]["owner"]["b7"] = serde_json::json!({"fields": {"nodes": []}});

        let snapshot = parse_round(&resp, &[], &[7, 9], RoundScope::Everything);

        assert_eq!(snapshot.board_fields[&7], Default::default());
        assert!(snapshot.warnings.is_empty(), "{:?}", snapshot.warnings);
    }

    // The whole owner failing takes issue types and every board with it, but
    // leaves milestones alone -- they hang off the repository, not the owner.
    #[test]
    fn a_failed_owner_subtree_fails_issue_types_and_every_board() {
        let mut resp = with_boards(org_owned_response());
        resp["data"]["repository"]["owner"] = Value::Null;
        resp["errors"] = serde_json::json!([{
            "message": "Resource not accessible by integration",
            "path": ["repository", "owner"]
        }]);

        let snapshot = parse_round(&resp, &[], &[7, 9], RoundScope::Everything);

        assert_eq!(snapshot.milestones.expect("milestones resolved").len(), 2);
        assert_eq!(snapshot.issue_types, None);
        assert!(snapshot.board_fields.is_empty());
        assert_eq!(snapshot.warnings.len(), 3, "{:?}", snapshot.warnings);
    }

    // The converse of the rule above, and the reason issue types are keyed on
    // `repository.owner.issueTypes` rather than on the owner: a board failing
    // must not condemn its sibling selection.
    #[test]
    fn a_failed_board_leaves_issue_types_intact_on_a_null_free_owner() {
        let mut resp = with_boards(org_owned_response());
        resp["errors"] = serde_json::json!([{
            "message": "Something went wrong",
            "path": ["repository", "owner", "b9"]
        }]);

        let snapshot = parse_round(&resp, &[], &[7, 9], RoundScope::Everything);

        assert_eq!(
            snapshot
                .issue_types
                .as_ref()
                .expect("issue types resolved")
                .len(),
            2
        );
        assert_eq!(status_options(&snapshot, 7), vec!["Review", "Done"]);
        assert!(!snapshot.board_fields.contains_key(&9));
    }

    // A response that carries no repository at all condemns the boards too, so
    // each keeps its prior schema and says why.
    #[test]
    fn a_null_repository_warns_for_every_requested_board() {
        let resp = serde_json::json!({
            "data": {"repository": Value::Null},
            "errors": [{"message": "Could not resolve to a Repository", "path": ["repository"]}]
        });

        let snapshot = parse_round(&resp, &[], &[7, 9], RoundScope::Everything);

        assert!(snapshot.board_fields.is_empty());
        assert_eq!(snapshot.warnings.len(), 4, "{:?}", snapshot.warnings);
        for number in [7, 9] {
            assert!(
                snapshot
                    .warnings
                    .iter()
                    .any(|w| w.message.starts_with(&format!(
                        "could not refresh field schema for board {}",
                        number
                    ))),
                "{:?}",
                snapshot.warnings
            );
        }
    }

    #[test]
    fn org_owned_round_yields_milestones_and_issue_types() {
        let snapshot = parse_round(&org_owned_response(), &[], &[], RoundScope::Everything);

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
        let snapshot = parse_round(&org_owned_response(), &[], &[], RoundScope::Everything);
        let milestones = snapshot.milestones.unwrap();
        assert_eq!(milestones[0].state, "open");
        assert_eq!(milestones[1].state, "closed");
    }

    // A user-owned repo has no `issueTypes` key at all. That is an answer --
    // "this account has none" -- not a failure, so it must resolve to an empty
    // set with no warning, never to `None` (which would keep stale ids alive).
    #[test]
    fn user_owned_round_yields_no_issue_types_and_no_warning() {
        let snapshot = parse_round(&user_owned_response(), &[], &[], RoundScope::Everything);

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

        let snapshot = parse_round(&resp, &[], &[], RoundScope::Everything);

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

        let snapshot = parse_round(&resp, &[], &[], RoundScope::Everything);

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

        let snapshot = parse_round(&resp, &[], &[], RoundScope::Everything);

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

        let snapshot = parse_round(&resp, &[], &[], RoundScope::Everything);

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

        let snapshot = parse_round(&resp, &[], &[], RoundScope::Everything);

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

        let snapshot = parse_round(&resp, &[], &[], RoundScope::Everything);

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

        let snapshot = fetch_round(&gh, "octo-org/repo", &[], &[], &no_cursors()).unwrap();

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
        let snapshot =
            fetch_round_best_effort(&UnreachableGh, "octo-org/repo", &[], &[], &no_cursors());

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
        assert!(fetch_round(&gh, "no-slash", &[], &[], &no_cursors()).is_err());
        assert!(gh.graphql_calls.borrow().is_empty());
    }

    /// One alias's page as GitHub sends it. `next` is the cursor to resume from;
    /// `None` is a finished connection, which still carries an `endCursor`.
    fn issue_page(numbers: std::ops::Range<u64>, next: Option<&str>) -> Value {
        let nodes: Vec<_> = numbers
            .map(|number| {
                json!({
                    "id": format!("I_kw{}", number),
                    "number": number,
                    "url": "",
                    "title": format!("issue {}", number),
                    "body": "",
                    "state": "OPEN",
                    "updatedAt": "2026-07-01T00:00:00Z",
                    "createdAt": "2026-07-01T00:00:00Z",
                    "author": {"login": "octocat"},
                    "issueType": Value::Null,
                    "milestone": Value::Null,
                    "labels": {"nodes": []},
                    "assignees": {"nodes": []}
                })
            })
            .collect();
        json!({
            "pageInfo": {"hasNextPage": next.is_some(), "endCursor": next.unwrap_or("cursor-end")},
            "nodes": nodes
        })
    }

    fn composed_aliases(query: &str) -> usize {
        (0usize..)
            .take_while(|index| query.contains(&format!("t{}: issues(", index)))
            .count()
    }

    /// A round transport where the first type's alias pages and every other
    /// answers in one. A response fills exactly the aliases its query composed,
    /// so a round that stopped asking for a type gets nothing back for it: what
    /// merges is decided by the loop, not by the fake.
    struct PagingRound {
        queries: std::cell::RefCell<Vec<String>>,
        /// How many 100-issue pages the first type holds.
        pages: usize,
    }

    impl PagingRound {
        fn with_pages(pages: usize) -> Self {
            PagingRound {
                queries: std::cell::RefCell::new(Vec::new()),
                pages,
            }
        }
    }

    impl GhGraphql for PagingRound {
        fn graphql(&self, query: &str, _vars: &[(&str, GqlVar)]) -> Result<Value> {
            let round = self.queries.borrow().len();
            self.queries.borrow_mut().push(query.to_string());

            let cursor = format!("cursor-page-{}", round + 1);
            let mut resp = round_response(&[], &[], &[]);
            for index in 0..composed_aliases(query) {
                let first = 1 + 100 * round as u64;
                resp["data"]["repository"][format!("t{}", index)] = match index {
                    0 => issue_page(
                        first..first + 100,
                        (round + 1 < self.pages).then_some(cursor.as_str()),
                    ),
                    _ => issue_page(900 + index as u64..901 + index as u64, None),
                };
            }
            Ok(resp)
        }

        fn project_items(&self, _repo: &str, _content_node_id: &str) -> Result<Vec<ProjectItem>> {
            unreachable!("the round only speaks graphql")
        }
    }

    fn ten_types() -> Vec<TypeMatchRule> {
        std::iter::once(rule("bulk", None, None))
            .chain((1..10).map(|index| rule(&format!("short{}", index), None, None)))
            .collect()
    }

    // RFC-065's pagination claim, and the reason a fetch can be complete rather
    // than capped: requests total the largest type's page count, not the sum
    // across types. One 300-issue type beside nine short ones costs three
    // requests, not twelve.
    #[test]
    fn a_three_page_type_beside_nine_short_ones_costs_three_rounds() {
        let gh = PagingRound::with_pages(3);
        let snapshot = fetch_all_pages(&gh, "owner/repo", &ten_types(), &[]);

        assert_eq!(gh.queries.borrow().len(), 3);
        assert_eq!(
            snapshot.issues["bulk"]
                .iter()
                .map(|issue| issue.number)
                .collect::<Vec<_>>(),
            (1..=300).collect::<Vec<u64>>(),
            "every page merges into one list, in server order"
        );
        assert_eq!(snapshot.issues["short9"].len(), 1);
        assert!(snapshot.next_pages.is_empty());
        assert!(snapshot.warnings.is_empty(), "{:?}", snapshot.warnings);
    }

    #[test]
    fn rounds_after_the_first_compose_only_the_types_still_paging() {
        let gh = PagingRound::with_pages(3);
        fetch_all_pages(&gh, "owner/repo", &ten_types(), &[]);

        let queries = gh.queries.borrow();
        assert_eq!(composed_aliases(&queries[0]), 10);
        for continuation in &queries[1..] {
            assert_eq!(
                composed_aliases(continuation),
                1,
                "only the unfinished alias: {}",
                continuation
            );
            assert!(!continuation.contains("milestones("), "{}", continuation);
        }
    }

    /// A transport whose second round fails the one alias it was asked for,
    /// leaving the first round's page of that type stranded.
    struct FailingSecondRound {
        rounds: std::cell::Cell<usize>,
    }

    impl GhGraphql for FailingSecondRound {
        fn graphql(&self, _query: &str, _vars: &[(&str, GqlVar)]) -> Result<Value> {
            let round = self.rounds.get();
            self.rounds.set(round + 1);

            let mut resp = round_response(&[], &[], &[]);
            if round > 0 {
                resp["data"]["repository"]["t0"] = Value::Null;
                resp["errors"] = json!([{
                    "type": "FORBIDDEN",
                    "message": "Resource not accessible by integration",
                    "path": ["repository", "t0"]
                }]);
                return Ok(resp);
            }
            resp["data"]["repository"]["t0"] = issue_page(1..101, Some("cursor-page-1"));
            resp["data"]["repository"]["t1"] = issue_page(901..902, None);
            Ok(resp)
        }

        fn project_items(&self, _repo: &str, _content_node_id: &str) -> Result<Vec<ProjectItem>> {
            unreachable!("the round only speaks graphql")
        }
    }

    // A list read in halves is not a list. If any round loses a type's alias,
    // the merged snapshot must drop that type entirely so its prior cache
    // survives, rather than hand a syncer the one page that did arrive and let
    // it prune everything else away.
    #[test]
    fn a_type_that_fails_mid_pagination_is_dropped_rather_than_left_half_read() {
        let gh = FailingSecondRound {
            rounds: std::cell::Cell::new(0),
        };
        let types = vec![rule("bulk", None, None), rule("short", None, None)];

        let snapshot = fetch_all_pages(&gh, "owner/repo", &types, &[]);

        assert_eq!(gh.rounds.get(), 2);
        assert!(
            !snapshot.issues.contains_key("bulk"),
            "a half-read type must be absent, never partial"
        );
        assert_eq!(snapshot.issues["short"].len(), 1);
        assert_eq!(
            snapshot
                .warnings
                .iter()
                .map(|w| w.message.as_str())
                .collect::<Vec<_>>(),
            vec![
                "could not refresh issues for type 'bulk' (keeping prior): \
                 Resource not accessible by integration"
            ]
        );
    }

    /// A transport that keeps claiming another page while handing back the same
    /// cursor -- the shape that would spin the round loop for ever. It gives up
    /// after a handful so an unguarded loop fails the test rather than hangs it.
    struct StuckCursorRound {
        rounds: std::cell::Cell<usize>,
    }

    impl GhGraphql for StuckCursorRound {
        fn graphql(&self, _query: &str, _vars: &[(&str, GqlVar)]) -> Result<Value> {
            self.rounds.set(self.rounds.get() + 1);
            if self.rounds.get() > 5 {
                anyhow::bail!("the round loop never stopped asking");
            }
            let mut resp = round_response(&[], &[], &[]);
            resp["data"]["repository"]["t0"] = issue_page(1..101, Some("stuck"));
            Ok(resp)
        }

        fn project_items(&self, _repo: &str, _content_node_id: &str) -> Result<Vec<ProjectItem>> {
            unreachable!("the round only speaks graphql")
        }
    }

    #[test]
    fn a_cursor_that_never_moves_ends_the_loop_instead_of_spinning() {
        let gh = StuckCursorRound {
            rounds: std::cell::Cell::new(0),
        };
        let snapshot = fetch_all_pages(&gh, "owner/repo", &[rule("bulk", None, None)], &[]);

        assert_eq!(gh.rounds.get(), 2);
        assert_eq!(snapshot.issues["bulk"].len(), 200);
    }

    // --- inline sub-issue and dependency edges ---

    /// The connections a round selects on an issue, attached to its node.
    fn edged(mut node: Value, sub_issues: &[&str], blocked_by: &[u64]) -> Value {
        node["subIssues"] = json!({
            "pageInfo": {"hasNextPage": false},
            "nodes": sub_issues.iter().map(|id| json!({"id": id})).collect::<Vec<_>>()
        });
        node["blockedBy"] = json!({
            "pageInfo": {"hasNextPage": false},
            "nodes": blocked_by.iter().map(|n| json!({"number": n})).collect::<Vec<_>>()
        });
        node
    }

    /// The same node with `connection` reporting another page -- what GitHub
    /// sends when an issue has more entries than the round's cap.
    fn truncated(mut node: Value, connection: Connection) -> Value {
        node[connection.field()]["pageInfo"]["hasNextPage"] = json!(true);
        node
    }

    #[test]
    fn the_round_selects_both_edge_connections_on_every_issue() {
        let query = round_query(&[rule("story", None, None)], &[], RoundScope::Everything);

        assert!(
            query.contains("subIssues(first: 50) { pageInfo { hasNextPage } nodes { id } }"),
            "{}",
            query
        );
        assert!(
            query.contains("blockedBy(first: 50) { pageInfo { hasNextPage } nodes { number } }"),
            "{}",
            query
        );
    }

    #[test]
    fn sub_issue_children_land_keyed_by_their_parents_node_id_in_server_order() {
        let types = [rule("story", None, None)];
        let resp = with_issues(
            org_owned_response(),
            &[(
                0,
                last_page(vec![edged(
                    issue_node(1, &["lazyspec:story"], None),
                    &["I_kw3", "I_kw2"],
                    &[],
                )]),
            )],
        );

        let snapshot = parse_round(&resp, &types, &[], RoundScope::Everything);

        assert_eq!(
            snapshot.sub_issues.get("I_kw1"),
            Some(&vec!["I_kw3".to_string(), "I_kw2".to_string()])
        );
    }

    #[test]
    fn blocked_by_edges_land_keyed_by_the_blocked_issues_number() {
        let types = [rule("story", None, None)];
        let resp = with_issues(
            org_owned_response(),
            &[(
                0,
                last_page(vec![edged(
                    issue_node(42, &["lazyspec:story"], None),
                    &[],
                    &[7, 9],
                )]),
            )],
        );

        let snapshot = parse_round(&resp, &types, &[], RoundScope::Everything);

        assert_eq!(snapshot.blocked_by.get(&42), Some(&vec![7, 9]));
    }

    #[test]
    fn an_issue_with_no_edges_is_absent_from_both_maps() {
        let types = [rule("story", None, None)];
        let resp = with_issues(
            org_owned_response(),
            &[(
                0,
                last_page(vec![edged(
                    issue_node(1, &["lazyspec:story"], None),
                    &[],
                    &[],
                )]),
            )],
        );

        let snapshot = parse_round(&resp, &types, &[], RoundScope::Everything);

        assert!(snapshot.sub_issues.is_empty());
        assert!(snapshot.blocked_by.is_empty());
        assert!(snapshot.truncations.is_empty());
    }

    #[test]
    fn edges_from_every_alias_fold_into_one_repo_wide_snapshot() {
        let types = [rule("story", None, None), rule("epic", None, Some("Epic"))];
        let resp = with_issues(
            org_owned_response(),
            &[
                (
                    0,
                    last_page(vec![edged(
                        issue_node(1, &["lazyspec:story"], None),
                        &[],
                        &[2],
                    )]),
                ),
                (
                    1,
                    last_page(vec![edged(
                        issue_node(2, &[], Some("Epic")),
                        &["I_kw1"],
                        &[],
                    )]),
                ),
            ],
        );

        let snapshot = parse_round(&resp, &types, &[], RoundScope::Everything);

        assert_eq!(snapshot.blocked_by.get(&1), Some(&vec![2]));
        assert_eq!(
            snapshot.sub_issues.get("I_kw2"),
            Some(&vec!["I_kw1".to_string()])
        );
    }

    #[test]
    fn a_connection_with_another_page_is_recorded_as_a_truncation() {
        let types = [rule("story", None, None)];
        let node = truncated(
            edged(issue_node(1, &["lazyspec:story"], None), &["I_kw2"], &[]),
            Connection::SubIssues,
        );
        let resp = with_issues(org_owned_response(), &[(0, last_page(vec![node]))]);

        let snapshot = parse_round(&resp, &types, &[], RoundScope::Everything);

        assert_eq!(
            snapshot.truncations,
            vec![Truncation {
                node_id: "I_kw1".to_string(),
                connection: Connection::SubIssues,
            }]
        );
        // What did arrive is still kept: truncation reports a loss, it is not
        // grounds for discarding the page that came back.
        assert_eq!(
            snapshot.sub_issues.get("I_kw1"),
            Some(&vec!["I_kw2".to_string()])
        );
    }

    /// A round whose first page reports another one, so the second round's
    /// edges have to be merged rather than replace the first's.
    struct TwoPageEdges;

    impl GhGraphql for TwoPageEdges {
        fn graphql(&self, query: &str, vars: &[(&str, GqlVar)]) -> Result<Value> {
            let resumed = vars.iter().any(|(k, _)| *k == "c0");
            let (number, has_next) = if resumed { (2, false) } else { (1, true) };
            let page = json!({
                "pageInfo": {"hasNextPage": has_next, "endCursor": format!("cursor{}", number)},
                "nodes": [edged(
                    issue_node(number, &["lazyspec:story"], None),
                    &[],
                    &[number + 100],
                )]
            });
            let mut resp = if query.contains("milestones") {
                org_owned_response()
            } else {
                json!({"data": {"repository": {}}})
            };
            resp["data"]["repository"]["t0"] = page;
            Ok(resp)
        }

        fn project_items(&self, _repo: &str, _content_node_id: &str) -> Result<Vec<ProjectItem>> {
            unreachable!("the round only speaks graphql")
        }
    }

    #[test]
    fn a_later_page_adds_its_edges_rather_than_replacing_the_first_pages() {
        let snapshot = fetch_all_pages(
            &TwoPageEdges,
            "owner/repo",
            &[rule("story", None, None)],
            &[],
        );

        assert_eq!(snapshot.blocked_by.get(&1), Some(&vec![101]));
        assert_eq!(snapshot.blocked_by.get(&2), Some(&vec![102]));
    }
}
