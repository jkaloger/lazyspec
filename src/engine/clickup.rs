//! ClickUp API client: lazyspec's first native reqwest HTTP client for a store
//! backend. GitHub speaks through the `gh` CLI (`gh.rs`); ClickUp has no
//! equivalent CLI, so this module owns transport and error classification
//! directly.
//!
//! Errors are classified off real `reqwest::Error` variants and the HTTP status
//! codes the ClickUp API actually returns -- never by scraping substrings out of
//! an error string. `gh.rs`'s `classify_gh_error`/`extract_http_status` scan
//! stderr for the literal `"http "` and parse the next token as a status; that
//! was seen turning `x509: certificate signed by unknown authority` into a fake
//! "HTTP 509". A transport failure here maps to a transport-class
//! [`ClickupError`] variant, so a TLS/DNS error can never masquerade as an HTTP
//! status.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, AUTHORIZATION};
use serde::Deserialize;

/// ClickUp's public v2 API base. `auth_status` hits `{base}/user`.
pub const CLICKUP_API_BASE: &str = "https://api.clickup.com/api/v2";

/// The ClickUp user behind a token, as returned by `GET /user`.
///
/// The response nests the user under a `user` key; [`UserEnvelope`] unwraps it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ClickupUser {
    pub id: u64,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub email: String,
}

#[derive(Debug, Deserialize)]
struct UserEnvelope {
    user: ClickupUser,
}

/// A ClickUp API failure, classified off the real transport error variant or
/// the HTTP status code -- never off a scraped substring.
///
/// Transport-class variants (`Connect`/`Timeout`/`Decode`/`Transport`) come from
/// a `reqwest::Error`; the HTTP-status variants come from a real response's
/// status line. The split is deliberate: a connect/TLS failure can never be
/// mistaken for an HTTP status the way `gh.rs` mistook `x509` for "HTTP 509".
#[derive(Debug, Clone, PartialEq)]
pub enum ClickupError {
    /// Could not establish the connection (DNS, TLS, refused).
    Connect(String),
    /// Request timed out.
    Timeout,
    /// The response body could not be decoded into the expected shape.
    Decode(String),
    /// 401/403 -- token missing, invalid, or revoked.
    InvalidToken { status: u16 },
    /// 429 -- per-token rate limit hit. `reset` carries the instant parsed from
    /// `X-RateLimit-Reset` (Unix epoch seconds) so the caller can back off to it
    /// rather than spinning; `remaining` is `X-RateLimit-Remaining` when present.
    RateLimited {
        reset: Option<SystemTime>,
        remaining: Option<u64>,
    },
    /// 5xx -- ClickUp server-side error.
    Upstream { status: u16 },
    /// Any other unexpected HTTP status.
    Unexpected { status: u16 },
    /// A transport error not covered by the variants above.
    Transport(String),
}

impl std::fmt::Display for ClickupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClickupError::Connect(msg) => write!(f, "failed to connect to ClickUp: {}", msg),
            ClickupError::Timeout => write!(f, "ClickUp request timed out"),
            ClickupError::Decode(msg) => write!(f, "failed to decode ClickUp response: {}", msg),
            ClickupError::InvalidToken { status } => {
                write!(f, "ClickUp token rejected (HTTP {})", status)
            }
            ClickupError::RateLimited { reset, .. } => match reset {
                Some(_) => write!(f, "ClickUp rate limit hit; retry after reset"),
                None => write!(f, "ClickUp rate limit hit"),
            },
            ClickupError::Upstream { status } => {
                write!(f, "ClickUp server error (HTTP {})", status)
            }
            ClickupError::Unexpected { status } => {
                write!(f, "unexpected ClickUp response (HTTP {})", status)
            }
            ClickupError::Transport(msg) => write!(f, "ClickUp transport error: {}", msg),
        }
    }
}

impl std::error::Error for ClickupError {}

