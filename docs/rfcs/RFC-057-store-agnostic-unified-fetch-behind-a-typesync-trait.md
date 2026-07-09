---
title: "Store-agnostic unified fetch behind a TypeSync trait"
type: rfc
status: accepted
author: "jkaloger"
date: 2026-07-09
tags: []
related: []
---<!-- intent: propose a design and the decisions it forces, before code -->

## Summary

Fetch (remote-cache refresh) is implemented twice — once in the CLI (`src/cli/fetch.rs`), once in the TUI background poll (`src/tui/infra/event_loop.rs`). Each hardcodes a per-backend sequence of `filter(|t| t.store == …)` blocks, and the two have already drifted: the TUI poll never captures ClickUp status colours, never persists derived lifecycles, never refreshes git-ref types, and never injects GitHub project fields. Replace both with one engine-level orchestrator, `sync_all`, that dispatches each type to a per-backend syncer via an exhaustive `match StoreBackend`. Each syncer implements a `TypeSync` contract; the sidecar maps stay owned by the caller and are lent to `sync_all` through a borrowed `SyncContext`. Adding a backend or a fetch step then happens in one place, and both surfaces stay in lockstep by construction.

## Motivation

Problems, priority order:

1. **Silent drift causes user-visible bugs.** The TUI poll refreshes ClickUp task cache (`refresh_clickup_cache`, `event_loop.rs:215`) but never writes `status-colors.json` — only the CLI path does (`fetch.rs:233`). A TUI-only user never gets the derived status colours ITERATION-283/284 shipped — statuses render `Color::Reset`. The same class of gap recurs across the poll: it skips lifecycle→config persist, skips git-ref refresh entirely, and skips GitHub project-field injection (`inject_project_fields_into_cache`, `fetch.rs:169`) that the CLI runs after each github-issues type. Four distinct drifts, one root cause: every fetch step must be wired twice or it half-works.
2. **No single contract per backend.** Adding a store backend today means editing two orchestrations, each a bespoke sequence of store-filtered blocks. There is no one place that says "here is how a backend fetches".
3. **Divergent error/ownership posture.** CLI builds fresh maps per run and `?`-aborts; TUI reuses `issue_map`/`issue_cache` across polls inside `Arc<Mutex<GithubIssuesStore>>` and warns-and-continues. The shared behaviour (what to fetch, in what order, what to persist) is tangled with the per-caller behaviour (error handling, state lifetime, output).

Without this, the colour bug is patched in isolation and the next fetch feature reintroduces the same split.

## Goals

- One engine-level orchestration of "refresh these types' caches" that both CLI and TUI call. Behaviour (what is fetched, ordering, which sidecar artifacts are written) is identical across surfaces by construction — including the previously CLI-only project-field injection.
- A per-backend syncer contract such that a new store backend is added by: adding a `StoreBackend` variant (the compiler then flags the `match StoreBackend` in `sync_all` until the new arm is handled), and writing one syncer.
- Preserve the milestones-before-issues ordering constraint (issue→milestone relation resolution depends on it).
- Per-caller concerns stay with the caller: CLI keeps summary-print (or `--json`) + config lifecycle persist + a non-zero exit on any failure; TUI keeps warn-and-continue + cross-poll state reuse + the `CacheRefresh` event.
- No behaviour change for filesystem types (never fetched) and no change to the rendering layers.

## Non-goals

- No change to the renderers (TUI stays foreground status colour; ITERATION-284 output is correct).
- No new fetch capability beyond closing the drift — this is consolidation, not features. Colour capture, git-ref refresh, and project-field injection appearing in the TUI poll are *consequences* of unification, not separate features.
- Not a daemon or a generic job system (see project scope: lazyspec stays a simple doc tool). `sync_all` is a plain synchronous function the caller invokes.
- Config lifecycle persistence in the TUI poll stays out — a background poll must not rewrite tracked `.lazyspec.toml` unprompted. The orchestrator *returns* derived lifecycles in the outcome; only the CLI persists them.
- No cross-stage dependency guard. `sync_all` does not skip issue fetch when milestone fetch fails (see Risks): out of scope, recorded so it is not later read as a regression.
- `Filesystem` and `GithubProjects` types are never fetched by `sync_all` — filesystem docs have no remote to refresh, and project fields are pulled *within* `GhIssueSync` (not as a top-level backend). Both are explicit skip arms in the dispatch match, not omissions.

