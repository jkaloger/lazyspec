---
title: 'Codebase health: high-priority solvable issues (2026-07-16)'
type: audit
status: draft
author: agent
date: 2026-07-16
tags: []
related:
- related-to: RFC-061
- related-to: AUDIT-016
---## Scope

Full-codebase health review, 2026-07-16. Criteria: high-priority, concretely solvable issues — correctness bugs, layering violations (CONVENTION principles 2/3), CLI/TUI/web parity, README drift, test gaps, dead code. Style nits and speculative hardening excluded. Evidence gathered by three parallel explore passes (correctness; layering/parity; tests/dead-code) plus direct verification.

## Findings

### Critical

**F1. `.lazyspec.toml` at HEAD is unparseable — every lazyspec command fails**
- Location: `.lazyspec.toml` (commit c83bb99 "chore: bug type")
- The `bug` type block declares inline `attributes = []` AND is followed by `[[types.attributes]]` blocks → TOML "duplicate key `attributes`" parse error. `lazyspec list/status/validate` all die.
- Root cause: the config-write path cannot serialize attribute definitions (`update_type_table` in `src/engine/config_write.rs:72` never writes `attributes`; `config add-type` has no attribute support), so the type was hand-edited and broke.
- Recommendation: fix the TOML (drop the inline `attributes = []`) — done in working tree during this audit; add attribute serialization to config-write so `configure-type` never hand-edits; regression test for a config carrying both forms.

### High

**C1. Corrupt `cache.lock` is silently erased, destroying all cache freshness state**
- Location: `src/engine/issue_cache.rs:62-64`; `src/engine/cache_lock.rs:47`
- `load_lock` maps any load error to an empty lock via `unwrap_or_default()`; every mutator then saves it back. A truncated `cache.lock` (non-atomic `fs::write`, crash/concurrent poll) → empty lock persisted → every freshness timestamp/SHA destroyed.
- Recommendation: distinguish file-absent (default) from present-but-corrupt (hard error); never re-save a defaulted lock; make save temp+rename atomic.

**C2. `lazyspec link` panics when frontmatter has a bare `related:` (YAML null)**
- Location: `src/cli/link.rs:123`
- `doc["related"].as_sequence_mut().unwrap()` — guard only covers absent key, not non-sequence value. Common after manual edit. (`unlink_inner` at `:463` handles it correctly — asymmetric.)
- Recommendation: coerce non-sequence to empty sequence or error cleanly.

**C3. `update` on git-ref docs silently drops new keys and writes unquoted values**
- Location: `src/engine/git_ref_store.rs:184-187`
- Key not already present in frontmatter → silently dropped, still returns `Ok(())`. Values written unquoted → `--title 'Plan: phase 2'` yields unparseable YAML (doc vanishes on next load).
- Recommendation: round-trip frontmatter through `serde_yaml` (as `set_provenance` `:262-274` does), insert missing keys, escape values.

**C4. `fetch_all` wipes the type cache dir before fallible writes, no rollback**
- Location: `src/engine/issue_cache.rs:404-411`; `src/engine/clickup_cache.rs:72-81`
- Disk-full on write #3 of 100 → old cache already deleted, type shows ~0 docs until next full success.
- Recommendation: staging dir + rename-swap, or write-then-delete-stale.

**C5. Cross-cutting: all sidecar persistence is truncate-in-place with no interprocess lock**
- Location: `cache_lock.rs:47`, `issue_map.rs:62`, `task_map.rs:53`, `sync.rs:426`, `store_dispatch.rs:2070`
- Load-mutate-save with no lock → concurrent lazyspec processes clobber each other last-writer-wins.
- Recommendation: one temp+rename helper + advisory file lock fixes the family (subsumes C1/C4 mechanics).

**F2. TUI depends directly on CLI modules (layering violation, CONVENTION principle 3)**
- Location: `src/tui/state/app.rs:2450,2503,2528,2554,2613,2894,3173`; `src/tui/infra/event_loop.rs:902`
- TUI calls `crate::cli::create::run`, `cli::link::link_with_config`, `cli::delete::run_with_config`, `cli::update::run_with_config`, `cli::fix::run_human`. Web layer (`src/web/routes.rs:3`) explicitly documents the opposite rule.
- Recommendation: hoist these op functions (no clap types in signatures) into `engine::ops`; CLI and TUI both call engine.

**F3. Engine writes to stderr directly (violates "engine has no I/O assumptions"; garbles raw-mode TUI)**
- Location: `src/engine/prompt.rs:121-127`, `src/engine/credentials.rs:310,425` (also `src/engine/lease.rs:259`, moot under RFC-061)
- `discover_prompts` eprintln!s warnings while the TUI is in raw/alternate-screen mode; `src/tui/state/app.rs:658` comments the gap.
- Recommendation: return warnings (Vec<PromptWarning> already exists); CLI prints, TUI routes to warnings panel (cf. STORY-163).

**F4. Six commands have no `--json` (violates CONVENTION principle 2)**
- Location: `src/cli.rs` — Delete (:151), Link (:157), Unlink (:169), Ignore (:223), Unignore (:229); `src/cli/setup.rs:105`; `src/cli/skills.rs` (whole file)
- Human-only println! output; agents cannot consume results.
- Recommendation: add `--json` mirroring the `tag` command pattern; outcomes are already structured.

