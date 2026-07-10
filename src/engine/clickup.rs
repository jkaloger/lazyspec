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
use serde::{Deserialize, Serialize};

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

/// A task's status, as ClickUp nests it under `status.status`. Only the raw
/// status string is read -- it maps verbatim onto the lazyspec doc `status`,
/// with no local mapping table.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ClickupTaskStatus {
    pub status: String,
}

/// One status in a List's workflow, from `GET /list/{id}`'s `statuses` array.
/// `orderindex` is the position ClickUp assigns the status in the workflow;
/// sorting by it reconstructs the workflow order. `status_type` is ClickUp's own
/// kind (`open`/`custom`/`closed`/`done`); it is retained for later stories but
/// the lifecycle derivation reads only the name and order.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ClickupStatus {
    pub status: String,
    #[serde(default, deserialize_with = "de_i64_flex")]
    pub orderindex: i64,
    #[serde(default, rename = "type")]
    pub status_type: String,
    #[serde(default)]
    pub color: String,
}

/// The `GET /list/{id}` response, of which only the `statuses` array is read.
#[derive(Debug, Deserialize)]
struct ListEnvelope {
    #[serde(default)]
    statuses: Vec<ClickupStatus>,
}

/// A task's priority, read as the nested object ClickUp returns
/// (`{"priority":"normal","color":..,"id":"3","orderindex":"3"}`). Only the
/// `priority` name is read; the bare-integer *write* shape is a later story.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ClickupPriority {
    pub priority: String,
}

/// A ClickUp tag, of which only the name is materialized.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ClickupTag {
    #[serde(default)]
    pub name: String,
}

/// The task creator, surfaced as the doc author.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ClickupCreator {
    #[serde(default)]
    pub username: String,
}

/// A custom-field value on a task, keyed by its ClickUp uuid. `value` is kept as
/// a raw JSON value because ClickUp returns different shapes per field type; the
/// read path decodes it against the type's `clickup_custom_field_map`
/// (`clickup_cache::task_to_doc`) into relations or non-native attributes.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ClickupCustomField {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub value: Option<serde_json::Value>,
}

/// A ClickUp task in its *read* shape (`GET /list/{id}/task` and `GET /task`).
///
/// The epoch/duration fields (`due_date`/`start_date`/`time_estimate`/
/// `date_updated`/`date_created`) arrive as epoch-millisecond *strings* on read
/// (e.g. `"1748541600000"`) but as integers on write; [`de_opt_epoch_ms`]
/// accepts either so the same struct can round-trip. `priority` is the nested
/// read object, not the bare write integer.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ClickupTask {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub status: ClickupTaskStatus,
    #[serde(default)]
    pub priority: Option<ClickupPriority>,
    #[serde(default, deserialize_with = "de_opt_epoch_ms")]
    pub due_date: Option<i64>,
    #[serde(default, deserialize_with = "de_opt_epoch_ms")]
    pub start_date: Option<i64>,
    #[serde(default, deserialize_with = "de_opt_epoch_ms")]
    pub time_estimate: Option<i64>,
    #[serde(default, deserialize_with = "de_opt_epoch_ms")]
    pub date_updated: Option<i64>,
    #[serde(default, deserialize_with = "de_opt_epoch_ms")]
    pub date_created: Option<i64>,
    #[serde(default)]
    pub markdown_description: String,
    #[serde(default)]
    pub text_content: String,
    #[serde(default)]
    pub custom_item_id: Option<i64>,
    #[serde(default)]
    pub tags: Vec<ClickupTag>,
    #[serde(default)]
    pub creator: Option<ClickupCreator>,
    #[serde(default)]
    pub custom_fields: Vec<ClickupCustomField>,
}

/// One page of `GET /list/{id}/task`. `last_page` is ClickUp's own signal that
/// pagination is exhausted; the real client stops paging on it.
#[derive(Debug, Deserialize)]
struct TaskListEnvelope {
    #[serde(default)]
    tasks: Vec<ClickupTask>,
    #[serde(default)]
    last_page: bool,
}

/// Deserialize an optional epoch-millisecond field that ClickUp may send as a
/// string, an integer, or `null`/absent. Every epoch/duration field on read is a
/// string; accepting the integer form too lets the same struct decode the write
/// shape without a second type.
fn de_opt_epoch_ms<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match value {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(s)) => s.trim().parse::<i64>().ok(),
        Some(serde_json::Value::Number(n)) => n.as_i64(),
        Some(_) => None,
    })
}

/// Deserialize an `i64` ClickUp may send as either a JSON number or a numeric
/// string. `orderindex` on a List status is a number, but ClickUp sends the same
/// field as a string elsewhere; accepting both keeps the decode robust.
fn de_i64_flex<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(serde_json::Value::Number(n)) => n.as_i64().unwrap_or(0),
        Some(serde_json::Value::String(s)) => s.trim().parse::<i64>().unwrap_or(0),
        _ => 0,
    })
}

