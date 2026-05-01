---
title: CLI next graph critical-path commands
type: iteration
status: accepted
author: agent
date: 2026-05-01
tags: []
related:
- implements: STORY-123
---



## Summary

Single iter covers all 13 ACs of STORY-123. CLI slice over engine graph layer (landed in iter 162/163/164). 3 new subcommands: `next`, `graph`, `critical-path`. 3 new validate diagnostics. `--help` + README updates. No engine logic added beyond format renderers (d2/dot/json) — engine primitives already shipped.

## Acceptance Criteria covered

- AC1: `next --json` shape (`ready`/`bottlenecks`/`warnings`; `ready[].id`/`kind`/`leased_by`).
- AC2: lease filter default-off + `--include-leased` opt-in w/ `leased_by` populated.
- AC3: `--scope` ⊕ `--after` mutually exclusive on all 3 commands.
- AC4: iteration id rejected as `--scope` w/ hint.
- AC5: RFC scope constrains `next` to implements-subtree.
- AC6: `--type` filter restricts `ready` by doc type.
- AC7: cycle-affected docs skipped from `ready`; `warnings` names members.
- AC8: `graph --format d2|json|dot` emits valid output for each.
- AC9: `critical-path --json` returns ordered id array.
- AC10: validate cycle error names members; non-zero exit.
- AC11: validate warning when RFC `accepted` + all implementing stories terminal.
- AC12: validate warning when upstream `blocks` is `rejected`.
- AC13: `--help` for all 3 commands documents every flag.
- AC14: README documents 3 new commands + 3 new validate diagnostics.

## Test Plan

DICTUM-004 conformant. Unit tests in `#[cfg(test)] mod tests` at bottom of source files. Integration tests in `tests/` for CLI surface. `tempfile::TempDir` for any I/O.

### `src/engine/sequencing_render.rs` (new)

- AC8-d2 — graph w/ 2 nodes 1 edge → `render_d2(...)` output contains valid d2 syntax (node decls + edge `A -> B` arrow). Substring asserts on stable tokens; not a full parser.
- AC8-dot — same fixture → `render_dot(...)` emits `digraph { ... }` + edge `"A" -> "B"`.
- AC8-json — same fixture → `render_json(...)` parses to JSON w/ `nodes[]` (id, type, status, priority) + `edges[]` (from, to, kind ∈ {blocks, implements}).
- empty graph — all 3 renderers return non-panic strings (d2 minimum header; dot = `digraph G {}`; json = `{"nodes":[],"edges":[]}`).
- scope honoured — `Scope::Under(rfc)` + 1 in-scope + 1 out-of-scope node → only in-scope serialized in all 3 renderers.

### `src/engine/validation.rs`

- AC10 — store w/ `blocks` cycle A→B→A → `validate_full` returns `ValidationIssue::Cycle { ids: [A,B] }` in `errors`. Set equality on ids.
- AC11-positive — RFC@accepted w/ 2 implementing stories both @complete → `ValidationIssue::AcceptedRfcChildrenComplete { rfc, children }` in `warnings`.
- AC11-negative-1 — RFC@accepted w/ one implementing story @draft → no AcceptedRfcChildrenComplete issue.
- AC11-negative-2 — RFC@complete w/ all children @complete → no issue (only fires on `accepted`).
- AC11-negative-3 — RFC@accepted w/ no implementing children → no issue.
- AC12-positive — story B `blocks: A`; A@rejected → `ValidationIssue::RejectedUpstreamBlocker { path: B, upstream: A }` in `warnings`.
- AC12-negative — A@complete → no issue.

### `tests/cli_sequencing_test.rs` (new)

`tempfile::TempDir` + setup helper. Run subcommand via lib entrypoint. Assert on stdout JSON (`serde_json::Value`) or stderr msg + exit code.

