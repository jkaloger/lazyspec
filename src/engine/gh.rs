use anyhow::{bail, Result};
use serde::Deserialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::process::Command;

// --- Data types ---

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GhLabel {
    pub name: String,
    #[serde(default)]
    pub color: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GhAuthor {
    pub login: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GhIssue {
    pub number: u64,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub labels: Vec<GhLabel>,
    #[serde(default)]
    pub state: String,
    #[serde(default, rename = "updatedAt")]
    pub updated_at: String,
    #[serde(default, rename = "createdAt")]
    pub created_at: String,
    #[serde(default)]
    pub author: Option<GhAuthor>,
}

// --- Error types ---

#[derive(Debug)]
pub enum GhError {
    NotInstalled,
    AuthFailed(String),
    ApiError { status: u16, message: String },
    RateLimited { retry_after: Option<u64> },
}

impl std::fmt::Display for GhError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GhError::NotInstalled => write!(f, "gh CLI is not installed"),
            GhError::AuthFailed(msg) => write!(f, "gh auth failed: {}", msg),
            GhError::ApiError { status, message } => {
                write!(f, "gh API error (HTTP {}): {}", status, message)
            }
            GhError::RateLimited { retry_after } => match retry_after {
                Some(secs) => write!(f, "gh API rate limited, retry after {}s", secs),
                None => write!(f, "gh API rate limited"),
            },
        }
    }
}

impl std::error::Error for GhError {}

// --- Auth ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthStatus {
    Authenticated { user: String, host: String },
    NotAuthenticated(String),
    GhNotInstalled,
}

// --- URL parsing ---

pub fn parse_issue_number_from_url(url: &str) -> Result<u64> {
    url.trim()
        .rsplit('/')
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| anyhow::anyhow!("failed to parse issue number from URL: {}", url))
}

// --- JSON parsing ---

pub fn parse_issue_json(stdout: &str) -> Result<GhIssue> {
    serde_json::from_str(stdout).map_err(|e| anyhow::anyhow!("failed to parse issue JSON: {}", e))
}

pub fn parse_issue_list_json(stdout: &str) -> Result<Vec<GhIssue>> {
    serde_json::from_str(stdout)
        .map_err(|e| anyhow::anyhow!("failed to parse issue list JSON: {}", e))
}

// --- Label helpers ---

pub fn type_label(type_name: &str) -> String {
    format!("lazyspec:{}", type_name)
}

pub fn deterministic_color(type_name: &str) -> String {
    let mut hasher = DefaultHasher::new();
    type_name.hash(&mut hasher);
    let hash = hasher.finish();
    format!("{:06x}", hash & 0xFFFFFF)
}

// --- Traits ---

pub trait GhIssueReader {
    fn issue_list(
        &self,
        repo: &str,
        labels: &[String],
        json_fields: &[String],
        limit: Option<u64>,
    ) -> Result<Vec<GhIssue>>;

    fn issue_view(&self, repo: &str, number: u64) -> Result<GhIssue>;
}

pub trait GhIssueWriter {
    fn issue_create(
        &self,
        repo: &str,
        title: &str,
        body: &str,
        labels: &[String],
    ) -> Result<GhIssue>;

    fn issue_edit(
        &self,
        repo: &str,
        number: u64,
        title: Option<&str>,
        body: Option<&str>,
        labels_add: &[String],
        labels_remove: &[String],
    ) -> Result<()>;

    fn issue_close(&self, repo: &str, number: u64) -> Result<()>;

    fn issue_reopen(&self, repo: &str, number: u64) -> Result<()>;

    fn label_create(&self, repo: &str, name: &str, description: &str, color: &str) -> Result<()>;

    fn label_ensure(&self, repo: &str, name: &str, description: &str, color: &str) -> Result<()>;
}

pub trait GhAuth {
    fn auth_status(&self) -> Result<AuthStatus>;
}

// --- Implementation ---

pub struct GhCli;

impl Default for GhCli {
    fn default() -> Self {
        Self::new()
    }
}

impl GhCli {
    pub fn new() -> Self {
        GhCli
    }

