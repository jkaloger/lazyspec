use crate::engine::config::{Config, StoreBackend};
use crate::engine::gh::{AuthStatus, GhAuth, GhIssueReader};
use crate::engine::github::infer_github_repo;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use anyhow::{bail, Result};
use std::path::Path;

pub fn run(root: &Path, config: &Config, gh: &(impl GhIssueReader + GhAuth)) -> Result<()> {
    let gh_types = github_issues_types(config);
    if gh_types.is_empty() {
        println!("No github-issues types configured; nothing to set up.");
        return Ok(());
    }

    let auth = gh.auth_status()?;
    match &auth {
        AuthStatus::GhNotInstalled => {
            bail!("gh CLI is not installed. Install it from https://cli.github.com/");
        }
        AuthStatus::NotAuthenticated(msg) => {
            bail!("gh auth failed: {}\nRun `gh auth login` to authenticate.", msg);
        }
        AuthStatus::Authenticated { user, host } => {
            println!("Authenticated as {} on {}", user, host);
        }
    }

    let repo = resolve_repo(config, root)?;
    let mut issue_map = IssueMap::load(root)?;
    let cache = IssueCache::new(root);

    for type_name in &gh_types {
        let type_def = config
            .type_by_name(type_name)
            .ok_or_else(|| anyhow::anyhow!("type '{}' not found in config", type_name))?;

        let result = cache.fetch_all(root, type_def, gh, &repo, &mut issue_map)?;

        println!(
            "Fetched {} {} issue{}",
            result.fetched,
            type_name,
            if result.fetched == 1 { "" } else { "s" }
        );
    }

    issue_map.save(root)?;
    println!("Wrote issue map to .lazyspec/issue-map.json");
    Ok(())
}

fn github_issues_types(config: &Config) -> Vec<&str> {
    config
        .documents
        .types
        .iter()
        .filter(|t| t.store == StoreBackend::GithubIssues)
        .map(|t| t.name.as_str())
        .collect()
}