- AC1 — clean DAG project, run `next --json`. Parse stdout → root has `ready`, `bottlenecks`, `warnings`. Each `ready[i]` has keys `id`, `kind` (∈ `claimable`/`needs-children`/`needs-status-update`), `leased_by` (null for unleased fixture).
- AC2-default — leased ready candidate (mock lease via fixture) + run `next --json`. Assert leased id absent from `ready`.
- AC2-include-leased — same fixture + `--include-leased` → leased id present, `leased_by == "agent-x"`.
- AC3-next — `next --scope X --after Y`: exit code != 0, stderr contains "mutually exclusive".
- AC3-graph + AC3-critical — same combo on `graph` and `critical-path`.
- AC4 — `next --scope ITERATION-001`: exit != 0, stderr names id + hints "scope only accepts RFC or Story". Same on `graph` and `critical-path`.
- AC5 — RFC w/ 2 implementing stories (story A in scope, story B unrelated) → `next --scope RFC-001 --json` ready ⊆ {A's descendants}; B absent.
- AC6 — multi-type ready set; `next --type story --json` → all `ready[].id` resolve to docs of type story.
- AC7 — cycle fixture (A blocks B blocks A) + ready candidate C unrelated → `next --json`: A,B absent from `ready`; `warnings[]` mentions A and B.
- AC8 — graph fixture (3 nodes, 2 blocks edges, 1 implements edge); run `graph --format d2`/`json`/`dot`. Each non-empty, format-specific tokens.
- AC9 — graph w/ priority weights configured; `critical-path --json` stdout parses to JSON array of doc ids; assert order matches direct `Graph::critical_path` invocation w/ same fixture (golden via engine call).
- AC13 — exec `next --help`, `graph --help`, `critical-path --help`. Stdout substring-check each documented flag (`--scope`, `--after`, `--type`, `--include-leased`, `--json`, `--format`).

### `tests/cli_validate_test.rs` (extend)

- AC10 — cycle fixture → `validate --json` exit 2, stdout `errors[]` contains cycle members.
- AC11 — RFC@accepted + all stories @complete → `validate --json --warnings` surfaces warning.
- AC12 — rejected upstream fixture → warning surfaces in JSON output.

### Tradeoffs noted

- Graph renderers tested via substring on stable tokens, not full parser. Less specific but no parser dep (Principle 6: no abstraction until two consumers).
- CLI integration tests run lib entrypoints (`lazyspec::cli::next::run(...)`), not `assert_cmd` subprocess. Faster, deterministic stdout capture.

## Changes

Tasks self-contained for zero-context subagent. ACs, files, intent, verification per task.

### 1. Add `ValidationIssue` variants for 3 new diagnostics

- ACs: 10, 11, 12.
- File: `src/engine/validation.rs`.
- Intent:
  - Add to `ValidationIssue` enum:
    - `Cycle { ids: Vec<String> }`
    - `AcceptedRfcChildrenComplete { rfc: PathBuf, children: Vec<PathBuf> }`
    - `RejectedUpstreamBlocker { path: PathBuf, upstream: String }`
  - Extend `Display` impl: e.g. `"cycle in blocks graph: A, B, C"`, `"RFC accepted but all implementing stories complete: <path>"`, `"upstream blocker rejected: <path> blocked by <id>"`.
- Verify: `cargo build`. Tests in tasks 2/3/4.

### 2. `CycleRule` checker

- ACs: 10.
- File: `src/engine/validation.rs`.
- Intent:
  - `pub struct CycleRule;` impl `Checker`.
  - `check`: build `Graph::from_store(store)`. Call `cycle_check()`. On `Err(CycleError { ids })` push `(Severity::Error, ValidationIssue::Cycle { ids })`. Sort ids for determinism.
  - Register in `default_checkers()`.
- Tests: AC10 cycle fixture → error w/ matching ids set.
- Verify: `cargo test engine::validation::tests::cycle`.

### 3. `AcceptedRfcChildrenCompleteRule` checker

- ACs: 11.
- File: `src/engine/validation.rs`.
- Intent:
  - `pub struct AcceptedRfcChildrenCompleteRule;` impl `Checker`.
  - For each RFC w/ status `Accepted`: collect docs whose `related[].rel_type == Implements && target == rfc.id`. If non-empty AND every child has `is_terminal(&child, config) == true`, push `(Warning, AcceptedRfcChildrenComplete { rfc, children })`.
  - Empty children set → skip.
  - Register.
- Tests: AC11 positive + 3 negatives.
- Verify: `cargo test engine::validation::tests::accepted_rfc`.

### 4. `RejectedUpstreamBlockerRule` checker

- ACs: 12.
- File: `src/engine/validation.rs`.
- Intent:
  - `pub struct RejectedUpstreamBlockerRule;` impl `Checker`.
  - For each doc: walk `related[]` where `rel_type == Blocks`. Resolve target via `store.docs_by_id` (or equivalent). If target's status == `Rejected`, push `(Warning, RejectedUpstreamBlocker { path: doc.path, upstream: target.id })`.
  - Skip if target unresolved (BrokenLinkRule covers that).
  - Register.
- Tests: AC12 positive + negative.
- Verify: `cargo test engine::validation::tests::rejected_upstream`.

### 5. Engine graph renderers (d2 / dot / json)

- ACs: 8.
- File: `src/engine/sequencing_render.rs` (new). Register `pub mod sequencing_render;` in `src/engine.rs`.
- Intent: pure functions taking `&Graph`, `&Scope`, `&[DocMeta]`. Return `String`.
  - `pub fn render_d2(graph: &Graph, scope: &Scope, docs: &[DocMeta]) -> String`.
  - `pub fn render_dot(graph: &Graph, scope: &Scope, docs: &[DocMeta]) -> String`.
  - `pub fn render_json(graph: &Graph, scope: &Scope, docs: &[DocMeta]) -> String`.
  - Filter via `graph.scope_membership(scope)`; only members serialized.
  - d2: header `direction: down\n`, one node decl per member (`<id>: { label: "<title>" }`), edges `A -> B` for blocks, `A -> B { style.stroke-dash: 4 }` for implements.
  - dot: `digraph G {\n` ... `}`. Edge attr `[style=dashed]` for implements.
  - json: serde-serialize struct `{ nodes: [{id,type,status,priority}], edges: [{from,to,kind}] }`. `kind` ∈ `"blocks"|"implements"`.
- Tests: AC8 + empty + scope-filter. Unit in same file.
- Verify: `cargo test engine::sequencing_render`.

### 6. CLI subcommand: `next`

- ACs: 1, 2, 3, 4, 5, 6, 7, 13.
- New file: `src/cli/next.rs`. Register `pub mod next;` in `src/cli.rs`.
- Intent:
  - `pub fn run(store: &Store, config: &Config, args: NextArgs) -> i32`.
  - `NextArgs`: `scope: Option<String>`, `after: Option<String>`, `type_filter: Option<String>`, `include_leased: bool`, `json: bool`.
  - Mutual excl: both `scope` + `after` → eprintln "--scope and --after are mutually exclusive", return 2.
  - Iter-scope guard: `scope.is_some()` + `Graph::is_iteration(&id)` → eprintln msg naming id + hint, return 2.
  - Build `Scope` from args.
  - Build `LeaseView` from `LeaseEngine::query(store.root)` if available; else empty.
  - Call `next_ready(&graph, &docs, &opts, &lease_view, config)`.
  - Apply `--type` filter post-hoc on `ready[]`.
  - JSON: serialize `NextResult` w/ keys `ready`, `bottlenecks`, `warnings`. `ready[]` items: `{id, kind: "claimable"|"needs-children"|"needs-status-update", leased_by: Option<String>}`. Human path: minimal pretty render.
  - Wire `Commands::Next { scope, after, type_filter, include_leased, json }` clap variant w/ doc-strings (AC13).
  - Dispatch in `src/main.rs`.
- Tests: AC1, 2, 3-next, 4-next, 5, 6, 7 in `tests/cli_sequencing_test.rs`.
- Verify: `cargo test --test cli_sequencing_test next_`. Manual: `cargo run -- next --json`.

### 7. CLI subcommand: `graph`

- ACs: 3, 4, 8, 13.
- New file: `src/cli/graph.rs`. Register in `src/cli.rs`.
- Intent:
  - `pub fn run(store: &Store, config: &Config, args: GraphArgs) -> i32`.
  - `GraphArgs`: `scope: Option<String>`, `after: Option<String>`, `format: GraphFormat` (clap `ValueEnum`: `D2`/`Json`/`Dot`, default `D2`).
  - Mutual-excl + iter-scope guard. Three uses → extract shared helper `validate_scope_args(...)` in new `src/cli/sequencing_args.rs`.
  - Dispatch to `render_d2`/`render_dot`/`render_json`. Print stdout. Return 0.
  - Wire `Commands::Graph { ... }` clap variant. `--format` via `value_enum`.
- Tests: AC3-graph, AC4-graph, AC8 (3 formats).
- Verify: `cargo test --test cli_sequencing_test graph_`. Manual: `cargo run -- graph --format d2`.

### 8. CLI subcommand: `critical-path`

- ACs: 3, 4, 9, 13.
- New file: `src/cli/critical_path.rs`. Register in `src/cli.rs`.
- Intent:
  - `pub fn run(store: &Store, config: &Config, args: CriticalPathArgs) -> i32`.
  - `CriticalPathArgs`: `scope: Option<String>`, `after: Option<String>`, `json: bool`.
  - Mutual-excl + iter-scope guard via shared helper.
  - Build `Weights` from `config.priority_weights()` (key → f64). For docs, weight by priority key; missing priority → lowest configured weight.
  - Per-node weights for `Graph::critical_path`: `HashMap<doc_id, f64>` from priority weights.
  - Call `graph.critical_path(scope, &weights)`.
  - JSON: serialize `Vec<String>` of ids in order. Human: one id per line.
  - Wire `Commands::CriticalPath { ... }`. clap rename `name = "critical-path"`.
- Tests: AC3-critical, AC4-critical, AC9.
- Verify: `cargo test --test cli_sequencing_test critical_path_`. Manual: `cargo run -- critical-path --json`.

### 9. README updates

- ACs: 14.
- File: `README.md`.
- Intent:
  - Under existing "DAG-driven work sequencing" feature bullet, add CLI usage subsection (after Quick Start) listing `next`, `graph`, `critical-path` w/ one-line description + flag table per command. Match existing CLI section style.
  - Document 3 new validate diagnostics: cycle error, accepted-RFC-children-done warning, rejected-upstream warning.
- Verify: `cargo run -- --help` matches README; manual read-through.

### 10. Wire dispatch + final smoke

- ACs: all (integration).
- Files: `src/cli.rs` (3 `Commands::*` variants if not done in 6/7/8), `src/main.rs` (3 match arms calling each `run`).
- Verify: `cargo build`. `cargo test`. `cargo run -- next --help` / `graph --help` / `critical-path --help`. `cargo run -- validate --json` on this repo (dogfood).

## Notes

- Scope helper (task 7) is the only candidate for shared helper across `next`/`graph`/`critical-path`. Three uses → introduces helper now (Principle 6 met).
- Engine renderers (task 5) live in `src/engine/sequencing_render.rs`, not in `sequencing.rs`, to keep render concerns out of graph algorithm module.
- Lease integration in `next` follows iter 163 pattern: caller builds `LeaseView` from `LeaseEngine::query`, passes pure value into `next_ready`. Engine remains pure.
- `validate` sorts errors/warnings by `Display` string. New issues integrate naturally.
- `is_terminal` used by AcceptedRfcChildrenCompleteRule must be config-driven version landed in iter 164. Confirm signature `is_terminal(&DocMeta, &Config) -> bool` before calling.

