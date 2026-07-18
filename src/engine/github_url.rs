//! GitHub deep-links for the read-only web view (RFC-052 / STORY-180).
//!
//! Two concerns, deliberately split so the mapping is unit-testable without a
//! git repo or network:
//!
//! - [`resolve_repo_coords`] resolves [`RepoCoords`] from config + git (touches
//!   `[web]` overrides, the `origin` remote, and the current branch). Returns
//!   `None` when owner or repo cannot be obtained.
//! - [`github_url`] is pure over `(doc, RepoCoords, ...)`: it dispatches on the
//!   document's resolved [`StoreBackend`] and produces the blob/issue/milestone
//!   deep-link, or `None` for every gap (no branch, no issue-map entry, an
//!   unsupported backend).
//!
//! The `url` crate is not a dependency, and pulling one in just to wrap these
//! strings is not warranted (the iteration's NOTE on the Url type), so the link
//! is modelled as the [`GithubUrl`] newtype and the maybe-link as
//! `Option<GithubUrl>` -- a typed Some/None, never a stringly empty-string
//! sentinel.

use std::path::Path;

use crate::engine::config::{Config, StoreBackend};
use crate::engine::document::DocMeta;
use crate::engine::gh::{GhIssue, GhMilestone};
use crate::engine::git_status::query_git_branch;
use crate::engine::github::infer_github_repo;
use crate::engine::issue_map::IssueMap;

/// A resolved GitHub URL. A newtype so a present link is distinct in the type
/// system from its absence (`Option<GithubUrl>`) and from any other string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubUrl(pub String);

impl GithubUrl {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for GithubUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The repo coordinates a deep-link is built against. `owner` and `repo` are
/// required for any link; `branch` is only needed for filesystem blob links, so
/// it is optional and its absence is a distinct `None` path from an unresolved
/// owner/repo (which yields no `RepoCoords` at all).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoCoords {
    pub owner: String,
    pub repo: String,
    pub branch: Option<String>,
}

impl RepoCoords {
    fn repo_base(&self) -> String {
        format!("https://github.com/{}/{}", self.owner, self.repo)
    }
}

/// Resolve repo coordinates: the `.lazyspec.toml` `[web]` table overrides each
/// field (any present field wins), otherwise owner/repo come from the `origin`
/// remote and branch from the current `HEAD`. Returns `None` when neither the
/// override nor the remote yields both owner and repo (no remote, or a remote
/// that doesn't parse) -- the caller then disables deep-links entirely. A
/// detached HEAD (no branch) is NOT fatal here: it only nulls the optional
/// `branch`, so issue/milestone links still resolve.
pub fn resolve_repo_coords(config: &Config, root: &Path) -> Option<RepoCoords> {
    let web = config.web.as_ref();
    let override_owner = web.and_then(|w| w.owner.clone());
    let override_repo = web.and_then(|w| w.repo.clone());
    let override_branch = web.and_then(|w| w.branch.clone());

    let (inferred_owner, inferred_repo) = match infer_github_repo(root) {
        Ok(owner_repo) => match owner_repo.split_once('/') {
            Some((o, r)) => (Some(o.to_string()), Some(r.to_string())),
            None => (None, None),
        },
        Err(_) => (None, None),
    };

    let owner = override_owner.or(inferred_owner)?;
    let repo = override_repo.or(inferred_repo)?;
    let branch = override_branch.or_else(|| query_git_branch(root));

    Some(RepoCoords {
        owner,
        repo,
        branch,
    })
}

/// The store backend a document resolves to, looked up via its `doc_type`
/// against the configured `[[types]]`. Unknown types fall back to the default
/// (`Filesystem`), matching how a `TypeDef.store` defaults when unset.
fn backend_for(doc: &DocMeta, config: &Config) -> StoreBackend {
    config
        .type_by_name(doc.doc_type.as_str())
        .map(|t| t.store.clone())
        .unwrap_or_default()
}

