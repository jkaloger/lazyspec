use anyhow::Result;
use lazyspec::engine::config::{Config, StoreBackend, TypeDef};
use lazyspec::engine::gh::{
    GhGraphql, GhIssue, GhIssueReader, GhIssueWriter, GhMilestone, GhMilestoneApi, GqlVar,
};
use lazyspec::engine::git_ref::{GitCli, GitRefOps};
use lazyspec::engine::git_ref_store::GitRefStore;
use lazyspec::engine::store_dispatch::DocumentStore;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

struct NoopGh;
impl GhIssueReader for NoopGh {
    fn issue_list(
        &self,
        _repo: &str,
        _labels: &[String],
        _json_fields: &[String],
        _limit: Option<u64>,
    ) -> Result<Vec<GhIssue>> {
        Ok(vec![])
    }
    fn issue_view(&self, _repo: &str, _number: u64) -> Result<GhIssue> {
        unreachable!("not used in this test")
    }
    fn issue_comments(
        &self,
        _repo: &str,
        _number: u64,
    ) -> Result<Vec<lazyspec::engine::gh::GhComment>> {
        Ok(vec![])
    }
}
impl GhGraphql for NoopGh {
    fn graphql(&self, _query: &str, _vars: &[(&str, GqlVar)]) -> Result<serde_json::Value> {
        unreachable!("not used in this test (git-ref types only)")
    }
    fn project_item_fields(
        &self,
        _repo: &str,
        _content_node_id: &str,
    ) -> Result<Vec<lazyspec::engine::gh::ProjectFieldValue>> {
        unreachable!("not used in this test (git-ref types only)")
    }
    fn update_project_v2_item_field_value(
        &self,
        _project_id: &str,
        _item_id: &str,
        _field_id: &str,
        _value: &lazyspec::engine::gh::GhFieldValueInput,
    ) -> Result<()> {
        unreachable!("not used in this test (git-ref types only)")
    }
    fn clear_project_field(
        &self,
        _project_id: &str,
        _item_id: &str,
        _field_id: &str,
    ) -> Result<()> {
        unreachable!("not used in this test (git-ref types only)")
    }
}
impl GhIssueWriter for NoopGh {
    fn issue_create(
        &self,
        _repo: &str,
        _title: &str,
        _body: &str,
        _labels: &[String],
    ) -> Result<GhIssue> {
        unreachable!("not used in this test")
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
        unreachable!("not used in this test")
    }
    fn issue_close(&self, _repo: &str, _number: u64) -> Result<()> {
        unreachable!("not used in this test")
    }
    fn issue_reopen(&self, _repo: &str, _number: u64) -> Result<()> {
        unreachable!("not used in this test")
    }
    fn label_create(
        &self,
        _repo: &str,
        _name: &str,
        _description: &str,
        _color: &str,
    ) -> Result<()> {
        unreachable!("not used in this test")
    }
    fn label_ensure(
        &self,
        _repo: &str,
        _name: &str,
        _description: &str,
        _color: &str,
    ) -> Result<()> {
        unreachable!("not used in this test")
    }
}
impl GhMilestoneApi for NoopGh {
    fn milestone_list(&self, _repo: &str) -> Result<Vec<GhMilestone>> {
        Ok(vec![])
    }
    fn milestone_view(&self, _repo: &str, _number: u64) -> Result<GhMilestone> {
        unreachable!("not used in this test")
    }
    fn milestone_create(
        &self,
        _repo: &str,
        _title: &str,
        _description: &str,
        _due_on: Option<&str>,
        _state: &str,
    ) -> Result<GhMilestone> {
        unreachable!("not used in this test")
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
        unreachable!("not used in this test")
    }
    fn milestone_delete(&self, _repo: &str, _number: u64) -> Result<()> {
        unreachable!("not used in this test")
    }
    fn issue_set_milestone(
        &self,
        _repo: &str,
        _issue_number: u64,
        _milestone: Option<u64>,
    ) -> Result<()> {
        unreachable!("not used in this test")
    }
}

