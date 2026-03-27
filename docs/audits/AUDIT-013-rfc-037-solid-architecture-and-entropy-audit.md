---
title: RFC-037 SOLID, architecture, and entropy audit
type: audit
status: accepted
author: jkaloger
date: 2026-03-28
tags: []
related:
- related-to: RFC-037
---



## Scope

Code quality audit of the RFC-037 (GitHub Issues Document Store) implementation across ~3,500 lines of new/modified Rust code. Audit type: architectural health check.

Files in scope:
- `src/engine/gh.rs` (721 lines)
- `src/engine/issue_body.rs` (501 lines)
- `src/engine/issue_map.rs` (129 lines)
- `src/engine/store_dispatch.rs` (1,170 lines)
- `src/engine/github.rs` (104 lines)
- `src/engine/config.rs` (StoreBackend, GithubConfig additions)
- `src/cli/init.rs`, `src/cli/setup.rs`, `src/cli/validate.rs`
- `src/tui/infra/event_loop.rs` (GitHub polling and push-on-edit)

## Criteria

Three lenses, applied to every file in scope:

1. SOLID principles (single responsibility, open-closed, Liskov substitution, interface segregation, dependency inversion)
2. Architectural soundness (layering, coupling, cohesion, error handling consistency, testability)
3. System entropy (duplicated logic, divergent representations of the same concept, dead code, naming drift, accumulating tech debt)

## Findings

### Finding 1: Duplicate `IssueMapEntry` struct

Severity: high
Location: `src/cli/setup.rs:10-14` vs `src/engine/issue_map.rs:9-13`
Description: `setup.rs` defines its own `IssueMapEntry` struct with identical fields to the one in `issue_map.rs`. The setup command then serializes a raw `HashMap<String, IssueMapEntry>` directly, bypassing the `IssueMap` abstraction entirely. This means two independent serialization paths exist for the same data format, and any schema change to the issue map must be applied in two places.
Recommendation: Delete the `IssueMapEntry` in `setup.rs`. Use `IssueMap` from `issue_map.rs` directly, calling `insert()` and `save()` as `store_dispatch.rs` already does.

### Finding 2: Duplicate `extract_doc_id` functions

Severity: high
Location: `src/cli/setup.rs:113-132` vs `src/tui/infra/event_loop.rs:29-47`
Description: `extract_doc_id` (setup) and `extract_doc_id_from_title` (event_loop) implement the same logic with identical structure: try title prefix, then scan whitespace-separated words. Neither calls the other. A bug fix or format change in one will not propagate to the other.
Recommendation: Extract into a shared function in `src/engine/issue_body.rs` or a new `src/engine/issue_id.rs` module. Both call sites import it.

### Finding 3: Duplicate `resolve_repo` functions

Severity: medium
Location: `src/cli/init.rs:68-75` vs `src/cli/setup.rs:87-97`
Description: Both `init.rs` and `setup.rs` define a `resolve_repo` function that checks `config.documents.github.repo` then falls back to `infer_github_repo()`. The only difference is the return type (`Option<String>` vs `Result<String>`). The `event_loop.rs` TUI code also resolves the repo inline via `gh_config.repo.as_ref()`. Three places, same logic, slightly different error handling.
Recommendation: Consolidate into a single `resolve_repo(config, root) -> Result<String>` in `src/engine/github.rs` (which already owns `infer_github_repo`). Callers that want `Option` can use `.ok()`.

### Finding 4: Duplicate `github_issues_types` filter functions

Severity: medium
Location: `src/cli/init.rs:113-121` vs `src/cli/setup.rs:77-85`
Description: Both files define an identical `github_issues_types(config) -> Vec<&str>` function that filters `config.documents.types` for `StoreBackend::GithubIssues`. The validate module has a similar `has_github_issues_types` predicate, and the event loop checks the same condition inline.
Recommendation: Add a method on `Config` or `DocumentConfig` (e.g. `config.documents.github_issues_types()`) and a convenience `config.documents.has_github_issues_types()`. Remove all local copies.

### Finding 5: Duplicate `make_type` test helpers

Severity: low
Location: `src/cli/init.rs`, `src/cli/setup.rs`, `src/cli/validate.rs`, `src/engine/store_dispatch.rs` (all in `#[cfg(test)]` modules)
Description: Four test modules define near-identical `make_type(name, store) -> TypeDef` helper functions. Test entropy: when `TypeDef` gains a new field, all four must be updated (and if one is missed, it silently gets the default).
Recommendation: Add a `TypeDef::test_fixture(name, store)` method behind `#[cfg(test)]` in `config.rs`, or a `test_helpers` module. Tests import from one place.

### Finding 6: Two divergent cache file formats

