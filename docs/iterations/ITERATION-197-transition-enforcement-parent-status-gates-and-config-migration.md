---
title: Transition enforcement, parent-status gates, and config migration
type: iteration
status: complete
author: agent
date: 2026-06-21
tags: []
related:
- implements: STORY-145
---

## Changes

This iteration is group B of STORY-145: ENFORCEMENT + MIGRATION. It builds on the
data model from ITERATION-196 (the `intent`/`authorship`/`lifecycle` axes on
`TypeDef`, the `Authorship` enum, the `Lifecycle` struct, and the `Status`
validated newtype with state validation). It does NOT re-plan those — it consumes
them.

This iteration owns `require_parent_status` end-to-end: the field, its parse, and
its enforcement.

Both gates (transition + parent-status) live in the CLI layer, ABOVE the
store-backend dispatch, so they apply uniformly to filesystem, github-issues, and
git-ref types. `update --status` flows through `src/cli/update.rs`
`run_with_config` (`src/cli/update.rs:18`) before any backend `update()` is
called; `create <child>` flows through `src/cli/create.rs` `run_with_body`
(`src/cli/create.rs:39`) before any backend `create()`. Both already receive the
loaded `Config`; the `store` (with `store.docs` and `store.reverse_links`) is
available at the dispatch sites in `src/main.rs` (`src/main.rs:166` Update,
`src/main.rs:100` Create).

### 1. Edge-transition validation on `update --status` (AC4)

**Files:** `src/cli/update.rs`, `src/engine/document.rs` (lifecycle edge lookup
helper — coordinate with ITERATION-196's `Lifecycle`).

**AC covered:** AC4.

Today `update --status` performs an unconstrained any→any frontmatter write:
`run_with_config` (`src/cli/update.rs:18`) resolves the doc and hands the
`("status", value)` pair straight to a backend `update()` or to
`fs_ops::update_document` (`src/engine/fs_ops.rs:264`). No transition check
exists. ADR-022 turns this into an edge-validated move.

Implementation:

- In `run_with_config`, when the `updates` slice contains a `status` key, look up
  the document's current status (from the resolved `DocMeta` via
  `resolve_shorthand_or_path`, `src/cli/update.rs:26`) and the requested target
  status, then consult the type's `lifecycle` (from `config.type_by_name(...)`,
  `src/engine/config.rs:659`).
- Add a lifecycle edge-lookup method on the `Lifecycle` struct (defined by
  ITERATION-196), e.g. `Lifecycle::has_edge(from, to) -> bool`, that returns true
  when an edge `from→to` is declared, treating a `*` edge source as matching any
  `from`. This iteration adds the method if ITERATION-196 did not; the edge data
  shape is ITERATION-196's.
- If the current status equals the target, treat as a no-op pass (idempotent;
  not a transition).
- If no matching edge exists, `bail!` with a message naming the type, the current
  status, the rejected target, and the set of declared targets from the current
  state. The frontmatter is NOT written (the bail occurs before the backend
  `update()` call), so status is unchanged.
- On a valid edge, proceed exactly as today.
- Only `status` updates are gated. Other keys (`title`, `body`) in the same
  `updates` slice are unaffected.

**Verification:** `cargo test` for the new transition tests (below). Manual:
`cargo run --quiet -- update <doc> --status <non-edge>` exits non-zero and the
file's `status:` line is unchanged; `--status <edge-target>` succeeds.

### 2. Add + parse `require_parent_status` on the parent-child rule (AC5, data)

**Files:** `src/engine/config.rs`, `src/engine/config_write.rs`.

**AC covered:** AC5 (data half).

- Add an optional `require_parent_status: Option<String>` field to
  `ValidationRule::ParentChild` (`src/engine/config.rs:17`). Use
  `#[serde(default, skip_serializing_if = "Option::is_none")]` so existing rules
  with no such key still parse and the serializer omits it when absent.
- The field deserializes through the same `RawConfig.rules` path
  (`src/engine/config.rs:361`) — no extra parse wiring needed beyond the field
  itself, since `ValidationRule` is `Deserialize`.
- Update every `ValidationRule::ParentChild { .. }` construction and destructure
  to account for the new field. Known sites: `default_rules`
  (`src/engine/config.rs:438`, set to `None` for the starter rules),
  `config_write.rs` `rule_name` (`src/engine/config_write.rs:325`), `rule_shape`
  (`src/engine/config_write.rs:407`), `update_rule_table`
  (`src/engine/config_write.rs:342`), and `hierarchy_from_config`
  (`src/engine/validation.rs:274`) and `ParentLinkRule`
  (`src/engine/validation.rs:405`) destructures (add `..` or bind the field).
- In `update_rule_table` (`src/engine/config_write.rs:342`), write the field as
  an optional string with `set_opt_str(entry, "require_parent_status", ...)`
  (mirroring how `parent_type` is handled at `src/engine/config_write.rs:86`).
  Also add `"require_parent_status"` to the stale-key clear list in the
  shape-change branch (`src/engine/config_write.rs:337`) so a parent-child →
  relation-existence switch drops it.

