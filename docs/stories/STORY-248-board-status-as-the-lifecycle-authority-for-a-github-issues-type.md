---
title: Board Status as the lifecycle authority for a github-issues type
type: story
status: complete
author: jkaloger
date: 2026-08-04
tags: []
related:
- implements: RFC-050
---## Context

A team whose day-to-day board is GitHub Projects v2 carries two statuses for the same work item, and lazyspec only sees the weaker one. The board column (`Ready To Start` / `In Progress` / `Review` / `Done`) is where the team tracks state; lazyspec shows `open` / `closed`, because `StoreBackend::canonical_lifecycle()` hands every `github-issues` type that pair (STORY-224). So `list`, the TUI status DAG, `advance` gates, and any skill reasoning about "what is in progress" cannot answer from the board, and `update --status` has nowhere useful to go.

RFC-050 shipped the parts and stopped short on purpose: per-board field values land as `PROJECT-n.<field>` attributes (STORY-162), and "lifecycle inheritance from a board's Status field" is an explicit non-goal, because a doc can be on many boards and picking a status-authority board was unresolved. This slice resolves it by letting a type nominate one board, on the type rather than globally, since `story` and `bug` may sit on different boards:

    [[types]]
    name = "story"
    store = "github-issues"
    status_authority = "PROJECT-7"

The value is that the board a team already moves cards on becomes the lifecycle lazyspec reports and writes, so there is one status instead of two.

The implementation follows the ClickUp precedent rather than inventing a second mechanism. `derive_lifecycle` (`src/engine/clickup_cache.rs:168`) turns a remote's own status set into a `Lifecycle`, and `persist_clickup_lifecycles` (`src/cli/fetch.rs:276`) writes it into `.lazyspec.toml` at fetch, so `TypeDef::effective_lifecycle()` (`src/engine/config.rs:1325`) returns it through its existing declared-lifecycle branch. STORY-224 rejected persist-on-sync for github because github status is *derived from* the lifecycle at cache-write time, which would show `draft` until a second fetch. That objection does not apply here: under a status authority the doc's status is the Status cell's raw string, exactly like a ClickUp task's. Persisting therefore leaves all 28 `effective_lifecycle()` call sites untouched.

Two decisions fall out of that choice and are recorded as AC:

- **A persisted lifecycle is indistinguishable from a hand-declared one.** Once fetch writes the board's columns into `lifecycle`, no runtime check can tell them apart, so "declared lifecycle wins" cannot be ranked as a precedence. Setting both `status_authority` and a non-empty `lifecycle` is instead a validate error — the conflict is made unrepresentable rather than ordered. STORY-224 AC3 stays intact for every type that sets no `status_authority`.
- **Option names are lowercased into states, and the option id is resolved case-insensitively.** `Status::new` and its `Deserialize` both lowercase unconditionally (`src/engine/document.rs:98`, `:119`), so a doc's status can never hold `In Progress` — it is always `in progress`. And `accepts_status` compares a lowercased `Status` against the raw declared states (`src/engine/config.rs:1315`), so persisting `In Progress` as a state would make every board status fail lifecycle membership. States are therefore the option names lowercased, matching what ClickUp's `derive_lifecycle` already does for the same reason. The board's original casing stays where it is needed: the write path matches the requested status against the snapshot's option names case-insensitively to recover the `singleSelectOptionId`. Preserving display casing end-to-end would mean changing the `Status` newtype, which is ADR-023's core and reaches every store plus the lowercased `status_colors` keys — disproportionate to this slice.

Issue open/closed is deliberately left uncoupled: teams express "moving to Done closes the issue" as Projects automation on the board itself, and duplicating that rule in lazyspec would fight it.

## Acceptance Criteria

- **Given** a `github-issues` type with `status_authority = "PROJECT-7"` and no declared `lifecycle`, and PROJECT-7 bound to a board whose `Status` single-select carries `Ready To Start` / `In Progress` / `Review` / `Done`
  **When** fetch runs
  **Then** the type's effective lifecycle states are exactly those four option names lowercased — `ready to start`, `in progress`, `review`, `done` — in board order, with an empty edge set (unconstrained, so any column moves to any other), persisted into `.lazyspec.toml` in place the way `persist_clickup_lifecycles` does — rewriting only when the derived lifecycle changed — and `config --json`, `list`, and the TUI status DAG all report them.

- **Given** an issue-doc of that type that is an item of the authority board with its `Status` cell set to `In Progress`
  **When** it syncs
  **Then** its status is `in progress`, read from the Status cell rather than from the issue's open/closed state.

- **Given** an issue-doc that is an item of the authority board with an empty `Status` cell
  **When** it syncs
  **Then** its status is left unset, no value is written to the board, and a fetch warning naming the document is emitted through the existing fetch warning channel — an empty cell is a real signal that triage has not happened, so it is reported rather than fabricated.

