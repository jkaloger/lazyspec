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
}

impl std::fmt::Display for GhError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GhError::NotInstalled => write!(f, "gh CLI is not installed"),
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

pub trait GhAuth {
    fn auth_status(&self) -> Result<AuthStatus>;
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
            bail!("{}", stderr.trim());
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
            "number,url,title,body,labels,state,updatedAt".to_string()
        } else {
            json_fields.join(",")
        };

        let limit_str = limit.map(|l| l.to_string());
        let mut args = vec!["issue", "list", "--repo", repo, "--json", &fields];

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
            "number,url,title,body,labels,state,updatedAt",
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
            return Ok(AuthStatus::NotAuthenticated(combined.trim().to_string()));
        }

        let user = extract_field(&combined, "Logged in to")
            .and_then(|_| extract_field(&combined, "account"))
            .or_else(|| extract_after(&combined, "account "))
            .unwrap_or_default();

        let host = extract_field(&combined, "Logged in to").unwrap_or_default();

        Ok(AuthStatus::Authenticated { user, host })
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
    }

    impl GhIssueWriter for MockGhClient {
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

        fn issue_close(&self, _repo: &str, _number: u64) -> Result<()> {
            Ok(())
        }

        fn issue_reopen(&self, _repo: &str, _number: u64) -> Result<()> {
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
            Ok(AuthStatus::Authenticated {
                user: "testuser".to_string(),
                host: "github.com".to_string(),
            })
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
            .issue_list("owner/repo", &[], &[], None)
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
        let issues = client.issue_list("owner/repo", &[], &[], None).unwrap();
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

    // --- parse_issue_number_from_url tests ---

    #[test]
    fn parse_issue_number_from_valid_url() {
        let num = parse_issue_number_from_url("https://github.com/owner/repo/issues/42").unwrap();
        assert_eq!(num, 42);
    }

    #[test]
    fn parse_issue_number_from_url_with_trailing_newline() {
        let num =
            parse_issue_number_from_url("https://github.com/owner/repo/issues/99\n").unwrap();
        assert_eq!(num, 99);
    }

    #[test]
    fn parse_issue_number_from_invalid_url() {
        let result = parse_issue_number_from_url("not-a-url");
        assert!(result.is_err());
    }

}
