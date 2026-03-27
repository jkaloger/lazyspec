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
}

// --- Error types ---

#[derive(Debug)]
pub enum GhError {
    NotInstalled,
    AuthFailure(String),
    RateLimit { retry_after: Option<String> },
    NetworkError(String),
    ApiError { status: Option<u16>, message: String },
    JsonParse(String),
}

impl std::fmt::Display for GhError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GhError::NotInstalled => write!(f, "gh CLI is not installed"),
            GhError::AuthFailure(msg) => write!(f, "gh auth failure: {}", msg),
            GhError::RateLimit { retry_after } => {
                write!(f, "GitHub rate limit exceeded")?;
                if let Some(after) = retry_after {
                    write!(f, " (retry after {})", after)?;
                }
                Ok(())
            }
            GhError::NetworkError(msg) => write!(f, "network error: {}", msg),
            GhError::ApiError { status, message } => {
                write!(f, "GitHub API error")?;
                if let Some(s) = status {
                    write!(f, " ({})", s)?;
                }
                write!(f, ": {}", message)
            }
            GhError::JsonParse(msg) => write!(f, "JSON parse error: {}", msg),
        }
    }
}

impl std::error::Error for GhError {}

pub fn classify_error(exit_code: i32, stderr: &str) -> GhError {
    let lower = stderr.to_lowercase();

    if exit_code == 127 || lower.contains("not found") || lower.contains("not installed") {
        return GhError::NotInstalled;
    }

    if lower.contains("rate limit") || lower.contains("secondary rate") {
        let retry_after = extract_retry_after(stderr);
        return GhError::RateLimit { retry_after };
    }

    if lower.contains("auth") || lower.contains("login") || lower.contains("401") {
        return GhError::AuthFailure(stderr.trim().to_string());
    }

    if lower.contains("could not resolve")
        || lower.contains("connection")
        || lower.contains("timeout")
        || lower.contains("network")
    {
        return GhError::NetworkError(stderr.trim().to_string());
    }

    let status = extract_http_status(stderr);
    GhError::ApiError {
        status,
        message: stderr.trim().to_string(),
    }
}

fn extract_retry_after(stderr: &str) -> Option<String> {
    for line in stderr.lines() {
        let lower = line.to_lowercase();
        if lower.contains("retry after") || lower.contains("retry-after") {
            return Some(line.trim().to_string());
        }
    }
    None
}

fn extract_http_status(stderr: &str) -> Option<u16> {
    for word in stderr.split_whitespace() {
        let trimmed = word.trim_matches(|c: char| !c.is_ascii_digit());
        if let Ok(n) = trimmed.parse::<u16>() {
            if (400..600).contains(&n) {
                return Some(n);
            }
        }
    }
    None
}

// --- Auth ---

#[derive(Debug, PartialEq, Eq)]
pub enum AuthStatus {
    Authenticated { user: String, host: String },
    NotAuthenticated(String),
    GhNotInstalled,
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

// --- Trait ---

pub trait GhClient {
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

    fn issue_list(
        &self,
        repo: &str,
        labels: &[String],
        json_fields: &[String],
    ) -> Result<Vec<GhIssue>>;

    fn issue_view(&self, repo: &str, number: u64) -> Result<GhIssue>;

    fn issue_close(&self, repo: &str, number: u64) -> Result<()>;

    fn issue_reopen(&self, repo: &str, number: u64) -> Result<()>;

    fn auth_status(&self) -> Result<AuthStatus>;

    fn label_create(
        &self,
        repo: &str,
        name: &str,
        description: &str,
        color: &str,
    ) -> Result<()>;

    fn label_ensure(
        &self,
        repo: &str,
        name: &str,
        description: &str,
        color: &str,
    ) -> Result<()>;
}

// --- Implementation ---

pub struct GhCli;

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
            let code = output.status.code().unwrap_or(1);
            let err = classify_error(code, &stderr);
            bail!(err);
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

impl GhClient for GhCli {
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
        args.extend_from_slice(&["--json", "number,url,title,body,labels,state,updatedAt"]);

        let stdout = self.run_gh_checked(&args)?;
        parse_issue_json(&stdout)
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

    fn issue_list(
        &self,
        repo: &str,
        labels: &[String],
        json_fields: &[String],
    ) -> Result<Vec<GhIssue>> {
        let label_filter = labels.join(",");
        let fields = if json_fields.is_empty() {
            "number,url,title,body,labels,state,updatedAt".to_string()
        } else {
            json_fields.join(",")
        };

        let mut args = vec!["issue", "list", "--repo", repo, "--json", &fields];

        if !labels.is_empty() {
            args.push("--label");
            args.push(&label_filter);
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
            "number,url,title,body,labels,state,updatedAt",
        ];

        let stdout = self.run_gh_checked(&args)?;
        parse_issue_json(&stdout)
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
            return Ok(AuthStatus::NotAuthenticated(combined.trim().to_string()));
        }