impl From<reqwest::Error> for ClickupError {
    fn from(err: reqwest::Error) -> Self {
        // Classify off reqwest's own predicates, not a stringified message. The
        // error's `Display` never contains the token (it lives in a header, not
        // the URL), so carrying `to_string()` for context is safe.
        if err.is_timeout() {
            ClickupError::Timeout
        } else if err.is_connect() {
            ClickupError::Connect(err.to_string())
        } else if err.is_decode() {
            ClickupError::Decode(err.to_string())
        } else {
            ClickupError::Transport(err.to_string())
        }
    }
}

/// Maps an HTTP status plus response headers to a [`ClickupError`]. Pure and
/// deterministic: no I/O, so the whole classification table is unit-testable.
fn classify_status(status: u16, headers: &HeaderMap) -> ClickupError {
    match status {
        401 | 403 => ClickupError::InvalidToken { status },
        429 => ClickupError::RateLimited {
            reset: parse_reset(headers),
            remaining: parse_u64_header(headers, "X-RateLimit-Remaining"),
        },
        500..=599 => ClickupError::Upstream { status },
        other => ClickupError::Unexpected { status: other },
    }
}

/// Parses `X-RateLimit-Reset` (Unix epoch seconds) into the reset instant.
fn parse_reset(headers: &HeaderMap) -> Option<SystemTime> {
    parse_u64_header(headers, "X-RateLimit-Reset")
        .map(|secs| UNIX_EPOCH + Duration::from_secs(secs))
}

fn parse_u64_header(headers: &HeaderMap, name: &str) -> Option<u64> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
}

/// Validates a ClickUp personal token and, more broadly, the transport a
/// backend needs. `auth_status` is the only method this iteration delivers; the
/// store CRUD methods land in later RFC-056 stories.
pub trait ClickupClient {
    /// Validates `token` against `GET /user`. On success returns the token
    /// owner's identity; on failure returns a classified [`ClickupError`].
    fn auth_status(&self, token: &str) -> Result<ClickupUser, ClickupError>;
}

/// reqwest-backed [`ClickupClient`]. Sends the personal token in a raw
/// `Authorization` header (ClickUp personal `pk_`-prefixed tokens take no
/// `Bearer` prefix).
pub struct ClickupHttpClient {
    http: Client,
    base_url: String,
}

impl ClickupHttpClient {
    pub fn new() -> Self {
        Self::with_base_url(CLICKUP_API_BASE)
    }

    /// Builds a client against an arbitrary base URL (for pointing at a mock
    /// server in integration tests).
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        ClickupHttpClient {
            http: Client::new(),
            base_url: base_url.into(),
        }
    }
}

impl Default for ClickupHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ClickupClient for ClickupHttpClient {
    fn auth_status(&self, token: &str) -> Result<ClickupUser, ClickupError> {
        let url = format!("{}/user", self.base_url);
        let response = self.http.get(&url).header(AUTHORIZATION, token).send()?;

        let status = response.status();
        if status.is_success() {
            let envelope: UserEnvelope = response.json()?;
            return Ok(envelope.user);
        }

        Err(classify_status(status.as_u16(), response.headers()))
    }
}

/// In-memory [`ClickupClient`] returning a scripted outcome, for downstream
/// store tests that must not touch the network. Mirrors the `MockGhClient` split
/// in `gh.rs`.
#[cfg(any(test, feature = "test-support"))]
pub struct FakeClickupClient {
    outcome: Result<ClickupUser, ClickupError>,
}

#[cfg(any(test, feature = "test-support"))]
impl FakeClickupClient {
    /// A client whose token validates to `user`.
    pub fn valid(user: ClickupUser) -> Self {
        FakeClickupClient { outcome: Ok(user) }
    }

    /// A client whose token is rejected as invalid (HTTP 401).
    pub fn invalid_token() -> Self {
        FakeClickupClient {
            outcome: Err(ClickupError::InvalidToken { status: 401 }),
        }
    }

