//! ITERATION-224 TASK-4: end-to-end verification that the real `fetch` CLI path
//! materializes GitHub native sub-issues as a nested cache layout, that the
//! loader nests them through the CLI path, that a doc with BOTH native
//! sub-issues and a semantic `implements` relation nests AND keeps `implements`
//! in `related`, and that the `--json` shape of fetch/status/show is unchanged
//! by nesting (nesting is visible only through existing parent/child fields).

use anyhow::Result;
use lazyspec::engine::config::{Config, GithubConfig, StoreBackend};
use lazyspec::engine::gh::{
    GhAuthor, GhComment, GhFieldValueInput, GhGraphql, GhIssue, GhIssueReader, GhIssueWriter,
    GhLabel, GhMilestone, GhMilestoneApi, GqlVar, ProjectFieldValue,
};
use lazyspec::engine::git_ref::GitCli;
use lazyspec::engine::store::{Filter, Store};
use std::collections::HashMap;
use tempfile::TempDir;

/// A gh mock that exposes flat issues via `issue_list` and native sub-issue
/// parentage via `graphql` (the `subIssues` query, keyed on the node `id` var).
/// A graphql call with no `id` var is the schema-snapshot refresh and returns an
/// empty issue-types response. All write/milestone calls are unused here.
struct NestingGh {
    issues: Vec<GhIssue>,
    sub_issues_by_node: HashMap<String, Vec<String>>,
}

impl NestingGh {
    fn new(issues: Vec<GhIssue>, sub_issues_by_node: &[(&str, &[&str])]) -> Self {
        Self {
            issues,
            sub_issues_by_node: sub_issues_by_node
                .iter()
                .map(|(node, kids)| {
                    (
                        node.to_string(),
                        kids.iter().map(|k| k.to_string()).collect(),
                    )
                })
                .collect(),
        }
    }
}

impl GhIssueReader for NestingGh {
    fn issue_list(
        &self,
        _repo: &str,
        _labels: &[String],
        _json_fields: &[String],
        _limit: Option<u64>,
    ) -> Result<Vec<GhIssue>> {
        Ok(self.issues.clone())
    }
    fn issue_view(&self, _repo: &str, _number: u64) -> Result<GhIssue> {
        unreachable!("issue_view not used in this test")
    }
    fn issue_comments(&self, _repo: &str, _number: u64) -> Result<Vec<GhComment>> {
        Ok(vec![])
    }
}

