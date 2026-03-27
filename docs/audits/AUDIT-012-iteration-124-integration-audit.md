---
title: ITERATION-124 integration audit
type: audit
status: complete
author: jkaloger
date: 2026-03-28
tags: []
related:
- related-to: ITERATION-124
- related-to: STORY-096
- related-to: RFC-037
---






## Scope

Spec compliance audit of ITERATION-124 against four criteria: current codebase alignment, STORY-096 acceptance criteria coverage, RFC-037 design fidelity, and SOLID principles.

## Criteria

1. Codebase alignment: do the iteration's proposed files, modules, and patterns match what actually exists?
2. AC coverage: does every STORY-096 acceptance criterion have at least one task addressing it?
3. RFC fidelity: does the iteration implement the RFC-037 cache/fetch design as specified?
4. SOLID: does the proposed design follow single responsibility, open/closed, Liskov substitution, interface segregation, and dependency inversion principles?

## Findings

### Finding 1: Task 1 proposes creating `src/engine/issue_cache.rs` but caching already exists in `setup.rs` and `store_dispatch.rs`

Severity: high
Location: `src/cli/setup.rs:99-111`, `src/engine/store_dispatch.rs:320-360`

ITERATION-124 Task 1 proposes a new `IssueCache` struct in `src/engine/issue_cache.rs` with `cache.lock` as a JSON file tracking per-document timestamps. The codebase already has two separate cache-write implementations: `setup.rs::write_cache_file` (writes `{number}.md` with non-standard frontmatter: `number`, `title`, `state`, `updated_at`) and `store_dispatch.rs::write_cache_file` (writes `{id}.md` with standard lazyspec frontmatter: `title`, `type`, `status`, `author`, `date`, `tags`, `related`).

These two functions produce incompatible cache file formats. The `setup.rs` version writes raw issue metadata, while `store_dispatch.rs` writes normalized lazyspec documents. Task 1 would introduce a third layer without reconciling the existing two.

Recommendation: consolidate the existing `write_cache_file` functions into the proposed `IssueCache` module. The `setup.rs` version should be replaced entirely, since `Store::load_with_fs` expects standard lazyspec frontmatter in cache files and the `setup.rs` format will fail to parse.

### Finding 2: `setup.rs` cache format is incompatible with `Store::load_with_fs`

Severity: critical
Location: `src/cli/setup.rs:99-111`, `src/engine/store.rs:46-64`

`Store::load_with_fs` reads `.lazyspec/cache/{type}/` and passes files through `loader::load_type_directory`, which calls `DocMeta::parse`. This expects YAML frontmatter with fields `title`, `type`, `status`, `author`, `date`, `tags`, `related`. But `setup.rs::write_cache_file` writes frontmatter with `number`, `title`, `state`, `updated_at` and names files `{number}.md` instead of `{id}-{slug}.md`.

This means `lazyspec setup` produces cache files that `lazyspec list`, `lazyspec show`, and `lazyspec search` cannot parse. The iteration's Task 7 assumes cache files have standard frontmatter, which is only true for files written by `store_dispatch.rs`.

Recommendation: this is a pre-existing bug that must be fixed before or as part of this iteration. The `setup.rs` cache writer should produce the same format as `store_dispatch.rs::write_cache_file`. This is not called out in ITERATION-124's task breakdown.

### Finding 3: No `cache.lock` or TTL mechanism exists in codebase

Severity: info
Location: n/a

ITERATION-124 Tasks 1-4 propose `cache.lock` with per-document timestamps and TTL-based freshness checking. None of this exists yet. The iteration is correctly identifying greenfield work. No conflict with existing code.

### Finding 4: Task 4 proposes `src/engine/github_read.rs` but read-through logic has no integration point

Severity: medium
Location: `src/engine/store.rs:40-65`

Task 4 proposes a cache-aware read-through path in a new `github_read.rs` module. However, `Store::load_with_fs` loads all documents eagerly at startup by scanning directories. There is no per-document lazy read path. The read-through logic (check freshness, fetch if stale, return content) has no callsite in the current architecture.

For single-document reads (`show`, `context`), the store is already loaded. Introducing read-through requires either: (a) making `Store::get` aware of staleness and able to trigger fetches, or (b) adding a pre-read hook before `Store::load` that refreshes stale entries. Neither is addressed in the iteration.

Recommendation: Task 4 should specify how the read-through integrates with the existing eager-load architecture. The simplest approach is to refresh stale documents _before_ `Store::load` runs, keeping the store itself stateless with respect to freshness.

### Finding 5: Task 5 (ETag/conditional refresh) has no `gh` CLI support path

Severity: medium
Location: `src/engine/gh.rs`

Task 5 proposes conditional refresh using ETag headers via `gh api` with custom headers. The `GhClient` trait has no method for raw API calls with custom headers. `GhCli::run_gh_checked` accepts args but doesn't expose header control for `gh issue view`. The task acknowledges this ("fall back to unconditional fetch"), but the entire task may reduce to a no-op if the `gh` CLI layer can't support it.

Recommendation: defer Task 5 or fold the ETag storage into Task 1 (store the field in `cache.lock`) without implementing conditional fetch. The actual conditional request can be added when a native HTTP client replaces `gh`.

### Finding 6: Task 6 proposes `src/cli/fetch.rs` but `setup.rs` already does most of the same work