/// The GitHub deep-link for `doc`, pure over its inputs. Dispatches on the
/// document's resolved [`StoreBackend`]:
///
/// - [`StoreBackend::Filesystem`] -> `…/blob/{branch}/{path}` (path is
///   `DocMeta.path`, relative to the repo root). No branch -> `None`.
/// - [`StoreBackend::GithubIssues`] -> the cached [`GhIssue::url`] if present,
///   else `…/issues/{n}` from the [`IssueMap`]. No map entry -> `None`.
/// - [`StoreBackend::GithubMilestones`] -> the cached [`GhMilestone::url`] if
///   present, else `…/milestone/{n}` from the [`IssueMap`]. No map entry ->
///   `None`.
/// - any other backend ([`StoreBackend::GithubProjects`], [`StoreBackend::GitRef`])
///   -> `None` (no stable single-document URL).
///
/// `issue` / `milestone` carry the already-resolved cached URL for the
/// github-backed doc when available (e.g. from the issue/milestone cache);
/// pass `None` to fall back to constructing the URL from the issue map.
pub fn github_url(
    doc: &DocMeta,
    coords: &RepoCoords,
    config: &Config,
    issue_map: &IssueMap,
    issue: Option<&GhIssue>,
    milestone: Option<&GhMilestone>,
) -> Option<GithubUrl> {
    match backend_for(doc, config) {
        StoreBackend::Filesystem => {
            let branch = coords.branch.as_ref()?;
            let path = doc.path.to_str()?;
            Some(GithubUrl(format!(
                "{}/blob/{}/{}",
                coords.repo_base(),
                branch,
                path
            )))
        }
        StoreBackend::GithubIssues => {
            if let Some(url) = issue.map(|i| i.url.as_str()).filter(|u| !u.is_empty()) {
                return Some(GithubUrl(url.to_string()));
            }
            let entry = issue_map.get(&doc.id)?;
            Some(GithubUrl(format!(
                "{}/issues/{}",
                coords.repo_base(),
                entry.issue_number
            )))
        }
        StoreBackend::GithubMilestones => {
            if let Some(url) = milestone.map(|m| m.url.as_str()).filter(|u| !u.is_empty()) {
                return Some(GithubUrl(url.to_string()));
            }
            let entry = issue_map.get(&doc.id)?;
            Some(GithubUrl(format!(
                "{}/milestone/{}",
                coords.repo_base(),
                entry.issue_number
            )))
        }
        // GithubProjects / GitRef / ClickupTasks have no GitHub single-document
        // URL (ClickUp docs live in ClickUp, not GitHub).
        StoreBackend::GithubProjects | StoreBackend::GitRef | StoreBackend::ClickupTasks => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{GithubConfig, StoreBackend, TypeDef, WebConfig};
    use crate::engine::document::{DocMeta, DocType, Status};
    use crate::engine::issue_map::EntryKind;
    use chrono::NaiveDate;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn doc(id: &str, doc_type: &str, path: &str) -> DocMeta {
        DocMeta {
            path: PathBuf::from(path),
            title: "T".to_string(),
            doc_type: DocType::new(doc_type),
            status: Status::new("draft"),
            author: "a".to_string(),
            date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            id: id.to_string(),
            attributes: BTreeMap::new(),
        }
    }

    /// A config whose `story`/`milestone`/`project`/`ref` types map to the named
    /// backends, so `backend_for` resolves a doc by its type.
    fn config_with_backends() -> Config {
        let mut config = Config::default();
        config.documents.github = Some(GithubConfig {
            repo: Some("acme/widgets".to_string()),
            cache_ttl: 60,
        });
        config.documents.types = vec![
            TypeDef::test_fixture("rfc", StoreBackend::Filesystem),
            TypeDef::test_fixture("story", StoreBackend::GithubIssues),
            TypeDef::test_fixture("milestone", StoreBackend::GithubMilestones),
            TypeDef::test_fixture("project", StoreBackend::GithubProjects),
            TypeDef::test_fixture("ref", StoreBackend::GitRef),
        ];
        config
    }

    fn coords() -> RepoCoords {
        RepoCoords {
            owner: "acme".to_string(),
            repo: "widgets".to_string(),
            branch: Some("main".to_string()),
        }
    }

    #[test]
    fn filesystem_doc_yields_blob_url() {
        let config = config_with_backends();
        let d = doc("RFC-001", "rfc", "docs/rfcs/RFC-001-x.md");
        let url = github_url(&d, &coords(), &config, &IssueMap::default(), None, None).unwrap();
        assert_eq!(
            url.as_str(),
            "https://github.com/acme/widgets/blob/main/docs/rfcs/RFC-001-x.md"
        );
    }

    #[test]
    fn filesystem_doc_without_branch_yields_none() {
        let config = config_with_backends();
        let d = doc("RFC-001", "rfc", "docs/rfcs/RFC-001-x.md");
        let mut c = coords();
        c.branch = None;
        assert!(github_url(&d, &c, &config, &IssueMap::default(), None, None).is_none());
    }

    #[test]
    fn github_issues_doc_uses_cached_url_when_present() {
        let config = config_with_backends();
        let d = doc("STORY-5", "story", "docs/stories/STORY-5.md");
        let issue = GhIssue {
            number: 42,
            id: String::new(),
            url: "https://github.com/acme/widgets/issues/42".to_string(),
            title: String::new(),
            body: String::new(),
            labels: vec![],
            state: String::new(),
            updated_at: String::new(),
            created_at: String::new(),
            author: None,
            issue_type: None,
            milestone: None,
        };
        let url = github_url(
            &d,
            &coords(),
            &config,
            &IssueMap::default(),
            Some(&issue),
            None,
        )
        .unwrap();
        assert_eq!(url.as_str(), "https://github.com/acme/widgets/issues/42");
    }

    #[test]
    fn github_issues_doc_constructs_url_from_map() {
        let config = config_with_backends();
        let d = doc("STORY-5", "story", "docs/stories/STORY-5.md");
        let mut map = IssueMap::default();
        map.insert("STORY-5", 42, "", "");
        let url = github_url(&d, &coords(), &config, &map, None, None).unwrap();
        assert_eq!(url.as_str(), "https://github.com/acme/widgets/issues/42");
    }

    #[test]
    fn github_issues_doc_without_map_entry_yields_none() {
        let config = config_with_backends();
        let d = doc("STORY-5", "story", "docs/stories/STORY-5.md");
        assert!(github_url(&d, &coords(), &config, &IssueMap::default(), None, None).is_none());
    }

    #[test]
    fn github_milestones_doc_uses_cached_url_when_present() {
        let config = config_with_backends();
        let d = doc("MILESTONE-1", "milestone", "docs/milestones/MILESTONE-1.md");
        let milestone = GhMilestone {
            number: 3,
            url: "https://github.com/acme/widgets/milestone/3".to_string(),
            ..Default::default()
        };
        let url = github_url(
            &d,
            &coords(),
            &config,
            &IssueMap::default(),
            None,
            Some(&milestone),
        )
        .unwrap();
        assert_eq!(url.as_str(), "https://github.com/acme/widgets/milestone/3");
    }

    #[test]
    fn github_milestones_doc_constructs_url_from_map() {
        let config = config_with_backends();
        let d = doc("MILESTONE-1", "milestone", "docs/milestones/MILESTONE-1.md");
        let mut map = IssueMap::default();
        map.insert_kind("MILESTONE-1", 3, "", "", EntryKind::Milestone);
        let url = github_url(&d, &coords(), &config, &map, None, None).unwrap();
        assert_eq!(url.as_str(), "https://github.com/acme/widgets/milestone/3");
    }

    #[test]
    fn github_milestones_doc_without_map_entry_yields_none() {
        let config = config_with_backends();
        let d = doc("MILESTONE-1", "milestone", "docs/milestones/MILESTONE-1.md");
        assert!(github_url(&d, &coords(), &config, &IssueMap::default(), None, None).is_none());
    }

    #[test]
    fn github_projects_backend_yields_none() {
        let config = config_with_backends();
        let d = doc("PROJECT-1", "project", "docs/projects/PROJECT-1.md");
        let mut map = IssueMap::default();
        map.insert_kind("PROJECT-1", 7, "", "", EntryKind::Project);
        assert!(github_url(&d, &coords(), &config, &map, None, None).is_none());
    }

    #[test]
    fn git_ref_backend_yields_none() {
        let config = config_with_backends();
        let d = doc("REF-1", "ref", "docs/ref/REF-1.md");
        assert!(github_url(&d, &coords(), &config, &IssueMap::default(), None, None).is_none());
    }

    #[test]
    fn web_override_beats_origin() {
        // No git resolution touched: a `[web]` table that fully specifies
        // owner/repo/branch resolves without consulting the (nonexistent) remote.
        let config = Config {
            web: Some(WebConfig {
                owner: Some("override-owner".to_string()),
                repo: Some("override-repo".to_string()),
                branch: Some("override-branch".to_string()),
            }),
            ..Config::default()
        };
        let resolved = resolve_repo_coords(&config, Path::new("/nonexistent")).unwrap();
        assert_eq!(resolved.owner, "override-owner");
        assert_eq!(resolved.repo, "override-repo");
        assert_eq!(resolved.branch.as_deref(), Some("override-branch"));
    }

    #[test]
    fn unresolved_coords_when_no_remote_and_no_override() {
        // No `[web]` table and a path with no git remote -> owner/repo cannot be
        // obtained -> None (deep-links disabled).
        let config = Config::default();
        assert!(resolve_repo_coords(&config, Path::new("/nonexistent")).is_none());
    }

    #[test]
    fn unresolved_coords_when_owner_repo_partial_override_and_no_remote() {
        // `[web]` supplies only a branch; owner/repo still must come from origin,
        // which is absent -> None.
        let config = Config {
            web: Some(WebConfig {
                owner: None,
                repo: None,
                branch: Some("main".to_string()),
            }),
            ..Config::default()
        };
        assert!(resolve_repo_coords(&config, Path::new("/nonexistent")).is_none());
    }
}