impl GhGraphql for NestingGh {
    fn graphql(&self, _query: &str, vars: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
        // Batched parentage query: `ids: [parent_node, ...]` -> `data.nodes`,
        // each `{ id, subIssues: { nodes: [{ id }] } }`.
        if let Some((_, GqlVar::StrList(ids))) = vars.iter().find(|(k, _)| *k == "ids") {
            let nodes: Vec<_> = ids
                .iter()
                .map(|parent| {
                    let kids = self
                        .sub_issues_by_node
                        .get(parent)
                        .cloned()
                        .unwrap_or_default();
                    let child_nodes: Vec<_> = kids
                        .iter()
                        .map(|n| serde_json::json!({ "id": n }))
                        .collect();
                    serde_json::json!({ "id": parent, "subIssues": { "nodes": child_nodes } })
                })
                .collect();
            return Ok(serde_json::json!({ "data": { "nodes": nodes } }));
        }

        let id = vars
            .iter()
            .find(|(k, _)| *k == "id")
            .and_then(|(_, v)| match v {
                GqlVar::Str(s) => Some(s.clone()),
                _ => None,
            });
        match id {
            Some(node) => {
                let kids = self
                    .sub_issues_by_node
                    .get(&node)
                    .cloned()
                    .unwrap_or_default();
                let nodes: Vec<_> = kids
                    .iter()
                    .map(|n| serde_json::json!({ "id": n }))
                    .collect();
                Ok(serde_json::json!({ "data": { "node": { "subIssues": { "nodes": nodes } } } }))
            }
            None => Ok(serde_json::json!({
                "data": { "organization": { "issueTypes": { "nodes": [] } } }
            })),
        }
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

impl GhIssueWriter for NestingGh {
    fn issue_create(
        &self,
        _repo: &str,
        _title: &str,
        _body: &str,
        _labels: &[String],
    ) -> Result<GhIssue> {
        unreachable!("issue_create not used in this test")
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
        unreachable!("issue_edit not used in this test")
    }
    fn issue_close(&self, _repo: &str, _number: u64) -> Result<()> {
        unreachable!()
    }
    fn issue_reopen(&self, _repo: &str, _number: u64) -> Result<()> {
        unreachable!()
    }
    fn label_create(
        &self,
        _repo: &str,
        _name: &str,
        _description: &str,
        _color: &str,
    ) -> Result<()> {
        unreachable!()
    }
    fn label_ensure(
        &self,
        _repo: &str,
        _name: &str,
        _description: &str,
        _color: &str,
    ) -> Result<()> {
        unreachable!()
    }
}

impl GhMilestoneApi for NestingGh {
    fn milestone_list(&self, _repo: &str) -> Result<Vec<GhMilestone>> {
        Ok(vec![])
    }
    fn milestone_view(&self, _repo: &str, _number: u64) -> Result<GhMilestone> {
        unreachable!()
    }
    fn milestone_create(
        &self,
        _repo: &str,
        _title: &str,
        _description: &str,
        _due_on: Option<&str>,
        _state: &str,
    ) -> Result<GhMilestone> {
        unreachable!()
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
        unreachable!()
    }
    fn milestone_delete(&self, _repo: &str, _number: u64) -> Result<()> {
        unreachable!()
    }
    fn issue_set_milestone(
        &self,
        _repo: &str,
        _issue_number: u64,
        _milestone: Option<u64>,
    ) -> Result<()> {
        unreachable!()
    }
}

/// A non-subdirectory github-issues `story` type. Non-subdirectory so the fetch
/// path's subdir sub-issue reconcile is skipped and nesting comes purely from
/// `fetch_all` materializing the native-sub-issue parentage.
fn story_config() -> Config {
    let mut config = Config::default();
    let mut story = config
        .type_by_name("story")
        .expect("default config has a story type")
        .clone();
    story.store = StoreBackend::GithubIssues;
    story.subdirectory = false;
    config.documents.types = vec![story];
    config.documents.github = Some(GithubConfig {
        repo: Some("owner/repo".to_string()),
        cache_ttl: 60,
    });
    config
}

fn gh_issue(number: u64, node: &str, title: &str, body: &str) -> GhIssue {
    GhIssue {
        number,
        id: node.to_string(),
        url: format!("https://github.com/owner/repo/issues/{}", number),
        title: title.to_string(),
        body: body.to_string(),
        labels: vec![GhLabel {
            name: "lazyspec:story".to_string(),
            color: String::new(),
        }],
        state: "OPEN".to_string(),
        updated_at: "2026-06-26T10:00:00Z".to_string(),
        created_at: "2026-06-26T10:00:00Z".to_string(),
        author: Some(GhAuthor {
            login: "octocat".to_string(),
        }),
        issue_type: None,
        milestone: None,
    }
}

/// A lazyspec issue body whose HTML-comment frontmatter carries an `implements`
/// relation, mirroring how a comment-backed semantic relation is stored remotely.
fn body_with_implements(target: &str) -> String {
    format!(
        "<!-- lazyspec\n---\ndate: 2026-06-26\nrelated:\n- implements: {}\n---\n-->\n\nchild body\n",
        target
    )
}

fn run_fetch(root: &std::path::Path, config: &Config, gh: &NestingGh) -> Result<()> {
    // git_ref_ops is unused for github-issues-only configs but the signature
    // requires a GitRefOps; GitCli is never called here.
    lazyspec::cli::fetch::run(
        root,
        config,
        gh,
        &GitCli,
        &lazyspec::engine::clickup::FakeClickupClient::with_tasks(vec![]),
        None,
        None,
        true,
    )
}

fn story_doc<'a>(store: &'a Store, id: &str) -> &'a lazyspec::engine::document::DocMeta {
    let filter = Filter {
        doc_type: None,
        status: None,
        tag: None,
    };
    store
        .list(&filter)
        .into_iter()
        .find(|d| d.id == id)
        .unwrap_or_else(|| panic!("doc {} not loaded by store", id))
}

