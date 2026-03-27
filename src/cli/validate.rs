use crate::cli::style::{error_prefix, warning_prefix};
use crate::engine::config::{Config, StoreBackend};
use crate::engine::gh::{AuthStatus, GhClient, GhCli};
use crate::engine::store::Store;
use console::{colors_enabled, Style};

fn success_message() -> String {
    if colors_enabled() {
        format!(
            "{} All documents valid.",
            Style::new().green().bold().apply_to("\u{2713}")
        )
    } else {
        "All documents valid.".to_string()
    }
}

fn has_github_issues_types(config: &Config) -> bool {
    config
        .documents
        .types
        .iter()
        .any(|t| t.store == StoreBackend::GithubIssues)
}

pub fn gh_auth_warnings(gh: &dyn GhClient) -> Vec<String> {
    match gh.auth_status() {
        Ok(AuthStatus::GhNotInstalled) => {
            vec!["gh CLI is not installed; github-issues types will not sync".to_string()]
        }
        Ok(AuthStatus::NotAuthenticated(msg)) => {
            vec![format!(
                "gh is not authenticated; github-issues types will not sync ({})",
                msg
            )]
        }
        Ok(AuthStatus::Authenticated { .. }) => vec![],
        Err(e) => {
            vec![format!("could not check gh auth status: {}", e)]
        }
    }
}

pub fn run_full(store: &Store, config: &Config, json: bool, warnings: bool) -> i32 {
    let result = store.validate_full(config);

    let gh_warnings = if has_github_issues_types(config) {
        let gh = GhCli::new();
        gh_auth_warnings(&gh)
    } else {
        vec![]
    };

    if json {
        let output = run_json(store, config, &gh_warnings);
        println!("{}", output);
    } else {
        let output = run_human(store, config, warnings, &gh_warnings);
        if output.is_empty() {
            println!("{}", success_message());
        } else {
            eprint!("{}", output);
        }
    }

    if result.errors.is_empty() && store.parse_errors().is_empty() { 0 } else { 2 }
}

pub fn run_json(store: &Store, config: &Config, extra_warnings: &[String]) -> String {
    let result = store.validate_full(config);
    let errors: Vec<_> = result.errors.iter().map(|e| format!("{}", e)).collect();
    let mut warnings: Vec<_> = result.warnings.iter().map(|w| format!("{}", w)).collect();
    warnings.extend(extra_warnings.iter().cloned());
    let parse_errors: Vec<_> = store.parse_errors().iter().map(|pe| {
        serde_json::json!({ "path": pe.path.display().to_string(), "error": pe.error })
    }).collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "errors": errors,
        "warnings": warnings,
        "parse_errors": parse_errors,
    }))
    .unwrap()
}

pub fn run_human(store: &Store, config: &Config, show_warnings: bool, extra_warnings: &[String]) -> String {
    let result = store.validate_full(config);
    let mut output = String::new();

    for pe in store.parse_errors() {
        output.push_str(&format!("  {} parse error in {}: {}\n", error_prefix(), pe.path.display(), pe.error));
    }
    for error in &result.errors {
        output.push_str(&format!("  {} {}\n", error_prefix(), error));
    }
    if show_warnings {
        for warning in &result.warnings {
            output.push_str(&format!("  {} {}\n", warning_prefix(), warning));
        }
        for warning in extra_warnings {
            output.push_str(&format!("  {} {}\n", warning_prefix(), warning));
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{NumberingStrategy, TypeDef};
    use crate::engine::gh::{AuthStatus, GhClient, GhIssue};
    use anyhow::Result;

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

    struct MockGh {
        auth: AuthStatus,
    }

    impl GhClient for MockGh {
        fn issue_create(&self, _: &str, _: &str, _: &str, _: &[String]) -> Result<GhIssue> {
            unimplemented!()
        }
        fn issue_edit(&self, _: &str, _: u64, _: Option<&str>, _: Option<&str>, _: &[String], _: &[String]) -> Result<()> {
            unimplemented!()
        }
        fn issue_list(&self, _: &str, _: &[String], _: &[String]) -> Result<Vec<GhIssue>> {
            unimplemented!()
        }
        fn issue_view(&self, _: &str, _: u64) -> Result<GhIssue> {
            unimplemented!()
        }
        fn issue_close(&self, _: &str, _: u64) -> Result<()> {
            unimplemented!()
        }
        fn issue_reopen(&self, _: &str, _: u64) -> Result<()> {
            unimplemented!()
        }
        fn auth_status(&self) -> Result<AuthStatus> {
            Ok(self.auth.clone())
        }
        fn label_create(&self, _: &str, _: &str, _: &str, _: &str) -> Result<()> {
            unimplemented!()
        }
        fn label_ensure(&self, _: &str, _: &str, _: &str, _: &str) -> Result<()> {
            unimplemented!()
        }
    }

    #[test]
    fn gh_auth_warnings_when_not_installed() {
        let gh = MockGh { auth: AuthStatus::GhNotInstalled };
        let warnings = gh_auth_warnings(&gh);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("not installed"));
    }

    #[test]
    fn gh_auth_warnings_when_not_authenticated() {
        let gh = MockGh {
            auth: AuthStatus::NotAuthenticated("token expired".to_string()),
        };
        let warnings = gh_auth_warnings(&gh);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("not authenticated"));
        assert!(warnings[0].contains("token expired"));
    }

    #[test]
    fn gh_auth_warnings_when_authenticated() {
        let gh = MockGh {
            auth: AuthStatus::Authenticated {
                user: "testuser".to_string(),
                host: "github.com".to_string(),
            },
        };
        let warnings = gh_auth_warnings(&gh);
        assert!(warnings.is_empty());
    }

    #[test]
    fn has_github_issues_types_true() {
        let mut config = Config::default();
        config.documents.types = vec![make_type("story", StoreBackend::GithubIssues)];
        assert!(has_github_issues_types(&config));
    }

    #[test]
    fn has_github_issues_types_false_for_filesystem_only() {
        let mut config = Config::default();
        config.documents.types = vec![make_type("rfc", StoreBackend::Filesystem)];
        assert!(!has_github_issues_types(&config));
    }

    #[test]
    fn has_github_issues_types_false_when_empty() {
        let config = Config::default();
        assert!(!has_github_issues_types(&config));
    }

    #[test]
    fn run_human_includes_gh_warnings_when_shown() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::default();
        let store = Store::load(dir.path(), &config).unwrap();
        let extra = vec!["gh CLI is not installed; github-issues types will not sync".to_string()];
        let output = run_human(&store, &config, true, &extra);
        assert!(output.contains("gh CLI is not installed"));
    }

    #[test]
    fn run_human_hides_gh_warnings_when_not_shown() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::default();
        let store = Store::load(dir.path(), &config).unwrap();
        let extra = vec!["gh CLI is not installed; github-issues types will not sync".to_string()];
        let output = run_human(&store, &config, false, &extra);
        assert!(!output.contains("gh CLI is not installed"));
    }

    #[test]
    fn run_json_includes_gh_warnings() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::default();
        let store = Store::load(dir.path(), &config).unwrap();
        let extra = vec!["gh not installed warning".to_string()];
        let output = run_json(&store, &config, &extra);
        assert!(output.contains("gh not installed warning"));
    }
}