**Verification:** `cargo test` for config parse/round-trip tests (below); a
`ParentChild` rule with and without `require_parent_status` parses, and the
writer round-trips it.

### 3. Enforce the parent-status creation gate on `create <child>` (AC5, enforcement)

**Files:** `src/cli/create.rs`, `src/main.rs` (pass `store` to the create path).

**AC covered:** AC5 (enforcement half).

`create <child>` currently never inspects parent status. It is gated only by
singleton and lease checks. The gate must: find each `ParentChild` rule whose
`child` equals the type being created AND that carries a `require_parent_status`;
for each such rule, confirm at least one existing parent doc (of the rule's
`parent` type, linked via the rule's `link`) has reached the required status; if
none has, refuse creation.

Implementation:

- `run_with_body` (`src/cli/create.rs:39`) already takes `config` and `store`.
  After the singleton check (`src/cli/create.rs:63`) and before any backend
  branch, iterate `config.rules` for `ParentChild { child, parent, link,
  require_parent_status: Some(required), .. }` where `child == doc_type`.
- For each, scan `store.docs` for docs of `parent` type whose status equals
  `required` (parse `required` through the parent type's lifecycle — an unknown
  required status in the rule is a config error worth surfacing). The gate is
  satisfied if at least one such parent exists at-or-having-reached the required
  status. (Use the lifecycle to decide "reached": if ITERATION-196 exposes
  reachability, treat any state from which `required` is unreachable as
  not-yet-reached; otherwise an exact status match is the minimum bar — keep it
  exact for this slice and note the simplification.)
- If a rule's gate is unsatisfied, `bail!` naming the child type, the required
  parent status, and the parent type. Creation stops before any file/issue/ref is
  written.
- If a type has no `require_parent_status` rule, behaviour is unchanged.

The `store` is loaded at the Create dispatch (`src/main.rs:110`); it is already
passed into `run_json_with_body`/`run_with_body`, so no signature change is
needed — the gate reads `store` and `config` already in scope.

**Verification:** `cargo test` for the gate test (below). Manual: with a rule
requiring parent `accepted`, `cargo run --quiet -- create <child> "t"` fails while
the parent is `draft`; after `update <parent> --status ... accepted`, the same
create succeeds.

### 4. `fix --config` default-lifecycle injection (AC6)

**Files:** `src/cli/fix/config.rs`, `src/engine/config.rs` (a
`default_lifecycle()` helper).

**AC covered:** AC6.

`fix --config` runs before strict `Config::load` via the lenient path
(`src/main.rs:68` → `fix::run_config` → `collect_config_fixes`,
`src/cli/fix/config.rs:39`). It is append-only today: it computes missing
standard relationships and rules and appends `[[relationships]]`/`[[rules]]`
blocks (`append_blocks`, `src/cli/fix/config.rs:94`). This iteration extends it to
inject a default `lifecycle` into any `[[types]]` entry that lacks one, so every
type ends with a valid lifecycle (migration for pre-lifecycle configs).

Implementation:

- Add `default_lifecycle()` to `src/engine/config.rs` returning the prior seven
  statuses (`draft`, `review`, `accepted`, `in-progress`, `complete`, `rejected`,
  `superseded`) as lifecycle states plus sensible edges (a DAG: `draft→review`,
  `review→accepted`, `accepted→in-progress`, `in-progress→complete`, and
  `*→rejected`, `*→superseded` so any state may be rejected/superseded). The
  exact `Lifecycle` shape is ITERATION-196's; this returns one populated with that
  default. The starter config's per-type lifecycle (the in-scope "starter ships a
  default lifecycle" item from STORY-145) is ITERATION-196's concern via
  `starter_types`; this helper is the shared source both can use.
- In `collect_config_fixes` (`src/cli/fix/config.rs:39`), after the lenient parse,
  determine which `[[types]]` entries have no lifecycle. Because the migration is
  append-only and edits per-existing-`[[types]]`-table, inject the lifecycle into
  each lifecycle-less type. Two viable mechanics, pick the one matching
  ITERATION-196's `Lifecycle` TOML representation:
  - If lifecycle is a sub-table/inline structure on the type, prefer reusing the
    in-place `write_config_in_place` machinery
    (`src/engine/config_write.rs:9`): build a buffer `Config` from the lenient
    parse with `default_lifecycle()` filled in for every type lacking one, then
    render via the existing reconcile path (it already updates `[[types]]` tables
    in place and preserves decor — `write_types`,
    `src/engine/config_write.rs:50`). This is cleaner than hand-appending TOML and
    keeps comments.
  - Note: today `collect_config_fixes` hand-appends only `[[relationships]]`/
    `[[rules]]`. Adding per-type lifecycle injection is the reason to route the
    type edits through `write_config_in_place` rather than `append_blocks`. Keep
    relationships/rules append behaviour intact (or fold them into the same
    in-place write if simpler — verify the writer emits them).
- Extend `ConfigFixResult` (`src/cli/fix/.rs` → `ConfigFixResult` at
  `src/cli/fix.rs:73`) with a `lifecycles_added: Vec<String>` (type names that
  received a default lifecycle), and surface it in `format_config_human`
  (`src/cli/fix/output.rs`) and the JSON output.
- Idempotent: a type that already declares a lifecycle is untouched; re-running
  `fix --config` on a fully-migrated config reports nothing added and writes
  nothing.

**Verification:** `cargo test` for the migration test (below). Manual:
`cargo run --quiet -- fix --config --json` on a config whose types lack lifecycles
reports each type in `lifecycles_added`; re-running reports none.

### 5. README / interface update

**File:** `README.md` (and any `--config` help text in `src/cli.rs:228`).

If the `fix --config` description or `update`/`create` behaviour is documented in
the README, update it to note transition enforcement, the parent-status gate, and
default-lifecycle injection. Per project convention, the CLI interface doc must
stay in sync.

## Test Plan

No test code here — behaviours only. Tests live alongside their modules
(`#[cfg(test)]` in `src/engine/config.rs`, `src/engine/config_write.rs`, and
integration-style CLI tests for `update`/`create`/`fix`).

### AC4 — transition enforcement on `update --status`

- **Reject off-edge move:** a type whose lifecycle has states {A, B} and a single
  edge A→B; a doc at A. `update --status` to a state with no edge from A (e.g. a
  third state C, or B→A when no reverse edge is declared) is rejected, exits
  non-zero, and the doc's status is still A after the call (assert the frontmatter
  line is unchanged).