/// The *write* shape of a task, sent as the JSON body of `POST /list/{id}/task`.
///
/// Distinct from the read [`ClickupTask`] because ClickUp's create/edit fields
/// are asymmetric with what it returns (RFC-056 §Field mapping): the body field
/// is `markdown_content` (not the read-side `markdown_description`), `priority`
/// is a bare integer (`1=Urgent 2=High 3=Normal 4=Low`, not the read object),
/// and the epoch/duration fields are integers (not the read strings). Every
/// field but `name` is optional and omitted from the payload when `None`, so a
/// create sends only what the doc actually carries and ClickUp defaults the
/// rest (e.g. an omitted `status` takes the List's default status).
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct TaskCreate {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markdown_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_estimate: Option<i64>,
    /// The ClickUp custom task type's `custom_item_id`, stamped from the bound
    /// type's `clickup_task_type` config so a new task materializes under that
    /// type (RFC-056 §Field mapping). Omitted when unset, so ClickUp defaults the
    /// task to the List's native "Task" type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_item_id: Option<i64>,
}

/// The *write* shape of an edit, sent as the JSON body of `PUT /task/{id}`.
///
/// Distinct from [`TaskCreate`] because an edit is a *partial* update: every
/// field is optional, `name` included, so a field the caller did not change is
/// omitted from the payload and ClickUp leaves it untouched. Sending an empty
/// `name` on a create is fine (the task starts named), but an edit that omitted
/// no field would blank the task's name -- hence the separate all-optional
/// shape. Field asymmetry with the read [`ClickupTask`] is the same as
/// `TaskCreate` (RFC-056 §Field mapping): `markdown_content` (not
/// `markdown_description`), a bare-integer `priority`, integer epoch/duration
/// fields.
///
/// `status` is the raw ClickUp status string, pushed verbatim on `advance`
/// (RFC-056 §Status handling). lazyspec derives no local transition edges for a
/// ClickUp-backed type, so it does not gate the move -- ClickUp enforces its own
/// transition rules and rejects an illegal target.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct TaskUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markdown_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_estimate: Option<i64>,
}

/// Validates a ClickUp personal token and fetches the tasks a bound List
/// contains. `auth_status` and `task_list` are the read path's methods;
/// `list_statuses` feeds the sync-time lifecycle derivation; `create_task` and
/// `update_task` are the create/edit write methods (RFC-056 story 5); the
/// remaining store CRUD methods land in later RFC-056 stories.
pub trait ClickupClient {
    /// Validates `token` against `GET /user`. On success returns the token
    /// owner's identity; on failure returns a classified [`ClickupError`].
    fn auth_status(&self, token: &str) -> Result<ClickupUser, ClickupError>;

    /// Fetches every task in the List `list_id` (`GET /list/{id}/task`),
    /// following pagination internally so callers receive the full set. Closed
    /// tasks and subtasks are included; archived tasks are excluded by the API,
    /// so they drop out of the returned set (and thus out of the cache on the
    /// next fetch).
    fn task_list(&self, token: &str, list_id: &str) -> Result<Vec<ClickupTask>, ClickupError>;

    /// Fetches the bound List's status workflow (`GET /list/{id}`), returning its
    /// `statuses` array. The type's effective lifecycle states are derived from
    /// this set at sync time; ClickUp owns the transition rules, so no edges are
    /// derived.
    fn list_statuses(&self, token: &str, list_id: &str)
        -> Result<Vec<ClickupStatus>, ClickupError>;

    /// Creates a task in the List `list_id` (`POST /list/{id}/task`) from the
    /// write-shape `payload`, returning the created task as ClickUp echoes it
    /// (with its assigned id, default-resolved status, and `date_updated`). The
    /// store materializes that response into the local cache and task map.
    fn create_task(
        &self,
        token: &str,
        list_id: &str,
        payload: &TaskCreate,
    ) -> Result<ClickupTask, ClickupError>;

    /// Edits the task `task_id` (`PUT /task/{id}`) from the partial write-shape
    /// `payload`, returning the updated task as ClickUp echoes it (with a fresh
    /// `date_updated` and the new field values). The store re-materializes that
    /// response into the cache and task map, so a subsequent read reflects the
    /// change (the native-field round-trip).
    fn update_task(
        &self,
        token: &str,
        task_id: &str,
        payload: &TaskUpdate,
    ) -> Result<ClickupTask, ClickupError>;

    /// Fetches a single task (`GET /task/{id}`) in its read shape. The write
    /// path calls this *before* a `PUT` to read ClickUp's current
    /// `date_updated` and compare it against the `TaskMap`'s recorded baseline:
    /// the optimistic-lock check that rejects a stale write before it can clobber
    /// a concurrent external change (RFC-056 §Caching/id-mapping).
    fn get_task(&self, token: &str, task_id: &str) -> Result<ClickupTask, ClickupError>;

    /// Archives the task `task_id` -- ClickUp's soft delete -- via
    /// `PUT /task/{id}` with `{"archived": true}` (RFC-056 §Design). An archived
    /// task drops out of `task_list` fetches, so the doc leaves the cache and the
    /// `TaskMap` on the next sync, but the task stays recoverable in ClickUp.
    /// lazyspec never hard-deletes: `DELETE /task/{id}` is not called anywhere.
    fn archive_task(&self, token: &str, task_id: &str) -> Result<(), ClickupError>;

    /// Sets a task's custom field value (`POST /task/{id}/field/{field_id}`) with
    /// `{"value": <value>}` (RFC-056 §Relations). This is a *full replace* of the
    /// field's stored value, not an add/remove diff: the relation write path
    /// serializes a doc's complete relation set into the configured text custom
    /// field and writes the whole block on every link/unlink. One custom field per
    /// request (ClickUp offers no batch on this endpoint).
    fn set_custom_field(
        &self,
        token: &str,
        task_id: &str,
        field_id: &str,
        value: &str,
    ) -> Result<(), ClickupError>;
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