fn config_with_git_ref_iteration() -> Config {
    let mut config = Config::default();
    for t in &mut config.documents.types {
        if t.name == "iteration" {
            t.store = StoreBackend::GitRef;
        }
    }
    config
}

fn iteration_type_def(config: &Config) -> TypeDef {
    config
        .type_by_name("iteration")
        .expect("iteration type")
        .clone()
}

fn make_clone_b(bare: &Path) -> TempDir {
    let dir = TempDir::new().unwrap();
    let bare_str = bare.to_str().unwrap();
    let target_str = dir.path().to_str().unwrap();

    let out = Command::new("git")
        .args(["clone", bare_str, target_str])
        .output()
        .expect("git clone");
    assert!(
        out.status.success(),
        "git clone failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Command::new("git")
        .args(["config", "user.email", "b@test.com"])
        .current_dir(dir.path())
        .output()
        .expect("git config email");
    Command::new("git")
        .args(["config", "user.name", "B"])
        .current_dir(dir.path())
        .output()
        .expect("git config name");

    dir
}

fn run_fetch(root: &Path, config: &Config) -> Result<()> {
    let gh = NoopGh;
    lazyspec::cli::fetch::run(
        root,
        config,
        &gh,
        &GitCli,
        &lazyspec::engine::clickup::FakeClickupClient::with_tasks(vec![]),
        None,
        None,
        true,
    )
}

#[test]
fn fetch_prunes_deleted_remote_doc_refs() {
    // AC1: After delete from clone A and fetch from clone B,
    // B's local doc ref and cache file no longer exist.
    let (fixture_a, bare) = crate::common::TestFixture::with_git_remote();
    let clone_b = make_clone_b(bare.path());

    let config = config_with_git_ref_iteration();
    let type_def = iteration_type_def(&config);

    // Clone A creates ITERATION-001 locally, then pushes the ref to the bare
    // remote (the store itself no longer pushes; RFC-061 removed the
    // [coordination]-gated push).
    let mut store_a = GitRefStore {
        git: Box::new(GitCli),
        root: fixture_a.root().to_path_buf(),
        remote: config.git_ref.remote.clone(),
        config: config.clone(),
        reserved_number: Some(1),
    };
    let created = store_a
        .create(&type_def, "First Iteration", "agent-a", "body content")
        .expect("create iteration on A");
    assert_eq!(created.id, "ITERATION-001");

    let git = GitCli;
    let refname = "refs/lazyspec/iteration/ITERATION-001";
    git.push_ref(fixture_a.root(), "origin", refname)
        .expect("A pushes doc ref to remote");

    // Clone B fetches; should see the ref + cache file.
    run_fetch(clone_b.path(), &config).expect("first fetch on B");
    let cache_file = clone_b
        .path()
        .join(".lazyspec/cache/iteration/ITERATION-001.md");

    assert!(
        git.resolve_ref(clone_b.path(), refname).unwrap().is_some(),
        "B should have local ref after first fetch"
    );
    assert!(
        cache_file.exists(),
        "B should have cache file after first fetch"
    );

    // Clone A deletes ITERATION-001 locally and removes the remote ref.
    store_a
        .delete(&type_def, "ITERATION-001")
        .expect("delete iteration on A");
    git.delete_remote_ref(fixture_a.root(), "origin", refname, None)
        .expect("A deletes remote doc ref");

    // Clone B fetches again; with --prune the local ref should disappear
    // and the cache cleanup loop should remove the cache file.
    run_fetch(clone_b.path(), &config).expect("second fetch on B");

    assert_eq!(
        git.resolve_ref(clone_b.path(), refname).unwrap(),
        None,
        "B's local doc ref should be pruned after fetch"
    );
    assert!(
        !cache_file.exists(),
        "B's cache file should be removed after fetch prunes the ref"
    );
}
