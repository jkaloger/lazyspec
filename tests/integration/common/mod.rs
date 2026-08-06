#![allow(dead_code, unused_imports)]

use lazyspec::engine::config::Config;
use lazyspec::engine::store::Store;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

pub struct TestFixture {
    pub dir: TempDir,
}

impl TestFixture {
    pub fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("docs/rfcs")).unwrap();
        std::fs::create_dir_all(root.join("docs/adrs")).unwrap();
        std::fs::create_dir_all(root.join("docs/stories")).unwrap();
        std::fs::create_dir_all(root.join("docs/iterations")).unwrap();
        std::fs::create_dir_all(root.join("docs/specs")).unwrap();
        // Strict load requires a config with [[types]]; mirror a real project.
        let config = lazyspec::cli::init::starter_config();
        std::fs::write(root.join(".lazyspec.toml"), config.to_toml().unwrap()).unwrap();
        Self { dir }
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    pub fn config(&self) -> Config {
        Config::default()
    }

    pub fn store(&self) -> Store {
        Store::load(self.root(), &self.config()).unwrap()
    }

    pub fn write_doc(&self, rel_path: &str, content: &str) -> PathBuf {
        let path = self.root().join(rel_path);
        std::fs::write(&path, content).unwrap();
        path
    }

    /// Write a user-authored agent prompt template under `.lazyspec/agents/`,
    /// creating the directory if needed.
    pub fn write_agent_prompt(&self, filename: &str, content: &str) -> PathBuf {
        let dir = self.root().join(".lazyspec").join("agents");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(filename);
        std::fs::write(&path, content).unwrap();
        path
    }

    pub fn write_subfolder_doc(&self, rel_path: &str, content: &str) -> PathBuf {
        let dir = self.root().join(rel_path);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("index.md");
        std::fs::write(&path, content).unwrap();
        path
    }

    pub fn write_rfc(&self, filename: &str, title: &str, status: &str) -> PathBuf {
        let content = format!(
            "---\ntitle: \"{}\"\ntype: rfc\nstatus: {}\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
            title, status
        );
        self.write_doc(&format!("docs/rfcs/{}", filename), &content)
    }

    pub fn write_story(
        &self,
        filename: &str,
        title: &str,
        status: &str,
        implements: Option<&str>,
    ) -> PathBuf {
        let related = match implements {
            Some(path) => format!("related:\n- implements: {}", path),
            None => String::new(),
        };
        let content = format!(
            "---\ntitle: \"{}\"\ntype: story\nstatus: {}\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n{}\n---\n",
            title, status, related
        );
        self.write_doc(&format!("docs/stories/{}", filename), &content)
    }

    pub fn write_iteration(
        &self,
        filename: &str,
        title: &str,
        status: &str,
        implements: Option<&str>,
    ) -> PathBuf {
        let related = match implements {
            Some(path) => format!("related:\n- implements: {}", path),
            None => String::new(),
        };
        let content = format!(
            "---\ntitle: \"{}\"\ntype: iteration\nstatus: {}\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n{}\n---\n",
            title, status, related
        );
        self.write_doc(&format!("docs/iterations/{}", filename), &content)
    }

    pub fn write_child_doc(&self, folder_rel_path: &str, filename: &str, content: &str) -> PathBuf {
        let dir = self.root().join(folder_rel_path);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(filename);
        std::fs::write(&path, content).unwrap();
        path
    }

    pub fn write_adr(
        &self,
        filename: &str,
        title: &str,
        status: &str,
        related_to: Option<&str>,
    ) -> PathBuf {
        let related = match related_to {
            Some(path) => format!("related:\n- related-to: {}", path),
            None => String::new(),
        };
        let content = format!(
            "---\ntitle: \"{}\"\ntype: adr\nstatus: {}\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n{}\n---\n",
            title, status, related
        );
        self.write_doc(&format!("docs/adrs/{}", filename), &content)
    }

    pub fn with_git_remote() -> (Self, TempDir) {
        let fixture = Self::new();
        let bare_dir = TempDir::new().unwrap();

        Command::new("git")
            .args(["init", "--bare"])
            .current_dir(bare_dir.path())
            .output()
            .expect("git init --bare");

        Command::new("git")
            .args(["init"])
            .current_dir(fixture.root())
            .output()
            .expect("git init");

        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(fixture.root())
            .output()
            .expect("git config email");

        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(fixture.root())
            .output()
            .expect("git config name");

        Command::new("git")
            .args(["commit", "--allow-empty", "-m", "init"])
            .current_dir(fixture.root())
            .output()
            .expect("git commit");

        let bare_path = bare_dir.path().to_str().unwrap().to_string();
        Command::new("git")
            .args(["remote", "add", "origin", &bare_path])
            .current_dir(fixture.root())
            .output()
            .expect("git remote add origin");

        // Push initial commit so the remote has a valid HEAD
        Command::new("git")
            .args(["push", "origin", "HEAD"])
            .current_dir(fixture.root())
            .output()
            .expect("git push initial");

        (fixture, bare_dir)
    }
}

/// Read-only no-op GitHub reader for tests that exercise filesystem/git-ref
/// document paths through `show`/`status` `run_json`, which now require a
/// `&dyn GhIssueReader`. Returns empty comment threads; never hits the network.
pub struct NoopGh;

