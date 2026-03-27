---
title: gh CLI integration layer
type: iteration
status: draft
author: agent
date: 2026-03-27
tags: []
related:
- implements: STORY-094
---


## Changes

### Task 1: gh CLI runner module

ACs addressed: execute-gh-issue-create, execute-gh-issue-edit, execute-gh-issue-list, execute-gh-issue-view, execute-gh-issue-close, parse-json-output

Files:
- Create: `src/engine/gh.rs`
- Modify: `src/engine.rs` (add `pub mod gh;`)

Introduce a `GhCli` struct that wraps `std::process::Command` calls to the `gh` binary. Follow the same pattern as `src/engine/git_status.rs` and `src/engine/reservation.rs` for shelling out and capturing output.

The struct exposes typed methods:

- `issue_create(repo, title, body, labels) -> Result<GhIssue>` runs `gh issue create --repo {repo} --title {title} --body {body} --label {labels} --json number,url,title,body,labels,state,updatedAt`
- `issue_edit(repo, number, body, labels) -> Result<()>` runs `gh issue edit {number} --repo {repo} --body {body} --remove-label/--add-label`
- `issue_list(repo, labels, json_fields) -> Result<Vec<GhIssue>>` runs `gh issue list --repo {repo} --label {labels} --json {fields} --limit 1000`
- `issue_view(repo, number) -> Result<GhIssue>` runs `gh issue view {number} --repo {repo} --json number,title,body,labels,state,updatedAt`
- `issue_close(repo, number) -> Result<()>` runs `gh issue close {number} --repo {repo}`
- `issue_reopen(repo, number) -> Result<()>` runs `gh issue reopen {number} --repo {repo}`

Each method builds a `Command`, captures stdout/stderr, and returns a typed result. JSON deserialization uses `serde_json` into a `GhIssue` struct with fields: `number: u64`, `url: String`, `title: String`, `body: String`, `labels: Vec<GhLabel>`, `state: String`, `updated_at: String`.

Use a trait `GhClient` with these methods so tests can substitute a mock implementation.

How to verify:
```
cargo test gh
```

---

### Task 2: JSON output parsing

ACs addressed: parse-json-output

Files:
- Modify: `src/engine/gh.rs`

Add a `parse_issue_json(stdout: &str) -> Result<GhIssue>` and `parse_issue_list_json(stdout: &str) -> Result<Vec<GhIssue>>` function. These are internal to the module but testable independently.

Handle edge cases: empty list response (`[]`), single-issue response (object vs array), fields missing from partial `--json` output.

The `GhLabel` struct has `name: String` and `color: String`.

How to verify:
```
cargo test gh::tests::parse
```

---

### Task 3: Auth validation

ACs addressed: validate-auth, handle-gh-not-installed, handle-auth-failure

Files:
- Modify: `src/engine/gh.rs`

Add `auth_status() -> Result<AuthStatus>` that runs `gh auth status` and returns an enum:

```rust
pub enum AuthStatus {
    Authenticated { user: String, host: String },
    NotAuthenticated(String),
    GhNotInstalled,
}
```

Detection logic:
- If `Command::new("gh")` fails with `NotFound`, return `GhNotInstalled`
- If exit code is non-zero, parse stderr for auth details and return `NotAuthenticated`
- If exit code is zero, parse stdout for the user/host and return `Authenticated`

How to verify:
```
cargo test gh::tests::auth
```

---

### Task 4: Label management

ACs addressed: manage-labels

Files:
- Modify: `src/engine/gh.rs`

Add `label_create(repo, name, description, color) -> Result<()>` that runs `gh label create {name} --repo {repo} --description {desc} --color {color}`.

Add `label_ensure(repo, name, description, color) -> Result<()>` that calls `label_create` and treats "already exists" errors as success (gh returns exit code 1 with "already exists" in stderr).

