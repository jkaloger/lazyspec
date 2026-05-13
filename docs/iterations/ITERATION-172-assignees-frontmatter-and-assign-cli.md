---
title: Assignees frontmatter and assign CLI
type: iteration
status: accepted
author: agent
date: 2026-05-12
tags: []
related:
- implements: STORY-126
---

## Goal

Ship `assignees: Vec<String>` on doc frontmatter + `[orchestration]` config (`agent_users`, `claim_type`) + `lazyspec assign` CLI. Bidirectional sync w/ GH native assignees. Per-backend validation @ store seam. Best-effort `kick` over daemon socket.

## Test Plan

- **AC1** filesystem round-trip — integration `tests/cli_assign_test.rs`. Write doc w/ `assignees: [alice, claude-bot]` via fs backend, reload via `Store::load`, assert `DocMeta.assignees == [alice, claude-bot]` (order preserved). Fixture: `TempDir` + fs config.
- **AC2** git-ref round-trip — integration `tests/cli_git_ref_assignees_test.rs`. Init real git repo (mirror `tests/cli_git_ref_*.rs` pattern). Write doc w/ `assignees: [bob]` via git-ref backend → blob in ref → re-materialize cache (cold) → assert `["bob"]`. Fixture: `TempDir`, real `git` binary.
- **AC3** local→GH sync — unit `src/engine/store_dispatch.rs::tests::gh_push_assignees_calls_issue_assign`. Cache file has `assignees: [claude-bot]`. Call `gh_store.push_cache(td, "RFC-001")`. `MockGhClient` records `assignees_add` call w/ `["claude-bot"]`. Fixture: `MockGhClient` extended w/ `last_assignees_add`/`last_assignees_remove`.
- **AC4** GH→local sync — unit `src/engine/store_dispatch.rs::tests::gh_load_pulls_native_assignees`. `MockGhClient::with_view_issue` w/ `assignees: [alice]` on `GhIssue`. Run path that materializes cache from remote (extend `GhIssue` struct + fetch path). Assert cache frontmatter `assignees: [alice]`.
- **AC5** GH rejects unknown user — unit `src/engine/store_dispatch.rs::tests::gh_write_unknown_assignee_errors`. `MockGhClient::with_user_lookup(["alice"])` (known set). Push cache w/ `assignees: [ghost]`. Assert err msg contains "ghost" and "not a GitHub user". Cache file unmodified after err.
- **AC6** fs+git-ref accept free strings — unit covered by AC1/AC2 round-trip w/ value `not-a-real-github-user`. No reject path on these backends.
- **AC7** default user — integration `tests/cli_assign_test.rs::assign_default_user_picks_first_agent_user`. Config `agent_users = ["claude-bot", "other-bot"]`. Run `lazyspec assign STORY-001`. Assert frontmatter `assignees` contains `claude-bot`.
- **AC8** explicit user — integration same file `assign_with_user_appends`. Run `lazyspec assign STORY-001 --user alice`. Assert `assignees` ends w/ `alice`.
- **AC9** `--json` — integration `assign_json_output`. Run w/ `--json`. Parse stdout once via `serde_json::from_str`. Assert `{ id, assignee_added, assignees: [...] }`.
- **AC10** kick socket — unit `src/cli/assign.rs::tests`. Two cases: (a) bound `UnixListener` on `<root>/.lazyspec/daemon.sock`, run assign, assert listener receives bytes (kick payload — single `kick\n` line is fine for v1). (b) no socket present, assign succeeds, no err. Fixture: `TempDir` + tokio-free `std::os::unix::net::UnixListener`.

Tradeoffs:
- AC3/4/5 need GH user-resolution + assignee mutation at GH seam. Extend `GhIssueWriter` w/ `issue_assignees(...)` + `GhIssueReader` w/ `user_lookup(login) -> bool` (or one batch `users_resolve(&[String]) -> Vec<bool>`). New trait methods → ADR-worthy (dictum 6: ≥2 uses — GH store push + GH store create both will call; daemon eligibility later). Plan ADR: "GitHub user resolution + assignee mutation at GhIssueWriter seam".
- `GhIssue` gains `assignees: Vec<GhAuthor>` field (extra `#[serde(default)]`). Real `gh` CLI surfaces this via `--json assignees`.
- `kick` payload is one line `kick\n` v1; full IPC protocol deferred to slice 6.

## Changes

