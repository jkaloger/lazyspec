---
title: Stamp reviewed on a local status transition
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: STORY-274
- blocks: ITERATION-428
- related-to: STORY-276
reviewed: 31d96c41e3dc06d0fc103303fc29d5043d9599d1
---

## Objective

A local status transition on a filesystem-backed document writes `reviewed: <HEAD>` in the same frontmatter write, and never fails for want of a HEAD.

## Satisfies

STORY-274 AC1, AC3, AC6, and AC2's CLI, skill and TUI clauses. AC2's web clause is vacuous -- see Context. AC4 and AC5 need the github-issues path; next slice.

## Finding: `Store::update_status` does not exist

RFC-069 Decision 4 and STORY-274 §Notes both name `Store::update_status` as the one engine hook. There is no such method -- `grep -rn update_status src/` returns only unrelated test names and `AgentStatus` bookkeeping in `src/tui/agent.rs`.

The claim is right, the name is wrong. The seam is `engine::ops::update::run_with_config` (`src/engine/ops/update.rs:52`), and it is genuinely single:

- CLI `update --status`: `src/main.rs:317`. The /advance skill is this caller -- it shells `lazyspec update <id> --status <next>` (`skills/advance/SKILL.md:17`, `:51`), so it is not a caller to hook separately.
- TUI status change: `confirm_status_change`, `src/tui/state/app.rs:3288`.
- **No web status form exists.** `src/web/server.rs:78`-`:84` routes six `get` handlers and no `post`. Same shape as the missing web validation view ITERATION-426 recorded. Do not build one. The clause is satisfied vacuously and stays satisfied the day a form lands, because it will call this function.
- Nothing bypasses it. `fs_ops::update_document_with_type` has exactly two callers: `ops/update.rs:106` and `FilesystemStore::update` (`store_dispatch.rs:521`), which `ops::update` never reaches -- `:83` sends `Filesystem` down the `fs_ops` path before the registry. `fetch`/`sync` write board status into the cache through `sync::reconcile_project_fields_into_cache` (`src/engine/sync.rs:568`), never through `ops::update`; that is AC4's mechanism and the next slice proves it.

## Context

- Story + ACs: STORY-274. Stamping rule: RFC-069 §Design "Stamping". `pin <id>` (STORY-270) stays the way to stamp without a transition and is not touched.
- The sha is `git.head(store.governs_root())`, as `pin` reads it and for its reason (`src/cli/pin.rs:161`-`:170`): under a docs-repo split the anchor must name the code repo's HEAD, because that is the repo `diff_stat` runs in.
- **Best-effort, unlike `pin`.** `pin` bails on an unreadable HEAD and leaves the file byte-identical (`pin.rs:170`, tested at `:569`). AC6 demands the opposite here: `.ok()`, no stamp, transition proceeds. A repo with no commits is the case.
- `reviewed` is a `DocMeta` field (`src/engine/document.rs:329`), not a declared attribute. Passed to `update_document_with_type` as an ordinary key it lands in `attr_updates` and `apply_attrs` rejects it for every type that does not declare it. So it joins `RESERVED_UPDATE_KEYS` (`fs_ops.rs:293`) and takes `assignee`'s insert-when-missing branch (`:356`), not the reserved keys' replace-only branch (`:370`) -- no document on disk carries a `reviewed:` line to replace.
- **One write, not two.** The stamp rides the same `updates` slice as the status, so it is one frontmatter rewrite here and (next slice) one `issue_edit`. Do not add a second `rewrite_frontmatter` pass after the dispatch; `pin.rs:110` `stamp_reviewed` is the shape to *not* copy.
- **Gate on the backend, permanently.** Only filesystem and github-issues documents can hold a `reviewed`: the clickup, git-ref and milestone cache builders hardcode `reviewed: None` (`clickup_cache.rs:274`, `git_ref_store.rs:172`, `milestone_cache.rs:56`). This slice appends the stamp for `StoreBackend::Filesystem` only. The gate is not scaffolding -- it stays for those three after the next slice adds github-issues.
- Threading git: `run` and `run_with_config` gain `git: &dyn GitRefOps`. Two production call sites pass `&GitCli`; about sixteen test sites across `tests/integration/` pass a mock. `tests/` already builds with `test-support` (`Cargo.toml:86`) and `cli_why_test.rs` already uses `MockGitRefClient`.