Add a helper `type_label(type_name: &str) -> String` that returns `format!("lazyspec:{}", type_name)`.

Add `deterministic_color(type_name: &str) -> String` that hashes the type name and returns a 6-char hex color. Use the same hashing approach as `src/engine/hashing.rs`.

How to verify:
```
cargo test gh::tests::label
```

---

### Task 5: Error handling

ACs addressed: handle-gh-not-installed, handle-auth-failure, handle-rate-limit, handle-network-error

Files:
- Modify: `src/engine/gh.rs`

Define a `GhError` enum:

```rust
pub enum GhError {
    NotInstalled,
    AuthFailure(String),
    RateLimit { retry_after: Option<String> },
    NetworkError(String),
    ApiError { status: Option<u16>, message: String },
    JsonParse(String),
}
```

Add a `classify_error(exit_code: i32, stderr: &str) -> GhError` function that inspects stderr for patterns:
- "not found" / IoError NotFound on spawn -> `NotInstalled`
- "auth" / "login" / "token" -> `AuthFailure`
- "rate limit" / "API rate" / "403" with rate limit message -> `RateLimit`
- "Could not resolve host" / "connection" / "timeout" -> `NetworkError`
- Everything else -> `ApiError`

Each command method in Task 1 calls `classify_error` on failure and returns the typed error.

How to verify:
```
cargo test gh::tests::error
```

## Test Plan

### Test 1: issue_create returns parsed issue (AC: execute-gh-issue-create, parse-json-output)
Mock `GhClient` returns a JSON string matching gh output format. Call `issue_create`, assert the returned `GhIssue` has the correct number, url, title, and labels. Isolated, fast, no network.

### Test 2: issue_list parses array response (AC: execute-gh-issue-list, parse-json-output)
Feed `parse_issue_list_json` a JSON array with two issues. Assert both are returned with correct fields. Feed an empty array, assert empty vec.

### Test 3: issue_view parses single issue (AC: execute-gh-issue-view, parse-json-output)
Feed `parse_issue_json` a single JSON object. Assert all fields deserialize correctly including `updated_at` and nested labels.

### Test 4: auth_status detects gh not installed (AC: handle-gh-not-installed)
When command spawn returns `NotFound` IO error, `auth_status` returns `GhNotInstalled`. Tested via the `GhClient` trait mock.

### Test 5: auth_status detects auth failure (AC: handle-auth-failure)
When `gh auth status` returns non-zero with stderr containing "not logged in", return `NotAuthenticated` with the message.

### Test 6: classify_error maps rate limit (AC: handle-rate-limit)
Feed stderr containing "API rate limit exceeded" to `classify_error`. Assert it returns `GhError::RateLimit`.

### Test 7: classify_error maps network error (AC: handle-network-error)
Feed stderr containing "Could not resolve host" to `classify_error`. Assert `GhError::NetworkError`.

### Test 8: label_ensure succeeds on existing label (AC: manage-labels)
Mock `label_create` to return an "already exists" error. Assert `label_ensure` returns `Ok(())`.

### Test 9: type_label format (AC: manage-labels)
Assert `type_label("iteration")` returns `"lazyspec:iteration"`. Assert `type_label("story")` returns `"lazyspec:story"`.

### Test 10: deterministic_color is stable (AC: manage-labels)
Assert `deterministic_color("iteration")` returns the same hex string across multiple calls. Assert different type names produce different colors.

## Notes

- The `GhClient` trait enables mock-based testing without network access. All unit tests use mocks; integration tests (not in this iteration) would call real `gh` against a test repo.
- `src/engine/gh.rs` is the sole module that imports `std::process::Command` for `gh` calls. All other modules interact through the `GhClient` trait.
- The `--json` flag on `gh` commands is the primary mechanism for structured output. The fallback is to parse stderr for error classification only.
- Rate limit handling captures the error but does not retry. Retry-with-backoff is a future concern for the cache layer (separate story).
