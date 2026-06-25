---
title: GitHub issue comments read-through
type: iteration
status: complete
author: jkaloger
date: 2026-06-25
tags: []
related:
- implements: STORY-160
---## Changes

### 1. Comment data type + reader seam (`src/engine/gh.rs`)
- Add `GhComment { author: String, body: String, timestamp: String }` (derive `Debug,Clone,Deserialize,PartialEq,Eq`). REST shape -> `author.login`, `body`, `created_at`; map via `RawComment` -> `GhComment` parse fn `parse_comments_json(stdout) -> Result<Vec<GhComment>>` (mirrors `parse_issue_list_json`).
- Extend `trait GhIssueReader` (`gh.rs:115`) with `fn issue_comments(&self, repo: &str, number: u64) -> Result<Vec<GhComment>>` (RFC-050: comments stay on existing reader, REST, no GraphQL). READ-ONLY -> NO method on `GhIssueWriter`.
- Impl on `GhCli`: shell `gh api repos/{repo}/issues/{number}/comments` via existing `run_gh_checked` -> `parse_comments_json`. GET only -> no mutation.
- Impl on `MockGhClient` (`test_support`, `gh.rs:469`): add `view_comments: RefCell<Vec<GhComment>>` + `with_comments(..)` builder + `comments_call_count: Cell<usize>` (MockGhClient has NO call counter today -> must add for AC4). `issue_comments` increments counter, returns clone.

### 2. Comments NOT on doc model
- DocMeta (`src/engine/document.rs:257`) is parsed-from-frontmatter -> NOT the place for live comments (would force them into cache body -> violates "never merged"). Surface comments as a sidecar fetched at serialization time, keyed by issue number.
- `parse_issue`/`build_cache_content` (`src/engine/issue_cache.rs:302,352`) UNCHANGED -> comments never enter cached frontmatter/body.

### 3. Fetch path (`src/cli/show.rs` + new helper)
- New fn `fetch_comments_for_doc(doc, config, root, gh: &dyn GhIssueReader) -> Vec<serde_json::Value>`:
  - Resolve doc store: `config.documents.types.iter().find(|t| t.name == doc.doc_type.as_str())` (lookup mirrors `config.rs:871`) -> `type_def.store` (`TypeDef.store`, `config.rs:234`). NOTE: store lives on TypeDef NOT DocMeta.
  - `store != StoreBackend::GithubIssues` -> return `vec![]`, NO `gh.issue_comments` call (filesystem AC4 short-circuit).
  - Else: `repo` via `github::resolve_repo(config, root)`; issue number via `IssueMap::load(root)?.get(&doc.id).issue_number` (`src/engine/issue_map.rs`).
  - `gh.issue_comments(repo, number)` -> map each `GhComment` -> `json!({author,body,timestamp})`.

### 4. Thread inputs through run_json signatures (required wiring)
- `show::run_json` (`src/cli/show.rs:117`) signature TODAY `(store, id, expand, max_ref_lines, fs)` -> NO config/root/gh. CHANGE to `(store, id, expand, max_ref_lines, fs, config: &Config, root: &Path, gh: &dyn GhIssueReader)`.
- `status::run_json` (`src/cli/status.rs:8`) signature TODAY `(store, config)` -> add `root: &Path, gh: &dyn GhIssueReader`.
- `main.rs` callers updated: Show site `~src/main.rs:170` and Status site `~:280`. `refresh_github_cache` (`src/main.rs:570`) builds repo/`GhCli`/`IssueMap` LOCALLY and returns nothing -> those are NOT in scope at run_json call sites today; construct `GhCli::new()` + pass `&cwd`,`&config` at each call site (reuse same resolve seam).

### 5. Surface in JSON via post-insert key (NOT a doc_to_json param)
- show::run_json calls `doc_to_json_with_family(doc, store)` (`src/cli/show.rs:143`); status::run_json calls `doc_to_json(d)` (`src/cli/status.rs:9`). Do NOT change either signature.
- show::run_json: after building `json`, `json["comments"] = Value::Array(fetch_comments_for_doc(doc,config,root,gh))`. `body` field still set from `get_body_raw`/`expanded` UNCHANGED -> comments never touch `--body`.
- status::run_json: per doc, build json via `doc_to_json(d)` then insert `comments` key (= fetch result) before pushing into `documents[]`. Each entry `{author,body,timestamp}`. Empty fetch -> `"comments": []`.

## Test Plan

- AC1 (show --json two comments): `MockGhClient::with_comments(vec![c1,c2])` -> `show::run_json` -> `json["comments"]` len==2, each has `author`+`body`+`timestamp` from fake.
- AC2 (status --json comments): same fake -> `status::run_json` -> `documents[i]["comments"]` len==2, author/body/timestamp present.
- AC3 (body byte-for-byte unchanged): assert `show::run_json` `json["body"]` == `get_body_raw` output with comments present; assert `build_cache_content` output contains NO comment text -> never merged. `GhIssueWriter` has no comment method -> never round-tripped.
- AC4 (filesystem-backed, no fetch): doc whose `type_def.store == StoreBackend::Filesystem` -> `fetch_comments_for_doc` returns `vec![]` AND `MockGhClient.comments_call_count == 0` (new counter) -> json `comments` absent-or-empty, NO fetch attempted.
- AC5 (zero comments): `MockGhClient::with_comments(vec![])` -> `show::run_json` `json["comments"]` present AND `== []`.
- AC6 (read-only, no write path): assert no `--attr`/comment flag on `update`; `GhIssueWriter` has no comment method; `issue_comments` lives only on `GhIssueReader`. Mock writer never receives comment.

## Notes

- READ-ONLY: no posting/editing/deleting -> `issue_comments` on `GhIssueReader` only; `GhIssueWriter` untouched (RFC-050 non-goal "Posting comments").
- NOT round-tripped: comments fetched on read, never written back -> `issue_edit` body arg never carries comment text.
- Body serialization stays clean: comments bypass `parse_issue`/`build_cache_content` -> HTML-comment metadata block stays sole source of authored content; `--body` unaffected -> AC3 trivially holds.
- Store backend lives on `TypeDef.store` (`config.rs:234`), reached via `doc.doc_type` -> type_def lookup; NOT a DocMeta field -> filesystem short-circuit keys off the resolved TypeDef.
- run_json signatures MUST grow (config/root/gh) -> they lack these inputs today; refresh_github_cache does not expose them. Concrete signature + caller changes in Changes #4.
- MockGhClient has no call counter today -> add `comments_call_count: Cell<usize>` for AC4 `== 0` assertion.
- Depends on STORY-155 (210) gh-access seam ONLY -> uses `GhIssueReader` trait + `MockGhClient` fake; ships independent of projects (no GraphQL, no `GhGraphql`).
- REST via `gh api repos/{o}/{r}/issues/{n}/comments` -> uses `gh auth login` token automatically; no new dep.
- Fetch-on-read (no cache): comments NOT persisted to `.lazyspec/cache` -> always live; cost is one extra `gh api` per `show`/`status` of a github-backed doc. Acceptable for JSON-only surface.