    fn run_gh(&self, args: &[&str]) -> Result<std::process::Output> {
        let output = Command::new("gh").args(args).output();

        match output {
            Ok(o) => Ok(o),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                bail!(GhError::NotInstalled)
            }
            Err(e) => bail!("failed to execute gh: {}", e),
        }
    }

    fn run_gh_checked(&self, args: &[&str]) -> Result<String> {
        let output = self.run_gh(args)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let msg = stderr.trim().to_string();
            bail!(classify_gh_error(&msg));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

impl GhIssueReader for GhCli {
    fn issue_list(
        &self,
        repo: &str,
        labels: &[String],
        json_fields: &[String],
        limit: Option<u64>,
    ) -> Result<Vec<GhIssue>> {
        let label_filter = labels.join(",");
        let fields = if json_fields.is_empty() {
            "number,url,title,body,labels,state,updatedAt,createdAt,author".to_string()
        } else {
            json_fields.join(",")
        };

        let limit_str = limit.map(|l| l.to_string());
        let mut args = vec![
            "issue", "list", "--repo", repo, "--state", "all", "--json", &fields,
        ];

        if !labels.is_empty() {
            args.push("--label");
            args.push(&label_filter);
        }

        if let Some(ref l) = limit_str {
            args.push("--limit");
            args.push(l);
        }

        let stdout = self.run_gh_checked(&args)?;
        parse_issue_list_json(&stdout)
    }

    fn issue_view(&self, repo: &str, number: u64) -> Result<GhIssue> {
        let num_str = number.to_string();
        let args = [
            "issue",
            "view",
            &num_str,
            "--repo",
            repo,
            "--json",
            "number,url,title,body,labels,state,updatedAt,createdAt,author",
        ];

        let stdout = self.run_gh_checked(&args)?;
        parse_issue_json(&stdout)
    }
}

impl GhIssueWriter for GhCli {
    fn issue_create(
        &self,
        repo: &str,
        title: &str,
        body: &str,
        labels: &[String],
    ) -> Result<GhIssue> {
        let mut args = vec![
            "issue", "create", "--repo", repo, "--title", title, "--body", body,
        ];
        for label in labels {
            args.push("--label");
            args.push(label);
        }

        let stdout = self.run_gh_checked(&args)?;
        let number = parse_issue_number_from_url(&stdout)?;
        self.issue_view(repo, number)
    }

    fn issue_edit(
        &self,
        repo: &str,
        number: u64,
        title: Option<&str>,
        body: Option<&str>,
        labels_add: &[String],
        labels_remove: &[String],
    ) -> Result<()> {
        let num_str = number.to_string();
        let mut args = vec!["issue", "edit", &num_str, "--repo", repo];

        if let Some(t) = title {
            args.push("--title");
            args.push(t);
        }
        if let Some(b) = body {
            args.push("--body");
            args.push(b);
        }
        for label in labels_add {
            args.push("--add-label");
            args.push(label);
        }
        for label in labels_remove {
            args.push("--remove-label");
            args.push(label);
        }

        self.run_gh_checked(&args)?;
        Ok(())
    }

    fn issue_close(&self, repo: &str, number: u64) -> Result<()> {
        let num_str = number.to_string();
        self.run_gh_checked(&["issue", "close", &num_str, "--repo", repo])?;
        Ok(())
    }

    fn issue_reopen(&self, repo: &str, number: u64) -> Result<()> {
        let num_str = number.to_string();
        self.run_gh_checked(&["issue", "reopen", &num_str, "--repo", repo])?;
        Ok(())
    }

    fn label_create(&self, repo: &str, name: &str, description: &str, color: &str) -> Result<()> {
        self.run_gh_checked(&[
            "label",
            "create",
            name,
            "--repo",
            repo,
            "--description",
            description,
            "--color",
            color,
        ])?;
        Ok(())
    }

    fn label_ensure(&self, repo: &str, name: &str, description: &str, color: &str) -> Result<()> {
        self.run_gh_checked(&[
            "label",
            "create",
            name,
            "--repo",
            repo,
            "--description",
            description,
            "--color",
            color,
            "--force",
        ])?;
        Ok(())
    }
}

impl GhAuth for GhCli {
    fn auth_status(&self) -> Result<AuthStatus> {
        let output = match Command::new("gh").args(["auth", "status"]).output() {
            Ok(o) => o,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(AuthStatus::GhNotInstalled);
            }
            Err(e) => bail!("failed to execute gh: {}", e),
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}{}", stdout, stderr);

        if !output.status.success() {
            let msg = combined.trim().to_string();
            let lower = msg.to_lowercase();
            if lower.contains("not logged in")
                || lower.contains("authentication")
                || lower.contains("auth")
            {
                bail!(GhError::AuthFailed(msg.clone()));
            }
            return Ok(AuthStatus::NotAuthenticated(msg));
        }

        let user = extract_field(&combined, "Logged in to")
            .and_then(|_| extract_field(&combined, "account"))
            .or_else(|| extract_after(&combined, "account "))
            .unwrap_or_default();

        let host = extract_field(&combined, "Logged in to").unwrap_or_default();

        Ok(AuthStatus::Authenticated { user, host })
    }
}

