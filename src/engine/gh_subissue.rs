//! Reconcile GitHub native sub-issue links from a materialized subdirectory.
//!
//! A subdir document type (`index.md` parent + sibling child `.md` files) maps
//! onto GitHub's native sub-issue edges. Sub-issue mutations key off issue
//! GraphQL node ids (`I_*`), never REST numbers. Only structural children of a
//! subdir parent route here; semantic relations stay in the issue-body comment.

use anyhow::{bail, Context, Result};

use crate::engine::config::StoreBackend;
use crate::engine::gh::{GhGraphql, GqlVar};

/// One desired structural child of a subdir parent, in loader order.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedChild {
    pub doc_id: String,
    pub node_id: String,
    pub store: StoreBackend,
    /// Position in loader order (path-sorted); the remote sub-issue list is
    /// reprioritized to match.
    pub order_index: usize,
}

/// The desired sub-issue shape for a single subdir parent: its node id plus its
/// ordered structural children. Built from `materialize_subdir` + config type
/// lookup (which supplies each doc's `StoreBackend` for the same-store guard).
#[derive(Debug, Clone)]
pub struct SubIssuePlan {
    pub parent_id: String,
    pub parent_node: String,
    pub parent_store: StoreBackend,
    pub children: Vec<PlannedChild>,
}

const SUB_ISSUES_QUERY: &str =
    "query($id: ID!) { node(id: $id) { ... on Issue { subIssues(first: 100) { nodes { id } } } } }";

pub(crate) const ADD_SUB_ISSUE_MUTATION: &str = "mutation($issueId: ID!, $subIssueId: ID!) { addSubIssue(input: {issueId: $issueId, subIssueId: $subIssueId}) { issue { id } } }";

pub(crate) const REMOVE_SUB_ISSUE_MUTATION: &str = "mutation($issueId: ID!, $subIssueId: ID!) { removeSubIssue(input: {issueId: $issueId, subIssueId: $subIssueId}) { issue { id } } }";

const REPRIORITIZE_SUB_ISSUE_MUTATION: &str = "mutation($issueId: ID!, $subIssueId: ID!, $afterId: ID!) { reprioritizeSubIssue(input: {issueId: $issueId, subIssueId: $subIssueId, afterId: $afterId}) { issue { id } } }";

/// Reconcile the remote sub-issue set of `plan.parent` to match its structural
/// children, in loader order. Add/remove for membership drift, reprioritize for
/// pure order drift. Same-store only: a child in a different `StoreBackend` than
/// the parent aborts the whole reconcile before any mutation.
pub fn reconcile_subissues(gql: &dyn GhGraphql, _repo: &str, plan: &SubIssuePlan) -> Result<()> {
    for child in &plan.children {
        if child.store != plan.parent_store {
            bail!(
                "sub-issue link rejected: parent {} (store {}) and child {} (store {}) \
                 are in different stores; lazyspec sub-issues are same-store only",
                plan.parent_id,
                plan.parent_store,
                child.doc_id,
                child.store
            );
        }
    }

    let remote = fetch_remote_sub_issue_nodes(gql, &plan.parent_node)?;

    let desired: Vec<&PlannedChild> = {
        let mut ordered: Vec<&PlannedChild> = plan.children.iter().collect();
        ordered.sort_by_key(|c| c.order_index);
        ordered
    };
    let desired_nodes: Vec<&str> = desired.iter().map(|c| c.node_id.as_str()).collect();

    for child in &desired {
        if !remote.iter().any(|r| r == &child.node_id) {
            gql.graphql(
                ADD_SUB_ISSUE_MUTATION,
                &[
                    ("issueId", GqlVar::Str(plan.parent_node.clone())),
                    ("subIssueId", GqlVar::Str(child.node_id.clone())),
                ],
            )
            .with_context(|| format!("addSubIssue for child {}", child.doc_id))?;
        }
    }

    for remote_node in &remote {
        if !desired_nodes.iter().any(|d| d == remote_node) {
            gql.graphql(
                REMOVE_SUB_ISSUE_MUTATION,
                &[
                    ("issueId", GqlVar::Str(plan.parent_node.clone())),
                    ("subIssueId", GqlVar::Str(remote_node.clone())),
                ],
            )
            .context("removeSubIssue for unlinked child")?;
        }
    }

    reprioritize_to_match(gql, plan, &remote, &desired_nodes)?;

    Ok(())
}

