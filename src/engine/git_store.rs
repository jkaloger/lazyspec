//! The `git` store (RFC-072 "The git store"): documents are files in a managed
//! clone under `.lazyspec/cache/<type>/`, and every write is a file write, a
//! commit, and a push to the type's declared branch.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::engine::config::{Config, StoreBackend, TypeDef};
use crate::engine::git_ref::{GitRefClient, GitRefOps};
use crate::engine::store::doc_root;
use crate::engine::store_dispatch::{CreatedDoc, DocumentStore, FilesystemStore, PushOutcome};

/// Commit and push `type_def`'s clone. Never `LocalOnly` (STORY-282 AC5): an
/// unreachable remote and a moved remote are both an error the human resolves
/// with `lazyspec fetch`, not a warning at exit 0.
fn commit_clone(root: &Path, type_def: &TypeDef, ops: &dyn GitRefOps, message: &str) -> Result<()> {
    let remote = type_def
        .remote
        .as_deref()
        .expect("Config::parse rejects a git store without a remote");
    let branch = type_def.branch.as_deref();
    let clone = root.join(".lazyspec/cache").join(&type_def.name);
    ops.commit_and_push(&clone, branch, message)
        .with_context(|| {
            format!(
            "pushing to {remote} ({}): the remote may have moved; run `lazyspec fetch` and retry",
            branch.unwrap_or("default branch")
        )
        })
}

/// The commit for a writer that rewrites a document file without going through
/// [`DocumentStore`] (`link`, `ignore`, `pin`, `fix`, the TUI's tag write).
/// `doc_path` is root-relative; anything outside `.lazyspec/cache/<type>/`, or
/// under a type whose store is not `git`, is not ours and is `Ok(())`. A clone
/// with nothing staged commits nothing, so callers that touch several files may
/// call this once per file.
pub fn commit_if_git_backed(
    root: &Path,
    config: &Config,
    doc_path: &Path,
    ops: &dyn GitRefOps,
    message: &str,
) -> Result<()> {
    if !doc_path.starts_with(".lazyspec/cache/") {
        return Ok(());
    }
    let type_def = doc_path
        .components()
        .nth(2)
        .and_then(|c| c.as_os_str().to_str())
        .and_then(|name| config.type_by_name(name));
    match type_def {
        Some(type_def) if type_def.store == StoreBackend::Git => {
            commit_clone(root, type_def, ops, message)
        }
        _ => Ok(()),
    }
}

/// [`FilesystemStore`] plus a commit: STORY-283 already points every git doc's
/// path into the clone, so the file writes are the filesystem store's, and this
/// store only adds the `commit_and_push` after each one (convention principle 6:
/// one write path).
pub struct GitStore {
    pub root: PathBuf,
    pub config: Config,
    pub ops: Box<dyn GitRefClient>,
}

impl GitStore {
    fn files(&self) -> FilesystemStore {
        FilesystemStore {
            root: self.root.clone(),
            config: self.config.clone(),
        }
    }

    fn commit(&self, type_def: &TypeDef, message: &str) -> Result<PushOutcome> {
        commit_clone(&self.root, type_def, &*self.ops, message)?;
        Ok(PushOutcome::Synced)
    }
}

impl DocumentStore for GitStore {
    fn create(
        &mut self,
        type_def: &TypeDef,
        title: &str,
        author: &str,
        body: &str,
    ) -> Result<CreatedDoc> {
        let clone_root = self.root.join(".lazyspec/cache").join(&type_def.name);
        let target = doc_root(&self.config, &self.root, type_def);
        let dir = target
            .strip_prefix(&self.root)
            .with_context(|| format!("{} is outside {}", target.display(), self.root.display()))?
            .to_string_lossy()
            .into_owned();
        let path = crate::engine::fs_ops::create_document(
            &self.root,
            &self.config,
            &type_def.name,
            &dir,
            &type_def.prefix,
            title,
            author,
            &type_def.numbering,
            type_def.subdirectory,
            Some(&clone_root),
            |_| {},
        )?;
        if !body.is_empty() {
            crate::engine::fs_ops::replace_body(&path, body)?;
        }
        let relative = path.strip_prefix(&self.root).unwrap_or(&path).to_path_buf();
        let id = crate::engine::store::extract_id(&relative);
        let push_outcome = self.commit(type_def, &format!("create {id}"))?;
        Ok(CreatedDoc {
            path: relative,
            id,
            push_outcome,
        })
    }

    fn update(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        updates: &[(&str, &str)],
    ) -> Result<PushOutcome> {
        self.files().update(type_def, doc_id, updates)?;
        self.commit(type_def, &format!("update {doc_id}"))
    }