Severity: high
Location: `src/cli/setup.rs:99-111` vs `src/engine/store_dispatch.rs:320-359`
Description: `setup.rs::write_cache_file` writes a format with `number`, `title`, `state`, `updated_at` fields in the frontmatter (raw issue metadata). `store_dispatch.rs::write_cache_file` writes a format with `title`, `type`, `status`, `author`, `date`, `tags`, `related` (document metadata). These are two completely different YAML schemas for files in the same `.lazyspec/cache/` directory. A cache file written by `setup` is not parseable by the code that reads cache files written by `store_dispatch`, and vice versa.
Recommendation: Settle on one canonical cache format. The `store_dispatch` format (document metadata) is the correct one since the cache is consumed by the `Store` parser. Rewrite `setup.rs::write_cache_file` to deserialize the issue body via `issue_body::deserialize`, then call `store_dispatch::write_cache_file`. This also removes the raw YAML formatting in `setup.rs`.

### Finding 7: `GithubIssuesStore` uses `RefCell<IssueMap>` for interior mutability

Severity: medium
Location: `src/engine/store_dispatch.rs:105`
Description: `GithubIssuesStore` wraps `IssueMap` in `RefCell` to allow mutation through the `&self` receiver of the `DocumentStore` trait. This is a runtime borrow check that panics on double-borrow. The `create` method does `borrow_mut()` then `drop(map)` manually to avoid overlapping borrows, which is fragile. If the trait's `&self` constraint is the cause, the trait itself may be too restrictive.
Recommendation: Consider whether `DocumentStore` methods should take `&mut self`. If shared ownership is genuinely needed (e.g. behind `Arc` in the TUI), use `Mutex` instead of `RefCell` for panic-safety. If `&mut self` is viable, remove the `RefCell` entirely.

### Finding 8: `DocumentStore` trait violates interface segregation

Severity: medium
Location: `src/engine/store_dispatch.rs:21-42`
Description: The `DocumentStore` trait bundles `create`, `update`, and `delete` into a single interface. The `dispatch_for_type` function returns `&dyn DocumentStore`, which means callers always get all three operations even when they only need one. More importantly, `FilesystemStore::create` calls back into `crate::cli::create::run`, coupling the engine layer to the CLI layer (see Finding 9).
Recommendation: This is acceptable at the current scale. If the trait grows further (e.g. `fetch`, `list`, `search`), split into reader/writer traits, mirroring the `GhIssueReader`/`GhIssueWriter` split that already exists in `gh.rs`.

### Finding 9: `FilesystemStore` creates a circular dependency with the CLI layer

Severity: high
Location: `src/engine/store_dispatch.rs:57-63`, `src/engine/store_dispatch.rs:86-88`, `src/engine/store_dispatch.rs:93-97`
Description: `FilesystemStore::create` calls `crate::cli::create::run`. `FilesystemStore::update` and `delete` call `crate::cli::update::run` and `crate::cli::delete::run`. The engine layer (`src/engine/`) depends on the CLI layer (`src/cli/`), inverting the expected dependency direction. The CLI should depend on the engine, not the reverse. This makes the engine untestable without the CLI, and means `store_dispatch` cannot be used in contexts that don't have the CLI (e.g. a library crate, a daemon, or WASM).
Recommendation: Extract the core document creation/update/delete logic from the CLI commands into engine-level functions. `FilesystemStore` calls those engine functions. The CLI commands become thin wrappers that parse args, call the engine, and format output.

### Finding 10: `write_cache_file` in `store_dispatch.rs` hand-formats YAML

Severity: medium
Location: `src/engine/store_dispatch.rs:320-359`
Description: The function manually constructs YAML frontmatter via `format!()` with hand-built strings for tags (`[{}]`) and related (`\n- {}: {}`). This is brittle: a tag containing a quote or special YAML character will produce invalid YAML. The existing `DocMeta` already has a serialization path through the document parser.
Recommendation: Use the existing frontmatter serializer (or `serde_yaml`) to produce the YAML block. If no serializer exists for the cache format, create one that handles escaping correctly.

### Finding 11: `GhCli` trait split is well-designed but `GhAuth` is underused

Severity: info
Location: `src/engine/gh.rs:95-148`
Description: The `GhIssueReader`, `GhIssueWriter`, and `GhAuth` trait split is the strongest architectural decision in the RFC-037 code. It enables mock-based testing throughout. The `GhAuth` trait is only consumed in `setup.rs` and `validate.rs`, which is appropriate for its scope.
Recommendation: No action needed. This pattern should be documented as the canonical approach for external service integration.

### Finding 12: `GhError` enum has a single variant

Severity: low
Location: `src/engine/gh.rs:35-38`
Description: `GhError` only has `NotInstalled`. Auth failures, rate limits, and API errors are all reported as plain `anyhow` strings. This means callers can only pattern-match on "not installed" vs "everything else". The `init.rs` code already downcasts to `GhError` to check for `NotInstalled`, which is a code smell when the rest of the error space is untyped.
Recommendation: Expand `GhError` to cover at least `NotInstalled`, `AuthFailed`, `RateLimited`, and `ApiError(status, message)`. This enables callers to handle rate limits (retry with backoff) and auth failures (prompt re-auth) without string matching.