    fn task_list(&self, token: &str, list_id: &str) -> Result<Vec<ClickupTask>, ClickupError> {
        let mut all = Vec::new();
        let mut page: u32 = 0;
        loop {
            let url = format!(
                "{}/list/{}/task?page={}&include_closed=true&subtasks=true&include_markdown_description=true",
                self.base_url, list_id, page
            );
            let response = self.http.get(&url).header(AUTHORIZATION, token).send()?;

            let status = response.status();
            if !status.is_success() {
                return Err(classify_status(status.as_u16(), response.headers()));
            }

            let envelope: TaskListEnvelope = response.json()?;
            let count = envelope.tasks.len();
            all.extend(envelope.tasks);
            if envelope.last_page || count == 0 {
                break;
            }
            page += 1;
        }
        Ok(all)
    }

    fn list_statuses(
        &self,
        token: &str,
        list_id: &str,
    ) -> Result<Vec<ClickupStatus>, ClickupError> {
        let url = format!("{}/list/{}", self.base_url, list_id);
        let response = self.http.get(&url).header(AUTHORIZATION, token).send()?;

        let status = response.status();
        if !status.is_success() {
            return Err(classify_status(status.as_u16(), response.headers()));
        }

        let envelope: ListEnvelope = response.json()?;
        Ok(envelope.statuses)
    }

    fn create_task(
        &self,
        token: &str,
        list_id: &str,
        payload: &TaskCreate,
    ) -> Result<ClickupTask, ClickupError> {
        let url = format!("{}/list/{}/task", self.base_url, list_id);
        let response = self
            .http
            .post(&url)
            .header(AUTHORIZATION, token)
            .json(payload)
            .send()?;

        let status = response.status();
        if !status.is_success() {
            return Err(classify_status(status.as_u16(), response.headers()));
        }

        // Create echoes the task object at the top level (no envelope).
        let task: ClickupTask = response.json()?;
        Ok(task)
    }

    fn update_task(
        &self,
        token: &str,
        task_id: &str,
        payload: &TaskUpdate,
    ) -> Result<ClickupTask, ClickupError> {
        let url = format!("{}/task/{}", self.base_url, task_id);
        let response = self
            .http
            .put(&url)
            .header(AUTHORIZATION, token)
            .json(payload)
            .send()?;

        let status = response.status();
        if !status.is_success() {
            return Err(classify_status(status.as_u16(), response.headers()));
        }

        // Edit echoes the updated task object at the top level (no envelope).
        let task: ClickupTask = response.json()?;
        Ok(task)
    }

    fn get_task(&self, token: &str, task_id: &str) -> Result<ClickupTask, ClickupError> {
        let url = format!(
            "{}/task/{}?include_markdown_description=true",
            self.base_url, task_id
        );
        let response = self.http.get(&url).header(AUTHORIZATION, token).send()?;

        let status = response.status();
        if !status.is_success() {
            return Err(classify_status(status.as_u16(), response.headers()));
        }

        // A single-task GET echoes the task object at the top level (no envelope).
        let task: ClickupTask = response.json()?;
        Ok(task)
    }

    fn archive_task(&self, token: &str, task_id: &str) -> Result<(), ClickupError> {
        // Archive rides the same `PUT /task/{id}` endpoint as an edit, sending
        // only `{"archived": true}`; the DELETE endpoint (a hard delete) is never
        // used. The echoed task body is ignored -- only the status matters.
        let url = format!("{}/task/{}", self.base_url, task_id);
        let response = self
            .http
            .put(&url)
            .header(AUTHORIZATION, token)
            .json(&serde_json::json!({ "archived": true }))
            .send()?;

        let status = response.status();
        if !status.is_success() {
            return Err(classify_status(status.as_u16(), response.headers()));
        }
        Ok(())
    }

    fn set_custom_field(
        &self,
        token: &str,
        task_id: &str,
        field_id: &str,
        value: &str,
    ) -> Result<(), ClickupError> {
        let url = format!("{}/task/{}/field/{}", self.base_url, task_id, field_id);
        let response = self
            .http
            .post(&url)
            .header(AUTHORIZATION, token)
            .json(&serde_json::json!({ "value": value }))
            .send()?;

        let status = response.status();
        if !status.is_success() {
            return Err(classify_status(status.as_u16(), response.headers()));
        }
        Ok(())
    }
}