    /// A client scripted to return an arbitrary error.
    pub fn failing(error: ClickupError) -> Self {
        FakeClickupClient {
            outcome: Err(error),
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl ClickupClient for FakeClickupClient {
    fn auth_status(&self, _token: &str) -> Result<ClickupUser, ClickupError> {
        self.outcome.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue};

    fn user(id: u64) -> ClickupUser {
        ClickupUser {
            id,
            username: "Jack".to_string(),
            email: "jack@example.com".to_string(),
        }
    }

    #[test]
    fn deserializes_user_envelope_from_clickup_shape() {
        let body =
            r#"{"user":{"id":123,"username":"Jack","email":"jack@example.com","color":"red"}}"#;
        let envelope: UserEnvelope = serde_json::from_str(body).unwrap();
        assert_eq!(envelope.user, user(123));
    }

    #[test]
    fn status_401_is_invalid_token() {
        let err = classify_status(401, &HeaderMap::new());
        assert_eq!(err, ClickupError::InvalidToken { status: 401 });
    }

    #[test]
    fn status_403_is_invalid_token() {
        let err = classify_status(403, &HeaderMap::new());
        assert_eq!(err, ClickupError::InvalidToken { status: 403 });
    }

    #[test]
    fn status_429_carries_parsed_reset_instant_and_remaining() {
        let mut headers = HeaderMap::new();
        headers.insert("X-RateLimit-Reset", HeaderValue::from_static("1700000000"));
        headers.insert("X-RateLimit-Remaining", HeaderValue::from_static("0"));

        let err = classify_status(429, &headers);

        let expected_reset = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert_eq!(
            err,
            ClickupError::RateLimited {
                reset: Some(expected_reset),
                remaining: Some(0),
            }
        );
    }

    #[test]
    fn status_429_without_reset_header_has_no_reset_instant() {
        let err = classify_status(429, &HeaderMap::new());
        assert_eq!(
            err,
            ClickupError::RateLimited {
                reset: None,
                remaining: None,
            }
        );
    }

    #[test]
    fn status_500_range_is_upstream() {
        assert_eq!(
            classify_status(500, &HeaderMap::new()),
            ClickupError::Upstream { status: 500 }
        );
        assert_eq!(
            classify_status(503, &HeaderMap::new()),
            ClickupError::Upstream { status: 503 }
        );
    }

    #[test]
    fn other_client_status_is_unexpected() {
        assert_eq!(
            classify_status(404, &HeaderMap::new()),
            ClickupError::Unexpected { status: 404 }
        );
    }

    #[test]
    fn garbage_reset_header_yields_no_reset_instant() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-RateLimit-Reset",
            HeaderValue::from_static("not-a-number"),
        );
        let err = classify_status(429, &headers);
        assert_eq!(
            err,
            ClickupError::RateLimited {
                reset: None,
                remaining: None,
            }
        );
    }

    /// A connection failure must map to a transport-class variant, never an
    /// HTTP-status variant. This is the `x509 -> "HTTP 509"` misparse guard: a
    /// TLS/DNS/refused error carries no HTTP status, so it can never become one.
    #[test]
    fn transport_failure_never_becomes_an_http_status() {
        // Reserved TEST-NET-1 address (RFC 5737): routing to it fails without
        // depending on any external service being up.
        let client = ClickupHttpClient::with_base_url("http://192.0.2.1:81");
        let http = Client::builder()
            .timeout(Duration::from_millis(200))
            .build()
            .unwrap();
        let client = ClickupHttpClient {
            http,
            base_url: client.base_url,
        };

        let err = client.auth_status("pk_test").unwrap_err();

        match err {
            ClickupError::Connect(_) | ClickupError::Timeout | ClickupError::Transport(_) => {}
            other => panic!("transport failure misclassified as {:?}", other),
        }
    }

    #[test]
    fn fake_valid_returns_scripted_user() {
        let client = FakeClickupClient::valid(user(7));
        assert_eq!(client.auth_status("pk_whatever").unwrap(), user(7));
    }

    #[test]
    fn fake_invalid_token_returns_invalid_token_error() {
        let client = FakeClickupClient::invalid_token();
        assert_eq!(
            client.auth_status("pk_bad").unwrap_err(),
            ClickupError::InvalidToken { status: 401 }
        );
    }

    #[test]
    fn fake_failing_returns_scripted_error() {
        let client = FakeClickupClient::failing(ClickupError::Timeout);
        assert_eq!(
            client.auth_status("pk_x").unwrap_err(),
            ClickupError::Timeout
        );
    }
}