impl lazyspec::engine::gh::GhIssueReader for NoopGh {
    fn issue_list(
        &self,
        _repo: &str,
        _labels: &[String],
        _json_fields: &[String],
        _limit: Option<u64>,
    ) -> anyhow::Result<Vec<lazyspec::engine::gh::GhIssue>> {
        Ok(vec![])
    }
    fn issue_view(
        &self,
        _repo: &str,
        _number: u64,
    ) -> anyhow::Result<lazyspec::engine::gh::GhIssue> {
        unreachable!("NoopGh::issue_view should not be called")
    }
    fn issue_comments(
        &self,
        _repo: &str,
        _number: u64,
    ) -> anyhow::Result<Vec<lazyspec::engine::gh::GhComment>> {
        Ok(vec![])
    }
}

fn edge(nodes: Vec<serde_json::Value>, has_next_page: bool) -> serde_json::Value {
    serde_json::json!({"pageInfo": {"hasNextPage": has_next_page}, "nodes": nodes})
}

/// An issue's sub-issue children as the round selects them inline.
pub fn sub_issue_edge(children: &[&str]) -> serde_json::Value {
    edge(
        children
            .iter()
            .map(|id| serde_json::json!({"id": id}))
            .collect(),
        false,
    )
}

/// An issue's board memberships and field cells, rendered back into the shape
/// the round selects them in, so a double drives the real parser.
pub fn project_items_edge(items: &[lazyspec::engine::gh::ProjectItem]) -> serde_json::Value {
    use lazyspec::engine::gh::GhFieldValueRepr;
    let nodes = items
        .iter()
        .map(|item| {
            let values = item
                .fields
                .iter()
                .map(|value| {
                    let field = serde_json::json!({"name": value.field_name});
                    match &value.value {
                        GhFieldValueRepr::OptionName(name) => serde_json::json!({
                            "__typename": "ProjectV2ItemFieldSingleSelectValue",
                            "name": name, "field": field
                        }),
                        GhFieldValueRepr::IterationTitle(title) => serde_json::json!({
                            "__typename": "ProjectV2ItemFieldIterationValue",
                            "title": title, "field": field
                        }),
                        GhFieldValueRepr::Number(number) => serde_json::json!({
                            "__typename": "ProjectV2ItemFieldNumberValue",
                            "number": number, "field": field
                        }),
                        GhFieldValueRepr::Date(date) => serde_json::json!({
                            "__typename": "ProjectV2ItemFieldDateValue",
                            "date": date.format("%Y-%m-%d").to_string(), "field": field
                        }),
                        GhFieldValueRepr::Text(text) => serde_json::json!({
                            "__typename": "ProjectV2ItemFieldTextValue",
                            "text": text, "field": field
                        }),
                    }
                })
                .collect();
            serde_json::json!({
                "id": item.item_id,
                "project": {"number": item.project_number},
                "fieldValues": edge(values, false)
            })
        })
        .collect();
    edge(nodes, false)
}

/// As [`with_sub_issues`], for the `projectItems` connection.
pub fn with_project_items(
    resp: serde_json::Value,
    node_id: &str,
    items: &[lazyspec::engine::gh::ProjectItem],
) -> serde_json::Value {
    with_issue_connection(resp, node_id, "projectItems", project_items_edge(items))
}

/// The round as a token without the `project` scope gets it: every issue's
/// `projectItems` null, with one `errors[]` entry naming the path. The issue
/// lists come back intact beside it.
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
        .map(|path| {
            serde_json::json!({"type": "INSUFFICIENT_SCOPES", "message": message, "path": path})
        })
        .collect();
    resp
}

/// Replace the `subIssues` connection of the issue with node id `node_id` on
/// every alias of a round response, so a double answers the connection the round
/// selects inline rather than a query of its own.
pub fn with_sub_issues(
    resp: serde_json::Value,
    node_id: &str,
    edge: serde_json::Value,
) -> serde_json::Value {
    with_issue_connection(resp, node_id, "subIssues", edge)
}

fn with_issue_connection(
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

/// What a composed fetch round (RFC-065) answers a double holding `issues`:
/// one finished page on every alias `query` composed, under a repository with
/// no milestones and a user owner -- so neither the milestone cache nor the
/// schema snapshot has anything to write.
///
/// A double that leaves an alias out is telling the parser that type's list
/// failed, so every alias the query asked for must be answered.
pub fn round_response_with_issues(
    query: &str,
    issues: &[lazyspec::engine::gh::GhIssue],
) -> serde_json::Value {
    let nodes: Vec<serde_json::Value> = issues
        .iter()
        .map(|issue| {
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
                "milestone": issue
                    .milestone
                    .as_ref()
                    .map(|m| serde_json::json!({"number": m.number})),
                "labels": {"nodes": issue.labels.iter()
                    .map(|l| serde_json::json!({"name": l.name}))
                    .collect::<Vec<_>>()},
                "assignees": {"nodes": issue.assignees.iter()
                    .map(|a| serde_json::json!({"login": a.login}))
                    .collect::<Vec<_>>()},
                "subIssues": edge(vec![], false),
                "blockedBy": edge(vec![], false),
                "projectItems": edge(vec![], false)
            })
        })
        .collect();

    let mut repository = serde_json::json!({
        "milestones": {"nodes": []},
        "owner": {"__typename": "User", "login": "owner"}
    });
    let mut index = 0;
    while query.contains(&format!("t{}: issues(", index)) {
        repository[format!("t{}", index)] = serde_json::json!({
            "pageInfo": {"hasNextPage": false, "endCursor": serde_json::Value::Null},
            "nodes": nodes
        });
        index += 1;
    }
    serde_json::json!({"data": {"repository": repository}})
}