/// In-memory [`ClickupClient`] returning a scripted outcome, for downstream
/// store tests that must not touch the network. Mirrors the `MockGhClient` split
/// in `gh.rs`.
#[cfg(any(test, feature = "test-support"))]
pub struct FakeClickupClient {
    auth: Result<ClickupUser, ClickupError>,
    tasks: Result<Vec<ClickupTask>, ClickupError>,
    statuses: Result<Vec<ClickupStatus>, ClickupError>,
    /// The task `create_task` returns; defaults to an error so an unscripted
    /// create fails loudly rather than fabricating a task.
    created: Result<ClickupTask, ClickupError>,
    /// The task `update_task` returns; defaults to an error so an unscripted
    /// edit fails loudly rather than fabricating a task.
    updated: Result<ClickupTask, ClickupError>,
    /// The task `get_task` returns -- the remote state the optimistic-lock check
    /// reads before a write. Defaults to an error so a write test that reaches
    /// the pre-write fetch without scripting a remote task fails loudly.
    viewed: Result<ClickupTask, ClickupError>,
    /// Every `create_task` call recorded as `(list_id, payload)`. Shared behind
    /// an `Rc` so a test can hold the handle after the client is boxed into a
    /// store and still read back the payload the store built.
    create_calls: std::rc::Rc<std::cell::RefCell<Vec<(String, TaskCreate)>>>,
    /// Every `update_task` call recorded as `(task_id, payload)`, shared the
    /// same way so a store test can read back the edit payload it built.
    update_calls: std::rc::Rc<std::cell::RefCell<Vec<(String, TaskUpdate)>>>,
    /// Every `get_task` call recorded as `task_id`, shared the same way so a
    /// store test can assert the pre-write optimistic-lock fetch happened.
    view_calls: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    /// The outcome `archive_task` returns -- ClickUp's soft delete. Defaults to
    /// `Ok(())` on a valid client so a delete succeeds unless scripted to fail.
    archived: Result<(), ClickupError>,
    /// Every `archive_task` call recorded as `task_id`, shared the same way so a
    /// store delete test can assert exactly one archive fired for the mapped task.
    archive_calls: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    /// The outcome `set_custom_field` returns -- the relation-persist write.
    /// Defaults to `Ok(())` on a valid client so a link succeeds unless scripted
    /// to fail.
    set_field: Result<(), ClickupError>,
    /// Every `set_custom_field` call recorded as `(task_id, field_id, value)`,
    /// shared behind an `Rc` so a link test can read back the serialized relations
    /// block the write path built and the field id it targeted.
    set_field_calls: std::rc::Rc<std::cell::RefCell<Vec<(String, String, String)>>>,
}

#[cfg(any(test, feature = "test-support"))]
impl FakeClickupClient {
    /// A client whose token validates to `user` and whose bound List is empty.
    pub fn valid(user: ClickupUser) -> Self {
        FakeClickupClient {
            auth: Ok(user),
            tasks: Ok(Vec::new()),
            statuses: Ok(Vec::new()),
            created: no_create_scripted(),
            updated: no_update_scripted(),
            viewed: no_view_scripted(),
            create_calls: Default::default(),
            update_calls: Default::default(),
            view_calls: Default::default(),
            archived: Ok(()),
            archive_calls: Default::default(),
            set_field: Ok(()),
            set_field_calls: Default::default(),
        }
    }

    /// A client whose token is rejected as invalid (HTTP 401).
    pub fn invalid_token() -> Self {
        FakeClickupClient {
            auth: Err(ClickupError::InvalidToken { status: 401 }),
            tasks: Err(ClickupError::InvalidToken { status: 401 }),
            statuses: Err(ClickupError::InvalidToken { status: 401 }),
            created: Err(ClickupError::InvalidToken { status: 401 }),
            updated: Err(ClickupError::InvalidToken { status: 401 }),
            viewed: Err(ClickupError::InvalidToken { status: 401 }),
            create_calls: Default::default(),
            update_calls: Default::default(),
            view_calls: Default::default(),
            archived: Err(ClickupError::InvalidToken { status: 401 }),
            archive_calls: Default::default(),
            set_field: Err(ClickupError::InvalidToken { status: 401 }),
            set_field_calls: Default::default(),
        }
    }

    /// A client whose every method returns an arbitrary error.
    pub fn failing(error: ClickupError) -> Self {
        FakeClickupClient {
            auth: Err(error.clone()),
            tasks: Err(error.clone()),
            statuses: Err(error.clone()),
            created: Err(error.clone()),
            updated: Err(error.clone()),
            viewed: Err(error.clone()),
            create_calls: Default::default(),
            update_calls: Default::default(),
            view_calls: Default::default(),
            archived: Err(error.clone()),
            archive_calls: Default::default(),
            set_field: Err(error),
            set_field_calls: Default::default(),
        }
    }

    /// A client whose bound List returns `tasks` (and whose token validates).
    pub fn with_tasks(tasks: Vec<ClickupTask>) -> Self {
        FakeClickupClient {
            auth: Ok(fake_user()),
            tasks: Ok(tasks),
            statuses: Ok(Vec::new()),
            created: no_create_scripted(),
            updated: no_update_scripted(),
            viewed: no_view_scripted(),
            create_calls: Default::default(),
            update_calls: Default::default(),
            view_calls: Default::default(),
            archived: Ok(()),
            archive_calls: Default::default(),
            set_field: Ok(()),
            set_field_calls: Default::default(),
        }
    }

    /// A client whose `task_list` returns an arbitrary error.
    pub fn failing_tasks(error: ClickupError) -> Self {
        FakeClickupClient {
            auth: Ok(fake_user()),
            tasks: Err(error),
            statuses: Ok(Vec::new()),
            created: no_create_scripted(),
            updated: no_update_scripted(),
            viewed: no_view_scripted(),
            create_calls: Default::default(),
            update_calls: Default::default(),
            view_calls: Default::default(),
            archived: Ok(()),
            archive_calls: Default::default(),
            set_field: Ok(()),
            set_field_calls: Default::default(),
        }
    }

    /// Scripts `create_task` to return `task` (builder-style). The default
    /// (unset) create errors, so a write test must opt in to a scripted task.
    pub fn with_created_task(mut self, task: ClickupTask) -> Self {
        self.created = Ok(task);
        self
    }

    /// Makes `create_task` return an arbitrary error (builder-style).
    pub fn failing_create(mut self, error: ClickupError) -> Self {
        self.created = Err(error);
        self
    }