**F5. `create --json` returns empty `id`**
- Location: `src/cli/create.rs` (JSON output path)
- Newly created doc serializes with `"id": ""` — an agent creating a doc can't learn its ID without re-listing. Observed live while creating this audit (AUDIT-018 returned `"id": ""`).
- Recommendation: resolve the ID from the written path before serializing.

**F6. RFC-061 (remove leases/claim/heartbeat) is accepted with no downstream delivery**
- Location: `docs/rfcs/RFC-061-...md`; ~2,100 lines across `src/engine/lease.rs`, `src/cli/lease.rs`, gates in `main.rs`, `[coordination]` config, TUI field, README
- Dead-weight subsystem gates every create/update/advance on a distributed-lock check.
- Recommendation: author story + iterations to execute the removal per the RFC's scope (keep `push_ref_with_lease` CAS primitive).

### Medium

**F7. Backend store wiring duplicated 7+ times, bypassing `build_registry`**
- Location: `src/cli/create.rs:132,157,181,324`, `src/cli/update.rs:94-118`, `src/cli/delete.rs:33-50`, `src/cli/link.rs:660`, `src/tui/infra/event_loop.rs:186,596`
- `store_dispatch.rs:2289` documents `build_registry` as "the single place a new backend is wired"; the ClickUp token+store block is copy-pasted verbatim 3×.
- Recommendation: registry/helper for write stores; route the GitHub branches through it. Largely subsumed by F2's ops hoist.

**F8. README drift: ~9 commands missing from command table; 7 TUI keybinds undocumented**
- Location: `README.md:262-289` vs `src/cli.rs`; `README.md:156-171` vs `src/tui/views/keybinds.rs:350-410`
- Missing: `fetch`, `config`, `convention`, `skills`, `completions`, `fix --renumber/--type`, etc.; keys `s p a x g G Space Tab` undocumented.
- Recommendation: sync tables after RFC-061 removal lands (avoid documenting commands about to be deleted).

**F9. CLI `tag add/remove` has no TUI equivalent**
- Location: `src/cli/tag.rs` vs `src/tui/views/keybinds.rs:383-396`
- Tags editable only at create time in TUI; project rule requires TUI/web/CLI parity.
- Recommendation: TUI tag-edit overlay calling the same engine path (post-F2 hoist).

**F10. Draft iterations 205–209 appear fully implemented; statuses stale**
- Location: `docs/iterations/ITERATION-205..209`; code evidence: `AttrValue` in `src/engine/document.rs:186`, attr validation in `src/engine/validation.rs:1030`, `context --anchor` live, attributes in status JSON, pivot in `src/tui/views/panels.rs`
- Backlog misrepresents reality; violates "change the rule or change the code" governance spirit.
- Recommendation: verify ACs and advance each to complete (or reopen genuinely unfinished ones).

**F11. `display_name` copy-pasted 3× in TUI; tests assert on the third copy**
- Location: `src/tui/views/panels.rs:242`, `src/tui/views/overlays.rs:1039`, test copy at `src/tui/views.rs:286`
- Recommendation: single `pub(crate) fn`, point tests at it.

**F12. `fs_ops.rs` (346 lines, filesystem write path) has zero direct tests**
- Location: `src/engine/fs_ops.rs`
- Template-resolution order and error branches untested; every other substantial engine module has 5–116 inline tests.
- Recommendation: tempdir unit tests for template resolution and create/update/delete edges.

**C6. `resolve_shorthand` `PARENT/child` path has no ambiguity guard**
- Location: `src/engine/store.rs:196-222`
- First `starts_with(parent_id)` match over a HashMap wins: `RFC-1` vs `RFC-12` → `show RFC-1/sub` nondeterministically hits `RFC-12`'s child. `resolve_unqualified` (`:239-262`) already does this right.
- Recommendation: collect matches, prefer exact id, error on >1.

**C7. GitHub issue open/closed derived from hardcoded status list, not the type's lifecycle**
- Location: `src/engine/store_dispatch.rs:1389-1400`
- `matches!(status, "draft"|"review"|"accepted"|"in-progress")` — custom non-terminal status like `blocked` closes the issue.
- Recommendation: derive the open set from lifecycle config (non-terminal states).

### Low

**F13. Dead code: `Config::load_lenient` never called** — `src/engine/config.rs:1052`; `fix --config` uses `parse_lenient` directly. Delete.

**F14. Hand-rolled `TypeDef` fixture literals ×17 in tests despite `TypeDef::test_fixture`** — `src/cli/link.rs`, `src/tui/state/app.rs`, others. Replace with fixture + overrides.

**F15. Unused failing-write fakes** — `src/engine/clickup.rs:775,795,815`, `src/engine/gh.rs:1384`. Delete or leave until needed.

**F16. Historical validation debt: ~11 broken links + missing `iterations-need-stories` parents in old docs** — `lazyspec validate` output. Mostly renamed/deleted targets from past renumbering. Fix links or `ignore` the historical docs.

### Noted, moot under RFC-061 (lease removal)

`parse_duration` panics on multi-byte suffix / overflow (`src/engine/lease.rs:22-29`); heartbeat state filename accepts path separators (`src/cli/lease.rs:258-261`); `lease.rs:259` eprintln. All deleted by the RFC-061 removal — do not fix separately. AUDIT-016 (lease protocol safety review, draft) should be superseded for the same reason.

## Method

Three parallel read-only explore agents (correctness; layering/parity/README; test-coverage/dead-code/duplication), findings cross-checked against source before inclusion. Lease-related findings filtered against RFC-061 scope.