## Design

### `TypeSync` contract + borrowed `SyncContext` (engine)

New module `src/engine/sync.rs`. The sidecar maps are **not** owned by the syncers, and **not** copied into the context — the context *borrows* them from whoever owns them. This is the crux: the TUI's `issue_map` must stay inside `GithubIssuesStore` (the edit-push path `try_push_gh_edit`, `event_loop.rs:63`, reads that same field), so the poll lends `&mut store.issue_map` into a per-poll `SyncContext` under the existing mutex; the CLI lends `&mut` to run-local maps it drops each run. Borrowing (not owning) is what keeps the poll and the push path reading one map with no stale alias, and lets the two GitHub syncers both touch that one `IssueMap` without a shared-borrow conflict (each `sync` call reborrows `&mut ctx` sequentially).

```rust
struct GhMaps<'a>      { issue_map: &'a mut IssueMap }
struct ClickupMaps<'a> { task_map: &'a mut TaskMap, status_colors: &'a mut StatusColors }

struct SyncContext<'a> {
    gh: Option<GhMaps<'a>>,           // Some when any github backend is configured
    clickup: Option<ClickupMaps<'a>>, // Some when clickup-tasks is configured
}

struct SyncOutcome {
    type_name: String,
    fetched: usize, new: usize, removed: usize,
    warnings: Vec<String>,
    error: Option<String>,        // this type's fetch failed; the run continued (see Errors)
    lifecycle: Option<Lifecycle>, // Some for backends that derive it (ClickUp)
}

trait TypeSync {
    // Refresh one type's cache + this backend's sidecar maps (borrowed via `ctx`);
    // return the outcome. Never aborts and never returns Result: a per-type
    // failure is an Ok-shaped `SyncOutcome` with `error` set, so one bad type
    // cannot sink the run or the other types.
    fn sync(&mut self, ctx: &mut SyncContext, root: &Path, td: &TypeDef, cfg: &Config)
        -> SyncOutcome;
}
```

`TypeSync` is a **static contract**, not a `dyn` trait — there is no `Box<dyn TypeSync>`. It documents the shape every syncer shares (4 impls today, justifying the abstraction per principle 6) and is the seam tests drive with the existing fake clients.

Syncer structs, each holding that backend's injected I/O deps (never the maps):

- `GhMilestoneSync` / `GhIssueSync` — hold `GhCli`/`repo`/type rules. Wrap `milestone_cache::fetch_milestones` / `IssueCache::fetch_all`; mutate `ctx.gh`. **`GhIssueSync` additionally runs project-field injection** (the logic currently in CLI-only `inject_project_fields_into_cache`) after `fetch_all`, so both surfaces inject identically. Best-effort as today: a GraphQL failure becomes a warning on the outcome and the cached doc keeps its other fields.
- `GitRefSync` — holds `GitRefOps`, `remote`, cache-lock. Wraps `fetch_git_ref_type` (relocated to engine — see Interfaces).
- `ClickupSync` — holds `ClickupClient`, token. `sync` = `fetch_tasks` + `fetch_lifecycle_and_colors` + `status_colors.set_type`; mutates `ctx.clickup`; returns the derived lifecycle in the outcome. This is where the colour bug closes.

### Dispatch: `match StoreBackend` in `sync_all`

`StoreBackend` is already an exhaustive enum with six variants. `sync_all` dispatches each configured type to its syncer through a `match` on that enum — no `dyn`, no vtable, and the match is the one site the compiler forces you to update when a variant is added:

```rust
fn sync_all(
    root: &Path, config: &Config,
    ctx: &mut SyncContext,
    syncers: &mut Syncers,     // per-backend Option fields, not a slice (see below)
    filter: Option<&str>,      // single-type filter, as CLI --type
) -> Vec<SyncOutcome>

// conceptually, per type in fixed backend order:
match type_def.store {
    StoreBackend::GithubMilestones => dispatch to syncers.milestone,
    StoreBackend::GithubIssues     => dispatch to syncers.issue,
    StoreBackend::GitRef           => dispatch to syncers.git_ref,
    StoreBackend::ClickupTasks     => dispatch to syncers.clickup,
    StoreBackend::Filesystem       => skip (no remote),
    StoreBackend::GithubProjects   => skip (fetched within GhIssueSync, not top-level),
}
```

`sync_all` iterates configured types grouped by backend in a fixed order (`GithubMilestones`, then `GithubIssues`, then `GitRef`, then `ClickupTasks`) and collects the `SyncOutcome`s. **The ordering rule lives here, in one place.** A configured type whose backend has no syncer in `Syncers` yields an `error`-bearing outcome rather than a panic (see below).

`Syncers` is a struct of per-backend `Option<…>` fields (`milestone: Option<GhMilestoneSync>`, `issue: Option<GhIssueSync>`, `git_ref: Option<GitRefSync>`, `clickup: Option<ClickupSync>`) rather than a `&mut [Syncer]` slice: it makes "backend not configured" a typed `None` the dispatch reads directly, instead of a runtime scan of a slice that could silently lack a needed syncer.

### Errors

`sync_all` returns `Vec<SyncOutcome>` and never aborts. A per-type fetch failure — or a missing syncer for a configured backend — is recorded in that type's `SyncOutcome.error`; the run continues to the remaining types. There is no `Result` on `sync` or `sync_all`: every failure has exactly one home (`outcome.error`), so no error channel can be silently dropped. **Severity is a per-caller decision:**

- **CLI** inspects the outcomes: prints each `error`, and **exits non-zero if any outcome has an error** (preserving the script/`--json` failure signal). It still persists everything that succeeded.
- **TUI** maps every `error`/`warnings` into the `CacheRefresh` event and never aborts the poll.

This replaces the CLI's current `?`-abort-on-first-failure. **Recorded consequence:** the CLI now continues past a milestone-fetch failure into issue fetch, inheriting the same latent `targets`-relation-drop the TUI poll already has (issues resolved against a stale milestone map). Accepted here without a dependency guard (Non-goals); flagged so it is not later read as a new drift bug.

### Save (no `flush`)

Because the maps are borrowed and owned by the caller, the syncers have nothing to flush — the trait is just `sync`. **The caller saves after `sync_all` returns**, which is also where the per-caller persistence asymmetry lives:

- **CLI:** save its run-local `issue_map` + `task_map` + `status_colors`, then `persist_clickup_lifecycles` from the outcomes' `lifecycle` values.
- **TUI:** save `GithubIssuesStore.issue_map` (its owned field, mutated in place through the borrow) + its per-poll `task_map` + **`status_colors`** (the fix) — **no** lifecycle persist (a poll must not rewrite `.lazyspec.toml`).

Single save-at-end is consistent: `sync_all` runs every stage (no partial abort), so there is no mid-run save.

### Caller wiring

- **CLI** (`fetch::run`): load run-local maps, build a `SyncContext` borrowing them (populating `gh`/`clickup` only for configured backends) and a `Syncers` with real clients/tokens, preserving the "no fetchable types" message. Token-absent / repo-unresolvable stay hard errors raised while *building* the `Syncers`. Call `sync_all`, then print summaries (or `--json`), save the maps, persist lifecycles, and exit non-zero if any outcome has an error.
- **TUI poll**: on the poll thread, lock `shared_gh_store`, build a `SyncContext` borrowing `&mut store.issue_map` (plus per-poll clickup maps), build/reuse a `Syncers`, call `sync_all`, map outcomes into the `CacheRefresh` event, then save through the borrow. Because the borrow is of the store's own field, `try_push_gh_edit` and the `gh_issue_map_stale` reload keep reading the one authoritative map — no duplicate to drift. No lifecycle persist.