    /// A shared handle to the recorded `create_task` calls (`(list_id,
    /// payload)`). Clone it before boxing the client into a store, then read the
    /// payload the store built back through this handle after the create runs.
    pub fn create_calls(&self) -> std::rc::Rc<std::cell::RefCell<Vec<(String, TaskCreate)>>> {
        std::rc::Rc::clone(&self.create_calls)
    }

    /// Scripts `update_task` to return `task` (builder-style). The default
    /// (unset) update errors, so an edit test must opt in to a scripted task.
    pub fn with_updated_task(mut self, task: ClickupTask) -> Self {
        self.updated = Ok(task);
        self
    }

    /// Makes `update_task` return an arbitrary error (builder-style).
    pub fn failing_update(mut self, error: ClickupError) -> Self {
        self.updated = Err(error);
        self
    }

    /// A shared handle to the recorded `update_task` calls (`(task_id,
    /// payload)`), read back the same way as [`Self::create_calls`].
    pub fn update_calls(&self) -> std::rc::Rc<std::cell::RefCell<Vec<(String, TaskUpdate)>>> {
        std::rc::Rc::clone(&self.update_calls)
    }

    /// Scripts `get_task` to return `task` (builder-style) -- the remote state
    /// the optimistic-lock check reads before a write. Its `date_updated` is what
    /// the check compares against the stored baseline.
    pub fn with_viewed_task(mut self, task: ClickupTask) -> Self {
        self.viewed = Ok(task);
        self
    }

    /// Makes `get_task` return an arbitrary error (builder-style).
    pub fn failing_view(mut self, error: ClickupError) -> Self {
        self.viewed = Err(error);
        self
    }

    /// A shared handle to the recorded `get_task` calls (each `task_id`), read
    /// back the same way as [`Self::update_calls`]. A store test asserts the
    /// pre-write optimistic-lock fetch fired -- and, on conflict, that no
    /// `update_task` followed.
    pub fn view_calls(&self) -> std::rc::Rc<std::cell::RefCell<Vec<String>>> {
        std::rc::Rc::clone(&self.view_calls)
    }

    /// Makes `archive_task` (the delete path) return an arbitrary error
    /// (builder-style), for asserting a failed archive surfaces as a store error.
    pub fn failing_archive(mut self, error: ClickupError) -> Self {
        self.archived = Err(error);
        self
    }

    /// A shared handle to the recorded `archive_task` calls (each `task_id`), read
    /// back the same way as [`Self::view_calls`]. A store delete test asserts
    /// exactly one archive fired for the resolved task -- and, on error, that the
    /// cache/map were left intact for the next sync to reconcile.
    pub fn archive_calls(&self) -> std::rc::Rc<std::cell::RefCell<Vec<String>>> {
        std::rc::Rc::clone(&self.archive_calls)
    }

    /// Makes `set_custom_field` (the relation-persist write) return an arbitrary
    /// error (builder-style), for asserting a failed field write surfaces as a
    /// link error and leaves the cache to reconcile on the next sync.
    pub fn failing_set_field(mut self, error: ClickupError) -> Self {
        self.set_field = Err(error);
        self
    }

    /// A shared handle to the recorded `set_custom_field` calls (`(task_id,
    /// field_id, value)`). Clone it before boxing the client, then read back the
    /// serialized relations block and the field id the relation write targeted.
    pub fn set_field_calls(
        &self,
    ) -> std::rc::Rc<std::cell::RefCell<Vec<(String, String, String)>>> {
        std::rc::Rc::clone(&self.set_field_calls)
    }

    /// Overrides the bound List's status set (builder-style), for lifecycle
    /// derivation tests.
    pub fn with_statuses(mut self, statuses: Vec<ClickupStatus>) -> Self {
        self.statuses = Ok(statuses);
        self
    }

    /// Makes `list_statuses` return an arbitrary error (builder-style).
    pub fn failing_statuses(mut self, error: ClickupError) -> Self {
        self.statuses = Err(error);
        self
    }
}

/// The default `create_task` outcome for a fake that was not scripted with
/// [`FakeClickupClient::with_created_task`]: a loud error, never a fabricated
/// task, so a write test that forgot to script a response fails clearly.
#[cfg(any(test, feature = "test-support"))]
fn no_create_scripted() -> Result<ClickupTask, ClickupError> {
    Err(ClickupError::Transport(
        "FakeClickupClient::create_task called without a scripted task".to_string(),
    ))
}

/// The default `update_task` outcome for a fake not scripted with
/// [`FakeClickupClient::with_updated_task`]: a loud error, never a fabricated
/// task, so an edit test that forgot to script a response fails clearly.
#[cfg(any(test, feature = "test-support"))]
fn no_update_scripted() -> Result<ClickupTask, ClickupError> {
    Err(ClickupError::Transport(
        "FakeClickupClient::update_task called without a scripted task".to_string(),
    ))
}

/// The default `get_task` outcome for a fake not scripted with
/// [`FakeClickupClient::with_viewed_task`]: a loud error, never a fabricated
/// task, so a write test whose path reaches the pre-write optimistic-lock fetch
/// without scripting a remote state fails clearly.
#[cfg(any(test, feature = "test-support"))]
fn no_view_scripted() -> Result<ClickupTask, ClickupError> {
    Err(ClickupError::Transport(
        "FakeClickupClient::get_task called without a scripted task".to_string(),
    ))
}

#[cfg(any(test, feature = "test-support"))]
fn fake_user() -> ClickupUser {
    ClickupUser {
        id: 1,
        username: "fake".to_string(),
        email: "fake@example.com".to_string(),
    }
}

