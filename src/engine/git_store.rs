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

/// The URL-`extends` clone (BUG-032 AC10): a commit target parallel to a
/// `git` type's, but one clone shared by every `filesystem` type instead of
/// one keyed by remote + branch (RFC-072 Decision 4: only `filesystem`
/// resolution moves under `extends`). `None` for a local-directory `extends`
/// (nothing lazyspec clones there) or no `extends` at all.
fn extends_clone_root(config: &Config) -> Option<&Path> {
    config
        .extends
        .as_ref()
        .filter(|e| e.remote.is_some())
        .map(|e| e.root.as_path())
}

/// The clone `doc_path` should commit to: a `git` type's by prefix match, or
/// the URL-`extends` clone when no `git` type claims it and `doc_path` falls
/// under it. `doc_path` is root-relative for every backend except a
/// `filesystem` type under `extends`, which is already absolute
/// (`path_root_for_relativizing`); resolving against `root` before the prefix
/// check handles both shapes alike. `None` when `doc_path` belongs to
/// neither.
fn clone_for_doc_path(root: &Path, config: &Config, doc_path: &Path) -> Option<PathBuf> {
    if let Some(type_def) = git_type_for_doc_path(config, doc_path) {
        return Some(type_clone_root(root, type_def));
    }
    let extends_root = extends_clone_root(config)?;
    let absolute = if doc_path.is_absolute() {
        doc_path.to_path_buf()
    } else {
        root.join(doc_path)
    };
    absolute
        .starts_with(extends_root)
        .then(|| extends_root.to_path_buf())
}

/// The commit for a writer that rewrites a document file without going through
/// [`DocumentStore`] (`link`, `ignore`, `pin`, `fix`, the TUI's tag write).
/// A path under no git type's clone and no `extends` clone is not ours and is
/// `Ok(())`. A clone with nothing staged commits nothing, so callers that
/// touch several files may call this once per file.
pub fn commit_if_git_backed(
    root: &Path,
    config: &Config,
    doc_path: &Path,
    ops: &dyn GitRefOps,
    message: &str,
) -> Result<()> {
    match clone_for_doc_path(root, config, doc_path) {
        Some(clone) => ops.commit(&clone, message),
        None => Ok(()),
    }
}

/// [`commit_if_git_backed`], reporting the push outcome: `LocalOnly` naming
/// the clone when `doc_path` is git- or `extends`-backed, `Synced` (a no-op)
/// otherwise -- for a caller (`create --parent`) that hands the outcome on to
/// `--json` rather than discarding it.
pub fn commit_if_git_backed_outcome(
    root: &Path,
    config: &Config,
    doc_path: &Path,
    ops: &dyn GitRefOps,
    message: &str,
) -> Result<PushOutcome> {
    match clone_for_doc_path(root, config, doc_path) {
        Some(clone) => {
            ops.commit(&clone, message)?;
            Ok(PushOutcome::LocalOnly {
                warning: local_only_warning(&clone),
            })
        }
        None => Ok(PushOutcome::Synced),
    }
}

/// [`commit_if_git_backed_outcome`], keyed by `type_def` rather than a doc
/// path already on disk: every `filesystem` type's documents share the one
/// `extends` clone (RFC-072 Decision 4), so a caller that has not yet
/// resolved a specific document -- `create`, or a backend dispatch (like
/// [`FilesystemStore`]) driven by a doc id, not a path -- can commit there
/// directly. Called by every writer that rewrites a document without a path
/// in hand (`create`, `update`, `delete`, `set_provenance`, `sync_tags`), so
/// none of them repeats this reasoning at its own call site.
///
/// `Synced` for every backend but a `filesystem` type whose resolved
/// [`doc_root`] actually falls under the `extends` clone -- a `git` type
/// commits through [`GitStore`] instead, and a `filesystem` type is `Synced`
/// with no `extends` clone at all, or with one that its own `dir` does not
/// resolve into (BUG-032 AC6). `Synced`, with no warning, also when the
/// clone had nothing to stage: only a real commit means "publish this with
/// `lazyspec push`".
pub fn commit_if_extends_backed(
    root: &Path,
    config: &Config,
    type_def: &TypeDef,
    ops: &dyn GitRefOps,
    message: &str,
) -> Result<PushOutcome> {
    if type_def.store != StoreBackend::Filesystem {
        return Ok(PushOutcome::Synced);
    }
    let Some(clone) = extends_clone_root(config) else {
        return Ok(PushOutcome::Synced);
    };
    if !doc_root(config, root, type_def).starts_with(clone) {
        return Ok(PushOutcome::Synced);
    }
    if !ops.has_uncommitted_changes(clone)? {
        return Ok(PushOutcome::Synced);
    }
    ops.commit(clone, message)?;
    Ok(PushOutcome::LocalOnly {
        warning: local_only_warning(clone),
    })
}