- **Accept on-edge move:** same lifecycle; a doc at A; `update --status B`
  succeeds and the doc reads B.
- **Wildcard edge:** a `*→rejected` edge lets a doc at any state move to
  `rejected`.
- **No-op:** setting the status to its current value is allowed (idempotent), not
  treated as a missing edge.
- **Non-status updates unaffected:** `update --title` on the same doc with no
  `--status` performs no transition check.

### AC5 — parent-status creation gate

- **Field parse:** a `[[rules]]` parent-child rule with
  `require_parent_status = "accepted"` parses and the field is readable;
  a parent-child rule WITHOUT the key parses with the field `None`.
- **Writer round-trip:** `write_config_in_place` round-trips a rule with
  `require_parent_status` set, and a shape change (parent-child →
  relation-existence) drops the key.
- **Gate blocks then allows:** given a rule requiring the parent at `accepted`
  and a parent doc at `draft`, `create <child>` is refused (non-zero, no document
  written). After the parent reaches `accepted`, the identical `create <child>`
  succeeds and the child document exists.
- **No gate when unset:** a child type whose rule has no `require_parent_status`
  is created freely regardless of parent status.

### AC6 — `fix --config` default-lifecycle migration

- **Injects on lifecycle-less config:** a pre-existing config whose `[[types]]`
  declare no lifecycle. After `fix --config`, every type has the default
  lifecycle (the seven prior statuses + sensible edges); re-parsing the written
  config via strict `Config::load` succeeds and each type's lifecycle is
  non-empty.
- **Reports added types:** the result (`ConfigFixResult`) lists each migrated
  type in `lifecycles_added` (and JSON output reflects it).
- **Idempotent:** running `fix --config` on an already-migrated config adds no
  lifecycles and does not rewrite the file.
- **Preserves user content:** comments and unrelated sections (`[github]`,
  `[coordination]`, existing `[[relationships]]`) survive the migration.
- **A type that already has a lifecycle is left untouched** by the migration.

## Notes

- Depends on ITERATION-196 (data model): the `lifecycle` axis on `TypeDef`, the
  `Lifecycle` struct + its edge representation, and the `Status` validated
  newtype. Build order within STORY-145: **196 then 197**. Do not start 197 until
  196's data model lands.
- Sibling boundary: ITERATION-196 owns the TypeDef axes, `Authorship`,
  `Lifecycle`, and `Status` validation, plus the starter config's per-type
  lifecycle. This iteration consumes those and owns transition enforcement, the
  `require_parent_status` field end-to-end, the creation gate, and the
  `fix --config` migration.
- Both gates sit in the CLI layer above store dispatch, so they cover all three
  backends (filesystem, github-issues, git-ref) without per-backend duplication.
- Coordinate the exact `Lifecycle` edge-lookup/reachability API with ITERATION-196
  rather than redefining it; if 196 already exposes `has_edge`/reachability, reuse
  it. `default_lifecycle()` is the shared source for both the migration here and
  196's starter config — agree on its home (`src/engine/config.rs`).
- AC4/AC5 reject by `bail!` before any write, so "status unchanged" / "no document
  created" fall out of the early return rather than needing rollback.