/// Issue reprioritize mutations so the remote order matches `desired_nodes`.
/// Only children already present remotely (no add/remove) and out of place are
/// moved; each is placed after its predecessor in the desired order (or to the
/// front when it is the first child, signalled by an empty `afterId`).
fn reprioritize_to_match(
    gql: &dyn GhGraphql,
    plan: &SubIssuePlan,
    remote: &[String],
    desired_nodes: &[&str],
) -> Result<()> {
    let common: Vec<&str> = desired_nodes
        .iter()
        .copied()
        .filter(|d| remote.iter().any(|r| r == d))
        .collect();

    let remote_common: Vec<&str> = remote
        .iter()
        .map(|s| s.as_str())
        .filter(|r| common.iter().any(|c| c == r))
        .collect();

    if common == remote_common {
        return Ok(());
    }

    let mut prev: Option<&str> = None;
    for node in &common {
        let after = prev.unwrap_or("");
        gql.graphql(
            REPRIORITIZE_SUB_ISSUE_MUTATION,
            &[
                ("issueId", GqlVar::Str(plan.parent_node.clone())),
                ("subIssueId", GqlVar::Str(node.to_string())),
                ("afterId", GqlVar::Str(after.to_string())),
            ],
        )
        .context("reprioritizeSubIssue")?;
        prev = Some(node);
    }
    Ok(())
}