- **Given** an issue-doc of the type whose issue is not an item of the authority board
  **When** fetch runs
  **Then** lazyspec adds it to the board via `addProjectV2ItemById` (STORY-161's mutation), and the doc — its `Status` cell now empty — resolves to the unset-plus-warning state above; it never falls back to `open`/`closed`, so the type never reports two disjoint lifecycles at once.

- **Given** a synced doc at `ready to start` on the authority board
  **When** `lazyspec update <id> --status "In Progress"` runs (or the equivalent `"in progress"`, since `Status` lowercases either)
  **Then** the field id and the `singleSelectOptionId` are resolved from the cached `.lazyspec/cache/gh-schema.json` snapshot by matching the requested status against the board's option names case-insensitively, `updateProjectV2ItemFieldValue` is called with a `value` object carrying exactly that one key (STORY-162's write path), and the doc's cached status becomes `in progress`.

- **Given** `update --status` naming a value that matches no option on the authority board's `Status` field under case-insensitive comparison
  **When** the update runs
  **Then** it is rejected offline against the cached snapshot, naming the valid options, before any mutation is attempted — so validation and the TUI status picker work with no network.

- **Given** a doc moved to any column, including the last one in board order
  **When** the status write completes
  **Then** the GitHub issue's `open`/`closed` state is left untouched, in both directions — no close on reaching a terminal column and no reopen on leaving one. Coupling is the board's own Projects automation to express.

- **Given** a type that sets both `status_authority` and a non-empty `lifecycle`
  **When** `validate` runs
  **Then** it reports an error naming both keys and the type, because after the first persist the two are indistinguishable.

- **Given** a doc that is a member of the authority board and also of another board that has its own `Status` field
  **When** it syncs
  **Then** only the authority board's `Status` drives lifecycle; the other board's stays the plain `PROJECT-n.Status` attribute from STORY-162, and the asymmetry is stated in the config reference docs.

- **Given** a `github-issues` or `github-milestones` type that sets no `status_authority`
  **When** it syncs
  **Then** the canonical `open`/`closed` lifecycle applies exactly as today (STORY-224 AC1, AC2, AC3, AC6 intact), and `filesystem`, `git-ref`, and `clickup-tasks` types are unaffected.

- **Given** any operation in this slice
  **When** it serializes its result
  **Then** the result is available via `--json`, including the fetch warning for an unset Status cell.

## Scope

### In Scope

- A `status_authority: Option<String>` key on `[[types]]`, naming a `github-projects` document id, with config schema (RFC-058) and README/config-reference coverage including the multiple-boards asymmetry.
- Deriving a `Lifecycle` from the authority board's `Status` single-select options — board order, option names lowercased, empty edges — and persisting it into `.lazyspec.toml` at fetch, reusing the `derive_lifecycle` + `persist_clickup_lifecycles` shape.
- Case-insensitive resolution of a requested status back to the board's `singleSelectOptionId` on the write path.
- Sync setting a member doc's status from its `Status` cell rather than from issue state.
- Unset status plus a fetch warning for a member with an empty `Status` cell.
- Adding non-member docs of the type to the authority board on fetch via `addProjectV2ItemById`.
- `update --status` write-through via `updateProjectV2ItemFieldValue` with a single `singleSelectOptionId` key, ids resolved from the cached schema snapshot.
- Offline rejection of a status that is not an option on the authority board.
- A `validate` error when a type sets both `status_authority` and a non-empty `lifecycle`.
- Fakes at the `GhGraphql` seam and TDD coverage for derive, persist, cell-to-status, empty cell, auto-add, write-through, offline rejection, and the validate conflict.
- CLI, TUI, and web view all reading the lifecycle through `effective_lifecycle()`, so all three pick up board columns with no per-surface change.

### Out of Scope

- Choosing an authority board automatically — the nomination is explicit config.
- Authoring, creating, or reordering board columns from lazyspec (RFC-050 non-goal).
- Coupling issue `open`/`closed` to the terminal column, in either direction. Teams express this as Projects automation on the board.
- A `github-projects` canonical lifecycle for project documents themselves (deferred in STORY-224).
- Milestone and ClickUp lifecycle behaviour, both unchanged.
- Non-`Status` fields driving lifecycle, and `Status` fields of any board other than the nominated one — those stay `PROJECT-n.<field>` attributes.
- Conflict detection on the status write; policy stays last-write-wins plus refresh per RFC-050.
- Preserving the board's display casing in a doc's status or in the TUI. `Status` lowercases by design (ADR-023); changing that is its own story if the lost casing ever matters.
- Transition gating between columns. The empty edge set is deliberate: a board carries order but no transition rules, and inventing a chain would be a second, stricter opinion than the board's own.

### Split Point

If the slice proves too large, the honest cut is at the write path: the first four criteria plus the last two ship a read-only authority (board columns become the lifecycle lazyspec reports), and `update --status` write-through plus its offline rejection follow as a second story.
