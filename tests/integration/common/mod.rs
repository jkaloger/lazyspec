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
                    .collect::<Vec<_>>()}
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