/// One old per-type git clone (`.lazyspec/cache/<type>/`, pre-BUG-032)
/// [`migrate_legacy_clones`] found clean and deleted -- superseded by the
/// shared clone `Store::load` now clones at `.lazyspec/git/<slug>` (AC1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedLegacyClone {
    pub type_name: String,
    pub path: PathBuf,
}

/// Delete every configured `git` type's old per-type clone that has nothing
/// unpushed and no uncommitted changes (BUG-032 AC8) -- `fetch` runs this so a
/// project upgrading past the per-type-clone era cleans up after itself. A
/// clone that is not clean is left alone and named in a returned warning
/// instead, for the human to push or copy the work out of and delete by hand.
/// A clone whose cleanliness cannot even be checked (a git error) is treated
/// the same way -- kept, warned about -- rather than failing `fetch` outright.
///
/// Only `.lazyspec/cache/<name>` for a type that is *itself* `git` is ever a
/// candidate: a cache-backed type (`github-issues` et al.) legitimately owns
/// `.lazyspec/cache/<name>` as its materialized cache. A `git` type literally
/// named `config` is skipped too, since its legacy path would collide with
/// `.lazyspec/cache/config`, the URL-`extends` clone (BUG-032 AC10) -- that
/// one is never a migration candidate, named or not.
pub fn migrate_legacy_clones(
    root: &Path,
    config: &Config,
    ops: &dyn GitRefOps,
) -> Result<(Vec<RemovedLegacyClone>, Vec<String>)> {
    let mut removed = Vec::new();
    let mut warnings = Vec::new();
    let extends_clone = extends_clone_root(config);
    for type_def in config
        .documents
        .types
        .iter()
        .filter(|t| t.store == StoreBackend::Git)
    {
        let legacy = root.join(".lazyspec/cache").join(&type_def.name);
        if !legacy.join(".git").exists() {
            continue;
        }
        if extends_clone == Some(legacy.as_path()) {
            continue;
        }
        let dirty = match ops.has_uncommitted_changes(&legacy) {
            Ok(dirty) => dirty,
            Err(err) => {
                warnings.push(format!(
                    "legacy clone for type `{}` at {}: could not verify it is clean ({err:#}); delete it by hand if unneeded",
                    type_def.name,
                    legacy.display(),
                ));
                continue;
            }
        };
        let unpushed = match ops.unpushed(&legacy, type_def.branch.as_deref()) {
            Ok(unpushed) => unpushed,
            Err(err) => {
                warnings.push(format!(
                    "legacy clone for type `{}` at {}: could not verify it is clean ({err:#}); delete it by hand if unneeded",
                    type_def.name,
                    legacy.display(),
                ));
                continue;
            }
        };
        if !dirty && unpushed == 0 {
            std::fs::remove_dir_all(&legacy)
                .with_context(|| format!("removing legacy clone {}", legacy.display()))?;
            removed.push(RemovedLegacyClone {
                type_name: type_def.name.clone(),
                path: legacy,
            });
            continue;
        }
        let reason = match (dirty, unpushed) {
            (true, 0) => "uncommitted changes".to_string(),
            (false, n) => format!("{n} unpushed commit{}", if n == 1 { "" } else { "s" }),
            (true, n) => format!(
                "uncommitted changes and {n} unpushed commit{}",
                if n == 1 { "" } else { "s" }
            ),
        };
        warnings.push(format!(
            "legacy clone for type `{}` at {} has {reason}; push it or copy the changes out, then delete it by hand",
            type_def.name,
            legacy.display(),
        ));
    }
    Ok((removed, warnings))
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

    // --- migrate_legacy_clones (BUG-032 AC8) ---

    fn write_legacy_clone(root: &Path, type_name: &str) -> PathBuf {
        let legacy = root.join(".lazyspec/cache").join(type_name);
        std::fs::create_dir_all(legacy.join(".git")).unwrap();
        legacy
    }

    #[test]
    fn a_clean_legacy_clone_is_removed_and_reported() {
        let tmp = TempDir::new().unwrap();
        let td = rfc_type(Some("next"));
        let mut config = Config::default();
        config.documents.types = vec![td];
        let legacy = write_legacy_clone(tmp.path(), "rfc");

        let ops = MockGitRefClient::new()
            .with_has_uncommitted_changes_result(Ok(false))
            .with_unpushed_result(Ok(0));

        let (removed, warnings) = migrate_legacy_clones(tmp.path(), &config, &ops).unwrap();

        assert_eq!(
            removed,
            vec![RemovedLegacyClone {
                type_name: "rfc".to_string(),
                path: legacy.clone(),
            }]
        );
        assert!(warnings.is_empty());
        assert!(!legacy.exists());
    }

    #[test]
    fn a_legacy_clone_with_an_unpushed_commit_is_kept_and_warned() {
        let tmp = TempDir::new().unwrap();
        let td = rfc_type(Some("next"));
        let mut config = Config::default();
        config.documents.types = vec![td];
        let legacy = write_legacy_clone(tmp.path(), "rfc");

        let ops = MockGitRefClient::new()
            .with_has_uncommitted_changes_result(Ok(false))
            .with_unpushed_result(Ok(2));

        let (removed, warnings) = migrate_legacy_clones(tmp.path(), &config, &ops).unwrap();

        assert!(removed.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].contains(&legacy.display().to_string()),
            "{warnings:?}"
        );
        assert!(warnings[0].contains("2 unpushed commits"), "{warnings:?}");
        assert!(legacy.exists());
    }

    #[test]
    fn a_legacy_clone_with_uncommitted_changes_is_kept_and_warned() {
        let tmp = TempDir::new().unwrap();
        let td = rfc_type(Some("next"));
        let mut config = Config::default();
        config.documents.types = vec![td];
        let legacy = write_legacy_clone(tmp.path(), "rfc");

        let ops = MockGitRefClient::new()
            .with_has_uncommitted_changes_result(Ok(true))
            .with_unpushed_result(Ok(0));

        let (removed, warnings) = migrate_legacy_clones(tmp.path(), &config, &ops).unwrap();

        assert!(removed.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("uncommitted changes"), "{warnings:?}");
        assert!(legacy.exists());
    }

    // A legacy clone whose cleanliness cannot even be checked (a git error --
    // a corrupt repo, an unreadable remote-tracking ref) is kept and warned
    // about, exactly like one confirmed dirty: `fetch` never fails outright
    // over an old clone `Store::load` has already stopped using.
    #[test]
    fn a_legacy_clone_whose_cleanliness_check_errors_is_kept_and_warned() {
        let tmp = TempDir::new().unwrap();
        let td = rfc_type(Some("next"));
        let mut config = Config::default();
        config.documents.types = vec![td];
        let legacy = write_legacy_clone(tmp.path(), "rfc");

        let ops = MockGitRefClient::new()
            .with_has_uncommitted_changes_result(Ok(false))
            .with_unpushed_result(Err(anyhow::anyhow!("git rev-list failed: bad revision")));

        let (removed, warnings) = migrate_legacy_clones(tmp.path(), &config, &ops).unwrap();

        assert!(removed.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].contains("could not verify it is clean"),
            "{warnings:?}"
        );
        assert!(
            warnings[0].contains("git rev-list failed: bad revision"),
            "{warnings:?}"
        );
        assert!(legacy.exists());
    }

    // BUG-032 AC10: a `git` type literally named `config` would otherwise
    // legacy-migrate at the same path as the URL-`extends` clone; skipped
    // entirely rather than risking that live clone.
    #[test]
    fn a_legacy_clone_path_matching_the_extends_clone_root_is_never_touched() {
        let tmp = TempDir::new().unwrap();
        let td = TypeDef {
            name: "config".to_string(),
            ..rfc_type(Some("next"))
        };
        let mut config = Config {
            extends: Some(crate::engine::config::Extends {
                root: tmp.path().join(".lazyspec/cache/config"),
                remote: Some("https://example.com/shared.git".to_string()),
                branch: None,
            }),
            ..Config::default()
        };
        config.documents.types = vec![td];
        let legacy = write_legacy_clone(tmp.path(), "config");

        let ops = MockGitRefClient::new();
        let calls = ops.call_log();

        let (removed, warnings) = migrate_legacy_clones(tmp.path(), &config, &ops).unwrap();

        assert!(removed.is_empty());
        assert!(warnings.is_empty());
        assert!(legacy.exists());
        assert!(calls.borrow().is_empty());
    }

    // A cache-backed (non-`git`) type legitimately owns `.lazyspec/cache/<name>`
    // as its materialized cache -- migration must never look at it, even when
    // it happens to hold a `.git` marker.
    #[test]
    fn a_non_git_types_cache_dir_is_left_untouched() {
        let tmp = TempDir::new().unwrap();
        let story = TypeDef::test_fixture("story", StoreBackend::GithubIssues);
        let mut config = Config::default();
        config.documents.types = vec![story];
        let cache_dir = write_legacy_clone(tmp.path(), "story");

        let ops = MockGitRefClient::new();
        let calls = ops.call_log();

        let (removed, warnings) = migrate_legacy_clones(tmp.path(), &config, &ops).unwrap();

        assert!(removed.is_empty());
        assert!(warnings.is_empty());
        assert!(cache_dir.exists());
        assert!(calls.borrow().is_empty());
    }

    // --- extends writes as a commit target (BUG-032 AC10) ---

    fn url_extends(root: &Path) -> Config {
        Config {
            extends: Some(crate::engine::config::Extends {
                root: root.join(".lazyspec/cache/config"),
                remote: Some("https://example.com/shared.git".to_string()),
                branch: Some("next".to_string()),
            }),
            ..Config::default()
        }
    }

    #[test]
    fn commit_if_git_backed_outcome_commits_a_filesystem_doc_under_a_url_extends() {
        let tmp = TempDir::new().unwrap();
        let config = url_extends(tmp.path());
        let doc_path = Path::new(".lazyspec/cache/config/docs/rfcs/RFC-001-a.md");
        let mock = MockGitRefClient::new();
        let calls = mock.call_log();

        let outcome =
            commit_if_git_backed_outcome(tmp.path(), &config, doc_path, &mock, "link RFC-001")
                .unwrap();

        let clone = tmp.path().join(".lazyspec/cache/config");
        assert_eq!(
            outcome,
            PushOutcome::LocalOnly {
                warning: local_only_warning(&clone),
            }
        );
        assert_eq!(
            *calls.borrow(),
            vec![format!("commit:{}:link RFC-001", clone.display())]
        );
    }

    #[test]
    fn commit_if_git_backed_outcome_ignores_a_directory_extends() {
        let tmp = TempDir::new().unwrap();
        let config = Config {
            extends: Some(crate::engine::config::Extends {
                root: tmp.path().join("shared"),
                remote: None,
                branch: None,
            }),
            ..Config::default()
        };
        let doc_path = Path::new("shared/docs/rfcs/RFC-001-a.md");
        let mock = MockGitRefClient::new();
        let calls = mock.call_log();

        let outcome =
            commit_if_git_backed_outcome(tmp.path(), &config, doc_path, &mock, "link RFC-001")
                .unwrap();

        assert_eq!(outcome, PushOutcome::Synced);
        assert!(calls.borrow().is_empty());
    }

    #[test]
    fn commit_if_extends_backed_commits_a_filesystem_type_under_a_url_extends() {
        let tmp = TempDir::new().unwrap();
        let config = url_extends(tmp.path());
        let td = TypeDef::test_fixture("rfc", StoreBackend::Filesystem);
        let mock = MockGitRefClient::new().with_has_uncommitted_changes_result(Ok(true));
        let calls = mock.call_log();

        let outcome =
            commit_if_extends_backed(tmp.path(), &config, &td, &mock, "tag RFC-001").unwrap();

        let clone = tmp.path().join(".lazyspec/cache/config");
        assert_eq!(
            outcome,
            PushOutcome::LocalOnly {
                warning: local_only_warning(&clone),
            }
        );
        assert_eq!(
            *calls.borrow(),
            vec![
                format!("has_uncommitted_changes:{}", clone.display()),
                format!("commit:{}:tag RFC-001", clone.display()),
            ]
        );
    }

    // BUG-032 AC6: nothing to publish is not "committed locally" -- a clean
    // clone (the caller's write left nothing staged, or didn't change
    // anything) reports `Synced` and never runs `commit` at all.
    #[test]
    fn commit_if_extends_backed_is_synced_when_the_clone_has_nothing_staged() {
        let tmp = TempDir::new().unwrap();
        let config = url_extends(tmp.path());
        let td = TypeDef::test_fixture("rfc", StoreBackend::Filesystem);
        let mock = MockGitRefClient::new().with_has_uncommitted_changes_result(Ok(false));
        let calls = mock.call_log();

        let outcome =
            commit_if_extends_backed(tmp.path(), &config, &td, &mock, "tag RFC-001").unwrap();

        assert_eq!(outcome, PushOutcome::Synced);
        assert!(
            !calls.borrow().iter().any(|c| c.starts_with("commit:")),
            "{:?}",
            calls.borrow()
        );
    }

    // BUG-032 AC6: a `filesystem` type whose own `dir` resolves outside the
    // `extends` clone (escaping it with `..`) has nothing to commit there --
    // gated on `doc_root`, not just "an `extends` clone exists".
    #[test]
    fn commit_if_extends_backed_is_synced_for_a_type_whose_dir_escapes_the_extends_clone() {
        let tmp = TempDir::new().unwrap();
        let config = url_extends(tmp.path());
        let td = TypeDef {
            dir: "../elsewhere".to_string(),
            ..TypeDef::test_fixture("rfc", StoreBackend::Filesystem)
        };
        let mock = MockGitRefClient::new();
        let calls = mock.call_log();

        let outcome =
            commit_if_extends_backed(tmp.path(), &config, &td, &mock, "tag RFC-001").unwrap();

        assert_eq!(outcome, PushOutcome::Synced);
        assert!(calls.borrow().is_empty());
    }

    #[test]
    fn commit_if_extends_backed_is_synced_for_a_git_type() {
        let tmp = TempDir::new().unwrap();
        let config = url_extends(tmp.path());
        let td = rfc_type(Some("next"));
        let mock = MockGitRefClient::new();
        let calls = mock.call_log();

        let outcome =
            commit_if_extends_backed(tmp.path(), &config, &td, &mock, "tag RFC-001").unwrap();

        assert_eq!(outcome, PushOutcome::Synced);
        assert!(calls.borrow().is_empty());
    }

    #[test]
    fn commit_if_extends_backed_is_synced_with_no_extends() {
        let tmp = TempDir::new().unwrap();
        let td = TypeDef::test_fixture("rfc", StoreBackend::Filesystem);
        let mock = MockGitRefClient::new();

        let outcome =
            commit_if_extends_backed(tmp.path(), &Config::default(), &td, &mock, "tag RFC-001")
                .unwrap();

        assert_eq!(outcome, PushOutcome::Synced);
    }
}
