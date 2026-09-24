//! The `git` store (RFC-072 "The git store"): documents are files in a managed
//! clone shared by every type on one remote + branch (BUG-032 AC1), and every
//! write is a file write and a local commit -- publishing is the separate,
//! explicit `lazyspec push`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::engine::config::{Config, StoreBackend, TypeDef};
use crate::engine::git_ref::{GitRefClient, GitRefOps};
use crate::engine::store::doc_root;
use crate::engine::store_dispatch::{CreatedDoc, DocumentStore, FilesystemStore, PushOutcome};

/// Where every git type sharing `remote` + `branch` clones to:
/// `.lazyspec/git/<slug>` (BUG-032 AC1). Keyed on the repo, not the type name,
/// so two types declaring the same remote and branch resolve to one clone
/// regardless of how many `[[types]]` entries name it. `branch: None` (the
/// remote's default branch) and that same default branch named explicitly are
/// different slugs, so they clone separately.
pub fn clone_root(root: &Path, remote: &str, branch: Option<&str>) -> PathBuf {
    root.join(clone_relative(remote, branch))
}

/// [`clone_root`] for a configured git type, reading its own `remote` and
/// `branch`.
pub(crate) fn type_clone_root(root: &Path, type_def: &TypeDef) -> PathBuf {
    root.join(type_clone_relative(type_def))
}

/// [`clone_root`], root-relative -- what a root-relative doc path is checked
/// against to find its clone.
fn clone_relative(remote: &str, branch: Option<&str>) -> PathBuf {
    Path::new(".lazyspec/git").join(slug(remote, branch))
}

pub(crate) fn type_clone_relative(type_def: &TypeDef) -> PathBuf {
    let remote = type_def
        .remote
        .as_deref()
        .expect("Config::parse rejects a git store without a remote");
    clone_relative(remote, type_def.branch.as_deref())
}

/// A readable, filesystem-safe slug for `remote` + `branch`: the URL scheme or
/// `git@` is stripped, every other run of non-alphanumerics folds to one `-`,
/// and the branch (when pinned) is appended -- so the same repo spelled with a
/// different scheme, host case, or trailing `.git` still slugs identically.
///
/// The readable part alone collides: `feature/x` and `feature-x`, differing
/// case, and `org/a.b` vs `org/a-b` all fold to the same text. A trailing
/// `-<hash>` of the raw, unfolded `remote` + `branch` (BUG-032) keeps those
/// apart while leaving the slug readable.
fn slug(remote: &str, branch: Option<&str>) -> String {
    let mut slug = slugify(strip_remote_scheme(remote));
    if let Some(branch) = branch {
        slug.push_str("--");
        slug.push_str(&slugify(branch));
    }
    slug.push('-');
    slug.push_str(&fnv1a_hex(remote, branch));
    slug
}

/// FNV-1a (32-bit) of `remote` + a NUL separator + `branch` (empty for the
/// default branch), rendered as 8 lowercase hex digits. Hand-written rather
/// than `DefaultHasher` (unstable across Rust versions) or a hashing crate
/// (no dependency already carries one).
fn fnv1a_hex(remote: &str, branch: Option<&str>) -> String {
    const OFFSET_BASIS: u32 = 0x811c_9dc5;
    const PRIME: u32 = 0x0100_0193;
    let mut hash = OFFSET_BASIS;
    for byte in remote
        .bytes()
        .chain(std::iter::once(0))
        .chain(branch.unwrap_or("").bytes())
    {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    format!("{hash:08x}")
}

fn strip_remote_scheme(remote: &str) -> &str {
    for prefix in ["https://", "http://", "ssh://", "git@"] {
        if let Some(rest) = remote.strip_prefix(prefix) {
            return rest;
        }
    }
    remote
}

fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// Commit `type_def`'s clone locally. Never pushes (BUG-032 AC2): `lazyspec
/// push` is the separate step that publishes it.
fn commit_clone(root: &Path, type_def: &TypeDef, ops: &dyn GitRefOps, message: &str) -> Result<()> {
    ops.commit(&type_clone_root(root, type_def), message)
}

fn local_only_warning(clone: &Path) -> String {
    format!(
        "committed locally to {}; run `lazyspec push` to publish",
        clone.display()
    )
}

/// The configured `git` type whose clone `doc_path` (root-relative) falls
/// under, matched by clone-root prefix (BUG-032 AC1) rather than by decoding a
/// type name out of the path -- two types sharing a clone both resolve here.
fn git_type_for_doc_path<'a>(config: &'a Config, doc_path: &Path) -> Option<&'a TypeDef> {
    config
        .documents
        .types
        .iter()
        .filter(|t| t.store == StoreBackend::Git)
        .find(|t| doc_path.starts_with(type_clone_relative(t)))
}

/// The commit for a writer that rewrites a document file without going through
/// [`DocumentStore`] (`link`, `ignore`, `pin`, `fix`, the TUI's tag write).
/// A path under no git type's clone is not ours and is `Ok(())`. A clone with
/// nothing staged commits nothing, so callers that touch several files may
/// call this once per file.
pub fn commit_if_git_backed(
    root: &Path,
    config: &Config,
    doc_path: &Path,
    ops: &dyn GitRefOps,
    message: &str,
) -> Result<()> {
    match git_type_for_doc_path(config, doc_path) {
        Some(type_def) => commit_clone(root, type_def, ops, message),
        None => Ok(()),
    }
}