### Layering (DICTUM-003 / 004)

Contract + syncers + `SyncContext` + `sync_all` live in engine. Clients and tokens are injected by each caller (constructed in the CLI/TUI layer), so network/token I/O never enters `Store::load`. The existing seams (`ClickupClient`, `GhIssueReader`/`GhGraphql`, `GitRefOps`) are unchanged — syncers depend on those traits, so tests drive them with the existing fakes without `TypeSync` needing to be `dyn`. CLI and TUI depend only on engine, never on each other.

## Interfaces

- `@draft` `engine::sync` module: `TypeSync` static trait, `SyncContext<'a>` / `GhMaps<'a>` / `ClickupMaps<'a>` structs, `SyncOutcome` struct, `Syncers` struct, `sync_all` fn (`src/engine/sync.rs`).
- `@draft` syncer structs: `GhMilestoneSync`, `GhIssueSync` (incl. project-field injection), `GitRefSync`, `ClickupSync`.
- Retained, wrapped unchanged: `milestone_cache::fetch_milestones`, `IssueCache::fetch_all`, `clickup_cache::fetch_tasks`, `clickup_cache::fetch_lifecycle_and_colors`, the project-field logic (`store_dispatch::inject_project_fields_for_meta` / `write_cache_file`), `persist_clickup_lifecycles` (moves to engine if both callers need it; stays CLI-only otherwise — only the CLI persists lifecycles, so likely stays CLI-side).
- Relocated to engine: `fetch_git_ref_type` (today a private `fetch.rs` fn returning the CLI-private `TypeSummary`) moves into `GitRefSync` and returns `SyncOutcome`. The CLI-private `TypeSummary` is replaced by `SyncOutcome` throughout `fetch::run`.
- Removed: `event_loop::refresh_clickup_cache`, `fetch::inject_project_fields_into_cache` (its body moves into `GhIssueSync`), and the bespoke fetch loops in `fetch::run` and the poll thread.
- CLI surface: `lazyspec fetch [--type T] [--json]`. Behaviour changes, all recorded: (a) on a per-type failure it completes the other types and exits non-zero, rather than aborting on the first failure; (b) `--json` output gains an **optional `error` field** on any entry whose type failed — successful entries keep the exact `{type,fetched,new,removed}` shape, failed entries add `"error": "<msg>"`, and the process still exits non-zero when any `error` is present.

## Decisions (ADRs to emit)