#[cfg(any(test, feature = "test-support"))]
impl ClickupClient for FakeClickupClient {
    fn auth_status(&self, _token: &str) -> Result<ClickupUser, ClickupError> {
        self.auth.clone()
    }

    fn task_list(&self, _token: &str, _list_id: &str) -> Result<Vec<ClickupTask>, ClickupError> {
        self.tasks.clone()
    }

    fn list_statuses(
        &self,
        _token: &str,
        _list_id: &str,
    ) -> Result<Vec<ClickupStatus>, ClickupError> {
        self.statuses.clone()
    }

    fn create_task(
        &self,
        _token: &str,
        list_id: &str,
        payload: &TaskCreate,
    ) -> Result<ClickupTask, ClickupError> {
        self.create_calls
            .borrow_mut()
            .push((list_id.to_string(), payload.clone()));
        self.created.clone()
    }

    fn update_task(
        &self,
        _token: &str,
        task_id: &str,
        payload: &TaskUpdate,
    ) -> Result<ClickupTask, ClickupError> {
        self.update_calls
            .borrow_mut()
            .push((task_id.to_string(), payload.clone()));
        self.updated.clone()
    }

    fn get_task(&self, _token: &str, task_id: &str) -> Result<ClickupTask, ClickupError> {
        self.view_calls.borrow_mut().push(task_id.to_string());
        self.viewed.clone()
    }

    fn archive_task(&self, _token: &str, task_id: &str) -> Result<(), ClickupError> {
        self.archive_calls.borrow_mut().push(task_id.to_string());
        self.archived.clone()
    }

    fn set_custom_field(
        &self,
        _token: &str,
        task_id: &str,
        field_id: &str,
        value: &str,
    ) -> Result<(), ClickupError> {
        self.set_field_calls.borrow_mut().push((
            task_id.to_string(),
            field_id.to_string(),
            value.to_string(),
        ));
        self.set_field.clone()
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

    #[test]
    fn deserializes_task_with_epoch_ms_strings_and_priority_object() {
        // The read shape: status/priority nested objects, epoch fields as *strings*.
        let body = r##"{
            "id": "86abc123",
            "name": "Wire the reader",
            "status": {"status": "in progress", "color": "#000", "type": "custom", "orderindex": 1},
            "priority": {"priority": "high", "color": "#f00", "id": "2", "orderindex": "2"},
            "due_date": "1748541600000",
            "start_date": "1748500000000",
            "time_estimate": "3600000",
            "date_updated": "1774587145901",
            "date_created": "1700000000000",
            "markdown_description": "# Body\ncontent",
            "text_content": "Body content",
            "custom_item_id": 1018,
            "tags": [{"name": "backend"}],
            "creator": {"username": "Jack"},
            "custom_fields": [{"id": "uuid-1", "name": "relations", "value": "x"}]
        }"##;
        let task: ClickupTask = serde_json::from_str(body).unwrap();
        assert_eq!(task.id, "86abc123");
        assert_eq!(task.name, "Wire the reader");
        assert_eq!(task.status.status, "in progress");
        assert_eq!(task.priority.as_ref().unwrap().priority, "high");
        assert_eq!(task.due_date, Some(1_748_541_600_000));
        assert_eq!(task.start_date, Some(1_748_500_000_000));
        assert_eq!(task.time_estimate, Some(3_600_000));
        assert_eq!(task.date_updated, Some(1_774_587_145_901));
        assert_eq!(task.date_created, Some(1_700_000_000_000));
        assert_eq!(task.markdown_description, "# Body\ncontent");
        assert_eq!(task.custom_item_id, Some(1018));
        assert_eq!(task.tags[0].name, "backend");
        assert_eq!(task.creator.as_ref().unwrap().username, "Jack");
        assert_eq!(task.custom_fields[0].id, "uuid-1");
    }