    fn delete(&mut self, type_def: &TypeDef, doc_id: &str) -> Result<PushOutcome> {
        self.files().delete(type_def, doc_id)?;
        self.commit(type_def, &format!("delete {doc_id}"))
    }

    fn set_provenance(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        provenance: &[String],
    ) -> Result<PushOutcome> {
        self.files().set_provenance(type_def, doc_id, provenance)?;
        self.commit(type_def, &format!("provenance {doc_id}"))
    }

    /// The CLI has already rewritten the cache file's `tags`; only the commit
    /// remains.
    fn sync_tags(
        &mut self,
        type_def: &TypeDef,
        doc_id: &str,
        _add: &[String],
        _remove: &[String],
    ) -> Result<PushOutcome> {
        self.commit(type_def, &format!("tag {doc_id}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::StoreBackend;
    use crate::engine::git_ref::test_support::MockGitRefClient;
    use std::cell::RefCell;
    use std::path::Path;
    use std::rc::Rc;
    use tempfile::TempDir;

    const REMOTE: &str = "https://example.com/specs.git";
    const DOC: &str = ".lazyspec/cache/rfc/docs/rfcs/RFC-001-a.md";

    fn rfc_type(branch: Option<&str>) -> TypeDef {
        TypeDef {
            dir: "docs/rfcs".to_string(),
            remote: Some(REMOTE.to_string()),
            branch: branch.map(str::to_string),
            ..TypeDef::test_fixture("rfc", StoreBackend::Git)
        }
    }

    /// A project whose `rfc` clone already holds `RFC-001-a.md`, so `Store::load`
    /// finds the clone and never spawns git (DICTUM-004).
    fn project(type_def: &TypeDef) -> (TempDir, GitStore, Rc<RefCell<Vec<String>>>) {
        let tmp = TempDir::new().unwrap();
        let doc = tmp.path().join(DOC);
        std::fs::create_dir_all(doc.parent().unwrap()).unwrap();
        std::fs::write(
            &doc,
            "---\ntitle: \"A\"\ntype: rfc\nstatus: draft\nauthor: tester\ndate: 2026-04-01\ntags: []\n---\n\nA.\n",
        )
        .unwrap();
        let mut config = Config::default();
        config.documents.types = vec![type_def.clone()];
        let mock = MockGitRefClient::new();
        let calls = mock.call_log();
        let store = GitStore {
            root: tmp.path().to_path_buf(),
            config,
            ops: Box::new(mock),
        };
        (tmp, store, calls)
    }

    fn failing_project(
        type_def: &TypeDef,
        error: &str,
    ) -> (TempDir, GitStore, Rc<RefCell<Vec<String>>>) {
        let (tmp, mut store, calls) = project(type_def);
        store.ops = Box::new(
            MockGitRefClient::new().with_commit_and_push_result(Err(anyhow::anyhow!("{error}"))),
        );
        (tmp, store, calls)
    }

    fn commit_call(root: &Path, type_def: &TypeDef, message: &str) -> String {
        format!(
            "commit_and_push:{}/.lazyspec/cache/rfc:{}:{message}",
            root.display(),
            type_def.branch.as_deref().unwrap_or("default")
        )
    }

    #[test]
    fn update_rewrites_the_clone_file_and_commits_once() {
        let td = rfc_type(Some("next"));
        let (tmp, mut store, calls) = project(&td);

        let outcome = store
            .update(&td, "RFC-001", &[("title", "Renamed")])
            .unwrap();

        assert_eq!(outcome, PushOutcome::Synced);
        let content = std::fs::read_to_string(tmp.path().join(DOC)).unwrap();
        assert!(content.contains("title: Renamed"), "{content}");
        assert_eq!(
            *calls.borrow(),
            vec![commit_call(tmp.path(), &td, "update RFC-001")]
        );
    }

    #[test]
    fn create_writes_under_the_clone_applies_the_body_and_reports_a_root_relative_path() {
        let td = rfc_type(Some("next"));
        let (tmp, mut store, calls) = project(&td);

        let created = store
            .create(&td, "Second one", "tester", "Body text.")
            .unwrap();

        assert_eq!(created.id, "RFC-002");
        assert_eq!(created.push_outcome, PushOutcome::Synced);
        assert_eq!(
            created.path,
            Path::new(".lazyspec/cache/rfc/docs/rfcs/RFC-002-second-one.md")
        );
        let content = std::fs::read_to_string(tmp.path().join(&created.path)).unwrap();
        assert!(content.contains("Body text."), "{content}");
        assert!(!tmp.path().join("docs/rfcs").exists());
        assert_eq!(
            *calls.borrow(),
            vec![commit_call(tmp.path(), &td, "create RFC-002")]
        );
    }

    #[test]
    fn create_without_a_branch_pushes_to_the_default_branch() {
        let td = rfc_type(None);
        let (tmp, mut store, calls) = project(&td);

        store.create(&td, "Second", "tester", "").unwrap();

        assert_eq!(
            *calls.borrow(),
            vec![commit_call(tmp.path(), &td, "create RFC-002")]
        );
    }

    // STORY-282 AC4/AC5: a rejected push is an error naming remote, branch and
    // the fetch that resolves it -- never `LocalOnly`.
    #[test]
    fn rejected_push_errors_naming_remote_branch_and_fetch() {
        let td = rfc_type(Some("next"));
        let (_tmp, mut store, _calls) = failing_project(&td, "! [rejected]");

        let Err(err) = store.update(&td, "RFC-001", &[("title", "Renamed")]) else {
            panic!("a rejected push is an error");
        };

        let msg = format!("{err:#}");
        assert!(msg.contains(REMOTE), "{msg}");
        assert!(msg.contains("(next)"), "{msg}");
        assert!(msg.contains("lazyspec fetch"), "{msg}");
        assert!(msg.contains("! [rejected]"), "{msg}");
    }

    #[test]
    fn delete_removes_the_clone_file_and_commits_once() {
        let td = rfc_type(Some("next"));
        let (tmp, mut store, calls) = project(&td);

        store.delete(&td, "RFC-001").unwrap();

        assert!(!tmp.path().join(DOC).exists());
        assert_eq!(
            *calls.borrow(),
            vec![commit_call(tmp.path(), &td, "delete RFC-001")]
        );
    }

    // --- commit_if_git_backed: the direct writers' commit ---

    #[test]
    fn direct_write_under_a_git_type_commits_once() {
        let td = rfc_type(Some("next"));
        let (tmp, store, calls) = project(&td);

        commit_if_git_backed(
            tmp.path(),
            &store.config,
            Path::new(DOC),
            &*store.ops,
            "link RFC-001",
        )
        .unwrap();

        assert_eq!(
            *calls.borrow(),
            vec![commit_call(tmp.path(), &td, "link RFC-001")]
        );
    }

    #[test]
    fn direct_write_outside_the_cache_commits_nothing() {
        let td = rfc_type(Some("next"));
        let (tmp, store, calls) = project(&td);

        commit_if_git_backed(
            tmp.path(),
            &store.config,
            Path::new("docs/rfcs/RFC-001-a.md"),
            &*store.ops,
            "link RFC-001",
        )
        .unwrap();

        assert!(calls.borrow().is_empty());
    }

    #[test]
    fn direct_write_under_another_backend_commits_nothing() {
        let story = TypeDef::test_fixture("story", StoreBackend::GithubIssues);
        let (tmp, mut store, calls) = project(&story);
        store.config.documents.types = vec![story];

        commit_if_git_backed(
            tmp.path(),
            &store.config,
            Path::new(".lazyspec/cache/story/STORY-1.md"),
            &*store.ops,
            "link STORY-1",
        )
        .unwrap();

        assert!(calls.borrow().is_empty());
    }

    #[test]
    fn direct_write_rejected_push_errors_naming_remote_branch_and_fetch() {
        let td = rfc_type(Some("next"));
        let (tmp, store, _calls) = failing_project(&td, "! [rejected]");

        let Err(err) = commit_if_git_backed(
            tmp.path(),
            &store.config,
            Path::new(DOC),
            &*store.ops,
            "link RFC-001",
        ) else {
            panic!("a rejected push is an error");
        };

        let msg = format!("{err:#}");
        assert!(msg.contains(REMOTE), "{msg}");
        assert!(msg.contains("(next)"), "{msg}");
        assert!(msg.contains("lazyspec fetch"), "{msg}");
    }

    #[test]
    fn sync_tags_only_commits() {
        let td = rfc_type(Some("next"));
        let (tmp, mut store, calls) = project(&td);
        let before = std::fs::read(tmp.path().join(DOC)).unwrap();

        store
            .sync_tags(&td, "RFC-001", &["shared".to_string()], &[])
            .unwrap();

        assert_eq!(std::fs::read(tmp.path().join(DOC)).unwrap(), before);
        assert_eq!(
            *calls.borrow(),
            vec![commit_call(tmp.path(), &td, "tag RFC-001")]
        );
    }
}