1. **DocMeta + RawFrontmatter assignees field**
   ACs: 1,2,6,7,8
   File: `src/engine/document.rs`
   Add `assignees: Vec<String>` to `DocMeta` (default `vec![]`). Add `#[serde(default)] assignees: Vec<String>` to `RawFrontmatter`. Populate in `DocMeta::parse`. Update test `DocMeta` fixtures across codebase (default `vec![]`).
   Verify: `cargo test -p lazyspec --lib engine::document`.

2. **CacheFrontmatter assignees field**
   ACs: 3,4
   File: `src/engine/store_dispatch.rs`
   Add `assignees: Vec<String>` to `CacheFrontmatter` struct (top of file). Wire through `write_cache_file` from `meta.assignees`. Round-trip test in `cache_frontmatter_round_trips_provenance`-style unit test → `cache_frontmatter_round_trips_assignees`.
   Verify: `cargo test -p lazyspec --lib engine::store_dispatch`.

3. **`[orchestration]` config section**
   ACs: 7
   File: `src/engine/config.rs`
   Add `OrchestrationConfig { agent_users: Vec<String>, claim_type: String (default "story") }`. Add `orchestration: Option<OrchestrationConfig>` on `Config` + `RawConfig`. Default `agent_users` = `vec![]`, default `claim_type` = `"story"`. Same shape as existing `CoordinationConfig`. Unit tests: defaults, explicit values, absent section.
   Verify: `cargo test -p lazyspec --lib engine::config`.

4. **Extend GhIssue + GhIssueReader/Writer traits**
   ACs: 3,4,5
   Files: `src/engine/gh.rs`, `src/engine/store_dispatch.rs`
   On `GhIssue`: add `#[serde(default)] assignees: Vec<GhAuthor>`. On `GhIssueWriter`: add `fn issue_assignees(&self, repo: &str, number: u64, add: &[String], remove: &[String]) -> Result<()>`. On `GhIssueReader`: add `fn user_exists(&self, login: &str) -> Result<bool>` (uses `gh api users/{login}`, treats 404 as `Ok(false)`). Implement on `GhCli` via `gh issue edit --add-assignee` / `--remove-assignee` and `gh api users/{login}`. Update json field lists on `issue_list`/`issue_view` to include `assignees`. Extend `MockGhClient`: `with_known_users`, `last_assignees_add`/`remove`, `user_exists` honoring the known-set. Default known-set behaviour: `true` (back-compat for existing tests).
   Verify: `cargo test -p lazyspec --lib engine::gh`.

