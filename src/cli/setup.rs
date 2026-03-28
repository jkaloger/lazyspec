use crate::engine::config::Config;
use crate::engine::gh::{AuthStatus, GhAuth, GhIssueReader};
use crate::engine::github::resolve_repo;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use anyhow::{bail, Context, Result};
use std::path::Path;

pub fn run(root: &Path, config: &Config, gh: &(impl GhIssueReader + GhAuth)) -> Result<()> {
    let gh_types = config.documents.github_issues_types();
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

    let repo = resolve_repo(config, root)
        .context("Could not determine GitHub repo. Set [documents.github].repo in .lazyspec.toml")?;
    let mut issue_map = IssueMap::load(root)?;
    let cache = IssueCache::new(root);

    for type_name in &gh_types {
        let type_def = config
            .type_by_name(type_name)
            .ok_or_else(|| anyhow::anyhow!("type '{}' not found in config", type_name))?;

        let all_type_names: Vec<String> = config.documents.types.iter().map(|t| t.name.clone()).collect();
        let result = cache.fetch_all(root, type_def, gh, &repo, &mut issue_map, &all_type_names)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{GithubConfig, StoreBackend, TypeDef};
    use crate::engine::gh::{GhIssue, GhLabel, test_support::MockGhClient};
    use std::fs;

    fn gh_issues_config() -> Config {
        let mut config = Config::default();
        config.documents.types = vec![
            TypeDef::test_fixture("rfc", StoreBackend::Filesystem),
            TypeDef::test_fixture("story", StoreBackend::GithubIssues),
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
            created_at: "2026-03-27T10:00:00Z".to_string(),
            author: None,
        }
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

    #[test]
    fn run_fails_when_gh_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        let config = gh_issues_config();
        let gh = MockGhClient::new().with_auth(AuthStatus::GhNotInstalled);
        let result = run(dir.path(), &config, &gh);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not installed"));
    }

    #[test]
    fn run_fails_when_not_authenticated() {
        let dir = tempfile::tempdir().unwrap();
        let config = gh_issues_config();
        let gh = MockGhClient::new()
            .with_auth(AuthStatus::NotAuthenticated("not logged in".to_string()));
        let result = run(dir.path(), &config, &gh);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("auth failed"));
    }

    #[test]
    fn run_creates_cache_and_issue_map() {
        let dir = tempfile::tempdir().unwrap();
        let config = gh_issues_config();
        let gh = MockGhClient::new().with_list_result(vec![
            make_issue(10, "STORY-001 First story", "Body 1", &["lazyspec:story"]),
            make_issue(11, "STORY-002 Second story", "Body 2", &["lazyspec:story"]),
        ]);

        run(dir.path(), &config, &gh).unwrap();

        // Cache files use doc ID derived from prefix + issue number
        let cache_dir = dir.path().join(".lazyspec/cache/story");
        assert!(cache_dir.join("STORY-10.md").exists());
        assert!(cache_dir.join("STORY-11.md").exists());

        // Verify standard frontmatter
        let content = fs::read_to_string(cache_dir.join("STORY-10.md")).unwrap();
        assert!(content.contains("title:"));
        assert!(content.contains("type: story"));

        // Issue map created
        let map = IssueMap::load(dir.path()).unwrap();
        assert_eq!(map.get("STORY-10").unwrap().issue_number, 10);
        assert_eq!(map.get("STORY-11").unwrap().issue_number, 11);
    }

    #[test]
    fn run_skips_when_no_github_issues_types() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::default();
        let gh = MockGhClient::new().with_auth(AuthStatus::GhNotInstalled);
        run(dir.path(), &config, &gh).unwrap();
    }

    #[test]
    fn run_handles_empty_issue_list() {
        let dir = tempfile::tempdir().unwrap();
        let config = gh_issues_config();
        let gh = MockGhClient::new();

        run(dir.path(), &config, &gh).unwrap();

        let cache_dir = dir.path().join(".lazyspec/cache/story");
        assert!(cache_dir.exists());
        let map = IssueMap::load(dir.path()).unwrap();
        assert!(map.get("anything").is_none());
    }
}