/// Ordered child node ids of a parent issue's native sub-issues (`subIssues`
/// query order). Empty when the issue has no sub-issues.
pub fn fetch_remote_sub_issue_nodes(gql: &dyn GhGraphql, parent_node: &str) -> Result<Vec<String>> {
    let resp = gql.graphql(
        SUB_ISSUES_QUERY,
        &[("id", GqlVar::Str(parent_node.to_string()))],
    )?;
    let nodes = resp
        .pointer("/data/node/subIssues/nodes")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|n| n.get("id").and_then(|i| i.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::gh::test_support::MockGhClient;

    fn child(doc_id: &str, node: &str, order: usize) -> PlannedChild {
        PlannedChild {
            doc_id: doc_id.to_string(),
            node_id: node.to_string(),
            store: StoreBackend::GithubIssues,
            order_index: order,
        }
    }

    fn plan(children: Vec<PlannedChild>) -> SubIssuePlan {
        SubIssuePlan {
            parent_id: "STORY-1".to_string(),
            parent_node: "I_parent".to_string(),
            parent_store: StoreBackend::GithubIssues,
            children,
        }
    }

    fn empty_sub_issues() -> serde_json::Value {
        serde_json::json!({"data": {"node": {"subIssues": {"nodes": []}}}})
    }

    fn sub_issues(nodes: &[&str]) -> serde_json::Value {
        let nodes: Vec<_> = nodes.iter().map(|n| serde_json::json!({"id": n})).collect();
        serde_json::json!({"data": {"node": {"subIssues": {"nodes": nodes}}}})
    }

    fn ok() -> serde_json::Value {
        serde_json::json!({"data": {}})
    }

    /// The read query response followed by `n` generic mutation-OK responses.
    fn responses(read: serde_json::Value, n: usize) -> Vec<serde_json::Value> {
        let mut v = vec![read];
        v.extend(std::iter::repeat_with(ok).take(n));
        v
    }

    fn mutation_calls<'a>(
        calls: &'a [(String, Vec<(String, GqlVar)>)],
        needle: &str,
    ) -> Vec<&'a (String, Vec<(String, GqlVar)>)> {
        calls.iter().filter(|(q, _)| q.contains(needle)).collect()
    }

    // AC2: each desired child not yet linked -> one addSubIssue with the parent
    // node as issueId and the child node as subIssueId.
    #[test]
    fn add_sub_issue_called_per_unlinked_child() {
        let client = MockGhClient::new().with_graphql_responses(responses(empty_sub_issues(), 2));
        let p = plan(vec![
            child("STORY-1/A", "I_a", 0),
            child("STORY-1/B", "I_b", 1),
        ]);

        reconcile_subissues(&client, "owner/repo", &p).unwrap();

        let calls = client.graphql_calls.borrow();
        let adds = mutation_calls(&calls, "addSubIssue");
        assert_eq!(adds.len(), 2);
        for (_, vars) in &adds {
            assert!(vars.contains(&("issueId".to_string(), GqlVar::Str("I_parent".to_string()))));
        }
        let sub_ids: Vec<_> = adds
            .iter()
            .flat_map(|(_, vars)| vars.iter())
            .filter(|(k, _)| k == "subIssueId")
            .map(|(_, v)| v.clone())
            .collect();
        assert!(sub_ids.contains(&GqlVar::Str("I_a".to_string())));
        assert!(sub_ids.contains(&GqlVar::Str("I_b".to_string())));
    }

    // AC3: a remotely-linked child no longer in the plan -> removeSubIssue for
    // that node, and no spurious addSubIssue.
    #[test]
    fn remove_sub_issue_on_unlink_without_spurious_add() {
        let client = MockGhClient::new()
            .with_graphql_responses(responses(sub_issues(&["I_a", "I_gone"]), 1));
        // Plan keeps only I_a (the I_gone child .md was removed from the fixture).
        let p = plan(vec![child("STORY-1/A", "I_a", 0)]);

        reconcile_subissues(&client, "owner/repo", &p).unwrap();

        let calls = client.graphql_calls.borrow();
        let removes = mutation_calls(&calls, "removeSubIssue");
        assert_eq!(removes.len(), 1);
        assert!(removes[0]
            .1
            .contains(&("subIssueId".to_string(), GqlVar::Str("I_gone".to_string()))));
        assert!(mutation_calls(&calls, "addSubIssue").is_empty());
    }

    // AC4: a child resolving to a different StoreBackend than the parent aborts
    // the reconcile; the error names the offending parent + child, and zero
    // addSubIssue (in fact zero graphql) mutations fire.
    #[test]
    fn cross_store_child_rejected_before_any_mutation() {
        let client = MockGhClient::new().with_graphql_responses(vec![empty_sub_issues()]);
        let mut cross = child("STORY-1/X", "I_x", 0);
        cross.store = StoreBackend::Filesystem;
        let p = plan(vec![cross]);

        let err = reconcile_subissues(&client, "owner/repo", &p).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("STORY-1"), "names parent: {msg}");
        assert!(msg.contains("STORY-1/X"), "names child: {msg}");
        assert!(msg.contains("different stores"), "explains: {msg}");
        assert!(
            client.graphql_calls.borrow().is_empty(),
            "no graphql issued on cross-store reject"
        );
    }

    // AC5: children all linked but in the wrong remote order -> reprioritize so
    // remote order matches loader order; no add/remove when only order differs.
    #[test]
    fn order_mismatch_reprioritizes_without_add_or_remove() {
        // Loader order: A, B, C. Remote currently: C, A, B.
        let client = MockGhClient::new()
            .with_graphql_responses(responses(sub_issues(&["I_c", "I_a", "I_b"]), 3));
        let p = plan(vec![
            child("STORY-1/A", "I_a", 0),
            child("STORY-1/B", "I_b", 1),
            child("STORY-1/C", "I_c", 2),
        ]);

        reconcile_subissues(&client, "owner/repo", &p).unwrap();

        let calls = client.graphql_calls.borrow();
        assert!(mutation_calls(&calls, "addSubIssue").is_empty(), "no adds");
        assert!(
            mutation_calls(&calls, "removeSubIssue").is_empty(),
            "no removes"
        );
        let repris = mutation_calls(&calls, "reprioritizeSubIssue");
        assert!(!repris.is_empty(), "reprioritize must fire");

        // First desired child (A) goes to the front (empty afterId); B after A;
        // C after B -> remote ends up A, B, C.
        let first = &repris[0].1;
        assert!(first.contains(&("subIssueId".to_string(), GqlVar::Str("I_a".to_string()))));
        assert!(first.contains(&("afterId".to_string(), GqlVar::Str(String::new()))));
    }

    // Already-correct order issues no reprioritize (and no add/remove).
    #[test]
    fn correct_order_is_a_noop() {
        let client = MockGhClient::new().with_graphql_responses(vec![sub_issues(&["I_a", "I_b"])]);
        let p = plan(vec![
            child("STORY-1/A", "I_a", 0),
            child("STORY-1/B", "I_b", 1),
        ]);

        reconcile_subissues(&client, "owner/repo", &p).unwrap();

        let calls = client.graphql_calls.borrow();
        // Only the subIssues read query, nothing else.
        assert_eq!(calls.len(), 1);
        assert!(calls[0].0.contains("subIssues"));
    }
}