5. **GH store: push + load + validate assignees**
   ACs: 3,4,5
   File: `src/engine/store_dispatch.rs`
   In `push_cache`: after deserializing meta, validate each `meta.assignees` entry via `client.user_exists(login)?`. On unknown → `bail!("assignee {} is not a GitHub user", login)` **before** any mutating call. After body push, diff local vs `remote_issue.assignees` and call `issue_assignees(repo, num, add, remove)`. In `update`: same pattern. On load (where cache file is built from remote — find existing GH→cache path and extend it; expected in `fetch` path), populate `meta.assignees` from `remote_issue.assignees.iter().map(|a| a.login)`. Update `IssueContext` or `deserialize` signature if needed so `issue_body::deserialize` can ferry assignees through (or stamp them onto meta post-deserialize, since they're GH-native, not body-embedded).
   Verify: `cargo test -p lazyspec --lib engine::store_dispatch`.

6. **Filesystem + git-ref assignee passthrough**
   ACs: 1,2,6
   Files: `src/engine/fs_ops.rs`, `src/engine/git_ref_store.rs`
   Verify-then-extend: serde round-trips on `RawFrontmatter` should already preserve `assignees` once Task 1 lands. Audit `fs_ops::update_document` and `git_ref_store::set_provenance`-style helpers to confirm assignees field is not stripped. If `git_ref_store` constructs a `DocMeta` manually (line ~118), wire through `assignees: vec![]`.
   Verify: `cargo test --test cli_assign_test` (AC1), `cargo test --test cli_git_ref_assignees_test` (AC2).

7. **`lazyspec assign` CLI command**
   ACs: 7,8,9,10
   Files: `src/cli.rs`, `src/cli/assign.rs` (new), `src/cli.rs` dispatch in `main.rs`/wherever subcommands are matched.
   Add `Commands::Assign { doc_id: String, user: Option<String>, json: bool }`. Mirror `cli/link.rs` shape — resolve doc → load store → resolve `TypeDef` → `dispatch_for_type` → for fs/git-ref backends, use `rewrite_frontmatter` to append `user` to `assignees` sequence; for github-issues, call a new `DocumentStore::add_assignee(type_def, doc_id, user)` method (cleaner than reusing `update`'s field-key map) OR re-use existing pattern of writing cache then `push_cache`. Recommendation: extend `DocumentStore` trait w/ `set_assignees(&mut self, td, doc_id, &[String]) -> Result<()>` (parallel to `set_provenance`). Default user: first of `config.orchestration.agent_users` (err if both `--user` absent and `agent_users` empty). After persist: best-effort kick (Task 8).
   Add CLI tests w/ `MockGhClient` for github-issues path.
   Verify: `cargo test --test cli_assign_test`.

8. **Best-effort daemon kick**
   ACs: 10
   File: `src/cli/assign.rs` (helper `fn send_kick(root: &Path)`)
   Const: `pub const DAEMON_SOCKET: &str = ".lazyspec/daemon.sock";` (declare in `src/cli/assign.rs` for now; future RFC-041 slice 2 may relocate). `std::os::unix::net::UnixStream::connect(root.join(DAEMON_SOCKET))` — on success write `b"kick\n"` and drop. On any err (ENOENT, ECONNREFUSED, EACCES): swallow, return `Ok(())`. Unit tests bind a real `UnixListener` for the success path.
   Verify: `cargo test -p lazyspec assign::tests`.

9. **README + help updates**
   ACs: 7,8,9
   File: `README.md`, `src/cli.rs` doc-comment on `Assign`.
   Document `lazyspec assign <DOC_ID> [--user X] [--json]` + `[orchestration]` config section w/ `agent_users` + `claim_type` example. Add to existing CLI table.
   Verify: manual `cargo run -- help assign` + `cargo run -- help` show new command.

10. **Integration tests file**
    ACs: 1,7,8,9,10
    File: `tests/cli_assign_test.rs` (new)
    Tests: `assign_appends_default_user`, `assign_with_user_flag`, `assign_json_output`, `assign_kicks_listening_socket`, `assign_succeeds_without_socket`, `filesystem_round_trip_assignees`. Use `tempfile::TempDir` + write `.lazyspec.toml` w/ `[orchestration] agent_users = ["claude-bot"]`. Spawn `lazyspec` via `assert_cmd` (mirror existing `cli_link_test.rs` / `cli_create_test.rs`).
    Verify: `cargo test --test cli_assign_test`.

11. **Git-ref integration tests file**
    AC: 2
    File: `tests/cli_git_ref_assignees_test.rs` (new)
    Mirror `tests/cli_git_ref_show_test.rs` setup. Real `git init` + git-ref-backed type. Write doc w/ assignees → push to ref → blow away cache → re-materialize → assert.
    Verify: `cargo test --test cli_git_ref_assignees_test`.

## Notes

Verified file paths:
- `src/engine/document.rs` — `DocMeta` line 188, `RawFrontmatter` line 204
- `src/engine/config.rs` — `Config` line 226, `CoordinationConfig` line 98 (template for `OrchestrationConfig`), `RawConfig` line 314
- `src/engine/store_dispatch.rs` — `CacheFrontmatter` line 20, `GithubIssuesStore::push_cache` line 159, `write_cache_file` line 423
- `src/engine/gh.rs` — `GhIssue` line 22, `GhIssueReader` line 115, `GhIssueWriter` line 127, `GhCli::issue_edit` line 271, `MockGhClient` in `test_support` mod line ~491
- `src/engine/git_ref_store.rs` — `DocMeta` construction line 118
- `src/cli.rs` — top-level `Commands` enum line 70
- `src/cli/link.rs` — closest existing model for new `assign` command (`rewrite_frontmatter` + backend-aware push)
- `src/cli/provenance.rs` — closest model for append-list field semantics (`set_provenance` trait method)
- `.lazyspec.toml` — confirmed no existing `[orchestration]` section; daemon socket path absent from src

ADR-worthy:
- New trait surface area on `GhIssueWriter` (`issue_assignees`) and `GhIssueReader` (`user_exists`) — 2nd use case (daemon eligibility, slice 4) coming. Satisfies dictum 6.
- New `DocumentStore::set_assignees` method on the store trait (parallels `set_provenance`).
- `DAEMON_SOCKET` constant placement: kept in `cli/assign.rs` for now; will migrate to a daemon module in slice 2.

Tradeoffs:
- GH issue body format does NOT embed assignees (they're GH-native primitives). So `issue_body::serialize`/`deserialize` stays untouched. Assignees ride alongside body push via separate `issue_assignees` call.
- Default `--user` w/ empty `agent_users` → hard error (better than silent no-op). Surface in `--json` as `{ "error": "..." }`.