## Tasks

1. Test-first, `fs_ops`: `("reviewed", sha)` on a document with no `reviewed:` line inserts one; a second call replaces it; a type declaring no attributes does not error.
2. Add `reviewed` to `RESERVED_UPDATE_KEYS` and to the insert-when-missing branch. Add it to `cli::update::RESERVED_ATTR_KEYS` (`src/cli/update.rs:5`) so `--attr reviewed=` is refused with the message the other reserved fields get.
3. Thread `git: &dyn GitRefOps` through `ops::update::run` and `run_with_config`. `src/main.rs:317` and `src/tui/state/app.rs:3288` pass `&GitCli`; sweep the test sites onto `MockGitRefClient::new()`. No `GitCli` in a test.
4. Test-first, AC1: `run_with_config` with a `status` update over a filesystem type stamps `FAKE_HEAD`, and the mock's `call_log()` holds exactly one `head:` entry, against `[governs] root`. `pin.rs:548` is the assertion to mirror.
5. Test-first, AC6: `with_head_result(Err(...))` -- the status still moves, exit is `Ok`, and `reviewed` is absent. Repeat with a document that already carries a `reviewed`: it is byte-identical afterwards.
6. Test-first: an update carrying no `status` (title, body, assignee, `--attr`) records zero `head:` calls and stamps nothing.
7. Test-first, AC2's TUI clause: `confirm_status_change` (`app.rs:3271`) stamps, with no occurrence of `reviewed` anywhere under `src/tui/`.
8. Test-first, AC3: a `drift` type with `governs` and a `reviewed` older than HEAD bands `Stale`; after `run_with_config` moves its status, `compute` logs the range `FAKE_HEAD..FAKE_HEAD` -- anchor equals HEAD, which real git answers empty. Assert the logged range, not the mock's fixed `Drift`. `staleness.rs:189` `store_with` is the fixture.
9. Implement the stamp in `run_with_config`, gated on `StoreBackend::Filesystem`.
10. README: the `update` reference and §Staleness -- `--status` stamps `reviewed` with HEAD, an unreadable HEAD skips the stamp silently, `pin` still stamps without a transition.

## Out of scope

- github-issues, `status_authority`, AC4 and AC5. Next slice, which also lifts the gate for that one backend.
- clickup, git-ref and milestone backends. They cannot hold a `reviewed` at all; see Context. Not deferred -- excluded.
- Building a web status form. Not this story's, same call as ITERATION-426's on the web validation view.
- `pin` (STORY-270). Unchanged.
- **Caching, and the cost this slice makes real.** Nothing in this repo carries a `reviewed` today, which is the only reason `StaleRule` on the `validate_full` path costs nothing: `compute` shells out only when `reviewed.is_some()`. This slice starts populating anchors on every local transition, so roughly one `git cat-file -p` per document plus one `git diff --numstat` per pinned document becomes real, synchronously, per `status --json` and per TUI validation refresh. STORY-276 owns it. Do not grow a cache here.
- TUI and web band badges (STORY-275).

## Principles/conventions

`cargo run --quiet -- convention`. Testing §Desiderata "Fast: no spawning processes" is why the git param exists rather than a `GitCli` inside the function. Principle 3: the stamp is engine work, and the TUI must inherit it without naming it.

## Verification

On a scratch repo with commits: `cargo run --quiet -- update <id> --status <next> --json | jq .reviewed` equals `git rev-parse HEAD`. On a fresh `git init` with no commits, the same command exits `0` and `.reviewed` is `null`.
