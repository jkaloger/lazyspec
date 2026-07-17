//! Resolve where a document opens externally (STORY-219): a GitHub web URL when
//! one exists, else the document's own file for a local viewer. Pure over its
//! inputs -- spawning the browser or viewer is the CLI/TUI layer's concern.

use std::path::PathBuf;

use crate::engine::config::Config;
use crate::engine::document::DocMeta;
use crate::engine::github_url::{github_url, GithubUrl, RepoCoords};
use crate::engine::issue_map::IssueMap;

/// Where `doc` opens: a resolved web [`OpenTarget::Url`] for a backend that has
/// one, or the [`OpenTarget::File`] path (relative to the repo root) for every
/// other case -- git-ref/clickup docs, and filesystem docs whose repo coords or
/// branch don't resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenTarget {
    Url(String),
    File(PathBuf),
}

/// Resolve the open target for `doc`: try [`github_url`] first (given resolved
/// repo `coords`), falling back to the document's file path when it yields no
/// URL -- an unresolved backend, a missing issue-map entry, or absent `coords`.
pub fn resolve_open_target(
    doc: &DocMeta,
    coords: Option<&RepoCoords>,
    config: &Config,
    issue_map: &IssueMap,
) -> OpenTarget {
    match coords.and_then(|c| github_url(doc, c, config, issue_map, None, None)) {
        Some(GithubUrl(url)) => OpenTarget::Url(url),
        None => OpenTarget::File(doc.path.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{GithubConfig, StoreBackend, TypeDef};
    use crate::engine::document::{DocType, Status};
    use chrono::NaiveDate;
    use std::collections::BTreeMap;

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
            id: id.to_string(),
            attributes: BTreeMap::new(),
        }
    }

    fn config_with_backends() -> Config {
        let mut config = Config::default();
        config.documents.github = Some(GithubConfig {
            repo: Some("acme/widgets".to_string()),
            cache_ttl: 60,
        });
        config.documents.types = vec![
            TypeDef::test_fixture("rfc", StoreBackend::Filesystem),
            TypeDef::test_fixture("story", StoreBackend::GithubIssues),
            TypeDef::test_fixture("ref", StoreBackend::GitRef),
            TypeDef::test_fixture("task", StoreBackend::ClickupTasks),
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
    fn filesystem_doc_resolves_to_blob_url() {
        let config = config_with_backends();
        let d = doc("RFC-1", "rfc", "docs/rfcs/RFC-1-x.md");
        let target = resolve_open_target(&d, Some(&coords()), &config, &IssueMap::default());
        assert_eq!(
            target,
            OpenTarget::Url(
                "https://github.com/acme/widgets/blob/main/docs/rfcs/RFC-1-x.md".to_string()
            )
        );
    }

    #[test]
    fn github_issues_doc_resolves_to_issue_url() {
        let config = config_with_backends();
        let d = doc("STORY-5", "story", "docs/stories/STORY-5.md");
        let mut map = IssueMap::default();
        map.insert("STORY-5", 42, "", "");
        let target = resolve_open_target(&d, Some(&coords()), &config, &map);
        assert_eq!(
            target,
            OpenTarget::Url("https://github.com/acme/widgets/issues/42".to_string())
        );
    }

    #[test]
    fn git_ref_doc_falls_back_to_file_path() {
        let config = config_with_backends();
        let d = doc("REF-1", "ref", "docs/ref/REF-1.md");
        let target = resolve_open_target(&d, Some(&coords()), &config, &IssueMap::default());
        assert_eq!(target, OpenTarget::File(PathBuf::from("docs/ref/REF-1.md")));
    }

    #[test]
    fn clickup_doc_falls_back_to_file_path() {
        let config = config_with_backends();
        let d = doc("TASK-1", "task", "docs/task/TASK-1.md");
        let target = resolve_open_target(&d, Some(&coords()), &config, &IssueMap::default());
        assert_eq!(
            target,
            OpenTarget::File(PathBuf::from("docs/task/TASK-1.md"))
        );
    }

    #[test]
    fn unresolved_coords_fall_back_to_file_path() {
        let config = config_with_backends();
        let d = doc("RFC-1", "rfc", "docs/rfcs/RFC-1-x.md");
        let target = resolve_open_target(&d, None, &config, &IssueMap::default());
        assert_eq!(
            target,
            OpenTarget::File(PathBuf::from("docs/rfcs/RFC-1-x.md"))
        );
    }
}