        let user = extract_field(&combined, "Logged in to")
            .and_then(|_| extract_field(&combined, "account"))
            .or_else(|| extract_after(&combined, "account "))
            .unwrap_or_default();

        let host = extract_field(&combined, "Logged in to").unwrap_or_default();

        Ok(AuthStatus::Authenticated { user, host })
    }

    fn label_create(
        &self,
        repo: &str,
        name: &str,
        description: &str,
        color: &str,
    ) -> Result<()> {
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

    fn label_ensure(
        &self,
        repo: &str,
        name: &str,
        description: &str,
        color: &str,
    ) -> Result<()> {
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

fn extract_field(text: &str, prefix: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let value = rest.trim().trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '-');
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
mod tests {
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

    // --- classify_error tests ---

    #[test]
    fn classify_not_installed() {
        let err = classify_error(127, "gh: command not found");
        assert!(matches!(err, GhError::NotInstalled));
    }

    #[test]
    fn classify_not_installed_from_stderr() {
        let err = classify_error(1, "gh is not installed");
        assert!(matches!(err, GhError::NotInstalled));
    }

    #[test]
    fn classify_rate_limit() {
        let err = classify_error(1, "API rate limit exceeded; retry after 60s");
        assert!(matches!(err, GhError::RateLimit { .. }));
        if let GhError::RateLimit { retry_after } = err {
            assert!(retry_after.is_some());
        }
    }

    #[test]
    fn classify_auth_failure() {
        let err = classify_error(1, "You are not logged in. Run gh auth login");
        assert!(matches!(err, GhError::AuthFailure(_)));
    }

    #[test]
    fn classify_network_error() {
        let err = classify_error(1, "could not resolve host github.com");
        assert!(matches!(err, GhError::NetworkError(_)));
    }

    #[test]
    fn classify_api_error_with_status() {
        let err = classify_error(1, "HTTP 422: Validation Failed");
        assert!(matches!(err, GhError::ApiError { status: Some(422), .. }));
    }

    #[test]
    fn classify_generic_api_error() {
        let err = classify_error(1, "something went wrong");
        assert!(matches!(err, GhError::ApiError { status: None, .. }));
    }

    // --- Mock-based tests ---

    struct MockGhClient {
        create_result: Option<GhIssue>,
        list_result: Vec<GhIssue>,
        label_create_fail: bool,
    }

    impl MockGhClient {
        fn new() -> Self {
            Self {
                create_result: None,
                list_result: vec![],
                label_create_fail: false,
            }
        }
    }

    impl GhClient for MockGhClient {
        fn issue_create(
            &self,
            _repo: &str,
            title: &str,
            body: &str,
            labels: &[String],
        ) -> Result<GhIssue> {
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
            })
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
            Ok(())
        }

        fn issue_list(
            &self,
            _repo: &str,
            _labels: &[String],
            _json_fields: &[String],
        ) -> Result<Vec<GhIssue>> {
            Ok(self.list_result.clone())
        }

        fn issue_view(&self, _repo: &str, number: u64) -> Result<GhIssue> {
            Ok(GhIssue {
                number,
                url: format!("https://github.com/test/repo/issues/{}", number),
                title: "Viewed issue".to_string(),
                body: String::new(),
                labels: vec![],
                state: "OPEN".to_string(),
                updated_at: String::new(),
            })
        }

        fn issue_close(&self, _repo: &str, _number: u64) -> Result<()> {
            Ok(())
        }

        fn issue_reopen(&self, _repo: &str, _number: u64) -> Result<()> {
            Ok(())
        }

        fn auth_status(&self) -> Result<AuthStatus> {
            Ok(AuthStatus::Authenticated {
                user: "testuser".to_string(),
                host: "github.com".to_string(),
            })
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
            // label_ensure always succeeds (treats "already exists" as OK)
            Ok(())
        }
    }

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
        let issues = client
            .issue_list("owner/repo", &[], &[])
            .unwrap();
        assert!(issues.is_empty());
    }

    #[test]
    fn mock_issue_list_with_results() {
        let mut client = MockGhClient::new();
        client.list_result = vec![
            GhIssue {
                number: 1,
                url: String::new(),
                title: "First".to_string(),
                body: String::new(),
                labels: vec![],
                state: "OPEN".to_string(),
                updated_at: String::new(),
            },
            GhIssue {
                number: 2,
                url: String::new(),
                title: "Second".to_string(),
                body: String::new(),
                labels: vec![],
                state: "OPEN".to_string(),
                updated_at: String::new(),
            },
        ];
        let issues = client.issue_list("owner/repo", &[], &[]).unwrap();
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn mock_label_ensure_succeeds_on_existing() {
        let mut client = MockGhClient::new();
        client.label_create_fail = true;

        // label_create fails
        assert!(client.label_create("owner/repo", "bug", "desc", "ff0000").is_err());
        // label_ensure still succeeds
        assert!(client.label_ensure("owner/repo", "bug", "desc", "ff0000").is_ok());
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
}