/// [`commit_if_git_backed`], reporting the push outcome: `LocalOnly` naming
/// the clone when `doc_path` is git-backed, `Synced` (a no-op) otherwise --
/// for a caller (`create --parent`) that hands the outcome on to `--json`
/// rather than discarding it.
pub fn commit_if_git_backed_outcome(
    root: &Path,
    config: &Config,
    doc_path: &Path,
    ops: &dyn GitRefOps,
    message: &str,
) -> Result<PushOutcome> {
    match git_type_for_doc_path(config, doc_path) {
        Some(type_def) => {
            let clone = type_clone_root(root, type_def);
            commit_clone(root, type_def, ops, message)?;
            Ok(PushOutcome::LocalOnly {
                warning: local_only_warning(&clone),
            })
        }
        None => Ok(PushOutcome::Synced),
    }
}

/// [`FilesystemStore`] plus a commit: STORY-283 already points every git doc's
/// path into the clone, so the file writes are the filesystem store's, and this
/// store only adds the local commit after each one (convention principle 6:
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

    /// Commit locally and report `LocalOnly` (BUG-032 AC2): a `git` write is
    /// never pushed here, so it is never `Synced` -- `lazyspec push` is what
    /// publishes it.
    fn commit(&self, type_def: &TypeDef, message: &str) -> Result<PushOutcome> {
        let clone = type_clone_root(&self.root, type_def);
        commit_clone(&self.root, type_def, &*self.ops, message)?;
        Ok(PushOutcome::LocalOnly {
            warning: local_only_warning(&clone),
        })
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
        let clone_root = type_clone_root(&self.root, type_def);
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
    use std::path::{Path, PathBuf};
    use std::rc::Rc;
    use tempfile::TempDir;

    const REMOTE: &str = "https://example.com/specs.git";

    fn rfc_type(branch: Option<&str>) -> TypeDef {
        TypeDef {
            dir: "docs/rfcs".to_string(),
            remote: Some(REMOTE.to_string()),
            branch: branch.map(str::to_string),
            ..TypeDef::test_fixture("rfc", StoreBackend::Git)
        }
    }

    /// `type_def`'s clone, root-relative -- same slug rule [`clone_root`] uses,
    /// so a test asserting a doc or a call log's clone path agrees with the
    /// store under test regardless of which branch it declares.
    fn clone_dir(root: &Path, type_def: &TypeDef) -> PathBuf {
        clone_root(root, REMOTE, type_def.branch.as_deref())
            .strip_prefix(root)
            .unwrap()
            .to_path_buf()
    }

    /// A project whose clone already holds `RFC-001-a.md`, so `Store::load`
    /// finds the clone and never spawns git (DICTUM-004).
    fn project(type_def: &TypeDef) -> (TempDir, GitStore, Rc<RefCell<Vec<String>>>, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let doc = clone_dir(tmp.path(), type_def).join("docs/rfcs/RFC-001-a.md");
        let full = tmp.path().join(&doc);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(
            &full,
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
        (tmp, store, calls, doc)
    }

    fn failing_project(
        type_def: &TypeDef,
        error: &str,
    ) -> (TempDir, GitStore, Rc<RefCell<Vec<String>>>, PathBuf) {
        let (tmp, mut store, calls, doc) = project(type_def);
        store.ops =
            Box::new(MockGitRefClient::new().with_commit_result(Err(anyhow::anyhow!("{error}"))));
        (tmp, store, calls, doc)
    }

    fn commit_call(root: &Path, type_def: &TypeDef, message: &str) -> String {
        format!(
            "commit:{}:{message}",
            root.join(clone_dir(root, type_def)).display()
        )
    }

    fn local_only(root: &Path, type_def: &TypeDef) -> PushOutcome {
        PushOutcome::LocalOnly {
            warning: format!(
                "committed locally to {}; run `lazyspec push` to publish",
                root.join(clone_dir(root, type_def)).display()
            ),
        }
    }

    #[test]
    fn update_rewrites_the_clone_file_and_commits_locally() {
        let td = rfc_type(Some("next"));
        let (tmp, mut store, calls, doc) = project(&td);

        let outcome = store
            .update(&td, "RFC-001", &[("title", "Renamed")])
            .unwrap();

        assert_eq!(outcome, local_only(tmp.path(), &td));
        let content = std::fs::read_to_string(tmp.path().join(&doc)).unwrap();
        assert!(content.contains("title: Renamed"), "{content}");
        assert_eq!(
            *calls.borrow(),
            vec![commit_call(tmp.path(), &td, "update RFC-001")]
        );
    }

    #[test]
    fn create_writes_under_the_clone_applies_the_body_and_reports_a_root_relative_path() {
        let td = rfc_type(Some("next"));
        let (tmp, mut store, calls, _doc) = project(&td);

        let created = store
            .create(&td, "Second one", "tester", "Body text.")
            .unwrap();

        assert_eq!(created.id, "RFC-002");
        assert_eq!(created.push_outcome, local_only(tmp.path(), &td));
        assert_eq!(
            created.path,
            clone_dir(tmp.path(), &td).join("docs/rfcs/RFC-002-second-one.md")
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
    fn create_without_a_branch_commits_locally() {
        let td = rfc_type(None);
        let (tmp, mut store, calls, _doc) = project(&td);

        store.create(&td, "Second", "tester", "").unwrap();

        assert_eq!(
            *calls.borrow(),
            vec![commit_call(tmp.path(), &td, "create RFC-002")]
        );
    }

    // BUG-032 AC2: a commit failure (a clean tree has nothing to reject, so
    // this is the local-only equivalent of the old rejected-push path) is
    // still an error, and never `LocalOnly`.
    #[test]
    fn commit_failure_errors() {
        let td = rfc_type(Some("next"));
        let (_tmp, mut store, _calls, _doc) = failing_project(&td, "git commit failed: bad tree");

        let err = store
            .update(&td, "RFC-001", &[("title", "Renamed")])
            .unwrap_err();

        assert!(format!("{err:#}").contains("git commit failed"), "{err:#}");
    }

    #[test]
    fn delete_removes_the_clone_file_and_commits_locally() {
        let td = rfc_type(Some("next"));
        let (tmp, mut store, calls, doc) = project(&td);

        store.delete(&td, "RFC-001").unwrap();

        assert!(!tmp.path().join(&doc).exists());
        assert_eq!(
            *calls.borrow(),
            vec![commit_call(tmp.path(), &td, "delete RFC-001")]
        );
    }

    // --- commit_if_git_backed: the direct writers' commit ---

    #[test]
    fn direct_write_under_a_git_type_commits_once() {
        let td = rfc_type(Some("next"));
        let (tmp, store, calls, doc) = project(&td);

        commit_if_git_backed(tmp.path(), &store.config, &doc, &*store.ops, "link RFC-001").unwrap();

        assert_eq!(
            *calls.borrow(),
            vec![commit_call(tmp.path(), &td, "link RFC-001")]
        );
    }

    #[test]
    fn direct_write_outside_the_cache_commits_nothing() {
        let td = rfc_type(Some("next"));
        let (tmp, store, calls, _doc) = project(&td);

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
        let (tmp, mut store, calls, _doc) = project(&story);
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
    fn direct_write_commit_failure_errors() {
        let td = rfc_type(Some("next"));
        let (tmp, store, _calls, doc) = failing_project(&td, "git commit failed: bad tree");

        let err =
            commit_if_git_backed(tmp.path(), &store.config, &doc, &*store.ops, "link RFC-001")
                .unwrap_err();

        assert!(format!("{err:#}").contains("git commit failed"), "{err:#}");
    }

    #[test]
    fn sync_tags_only_commits() {
        let td = rfc_type(Some("next"));
        let (tmp, mut store, calls, doc) = project(&td);
        let before = std::fs::read(tmp.path().join(&doc)).unwrap();

        store
            .sync_tags(&td, "RFC-001", &["shared".to_string()], &[])
            .unwrap();

        assert_eq!(std::fs::read(tmp.path().join(&doc)).unwrap(), before);
        assert_eq!(
            *calls.borrow(),
            vec![commit_call(tmp.path(), &td, "tag RFC-001")]
        );
    }

    // BUG-032 AC1: two types on the same remote + branch resolve to one clone,
    // regardless of their own name or `dir`; a different branch or remote is a
    // different clone.
    #[test]
    fn clone_root_is_keyed_on_remote_and_branch_not_the_type() {
        let root = Path::new("/proj");

        assert_eq!(
            clone_root(root, REMOTE, Some("next")),
            clone_root(root, REMOTE, Some("next")),
        );
        assert_ne!(
            clone_root(root, REMOTE, Some("next")),
            clone_root(root, REMOTE, Some("other")),
        );
        assert_ne!(
            clone_root(root, REMOTE, Some("next")),
            clone_root(root, "https://example.com/other.git", Some("next")),
        );
    }

    // BUG-032: the readable slug alone folds `feature/x` and `feature-x` (and
    // case/`.`-vs-`-` spellings of a remote) to the same text; the trailing
    // hash of the raw remote + branch keeps them apart. Stable and literal, so
    // a future change to the algorithm shows up as a diff here.
    #[test]
    fn slug_hash_separates_folded_collisions_and_is_stable() {
        let root = Path::new("/proj");

        assert_ne!(
            clone_root(root, REMOTE, Some("feature/x")),
            clone_root(root, REMOTE, Some("feature-x")),
        );
        assert_ne!(
            clone_root(root, "https://Example.com/org/a.b.git", None),
            clone_root(root, "https://example.com/org/a-b.git", None),
        );
        assert_eq!(
            clone_root(root, REMOTE, Some("next")),
            Path::new("/proj/.lazyspec/git/example-com-specs-git--next-b14d63f7"),
        );
    }
}