Severity: medium
Location: `src/cli/setup.rs:16-75`, ITERATION-124 Task 6

`setup.rs::run` already fetches all issues for each github-issues type, writes cache files, and builds the issue map. Task 6 proposes a `lazyspec fetch` command that does the same thing plus cleanup of removed issues. The overlap is substantial.

Recommendation: extract the shared fetch-and-cache logic into a reusable function (in the proposed `IssueCache` module or a shared `fetch` module), then have both `setup` and `fetch` call it. Task 6 should call out this refactoring explicitly rather than building fetch from scratch.

### Finding 7: AC "Offline degradation with stale cache" has thin coverage

Severity: medium
Location: STORY-096 AC: "Offline degradation with stale cache"

The iteration's Test 6 and Test 7 cover offline degradation via mocked `GhClient` failures. However, Task 4's read-through logic (the only place degradation would occur) has no integration point in the store (Finding 4). If the read-through module exists but is never called, the AC is implemented but not exercised in the actual read path.

Recommendation: ensure Task 7 (store integration) wires in the degradation path, not just the happy path.

### Finding 8: `IssueMap` has two definitions

Severity: medium
Location: `src/engine/issue_map.rs`, `src/cli/setup.rs:10-14`

`setup.rs` defines its own `IssueMapEntry` struct and writes `issue-map.json` using raw `HashMap<String, IssueMapEntry>`. Meanwhile `src/engine/issue_map.rs` has a proper `IssueMap` struct with `load`/`save`/`insert`/`get`/`remove` methods. `setup.rs` doesn't use the `issue_map` module at all.

The iteration's tasks assume a single `IssueMap` implementation. The `setup.rs` duplicate will cause confusion.

Recommendation: migrate `setup.rs` to use `src/engine/issue_map::IssueMap`. The two `IssueMapEntry` definitions have identical fields, so this is straightforward.

### Finding 9: SOLID - `GithubIssuesStore` has too many responsibilities (SRP)

Severity: low
Location: `src/engine/store_dispatch.rs:100-317`

`GithubIssuesStore` handles: CRUD operations, issue body serialization, cache file writing, issue map management, optimistic lock checking, and status-to-open/closed mapping. This is a lot of responsibility for a single struct, particularly when the iteration proposes adding cache freshness and read-through on top.

Recommendation: the iteration's approach of putting cache logic in a separate `IssueCache` module is the right instinct. Ensure `GithubIssuesStore` delegates to `IssueCache` for all cache operations rather than continuing to call `write_cache_file` directly.

### Finding 10: SOLID - `GhClient` trait is not segregated (ISP)

Severity: low
Location: `src/engine/gh.rs:95-143`

`GhClient` has 9 methods. Every mock in the codebase (`MockGhClient` in `gh.rs`, `SetupMockGh` in `setup.rs`, test mocks in `store_dispatch.rs`) must implement all 9, even when a test only exercises 1-2 methods. The `SetupMockGh` has 5 `unimplemented!()` stubs.

This isn't blocking, but it makes tests brittle and noisy. The iteration would add more call sites, amplifying the problem.

Recommendation: consider splitting `GhClient` into smaller traits (e.g., `GhIssueReader`, `GhIssueWriter`, `GhAuth`) or using a builder/mock framework. Not urgent but worth noting for future cleanup.

### Finding 11: SOLID - Dependency inversion is well applied

Severity: info
Location: `src/engine/gh.rs`, `src/engine/fs.rs`, `src/engine/store_dispatch.rs`

The codebase already inverts dependencies through `GhClient` and `FileSystem` traits. `GithubIssuesStore<G: GhClient>` uses generics for compile-time dispatch. The iteration's proposed `GhClient` trait in Task 4 aligns with this existing pattern. No issues here.

### Finding 12: AC "Fetch command uses label filtering and pagination" - pagination not addressed

Severity: medium
Location: STORY-096 AC: "Fetch command uses label filtering and pagination"

Label filtering is handled by `GhCli::issue_list` which passes `--label`. But pagination is not implemented. `gh issue list` defaults to 30 results. The existing `GhCli::issue_list` doesn't pass `--limit` or handle pagination. Task 6 mentions `--limit 100` but doesn't address what happens when there are more than 100 issues.

Recommendation: Task 6 should either use `gh api` with Link header pagination, or pass `--limit 9999` as a pragmatic cap, or iterate with `--page`. The AC explicitly requires pagination handling.

## Summary

Two critical-path issues and several medium-severity gaps.

The most urgent problem is the cache format divergence between `setup.rs` and `store_dispatch.rs` (Findings 1-2). `setup.rs` produces cache files that `Store::load_with_fs` cannot parse, which means `lazyspec setup` followed by `lazyspec list` will fail for github-issues types. This is a pre-existing bug the iteration inherits but doesn't address.

The second structural issue is the read-through integration gap (Finding 4). The iteration proposes a read-through module but doesn't specify how it connects to the eager-loading `Store`. Without this, the TTL/freshness logic exists in isolation.

On SOLID, the design is generally sound. The `GhClient` trait is slightly fat (ISP), and `GithubIssuesStore` is accumulating responsibilities (SRP), but neither is blocking.

The iteration covers 7 of 8 STORY-096 ACs. Pagination (Finding 12) and offline degradation wiring (Finding 7) need stronger task-level coverage.