- **ADR: fetch is orchestrated by an engine-level `sync_all` dispatching over `match StoreBackend`, not per-caller sequences and not a `dyn` trait.** Records why a static `match` beats a vtable (the backend set is closed at compile time — new backends like Linear are added in-repo, not loaded as plugins — so the exhaustive `match StoreBackend` in `sync_all` is the single site the compiler forces open when a variant is added, keeping the ordering constraint checked as backends grow), why `TypeSync` stays a non-`dyn` contract (4 impls justify it per principle 6; DICTUM-004 seams already exist at the client level), why `Filesystem`/`GithubProjects` are explicit skip arms, and why the orchestrator owns ordering.
- **ADR: sidecar maps are caller-owned and borrowed into `SyncContext`, not owned by it.** Records that the TUI's `issue_map` must remain the single field inside `GithubIssuesStore` so the poll and `try_push_gh_edit` share one authoritative map; borrowing (with a `SyncContext<'a>` lifetime) is what prevents a duplicated, drifting copy while still letting the CLI use throwaway run-local maps.
- **ADR: `sync_all` never aborts and never returns `Result`; per-type failure is folded into `SyncOutcome.error` and severity is a caller decision.** Records the CLI's shift from `?`-abort to continue-then-exit-non-zero, the `--json` `error`-field addition, and the deliberate absence of a milestone→issue dependency guard (both callers may fetch issues against a stale milestone map on milestone failure).
- **ADR: the background TUI poll does not persist derived lifecycles to config.** Records the deliberate asymmetry (poll must not mutate tracked config), so it is not read later as another drift bug.

## Stories

1. **Introduce the engine seam and migrate the CLI wholesale.** `TypeSync` contract + borrowed `SyncContext` (`GhMaps`/`ClickupMaps`) + `Syncers` + `sync_all` (with the never-abort error model, the `match StoreBackend` dispatch incl. Filesystem/GithubProjects skips, and milestones-before-issues ordering) + all four syncers (`GhMilestoneSync`, `GhIssueSync` **with project-field injection folded in**, `GitRefSync`, `ClickupSync`). Migrate `fetch::run` off its bespoke loops onto `sync_all` for every backend at once; relocate `fetch_git_ref_type` to engine; replace `TypeSummary` with `SyncOutcome`. Behaviour-preserving for the CLI except the recorded error-model + `--json` changes. Enum dispatch is exhaustive from birth — git-ref is included here, not deferred.
2. **Migrate the TUI poll onto `sync_all`.** Lock `shared_gh_store`, borrow its `issue_map` into a `SyncContext`, call `sync_all`, map outcomes into `CacheRefresh`, save through the borrow. Closes the colour-capture bug and adds git-ref refresh + project-field injection to the poll (free once they live in the syncers), keeps warn-and-continue + cross-poll state, no lifecycle persist. Supersedes the dropped ITERATION-285. Satisfies STORY-201 on the TUI surface.

Sequence: 1 → 2. (The original story 3 — git-ref behind the trait — dissolves into story 1, because the dispatch match should be exhaustive when created rather than edited twice.)

## Risks and tradeoffs

- **State lifetime mismatch (TUI reuse vs CLI fresh) — resolved by design.** The `SyncContext<'a>` borrows the caller's maps rather than owning them, so the TUI keeps its `issue_map` inside `GithubIssuesStore` (shared with the edit-push path) and the CLI drops run-local maps each run. The borrow is also what lets the two GitHub syncers touch one `IssueMap` without a shared-borrow conflict. Cost: `sync.rs` carries a lifetime parameter — mild, idiomatic.
- **TUI poll gains project-field GraphQL cost.** Folding injection into `GhIssueSync` means each poll now issues the per-item project-field GraphQL the CLI already does. Best-effort (warns on failure); accepted as the price of removing the drift. If poll latency becomes a problem, the injection is a bounded follow-up (e.g. TTL or diff-only), not a reason to keep the surfaces divergent.
- **Over-abstraction.** A contract for 4 backends risks premature generality. Guards: `TypeSync` has exactly the one method `sync_all` needs (no `flush`, no `Result`, no speculative hooks); dispatch is a static `match`, not `dyn`, so no plugin machinery is implied. Justified by four concrete impls today (principle 6).
- **Ordering regression.** Centralizing milestones-before-issues in `sync_all` is safer than two copies; the migration must carry a test asserting order (a milestone type and an issue type with a cross relation resolve correctly after `sync_all`).
- **Stale-milestone relation drop on milestone failure.** With no dependency guard, a milestone-fetch failure lets issue fetch proceed against a stale map, silently dropping `targets` relations — a latent bug the TUI already has, now shared by the CLI. Accepted (Non-goals); a future guard can skip dependent stages on upstream failure.
- **Bug stays live until story 2 lands.** Chosen over a throwaway minimal patch (dropped ITERATION-285) to avoid building the clickup fetch twice. Acceptable: colours already work via CLI `lazyspec fetch` in the interim.