fn classify_gh_error(stderr: &str) -> GhError {
    let lower = stderr.to_lowercase();

    if lower.contains("rate limit") || lower.contains("api rate limit") {
        let retry_after = lower.find("retry after").and_then(|idx| {
            lower[idx..]
                .split_whitespace()
                .find_map(|token| token.trim_end_matches('s').parse::<u64>().ok())
        });
        return GhError::RateLimited { retry_after };
    }

    if lower.contains("not logged in")
        || lower.contains("authentication")
        || lower.contains("auth token")
    {
        return GhError::AuthFailed(stderr.to_string());
    }

    // Try to extract HTTP status from gh stderr (e.g., "HTTP 404", "422 Validation Failed")
    let status = extract_http_status(&lower);
    GhError::ApiError {
        status: status.unwrap_or(0),
        message: stderr.to_string(),
    }
}

fn extract_http_status(lower: &str) -> Option<u16> {
    if let Some(idx) = lower.find("http ") {
        let rest = &lower[idx + 5..];
        if let Some(code) = rest
            .split_whitespace()
            .next()
            .and_then(|s| s.parse::<u16>().ok())
        {
            return Some(code);
        }
    }
    // Also match bare "404:" or "422 " patterns
    for token in lower.split_whitespace() {
        if let Ok(code) = token
            .trim_matches(|c: char| !c.is_ascii_digit())
            .parse::<u16>()
        {
            if (400..=599).contains(&code) {
                return Some(code);
            }
        }
    }
    None
}

fn extract_field(text: &str, prefix: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let value = rest
                .trim()
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '-');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn extract_after(text: &str, needle: &str) -> Option<String> {
    let idx = text.find(needle)?;
    let rest = &text[idx + needle.len()..];
    let token = rest.split_whitespace().next()?;
    let cleaned = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '.');
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned.to_string())
    }
}

#[cfg(test)]
pub mod test_support {
    use super::*;
    use std::cell::{Cell, RefCell};

    pub struct MockGhClient {
        pub auth: AuthStatus,
        pub list_result: Vec<GhIssue>,
        pub view_issue: RefCell<Option<GhIssue>>,
        pub create_result: Option<GhIssue>,
        pub label_create_fail: bool,
        pub closed: Cell<bool>,
        pub reopened: Cell<bool>,
        pub last_edit_title: RefCell<Option<String>>,
        pub last_edit_body: RefCell<Option<String>>,
        pub last_edit_labels_remove: RefCell<Vec<String>>,
        pub last_create_body: RefCell<Option<String>>,
    }

    impl Default for MockGhClient {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MockGhClient {
        pub fn new() -> Self {
            Self {
                auth: AuthStatus::Authenticated {
                    user: "testuser".to_string(),
                    host: "github.com".to_string(),
                },
                list_result: vec![],
                view_issue: RefCell::new(None),
                create_result: None,
                label_create_fail: false,
                closed: Cell::new(false),
                reopened: Cell::new(false),
                last_edit_title: RefCell::new(None),
                last_edit_body: RefCell::new(None),
                last_edit_labels_remove: RefCell::new(vec![]),
                last_create_body: RefCell::new(None),
            }
        }

        pub fn with_auth(mut self, auth: AuthStatus) -> Self {
            self.auth = auth;
            self
        }

        pub fn with_list_result(mut self, issues: Vec<GhIssue>) -> Self {
            self.list_result = issues;
            self
        }