### Finding 13: `extract_type_and_tags` has a hardcoded known-types list

Severity: medium
Location: `src/engine/issue_body.rs:116-140`
Description: The function hardcodes `[RFC, STORY, ITERATION, ADR, SPEC]` as known doc types. If a user adds a custom type in `.lazyspec.toml` with `store = "github-issues"`, it won't be recognized by the deserializer. The `lazyspec:custom-type` label will be silently dropped and the document defaults to `spec`.
Recommendation: Pass the configured type names into the deserialization context (alongside `IssueContext`), or derive known types from the `Config`. This makes the deserializer respect the user's configuration.

### Finding 14: `issue_body::serialize` doesn't round-trip all statuses correctly

Severity: medium
Location: `src/engine/issue_body.rs:90-92`
Description: `is_non_lifecycle_status` treats `Draft` and `Complete` as lifecycle (omitted from frontmatter). But the RFC design specifies that `review`, `accepted`, and `in-progress` are _also_ lifecycle statuses that map to "open". Currently these are written to frontmatter and round-trip correctly, but the naming `is_non_lifecycle_status` is misleading because it returns `true` for `review` and `accepted`, which are lifecycle statuses that happen to need frontmatter storage. The real distinction is "can this status be reconstructed from open/closed alone?" (only `draft` and `complete` can).
Recommendation: Rename to `needs_frontmatter_status` or `is_ambiguous_without_frontmatter`. The current logic is correct; the name is wrong.

### Finding 15: TUI GitHub push is synchronous and blocks the render loop

Severity: high
Location: `src/tui/infra/event_loop.rs:70-107`
Description: `try_push_gh_edit` is called synchronously in the event loop after the editor closes. It creates a new `GhCli`, loads the `IssueMap`, and makes API calls to GitHub. On a slow connection or rate-limited API, this blocks the entire TUI. The user sees a frozen screen with no feedback.
Recommendation: Move the push to a background thread (like the existing `CacheRefresh` pattern). Show a "pushing..." indicator in the status bar. On completion, send an `AppEvent` variant with the result.

### Finding 16: `GithubIssuesStore` is reconstructed from scratch on every TUI edit

Severity: medium
Location: `src/tui/infra/event_loop.rs:89-102`
Description: Every call to `try_push_gh_edit` creates a new `GhCli`, loads `IssueMap` from disk, and constructs a fresh `GithubIssuesStore`. The `IssueMap` is loaded from `.lazyspec/issue-map.json` each time. If the TUI already has a `GithubIssuesStore` in memory (for the polling loop), there are now two independent copies of the issue map that can diverge.
Recommendation: Share a single `GithubIssuesStore` instance within the TUI event loop. Pass it to both the polling code and the edit-push code.

### Finding 17: `MockGhClient` is duplicated across three test modules

Severity: low
Location: `src/engine/gh.rs:481-590`, `src/engine/store_dispatch.rs:417-539`, `src/cli/setup.rs:278-302`
Description: Three test modules define their own mock implementations of `GhIssueReader`/`GhIssueWriter`. Each has different capabilities (some record calls, some return canned data). This is standard for early development but will become a maintenance burden as the trait surface grows.
Recommendation: Create a shared `MockGhClient` in a `#[cfg(test)]` module under `src/engine/gh.rs` (or a dedicated `testutil` module). Builders or closures can customize per-test behavior. The `store_dispatch` mock that records calls is the most capable starting point.

### Finding 18: No `Display` impl for `StoreBackend`

Severity: low
Location: `src/engine/config.rs:77-84`
Description: Error messages and debug output format `StoreBackend` via `Debug` or hardcode strings like `"github-issues"`. A `Display` impl would give consistent, user-facing formatting.
Recommendation: Add `impl Display for StoreBackend` that matches the serde rename (`"filesystem"`, `"github-issues"`).

## Summary

The RFC-037 implementation is architecturally sound in its broad strokes. The trait-based abstraction for the `gh` CLI (`GhIssueReader`/`GhIssueWriter`/`GhAuth`) is the standout design decision, enabling thorough mock-based testing without touching the network. The `issue_body` serialize/deserialize module is well-tested with round-trip fidelity checks.

The primary concern is entropy accumulation. Six instances of duplicated logic (findings 1-5, 17) have already appeared across the ~3,500 lines of new code. The most architecturally damaging are the two divergent cache file formats (finding 6) and the circular engine-to-CLI dependency (finding 9). If left unaddressed, these will compound as the remaining stories (STORY-096 cache/fetch, STORY-099 cross-backend) add more code that interacts with these same surfaces.

Prioritised action areas:

1. Cache format convergence (finding 6) and `setup.rs` deduplication (findings 1-2) should be addressed before STORY-096 lands, since that story adds more cache-related code.
2. The engine-to-CLI inversion (finding 9) is the deepest structural issue but also the most disruptive to fix. It may warrant its own iteration.
3. The TUI synchronous push (finding 15) is the most user-visible problem and is a straightforward background-thread fix.