fn resolve_repo(config: &Config, root: &Path) -> Result<String> {
    if let Some(ref gh) = config.documents.github {
        if let Some(ref repo) = gh.repo {
            return Ok(repo.clone());
        }
    }
    match infer_github_repo(root) {
        Ok(repo) => Ok(repo),
        Err(_) => bail!("Could not determine GitHub repo. Set [documents.github].repo in .lazyspec.toml"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{GithubConfig, NumberingStrategy, TypeDef};
    use crate::engine::gh::{GhAuth, GhIssue, GhIssueReader, GhLabel};
    use std::fs;

    fn make_type(name: &str, store: StoreBackend) -> TypeDef {
        TypeDef {
            name: name.to_string(),
            plural: format!("{}s", name),
            dir: format!("docs/{}", name),
            prefix: name.to_uppercase(),
            icon: None,
            numbering: NumberingStrategy::default(),
            subdirectory: false,
            store,
        }
    }

    fn gh_issues_config() -> Config {
        let mut config = Config::default();
        config.documents.types = vec![
            make_type("rfc", StoreBackend::Filesystem),
            make_type("story", StoreBackend::GithubIssues),
        ];
        config.documents.github = Some(GithubConfig {
            repo: Some("owner/repo".to_string()),
            cache_ttl: 60,
        });
        config
    }

    fn make_issue(number: u64, title: &str, body: &str, labels: &[&str]) -> GhIssue {
        GhIssue {
            number,
            url: format!("https://github.com/owner/repo/issues/{}", number),
            title: title.to_string(),
            body: body.to_string(),
            labels: labels
                .iter()
                .map(|l| GhLabel {
                    name: l.to_string(),
                    color: String::new(),
                })
                .collect(),
            state: "OPEN".to_string(),
            updated_at: "2026-03-27T10:00:00Z".to_string(),
        }
    }

    // --- github_issues_types ---

    #[test]
    fn filters_github_issues_types() {
        let config = gh_issues_config();
        let types = github_issues_types(&config);
        assert_eq!(types, vec!["story"]);
    }

    #[test]
    fn no_github_issues_types_for_filesystem_only() {
        let config = Config::default();
        let types = github_issues_types(&config);
        assert!(types.is_empty());
    }

    // --- issue_map via IssueMap ---

    #[test]
    fn issue_map_roundtrips_via_issue_map() {
        let dir = tempfile::tempdir().unwrap();
        let mut map = IssueMap::load(dir.path()).unwrap();
        map.insert("ITERATION-042", 87, "2026-03-27T10:00:00Z");
        map.save(dir.path()).unwrap();

        let loaded = IssueMap::load(dir.path()).unwrap();
        let entry = loaded.get("ITERATION-042").unwrap();
        assert_eq!(entry.issue_number, 87);
    }

    // --- run with mock ---

    struct SetupMockGh {
        auth: AuthStatus,
        issues: Vec<GhIssue>,
    }

    impl GhIssueReader for SetupMockGh {
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
            unimplemented!()
        }
    }

    impl GhAuth for SetupMockGh {
        fn auth_status(&self) -> Result<AuthStatus> {
            Ok(self.auth.clone())
        }
    }

    #[test]
    fn run_fails_when_gh_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        let config = gh_issues_config();
        let gh = SetupMockGh {
            auth: AuthStatus::GhNotInstalled,
            issues: vec![],
        };
        let result = run(dir.path(), &config, &gh);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not installed"));
    }

    #[test]
    fn run_fails_when_not_authenticated() {
        let dir = tempfile::tempdir().unwrap();
        let config = gh_issues_config();
        let gh = SetupMockGh {
            auth: AuthStatus::NotAuthenticated("not logged in".to_string()),
            issues: vec![],
        };
        let result = run(dir.path(), &config, &gh);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("auth failed"));
    }

    #[test]
    fn run_creates_cache_and_issue_map() {
        let dir = tempfile::tempdir().unwrap();
        let config = gh_issues_config();
        let gh = SetupMockGh {
            auth: AuthStatus::Authenticated {
                user: "testuser".to_string(),
                host: "github.com".to_string(),
            },
            issues: vec![
                make_issue(10, "STORY-001 First story", "Body 1", &["lazyspec:story"]),
                make_issue(11, "STORY-002 Second story", "Body 2", &["lazyspec:story"]),
            ],
        };

        run(dir.path(), &config, &gh).unwrap();

        // Cache files use doc ID, not issue number
        let cache_dir = dir.path().join(".lazyspec/cache/story");
        assert!(cache_dir.join("STORY-001.md").exists());
        assert!(cache_dir.join("STORY-002.md").exists());

        // Verify standard frontmatter
        let content = fs::read_to_string(cache_dir.join("STORY-001.md")).unwrap();
        assert!(content.contains("title:"));
        assert!(content.contains("type: story"));

        // Issue map created
        let map = IssueMap::load(dir.path()).unwrap();
        assert_eq!(map.get("STORY-001").unwrap().issue_number, 10);
        assert_eq!(map.get("STORY-002").unwrap().issue_number, 11);
    }

    #[test]
    fn run_skips_when_no_github_issues_types() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::default();
        let gh = SetupMockGh {
            auth: AuthStatus::GhNotInstalled,
            issues: vec![],
        };
        run(dir.path(), &config, &gh).unwrap();
    }

    #[test]
    fn run_handles_empty_issue_list() {
        let dir = tempfile::tempdir().unwrap();
        let config = gh_issues_config();
        let gh = SetupMockGh {
            auth: AuthStatus::Authenticated {
                user: "testuser".to_string(),
                host: "github.com".to_string(),
            },
            issues: vec![],
        };

        run(dir.path(), &config, &gh).unwrap();

        let cache_dir = dir.path().join(".lazyspec/cache/story");
        assert!(cache_dir.exists());
        let map = IssueMap::load(dir.path()).unwrap();
        assert!(map.get("anything").is_none());
    }
}
