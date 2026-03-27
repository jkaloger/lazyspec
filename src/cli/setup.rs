use crate::engine::config::{Config, StoreBackend};
use crate::engine::gh::{type_label, AuthStatus, GhClient, GhIssue};
use crate::engine::github::infer_github_repo;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct IssueMapEntry {
    pub issue_number: u64,
    pub updated_at: String,
}

pub fn run(root: &Path, config: &Config, gh: &dyn GhClient) -> Result<()> {
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
    let mut issue_map: HashMap<String, IssueMapEntry> = HashMap::new();

    for type_name in &gh_types {
        let label = type_label(type_name);
        let labels = vec![label];
        let issues = gh.issue_list(&repo, &labels, &[])?;

        let cache_dir = root.join(".lazyspec").join("cache").join(type_name);
        fs::create_dir_all(&cache_dir)?;

        for issue in &issues {
            write_cache_file(&cache_dir, issue)?;
            if let Some(id) = extract_doc_id(issue, type_name) {
                issue_map.insert(
                    id,
                    IssueMapEntry {
                        issue_number: issue.number,
                        updated_at: issue.updated_at.clone(),
                    },
                );
            }
        }

        println!(
            "Fetched {} {} issue{}",
            issues.len(),
            type_name,
            if issues.len() == 1 { "" } else { "s" }
        );
    }

    let map_path = root.join(".lazyspec").join("issue-map.json");
    fs::create_dir_all(map_path.parent().unwrap())?;
    let json = serde_json::to_string_pretty(&issue_map)?;
    fs::write(&map_path, json)?;

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

fn write_cache_file(cache_dir: &Path, issue: &GhIssue) -> Result<()> {
    let filename = format!("{}.md", issue.number);
    let content = format!(
        "---\nnumber: {}\ntitle: {}\nstate: {}\nupdated_at: {}\n---\n\n{}",
        issue.number,
        issue.title,
        issue.state,
        issue.updated_at,
        issue.body
    );
    fs::write(cache_dir.join(filename), content)?;
    Ok(())
}

fn extract_doc_id(issue: &GhIssue, type_name: &str) -> Option<String> {
    let prefix = type_name.to_uppercase();
    let title = &issue.title;
    if let Some(rest) = title.strip_prefix(&format!("{}-", prefix)) {
        let id_part: String = rest.chars().take_while(|c| c.is_alphanumeric()).collect();
        if !id_part.is_empty() {
            return Some(format!("{}-{}", prefix, id_part));
        }
    }
    // Also check for the ID anywhere in the title
    for word in title.split_whitespace() {
        if let Some(rest) = word.strip_prefix(&format!("{}-", prefix)) {
            let id_part: String = rest.chars().take_while(|c| c.is_alphanumeric()).collect();
            if !id_part.is_empty() {
                return Some(format!("{}-{}", prefix, id_part));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{GithubConfig, NumberingStrategy, TypeDef};
    use crate::engine::gh::{GhIssue, GhLabel};

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

    // --- extract_doc_id ---

    #[test]
    fn extract_doc_id_from_title_prefix() {
        let issue = make_issue(1, "STORY-042 Implement feature", "", &[]);
        assert_eq!(
            extract_doc_id(&issue, "story"),
            Some("STORY-042".to_string())
        );
    }

    #[test]
    fn extract_doc_id_from_title_word() {
        let issue = make_issue(1, "Some prefix STORY-007 suffix", "", &[]);
        assert_eq!(
            extract_doc_id(&issue, "story"),
            Some("STORY-007".to_string())
        );
    }

    #[test]
    fn extract_doc_id_none_when_missing() {
        let issue = make_issue(1, "Just a random title", "", &[]);
        assert_eq!(extract_doc_id(&issue, "story"), None);
    }

    #[test]
    fn extract_doc_id_different_type() {
        let issue = make_issue(1, "RFC-001 Some RFC", "", &[]);
        assert_eq!(extract_doc_id(&issue, "rfc"), Some("RFC-001".to_string()));
    }

    // --- write_cache_file ---

    #[test]
    fn write_cache_file_creates_markdown() {
        let dir = tempfile::tempdir().unwrap();
        let issue = make_issue(42, "STORY-001 Test", "Body content", &["lazyspec:story"]);
        write_cache_file(dir.path(), &issue).unwrap();

        let path = dir.path().join("42.md");
        assert!(path.exists());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("number: 42"));
        assert!(content.contains("title: STORY-001 Test"));
        assert!(content.contains("Body content"));
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

    // --- issue_map serialization ---

    #[test]
    fn issue_map_entry_serializes() {
        let entry = IssueMapEntry {
            issue_number: 87,
            updated_at: "2026-03-27T10:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("87"));
        assert!(json.contains("2026-03-27T10:00:00Z"));
    }

    #[test]
    fn issue_map_roundtrips() {
        let mut map: HashMap<String, IssueMapEntry> = HashMap::new();
        map.insert(
            "ITERATION-042".to_string(),
            IssueMapEntry {
                issue_number: 87,
                updated_at: "2026-03-27T10:00:00Z".to_string(),
            },
        );
        let json = serde_json::to_string_pretty(&map).unwrap();
        let parsed: HashMap<String, IssueMapEntry> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["ITERATION-042"].issue_number, 87);
    }

    // --- run with mock ---

    struct SetupMockGh {
        auth: AuthStatus,
        issues: Vec<GhIssue>,
    }

    impl GhClient for SetupMockGh {
        fn issue_create(
            &self,
            _repo: &str,
            _title: &str,
            _body: &str,
            _labels: &[String],
        ) -> Result<GhIssue> {
            unimplemented!()
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
            unimplemented!()
        }

        fn issue_list(
            &self,
            _repo: &str,
            _labels: &[String],
            _json_fields: &[String],
        ) -> Result<Vec<GhIssue>> {
            Ok(self.issues.clone())
        }

        fn issue_view(&self, _repo: &str, _number: u64) -> Result<GhIssue> {
            unimplemented!()
        }

        fn issue_close(&self, _repo: &str, _number: u64) -> Result<()> {
            unimplemented!()
        }

        fn issue_reopen(&self, _repo: &str, _number: u64) -> Result<()> {
            unimplemented!()
        }

        fn auth_status(&self) -> Result<AuthStatus> {
            Ok(self.auth.clone())
        }

        fn label_create(
            &self,
            _repo: &str,
            _name: &str,
            _description: &str,
            _color: &str,
        ) -> Result<()> {
            unimplemented!()
        }

        fn label_ensure(
            &self,
            _repo: &str,
            _name: &str,
            _description: &str,
            _color: &str,
        ) -> Result<()> {
            unimplemented!()
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

        // Cache files created
        let cache_dir = dir.path().join(".lazyspec/cache/story");
        assert!(cache_dir.join("10.md").exists());
        assert!(cache_dir.join("11.md").exists());

        // Issue map created
        let map_path = dir.path().join(".lazyspec/issue-map.json");
        assert!(map_path.exists());
        let map_json = fs::read_to_string(&map_path).unwrap();
        let map: HashMap<String, IssueMapEntry> = serde_json::from_str(&map_json).unwrap();
        assert_eq!(map["STORY-001"].issue_number, 10);
        assert_eq!(map["STORY-002"].issue_number, 11);
    }

    #[test]
    fn run_skips_when_no_github_issues_types() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::default();
        let gh = SetupMockGh {
            auth: AuthStatus::GhNotInstalled,
            issues: vec![],
        };
        // Should succeed without even checking auth
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
        let map_path = dir.path().join(".lazyspec/issue-map.json");
        let map: HashMap<String, IssueMapEntry> =
            serde_json::from_str(&fs::read_to_string(&map_path).unwrap()).unwrap();
        assert!(map.is_empty());
    }
}