    #[test]
    fn deserializes_task_with_integer_epoch_fields() {
        // The write shape uses integers; the same struct must still decode them.
        let body = r#"{
            "id": "1",
            "name": "n",
            "status": {"status": "open"},
            "due_date": 1748541600000,
            "time_estimate": 3600000
        }"#;
        let task: ClickupTask = serde_json::from_str(body).unwrap();
        assert_eq!(task.due_date, Some(1_748_541_600_000));
        assert_eq!(task.time_estimate, Some(3_600_000));
    }

    #[test]
    fn deserializes_task_with_null_and_absent_optional_fields() {
        let body = r#"{
            "id": "1",
            "name": "n",
            "status": {"status": "open"},
            "priority": null,
            "due_date": null
        }"#;
        let task: ClickupTask = serde_json::from_str(body).unwrap();
        assert_eq!(task.priority, None);
        assert_eq!(task.due_date, None);
        assert_eq!(task.start_date, None);
        assert_eq!(task.time_estimate, None);
    }

    #[test]
    fn deserializes_task_list_envelope_and_last_page_flag() {
        let body =
            r#"{"tasks":[{"id":"1","name":"a","status":{"status":"open"}}],"last_page":true}"#;
        let envelope: TaskListEnvelope = serde_json::from_str(body).unwrap();
        assert_eq!(envelope.tasks.len(), 1);
        assert!(envelope.last_page);
    }

    #[test]
    fn fake_with_tasks_returns_scripted_tasks() {
        let task: ClickupTask =
            serde_json::from_str(r#"{"id":"7","name":"t","status":{"status":"open"}}"#).unwrap();
        let client = FakeClickupClient::with_tasks(vec![task.clone()]);
        assert_eq!(client.task_list("pk", "list1").unwrap(), vec![task]);
    }

    #[test]
    fn fake_failing_tasks_returns_scripted_error() {
        let client = FakeClickupClient::failing_tasks(ClickupError::Timeout);
        assert_eq!(
            client.task_list("pk", "list1").unwrap_err(),
            ClickupError::Timeout
        );
    }

    #[test]
    fn deserializes_list_envelope_statuses_with_orderindex_and_type() {
        // The `GET /list/{id}` shape: a `statuses` array carrying name, orderindex
        // (a number here), and ClickUp's status `type`.
        let body = r##"{
            "id": "901234567890",
            "name": "Sprint",
            "statuses": [
                {"status": "to do", "orderindex": 0, "type": "open", "color": "#d3d3d3"},
                {"status": "in progress", "orderindex": 1, "type": "custom", "color": "#f00"},
                {"status": "done", "orderindex": 2, "type": "closed", "color": "#0f0"}
            ]
        }"##;
        let envelope: ListEnvelope = serde_json::from_str(body).unwrap();
        assert_eq!(envelope.statuses.len(), 3);
        assert_eq!(envelope.statuses[0].status, "to do");
        assert_eq!(envelope.statuses[0].orderindex, 0);
        assert_eq!(envelope.statuses[0].status_type, "open");
        assert_eq!(envelope.statuses[0].color, "#d3d3d3");
        assert_eq!(envelope.statuses[2].status, "done");
        assert_eq!(envelope.statuses[2].status_type, "closed");
        assert_eq!(envelope.statuses[2].color, "#0f0");
    }

    #[test]
    fn deserializes_list_status_with_string_orderindex() {
        // ClickUp sends orderindex as a string in some responses; accept both.
        let body = r#"{"status": "review", "orderindex": "3", "type": "custom"}"#;
        let status: ClickupStatus = serde_json::from_str(body).unwrap();
        assert_eq!(status.orderindex, 3);
        assert_eq!(status.color, "");
    }

    #[test]
    fn fake_with_statuses_returns_scripted_statuses() {
        let statuses = vec![ClickupStatus {
            status: "open".to_string(),
            orderindex: 0,
            status_type: "open".to_string(),
            color: String::new(),
        }];
        let client = FakeClickupClient::with_tasks(vec![]).with_statuses(statuses.clone());
        assert_eq!(client.list_statuses("pk", "list1").unwrap(), statuses);
    }

    #[test]
    fn fake_failing_statuses_returns_scripted_error() {
        let client = FakeClickupClient::with_tasks(vec![]).failing_statuses(ClickupError::Timeout);
        assert_eq!(
            client.list_statuses("pk", "list1").unwrap_err(),
            ClickupError::Timeout
        );
    }

    #[test]
    fn task_create_serializes_write_shape_and_omits_none_fields() {
        // The write shape is asymmetric with the read shape: body is
        // `markdown_content` (not `markdown_description`), priority is a bare int.
        let payload = TaskCreate {
            name: "Wire create".to_string(),
            markdown_content: Some("the body".to_string()),
            status: Some("open".to_string()),
            priority: Some(2),
            due_date: Some(1_748_541_600_000),
            start_date: None,
            time_estimate: Some(3_600_000),
            custom_item_id: None,
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["name"], "Wire create");
        assert_eq!(json["markdown_content"], "the body");
        assert_eq!(json["status"], "open");
        assert_eq!(json["priority"], 2);
        assert_eq!(json["due_date"], 1_748_541_600_000i64);
        assert_eq!(json["time_estimate"], 3_600_000i64);
        // A `None` field is omitted entirely, not sent as JSON null.
        assert!(json.get("start_date").is_none());
    }

    #[test]
    fn task_create_serializes_custom_item_id_when_set() {
        // A type-stamped create carries the ClickUp custom_item_id so the new task
        // materializes under the bound List's configured task type (RFC-056).
        let payload = TaskCreate {
            name: "Wire create".to_string(),
            custom_item_id: Some(1001),
            ..Default::default()
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["custom_item_id"], 1001i64);
    }

    #[test]
    fn task_create_omits_custom_item_id_when_unset() {
        // No configured task type: the field is omitted, so ClickUp defaults the
        // new task to the List's native "Task" type (no behavior change).
        let payload = TaskCreate {
            name: "Wire create".to_string(),
            ..Default::default()
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert!(json.get("custom_item_id").is_none());
    }

    #[test]
    fn task_create_minimal_payload_serializes_only_name() {
        let payload = TaskCreate {
            name: "just a name".to_string(),
            ..Default::default()
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json, serde_json::json!({"name": "just a name"}));
    }

    #[test]
    fn fake_create_task_records_payload_and_returns_scripted_task() {
        let created: ClickupTask =
            serde_json::from_str(r#"{"id":"90abc","name":"n","status":{"status":"open"}}"#)
                .unwrap();
        let client = FakeClickupClient::valid(user(1)).with_created_task(created.clone());
        let calls = client.create_calls();

        let payload = TaskCreate {
            name: "Wire create".to_string(),
            markdown_content: Some("body".to_string()),
            ..Default::default()
        };
        let result = client.create_task("pk_x", "list123", &payload).unwrap();

        assert_eq!(result, created);
        let recorded = calls.borrow();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, "list123");
        assert_eq!(recorded[0].1, payload);
    }

    #[test]
    fn fake_create_task_without_scripted_task_errors() {
        let client = FakeClickupClient::valid(user(1));
        let err = client
            .create_task("pk_x", "list1", &TaskCreate::default())
            .unwrap_err();
        assert!(matches!(err, ClickupError::Transport(_)));
    }

    #[test]
    fn task_update_serializes_only_present_fields() {
        // A partial edit: `name` is omitted (unchanged), so ClickUp leaves the
        // task's name alone rather than blanking it; priority is a bare int.
        let payload = TaskUpdate {
            name: None,
            markdown_content: Some("new body".to_string()),
            status: None,
            priority: Some(2),
            due_date: Some(1_748_541_600_000),
            start_date: None,
            time_estimate: Some(3_600_000),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["markdown_content"], "new body");
        assert_eq!(json["priority"], 2);
        assert_eq!(json["due_date"], 1_748_541_600_000i64);
        assert_eq!(json["time_estimate"], 3_600_000i64);
        assert!(json.get("name").is_none());
        assert!(json.get("status").is_none());
        assert!(json.get("start_date").is_none());
    }

    #[test]
    fn task_update_serializes_status_verbatim_under_status_key() {
        // An advance is a status-only PUT: the raw ClickUp string rides the
        // `status` wire field, everything else omitted.
        let payload = TaskUpdate {
            status: Some("in progress".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["status"], "in progress");
        assert_eq!(json, serde_json::json!({"status": "in progress"}));
    }

    #[test]
    fn task_update_default_serializes_to_empty_object() {
        let json = serde_json::to_value(TaskUpdate::default()).unwrap();
        assert_eq!(json, serde_json::json!({}));
    }

    #[test]
    fn fake_update_task_records_payload_and_returns_scripted_task() {
        let updated: ClickupTask = serde_json::from_str(
            r#"{"id":"90abc","name":"n","status":{"status":"open"},"date_updated":"1774587145901"}"#,
        )
        .unwrap();
        let client = FakeClickupClient::valid(user(1)).with_updated_task(updated.clone());
        let calls = client.update_calls();

        let payload = TaskUpdate {
            markdown_content: Some("body".to_string()),
            priority: Some(2),
            ..Default::default()
        };
        let result = client.update_task("pk_x", "90abc", &payload).unwrap();

        assert_eq!(result, updated);
        let recorded = calls.borrow();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, "90abc");
        assert_eq!(recorded[0].1, payload);
    }

    #[test]
    fn fake_update_task_without_scripted_task_errors() {
        let client = FakeClickupClient::valid(user(1));
        let err = client
            .update_task("pk_x", "90abc", &TaskUpdate::default())
            .unwrap_err();
        assert!(matches!(err, ClickupError::Transport(_)));
    }

    #[test]
    fn fake_get_task_records_id_and_returns_scripted_task() {
        let viewed: ClickupTask = serde_json::from_str(
            r#"{"id":"90abc","name":"n","status":{"status":"open"},"date_updated":"1774587145901"}"#,
        )
        .unwrap();
        let client = FakeClickupClient::valid(user(1)).with_viewed_task(viewed.clone());
        let calls = client.view_calls();

        let result = client.get_task("pk_x", "90abc").unwrap();

        assert_eq!(result, viewed);
        let recorded = calls.borrow();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0], "90abc");
    }

    #[test]
    fn fake_get_task_without_scripted_task_errors() {
        let client = FakeClickupClient::valid(user(1));
        let err = client.get_task("pk_x", "90abc").unwrap_err();
        assert!(matches!(err, ClickupError::Transport(_)));
    }

    #[test]
    fn fake_archive_task_records_id_and_succeeds_by_default() {
        // A valid client archives (soft-deletes) successfully; the call is
        // recorded by task id so a store delete test can assert it fired once.
        let client = FakeClickupClient::valid(user(1));
        let calls = client.archive_calls();

        client.archive_task("pk_x", "90abc").unwrap();

        let recorded = calls.borrow();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0], "90abc");
    }

    #[test]
    fn fake_failing_archive_returns_scripted_error() {
        let client = FakeClickupClient::valid(user(1))
            .failing_archive(ClickupError::Upstream { status: 500 });
        let err = client.archive_task("pk_x", "90abc").unwrap_err();
        assert_eq!(err, ClickupError::Upstream { status: 500 });
    }

    #[test]
    fn fake_set_custom_field_records_call_and_succeeds_by_default() {
        // A valid client persists the field successfully; the call is recorded as
        // (task_id, field_id, value) so a link test can read back the serialized
        // relations block and the field id the write targeted.
        let client = FakeClickupClient::valid(user(1));
        let calls = client.set_field_calls();

        client
            .set_custom_field("pk_x", "90abc", "uuid-rel", "- implements: RFC-056")
            .unwrap();

        let recorded = calls.borrow();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, "90abc");
        assert_eq!(recorded[0].1, "uuid-rel");
        assert_eq!(recorded[0].2, "- implements: RFC-056");
    }

    #[test]
    fn fake_failing_set_field_returns_scripted_error() {
        let client = FakeClickupClient::valid(user(1))
            .failing_set_field(ClickupError::Upstream { status: 500 });
        let err = client
            .set_custom_field("pk_x", "90abc", "uuid-rel", "v")
            .unwrap_err();
        assert_eq!(err, ClickupError::Upstream { status: 500 });
    }
}
