use crate::engine::clickup::ClickupClient;
use crate::engine::config::Config;
use crate::engine::credentials::{CredentialLocation, CredentialStore, Token};
use crate::engine::gh::{AuthStatus, GhAuth, GhGraphql, GhIssueDependencyApi, GhIssueReader};
use crate::engine::github::resolve_repo;
use crate::engine::issue_body::TypeMatchRule;
use crate::engine::issue_cache::IssueCache;
use crate::engine::issue_map::IssueMap;
use crate::engine::store_dispatch;
use anyhow::{bail, Context, Result};
use clap::Subcommand;
use serde::Serialize;
use std::path::Path;

/// `lazyspec setup <backend>` subcommands. Bare `lazyspec setup` keeps its
/// existing behaviour (github-issues auth + fetch); the ClickUp backend has no
/// external CLI to piggyback on, so it captures and stores its own credential.
#[derive(Subcommand)]
pub enum SetupCommand {
    /// Validate a ClickUp personal API token and store it globally
    Clickup {
        /// Personal API token (`pk_...`); prompts securely when omitted
        #[arg(long)]
        token: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Serialize)]
struct SetupClickupOutput {
    ok: bool,
    user_id: u64,
    username: String,
    storage: String,
}

/// Validates a ClickUp personal token against the API, then -- only on success
/// -- persists it via `store`. Nothing is written when validation fails, so an
/// invalid/revoked token leaves any existing credential untouched.
///
/// `client` and `store` are injected so tests exercise this end to end without a
/// network call or touching the real home dir.
pub fn run_clickup(
    client: &dyn ClickupClient,
    store: &dyn CredentialStore,
    token_arg: Option<String>,
    json: bool,
) -> Result<()> {
    let raw = match token_arg {
        Some(t) => t,
        None => prompt_token()?,
    };
    let raw = raw.trim().to_string();
    if raw.is_empty() {
        bail!("no token provided");
    }
    let token = Token::new(raw);

    // Validate before any write: a rejected token must not clobber a stored one.
    let user = client
        .auth_status(token.expose())
        .map_err(|e| anyhow::anyhow!("ClickUp token validation failed: {}", e))?;

    // Keychain-first; the store emits the loud fallback log itself when (and
    // only when) no keychain backend is reachable.
    let location = store.store_clickup_token(&token)?;

    if json {
        let output = SetupClickupOutput {
            ok: true,
            user_id: user.id,
            username: user.username.clone(),
            storage: location.to_string(),
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "Authenticated with ClickUp as {} (id {})",
            user.username, user.id
        );
        match &location {
            CredentialLocation::Keychain => {
                println!("Stored ClickUp token in the OS keychain")
            }
            CredentialLocation::File(path) => {
                println!("Stored ClickUp token in plaintext at {}", path.display())
            }
        }
    }
    Ok(())
}

/// Reads a token from the terminal without echoing it.
fn prompt_token() -> Result<String> {
    let term = console::Term::stderr();
    term.write_str("ClickUp personal API token: ")
        .context("failed to write token prompt")?;
    let line = term
        .read_secure_line()
        .context("failed to read token from terminal (use --token for non-interactive use)")?;
    Ok(line)
}