        pub fn with_view_issue(mut self, issue: GhIssue) -> Self {
            self.view_issue = RefCell::new(Some(issue));
            self
        }

        pub fn with_create_result(mut self, issue: GhIssue) -> Self {
            self.create_result = Some(issue);
            self
        }

        pub fn with_label_create_fail(mut self) -> Self {
            self.label_create_fail = true;
            self
        }
    }

    impl GhIssueReader for MockGhClient {
        fn issue_list(
            &self,
            _repo: &str,
            _labels: &[String],
            _json_fields: &[String],
            _limit: Option<u64>,
        ) -> Result<Vec<GhIssue>> {
            Ok(self.list_result.clone())
        }

        fn issue_view(&self, _repo: &str, number: u64) -> Result<GhIssue> {
            if let Some(issue) = self.view_issue.borrow().as_ref() {
                return Ok(issue.clone());
            }
            Ok(GhIssue {
                number,
                url: format!("https://github.com/test/repo/issues/{}", number),
                title: "Viewed issue".to_string(),
                body: String::new(),
                labels: vec![],
                state: "OPEN".to_string(),
                updated_at: String::new(),
                created_at: String::new(),
                author: None,
            })
        }
    }

    impl GhIssueWriter for MockGhClient {
        fn issue_create(
            &self,
            _repo: &str,
            title: &str,
            body: &str,
            labels: &[String],
        ) -> Result<GhIssue> {
            *self.last_create_body.borrow_mut() = Some(body.to_string());
            if let Some(ref issue) = self.create_result {
                return Ok(issue.clone());
            }
            Ok(GhIssue {
                number: 1,
                url: "https://github.com/test/repo/issues/1".to_string(),
                title: title.to_string(),
                body: body.to_string(),
                labels: labels
                    .iter()
                    .map(|l| GhLabel {
                        name: l.clone(),
                        color: String::new(),
                    })
                    .collect(),
                state: "OPEN".to_string(),
                updated_at: "2026-03-27T00:00:00Z".to_string(),
                created_at: String::new(),
                author: None,
            })
        }

        fn issue_edit(
            &self,
            _repo: &str,
            _number: u64,
            title: Option<&str>,
            body: Option<&str>,
            _labels_add: &[String],
            labels_remove: &[String],
        ) -> Result<()> {
            *self.last_edit_title.borrow_mut() = title.map(|s| s.to_string());
            *self.last_edit_body.borrow_mut() = body.map(|s| s.to_string());
            *self.last_edit_labels_remove.borrow_mut() = labels_remove.to_vec();
            Ok(())
        }

        fn issue_close(&self, _repo: &str, _number: u64) -> Result<()> {
            self.closed.set(true);
            Ok(())
        }

        fn issue_reopen(&self, _repo: &str, _number: u64) -> Result<()> {
            self.reopened.set(true);
            Ok(())
        }

        fn label_create(
            &self,
            _repo: &str,
            _name: &str,
            _description: &str,
            _color: &str,
        ) -> Result<()> {
            if self.label_create_fail {
                bail!("label already exists");
            }
            Ok(())
        }

        fn label_ensure(
            &self,
            _repo: &str,
            _name: &str,
            _description: &str,
            _color: &str,
        ) -> Result<()> {
            Ok(())
        }
    }

    impl GhAuth for MockGhClient {
        fn auth_status(&self) -> Result<AuthStatus> {
            Ok(self.auth.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::MockGhClient;
    use super::*;

    // --- JSON parsing tests ---

    #[test]
    fn parse_single_issue() {
        let json = r#"{
            "number": 42,
            "url": "https://github.com/owner/repo/issues/42",
            "title": "Test issue",
            "body": "Some body text",
            "labels": [{"name": "bug", "color": "d73a4a"}],
            "state": "OPEN",
            "updatedAt": "2026-03-27T00:00:00Z"
        }"#;

        let issue = parse_issue_json(json).unwrap();
        assert_eq!(issue.number, 42);
        assert_eq!(issue.url, "https://github.com/owner/repo/issues/42");
        assert_eq!(issue.title, "Test issue");
        assert_eq!(issue.body, "Some body text");
        assert_eq!(issue.labels.len(), 1);
        assert_eq!(issue.labels[0].name, "bug");
        assert_eq!(issue.labels[0].color, "d73a4a");
        assert_eq!(issue.state, "OPEN");
        assert_eq!(issue.updated_at, "2026-03-27T00:00:00Z");
    }

    #[test]
    fn parse_issue_list() {
        let json = r#"[
            {"number": 1, "title": "First"},
            {"number": 2, "title": "Second"}
        ]"#;

        let issues = parse_issue_list_json(json).unwrap();
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].number, 1);
        assert_eq!(issues[0].title, "First");
        assert_eq!(issues[1].number, 2);
    }