// AC (iteration AC, e2e): the full `fetch` CLI path materializes the nested cache
// layout (<type>/<PARENT>/index.md + <type>/<PARENT>/NN-child.md) AND `Store::load`
// over the project nests the children under the parent.
#[test]
fn fetch_cli_materializes_nested_layout_and_loader_nests() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let config = story_config();

    let gh = NestingGh::new(
        vec![
            gh_issue(100, "I_parent", "Parent story", "parent body"),
            gh_issue(11, "I_a", "Child A", "child a body"),
            gh_issue(12, "I_b", "Child B", "child b body"),
        ],
        // GitHub sub-issue order: STORY-12 (I_b) before STORY-11 (I_a).
        &[("I_parent", &["I_b", "I_a"])],
    );

    run_fetch(root, &config, &gh).expect("fetch should succeed");

    let folder = root.join(".lazyspec/cache/story/STORY-100");
    assert!(folder.join("index.md").is_file(), "parent index.md on disk");
    assert!(
        folder.join("00-STORY-12.md").is_file(),
        "first child by GH sub-issue order"
    );
    assert!(
        folder.join("01-STORY-11.md").is_file(),
        "second child by GH sub-issue order"
    );
    assert!(
        !root.join(".lazyspec/cache/story/STORY-100.md").exists(),
        "parent must not also be written flat"
    );

    let store = Store::load(root, &config).unwrap();
    let parent = story_doc(&store, "STORY-100");
    let children = store.children_of(&parent.path);
    assert_eq!(
        children.len(),
        2,
        "loader should nest both children under the parent through the CLI path"
    );
}

// Iteration AC 6 (coexistence): a child issue with BOTH a native sub-issue link
// AND a semantic `implements` relation nests via the sub-issue AND keeps the
// `implements` relation in `related` (comment-backed, unchanged by nesting).
#[test]
fn fetch_cli_nests_via_subissue_and_keeps_implements_relation() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let config = story_config();

    let gh = NestingGh::new(
        vec![
            gh_issue(100, "I_parent", "Parent story", "parent body"),
            gh_issue(11, "I_a", "Child A", &body_with_implements("RFC-001")),
        ],
        &[("I_parent", &["I_a"])],
    );

    run_fetch(root, &config, &gh).expect("fetch should succeed");

    // Nests via the native sub-issue.
    let store = Store::load(root, &config).unwrap();
    let parent = story_doc(&store, "STORY-100");
    let children = store.children_of(&parent.path);
    assert_eq!(children.len(), 1, "child nests via native sub-issue");

    // The `implements` relation survives into the child's `related`.
    let child = store_doc_at(&store, &children[0]);
    let implements: Vec<_> = child
        .related
        .iter()
        .filter(|r| format!("{}", r.rel_type) == "implements")
        .collect();
    assert_eq!(
        implements.len(),
        1,
        "child should keep exactly one implements relation, got {:?}",
        child.related
    );
    assert_eq!(implements[0].target, "RFC-001");
}

fn store_doc_at<'a>(
    store: &'a Store,
    path: &std::path::Path,
) -> &'a lazyspec::engine::document::DocMeta {
    store
        .get(path)
        .unwrap_or_else(|| panic!("doc at {} not in store", path.display()))
}