pub fn run(
    root: &Path,
    config: &Config,
    gh: &(impl GhIssueReader + GhAuth + GhGraphql + GhIssueDependencyApi),
) -> Result<()> {
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
            bail!(
                "gh auth failed: {}\nRun `gh auth login` to authenticate.",
                msg
            );
        }
        AuthStatus::Authenticated { user, host } => {
            println!("Authenticated as {} on {}", user, host);
        }
    }

    let repo = resolve_repo(config, root).context(
        "Could not determine GitHub repo. Set [documents.github].repo in .lazyspec.toml",
    )?;
    let mut issue_map = IssueMap::load(root)?;
    let cache = IssueCache::new(root);
    // One composed read for the whole setup fetch, not one per type.
    let round = crate::engine::gh_fetch::fetch_all_pages(
        gh,
        &repo,
        &crate::engine::gh_fetch::issue_rules(config),
        &store_dispatch::authority_board_numbers(config),
    );
    for w in &round.warnings {
        eprintln!("warning: {}", w.message);
    }

    for type_name in &gh_types {
        let type_def = config
            .type_by_name(type_name)
            .ok_or_else(|| anyhow::anyhow!("type '{}' not found in config", type_name))?;

        let all_type_rules: Vec<TypeMatchRule> = config
            .documents
            .types
            .iter()
            .map(TypeMatchRule::from)
            .collect();
        let result = cache.fetch_all(
            root,
            type_def,
            gh,
            gh,
            Some(&round),
            &repo,
            &mut issue_map,
            &all_type_rules,
            config,
        )?;

        for w in &result.warnings {
            eprintln!("warning: {}", w.message);
        }

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
    use crate::engine::gh::{test_support::MockGhClient, GhIssue, GhLabel};
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
            id: String::new(),
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
            issue_type: None,
            milestone: None,
            assignees: vec![],
        }
    }

    // --- issue_map via IssueMap ---

    #[test]
    fn issue_map_roundtrips_via_issue_map() {
        let dir = tempfile::tempdir().unwrap();
        let mut map = IssueMap::load(dir.path()).unwrap();
        map.insert("ITERATION-042", 87, "2026-03-27T10:00:00Z", "");
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

    // --- setup clickup ---

    use crate::engine::clickup::{ClickupError, ClickupUser, FakeClickupClient};
    use crate::engine::credentials::FileCredentialStore;

    fn clickup_user() -> ClickupUser {
        ClickupUser {
            id: 42,
            username: "Jack".to_string(),
            email: "jack@example.com".to_string(),
        }
    }

    #[test]
    fn run_clickup_valid_token_stores_credential() {
        let dir = tempfile::tempdir().unwrap();
        let cred_path = dir.path().join(".lazyspec/credentials.toml");
        let store = FileCredentialStore::at_path(&cred_path);
        let client = FakeClickupClient::valid(clickup_user());

        run_clickup(&client, &store, Some("pk_valid".to_string()), false).unwrap();

        assert_eq!(
            store.load_clickup_token().unwrap().unwrap().expose(),
            "pk_valid"
        );
    }

    #[test]
    fn run_clickup_invalid_token_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let cred_path = dir.path().join(".lazyspec/credentials.toml");
        let store = FileCredentialStore::at_path(&cred_path);
        let client = FakeClickupClient::invalid_token();

        let result = run_clickup(&client, &store, Some("pk_bad".to_string()), false);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("validation failed"));
        assert!(!cred_path.exists());
        assert_eq!(store.load_clickup_token().unwrap(), None);
    }

    #[test]
    fn run_clickup_invalid_token_leaves_existing_credential_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let cred_path = dir.path().join("credentials.toml");
        let store = FileCredentialStore::at_path(&cred_path);
        store
            .store_clickup_token(&Token::new("pk_existing"))
            .unwrap();

        let client = FakeClickupClient::invalid_token();
        let result = run_clickup(&client, &store, Some("pk_bad".to_string()), false);

        assert!(result.is_err());
        assert_eq!(
            store.load_clickup_token().unwrap().unwrap().expose(),
            "pk_existing"
        );
    }

    #[test]
    fn run_clickup_empty_token_errors_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let cred_path = dir.path().join("credentials.toml");
        let store = FileCredentialStore::at_path(&cred_path);
        let client = FakeClickupClient::valid(clickup_user());

        let result = run_clickup(&client, &store, Some("   ".to_string()), false);

        assert!(result.is_err());
        assert!(!cred_path.exists());
    }

    #[test]
    fn run_clickup_transport_error_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let cred_path = dir.path().join("credentials.toml");
        let store = FileCredentialStore::at_path(&cred_path);
        let client = FakeClickupClient::failing(ClickupError::Timeout);

        let result = run_clickup(&client, &store, Some("pk_x".to_string()), false);

        assert!(result.is_err());
        assert!(!cred_path.exists());
    }
}