    #[test]
    fn parse_empty_list() {
        let issues = parse_issue_list_json("[]").unwrap();
        assert!(issues.is_empty());
    }

    #[test]
    fn parse_issue_json_with_author() {
        let json = r#"{
            "number": 5,
            "title": "Authored issue",
            "author": {"login": "jkaloger"}
        }"#;

        let issue = parse_issue_json(json).unwrap();
        assert_eq!(
            issue.author,
            Some(GhAuthor {
                login: "jkaloger".to_string()
            })
        );
    }

    #[test]
    fn parse_partial_json_fields() {
        let json = r#"{"number": 10, "title": "Partial"}"#;
        let issue = parse_issue_json(json).unwrap();
        assert_eq!(issue.number, 10);
        assert_eq!(issue.title, "Partial");
        assert_eq!(issue.url, "");
        assert_eq!(issue.body, "");
        assert!(issue.labels.is_empty());
        assert_eq!(issue.state, "");
        assert_eq!(issue.updated_at, "");
    }

    // --- type_label tests ---

    #[test]
    fn type_label_format() {
        assert_eq!(type_label("RFC"), "lazyspec:RFC");
        assert_eq!(type_label("ADR"), "lazyspec:ADR");
        assert_eq!(type_label("story"), "lazyspec:story");
    }

    // --- deterministic_color tests ---

    #[test]
    fn deterministic_color_stability() {
        let c1 = deterministic_color("RFC");
        let c2 = deterministic_color("RFC");
        assert_eq!(c1, c2);
        assert_eq!(c1.len(), 6);
        assert!(c1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn deterministic_color_varies_by_input() {
        let c1 = deterministic_color("RFC");
        let c2 = deterministic_color("ADR");
        assert_ne!(c1, c2);
    }

    // --- Mock-based tests ---

    #[test]
    fn mock_issue_create() {
        let client = MockGhClient::new();
        let issue = client
            .issue_create("owner/repo", "title", "body", &["bug".to_string()])
            .unwrap();
        assert_eq!(issue.number, 1);
        assert_eq!(issue.title, "title");
        assert_eq!(issue.labels[0].name, "bug");
    }

    #[test]
    fn mock_issue_list_empty() {
        let client = MockGhClient::new();
        let issues = client.issue_list("owner/repo", &[], &[], None).unwrap();
        assert!(issues.is_empty());
    }

    #[test]
    fn mock_issue_list_with_results() {
        let client = MockGhClient::new().with_list_result(vec![
            GhIssue {
                number: 1,
                url: String::new(),
                title: "First".to_string(),
                body: String::new(),
                labels: vec![],
                state: "OPEN".to_string(),
                updated_at: String::new(),
                created_at: String::new(),
                author: None,
            },
            GhIssue {
                number: 2,
                url: String::new(),
                title: "Second".to_string(),
                body: String::new(),
                labels: vec![],
                state: "OPEN".to_string(),
                updated_at: String::new(),
                created_at: String::new(),
                author: None,
            },
        ]);
        let issues = client.issue_list("owner/repo", &[], &[], None).unwrap();
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn mock_label_ensure_succeeds_on_existing() {
        let client = MockGhClient::new().with_label_create_fail();

        // label_create fails
        assert!(client
            .label_create("owner/repo", "bug", "desc", "ff0000")
            .is_err());
        // label_ensure still succeeds
        assert!(client
            .label_ensure("owner/repo", "bug", "desc", "ff0000")
            .is_ok());
    }

    #[test]
    fn mock_auth_status() {
        let client = MockGhClient::new();
        let status = client.auth_status().unwrap();
        assert_eq!(
            status,
            AuthStatus::Authenticated {
                user: "testuser".to_string(),
                host: "github.com".to_string(),
            }
        );
    }

    #[test]
    fn mock_issue_view() {
        let client = MockGhClient::new();
        let issue = client.issue_view("owner/repo", 42).unwrap();
        assert_eq!(issue.number, 42);
    }

    #[test]
    fn mock_issue_edit() {
        let client = MockGhClient::new();
        let result = client.issue_edit(
            "owner/repo",
            42,
            None,
            Some("updated body"),
            &["new-label".to_string()],
            &["old-label".to_string()],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn mock_issue_close_reopen() {
        let client = MockGhClient::new();
        assert!(client.issue_close("owner/repo", 1).is_ok());
        assert!(client.issue_reopen("owner/repo", 1).is_ok());
    }

    // --- parse_issue_number_from_url tests ---

    #[test]
    fn parse_issue_number_from_valid_url() {
        let num = parse_issue_number_from_url("https://github.com/owner/repo/issues/42").unwrap();
        assert_eq!(num, 42);
    }

    #[test]
    fn parse_issue_number_from_url_with_trailing_newline() {
        let num = parse_issue_number_from_url("https://github.com/owner/repo/issues/99\n").unwrap();
        assert_eq!(num, 99);
    }

    #[test]
    fn parse_issue_number_from_invalid_url() {
        let result = parse_issue_number_from_url("not-a-url");
        assert!(result.is_err());
    }

    // --- classify_gh_error tests ---

    #[test]
    fn classify_rate_limit_error() {
        let err = classify_gh_error("API rate limit exceeded for user");
        assert!(matches!(err, GhError::RateLimited { retry_after: None }));
    }

    #[test]
    fn classify_rate_limit_with_retry_after() {
        let err = classify_gh_error("API rate limit exceeded. Retry after 60s");
        match err {
            GhError::RateLimited { retry_after } => assert_eq!(retry_after, Some(60)),
            other => panic!("expected RateLimited, got {:?}", other),
        }
    }

    #[test]
    fn classify_auth_failure() {
        let err = classify_gh_error("not logged in to any github hosts");
        assert!(matches!(err, GhError::AuthFailed(_)));
    }

    #[test]
    fn classify_auth_token_error() {
        let err = classify_gh_error("auth token not found");
        assert!(matches!(err, GhError::AuthFailed(_)));
    }

    #[test]
    fn classify_api_error_with_http_status() {
        let err = classify_gh_error("HTTP 404: Not Found");
        match err {
            GhError::ApiError { status, message } => {
                assert_eq!(status, 404);
                assert_eq!(message, "HTTP 404: Not Found");
            }
            other => panic!("expected ApiError, got {:?}", other),
        }
    }

    #[test]
    fn classify_api_error_with_422() {
        let err = classify_gh_error("422 Validation Failed");
        match err {
            GhError::ApiError { status, .. } => assert_eq!(status, 422),
            other => panic!("expected ApiError, got {:?}", other),
        }
    }

    #[test]
    fn classify_unknown_error_as_api_error() {
        let err = classify_gh_error("something went wrong");
        match err {
            GhError::ApiError { status, message } => {
                assert_eq!(status, 0);
                assert_eq!(message, "something went wrong");
            }
            other => panic!("expected ApiError with status 0, got {:?}", other),
        }
    }

    #[test]
    fn gh_error_display_variants() {
        let not_installed = GhError::NotInstalled;
        assert_eq!(format!("{}", not_installed), "gh CLI is not installed");

        let auth = GhError::AuthFailed("bad token".to_string());
        assert_eq!(format!("{}", auth), "gh auth failed: bad token");

        let api = GhError::ApiError {
            status: 404,
            message: "not found".to_string(),
        };
        assert_eq!(format!("{}", api), "gh API error (HTTP 404): not found");

        let rate = GhError::RateLimited {
            retry_after: Some(30),
        };
        assert_eq!(format!("{}", rate), "gh API rate limited, retry after 30s");

        let rate_none = GhError::RateLimited { retry_after: None };
        assert_eq!(format!("{}", rate_none), "gh API rate limited");
    }
}