// Iteration AC 7 (json-shape guard): the `--json` shape of status/show is
// unchanged by nesting -- the nested parent/child carry exactly the same json
// key set as a flat, childless baseline. Nesting is visible only through the
// pre-existing `children`/`parent` fields (asserted in the e2e test above and in
// the json-family unit tests), never via a NEW top-level key.
#[test]
fn fetch_cli_json_shape_unchanged_by_nesting() {
    // Flat baseline: a single childless issue, fetched the same way.
    let flat_tmp = TempDir::new().unwrap();
    let flat_root = flat_tmp.path();
    let config = story_config();
    let flat_gh = NestingGh::new(vec![gh_issue(50, "I_lone", "Lone story", "lone body")], &[]);
    run_fetch(flat_root, &config, &flat_gh).expect("flat fetch");
    let flat_store = Store::load(flat_root, &config).unwrap();

    // Nested: a parent with one child, fetched the same way.
    let nested_tmp = TempDir::new().unwrap();
    let nested_root = nested_tmp.path();
    let nested_gh = NestingGh::new(
        vec![
            gh_issue(100, "I_parent", "Parent story", "parent body"),
            gh_issue(11, "I_a", "Child A", "child a body"),
        ],
        &[("I_parent", &["I_a"])],
    );
    run_fetch(nested_root, &config, &nested_gh).expect("nested fetch");
    let nested_store = Store::load(nested_root, &config).unwrap();

    // status --json: the per-doc object key set is identical for a flat doc and
    // a nested child. status uses `doc_to_json` (no family fields), so nesting
    // must introduce no new keys at all.
    let flat_status = json_array(&lazyspec::cli::status::run_json(
        &flat_store,
        &config,
        flat_root,
        &flat_gh,
    ));
    let nested_status = json_array(&lazyspec::cli::status::run_json(
        &nested_store,
        &config,
        nested_root,
        &nested_gh,
    ));
    let flat_status_keys = doc_keys(&flat_status, "STORY-50");
    let child_status_keys = doc_keys(&nested_status, "STORY-11");
    assert_eq!(
        child_status_keys, flat_status_keys,
        "status --json key set for a nested child must equal the flat baseline"
    );

    // show --json: a nested parent adds only the pre-existing `children` field
    // relative to a flat childless doc. No other new top-level key may appear.
    let fs = lazyspec::engine::fs::RealFileSystem;
    let flat_show: serde_json::Value = serde_json::from_str(
        &lazyspec::cli::show::run_json(
            &flat_store,
            "STORY-50",
            false,
            0,
            &fs,
            &config,
            flat_root,
            &flat_gh,
        )
        .unwrap(),
    )
    .unwrap();
    let parent_show: serde_json::Value = serde_json::from_str(
        &lazyspec::cli::show::run_json(
            &nested_store,
            "STORY-100",
            false,
            0,
            &fs,
            &config,
            nested_root,
            &nested_gh,
        )
        .unwrap(),
    )
    .unwrap();

    let flat_show_keys = obj_keys(&flat_show);
    let parent_show_keys = obj_keys(&parent_show);
    let extra: Vec<_> = parent_show_keys
        .iter()
        .filter(|k| !flat_show_keys.contains(*k))
        .cloned()
        .collect();
    assert_eq!(
        extra,
        vec!["children".to_string()],
        "a nested parent may only add the pre-existing `children` field; got extra keys {:?}",
        extra
    );
    // And `children` is the only nesting-specific surface -- no nested child id
    // leaks as a new top-level key.
    assert!(
        parent_show["children"].is_array(),
        "`children` must be the existing array field"
    );
    assert_eq!(
        parent_show["children"].as_array().unwrap().len(),
        1,
        "parent show should report its one nested child via `children`"
    );
}

fn json_array(s: &str) -> serde_json::Value {
    let v: serde_json::Value = serde_json::from_str(s).unwrap();
    v["documents"].clone()
}

fn doc_keys(documents: &serde_json::Value, id: &str) -> Vec<String> {
    let obj = documents
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == id)
        .unwrap_or_else(|| panic!("doc {} not in status json", id));
    obj_keys(obj)
}

fn obj_keys(v: &serde_json::Value) -> Vec<String> {
    let mut keys: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}
