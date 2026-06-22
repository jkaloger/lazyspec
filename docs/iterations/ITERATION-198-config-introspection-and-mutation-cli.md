---
title: Config introspection and mutation CLI
type: iteration
status: draft
author: agent
date: 2026-06-21
tags: []
related:
- implements: STORY-146
---

## Changes

One iteration, all six ACs of STORY-146. Spine: binary owns data (ADR-019). Add a
`config` command — `--json` reads the whole `Config`, three subcommands mutate
`.lazyspec.toml` in place via the existing `config_write.rs` reconciler. Depends on
STORY-145 (axes `intent`/`authorship`/`lifecycle` on `TypeDef`, `require_parent_status`
on the parent-child rule). Assume those fields exist — do NOT plan them here.

1. **Add the `Config` subcommand + dispatch.** [AC1, AC2]
   - `src/cli.rs` (`Commands` enum, lines 66-340): add a `Config { #[command(subcommand)] command: ConfigCommand }` variant. Define `ConfigCommand` (a `#[derive(Subcommand)]`, mirroring `ReservationsCommand`/`ProvenanceCommand` at `src/cli/reservations.rs:11` / `src/cli/provenance.rs:11`) with: a default-ish `--json` read (model as `ConfigCommand::Show { json: bool }` so `config --json` reads) plus `AddType`, `SetLifecycle`, `AddGate` (args below). Re-export from `src/cli.rs` like the other subcommand enums (lines 26-27).
   - New module `src/cli/config.rs`; register `pub mod config;` in `src/cli.rs` (alphabetical, lines 1-24). Hold the four run fns.
   - `src/main.rs` dispatch (match arm near the other `Some(Commands::…)` arms, ~line 81+): `Config` loads via `Config::load(&cwd, &fs)` (line 79 already does this for the general path — the read/mutation subcommands all need a loadable config, so they sit AFTER the load, unlike `fix --config` which is special-cased before load at lines 68-77). Route `Show` to the JSON reader, the three mutators to their run fns.
   - Verify: `cargo run --quiet -- config --help` lists the four subcommands; `config --json` prints JSON.

2. **`config --json` reads the full Config.** [AC1, AC2]
   - In `src/cli/config.rs`, `run_show_json(config: &Config) -> String` = `serde_json::to_string_pretty(config)`. `Config` already derives `Serialize` (`src/engine/config.rs:260`).
   - Confirm the four AC surfaces serialize OUT. `types` (`config.rs:189`), `relationships` (`config.rs:268`), and `rules` (`config.rs:274`) are `#[serde(skip_deserializing)]` — they skip the DERIVE on the way IN (parsed via `RawConfig`, `config.rs:357-375`) but DO serialize OUT. So types (with STORY-145's `intent`/`authorship`/`lifecycle`), relationships, and rules (with `require_parent_status` gate) all appear. AC1 + AC2 met by the derive alone.
   - Gotcha: `DocumentConfig::{sqids,reserved,github}` (`config.rs:192-197`), `Config::{ref_count_ceiling,coordination}` (`config.rs:276,280`) are `#[serde(skip)]` — they will NOT appear in `--json`. STORY-146's ACs don't require them, so leave as-is; just don't claim "the full config" includes those.
   - Verify: `config --json | jq '.types[0] | {intent, authorship, lifecycle}'` is populated; `jq '.relationships, .rules'` present; a `rules[]` entry of shape `parent-child` shows `require_parent_status`.

3. **`config add-type` — append a type, round-trip.** [AC3, AC6]
   - `ConfigCommand::AddType` args (clap): `name`, `plural`, `dir`, `prefix`, plus `--icon`, `--parent-type`, `--singleton`, `--store`, `--numbering`, and the STORY-145 axes `--intent`, `--authorship`. Lifecycle is set separately via `set-lifecycle` (keep `add-type` to scalar/simple fields; a type added without a lifecycle relies on STORY-145's default — confirm during build).
   - `run_add_type(root, fs, args…)`: read `.lazyspec.toml` text, `Config::load` (or `parse`) it, push a new `TypeDef` onto `config.documents.types`, then `write_config_in_place(&existing_src, &config)` (`src/engine/config_write.rs:9`) and write the result back. The reconciler appends a new `[[types]]` table by identity (name not in source → fresh table via `reconcile_array_of_tables`, `config_write.rs:378-403`; proven by `adding_a_type_appends_and_preserves_existing_comments`, `config_write.rs:787`).
   - Reject a duplicate type name (`config.type_by_name`, `config.rs:659`) with a clear error before writing.
   - Verify: `config add-type spike spikes docs/spikes SPIKE --intent "..."` then `config --json | jq '.types[] | select(.name=="spike")'` shows the supplied fields; re-running `config --json` is stable (round-trip).

4. **`config set-lifecycle` — update a type's states/edges.** [AC4, AC6]
   - `ConfigCommand::SetLifecycle` args: the type `name` (positional) + a way to express states and edges. Decide the surface during build to match STORY-145's `lifecycle` shape (likely repeated `--state` flags and `--edge FROM:TO` flags, `*` allowed as edge source). Keep it declarative; replace the type's whole lifecycle (set, not merge) so the command is idempotent.
   - `run_set_lifecycle`: read text + parse, locate the `TypeDef` by name, replace its `lifecycle`, `write_config_in_place`, write back. `update_type_table` (`config_write.rs:71`) must write the new lifecycle keys — STORY-145 owns adding the lifecycle write there; if absent, this iteration adds the lifecycle branch to `update_type_table` (flag in Notes). In-place edit preserves the type's decor (see `set_value`, `config_write.rs:544`).
   - Error if the type name is unknown.
   - Verify: starting from the default DAG, `config set-lifecycle iteration --state draft --state done --edge draft:done` then `config --json | jq '.types[] | select(.name=="iteration").lifecycle'` reports the new states + edges.

5. **`config add-gate` — set `require_parent_status` on a parent-child rule.** [AC5, AC6]
   - `ConfigCommand::AddGate` args: the rule `name` (positional) + `--status <required-parent-status>`.
   - `run_add_gate`: read + parse, find the `ValidationRule::ParentChild` by name (`config.rs:16-23`), set its `require_parent_status` (STORY-145 field), `write_config_in_place`, write back. `update_rule_table` (`config_write.rs:331`) writes rule keys; STORY-145 owns adding the `require_parent_status` key there — if absent, add the `set_opt_str`/`set_str` branch for it (flag in Notes). Reconcile matches the rule by `name` (`rule_name`, `config_write.rs:324`) and updates in place, preserving decor.
   - Error if the rule name is unknown or names a `relation-existence` rule (gate only applies to `parent-child`).
   - Verify: on a parent-child rule with no gate, `config add-gate stories-need-rfcs --status approved` then `config --json | jq '.rules[] | select(.name=="stories-need-rfcs").require_parent_status'` is `"approved"`.

6. **Help text.** [all]
   - Doc-comment each `ConfigCommand` variant and its args (clap renders these as `--help`). Top-level `Config` variant gets `/// Inspect and edit .lazyspec.toml` matching the style of the existing variants (`src/cli.rs:68-339`).
   - Verify: `cargo run --quiet -- config --help` and `config add-type --help` read cleanly.

7. **README — new CLI surface.** [all]
   - `README.md` Configuration `<details>` block (lines 325-490). Add a `### Inspecting and Editing the Config` subsection after `### Migrating an Existing Config` (line 358) documenting `config --json` and the three mutators, noting that mutations preserve comments/formatting/order (same guarantee as TUI settings and `fix --config`).
   - Per project CLAUDE.md: the CLI interface changed, so the README MUST be updated.
   - Verify: README shows the four `config` subcommands with example invocations.

## Test Plan

No code here — shapes only. Reuse the `config_write.rs` test harness conventions
(`SRC` fixture + `changed_lines`, `config_write.rs:561-630`) for preservation tests.

- **AC1 — `config --json` type axes shape.** Given a config whose types carry `intent`/`authorship`/`lifecycle`, assert `config --json` (or `run_show_json`) emits every type with all three axes populated, lifecycle showing both `states` and `edges`. Drives Change 2.
- **AC2 — `config --json` relations/rules/gates shape.** Same JSON: assert `relationships`, `rules` arrays present, and a `parent-child` rule entry carries `require_parent_status`. Guards against a future re-introduction of `#[serde(skip)]` silently dropping them.
- **AC3 — `add-type` round-trip.** Add a type, re-read `config --json`, assert the new type is present with the supplied fields; assert a second read is byte-identical (idempotent). Plus a duplicate-name rejection test (no write).
- **AC4 — `set-lifecycle` round-trip.** From the default DAG, set new states/edges, assert `config --json` reports exactly the new lifecycle (replace, not merge); unknown-type rejection.
- **AC5 — `add-gate` round-trip.** On a gateless parent-child rule, add the gate, assert `config --json` reports the `require_parent_status`; reject unknown rule and `relation-existence` target.
- **AC6 — formatting preservation (per mutator).** For each of add-type / set-lifecycle / add-gate, run against a fixture with standalone + inline comments and non-default table order; assert the comments survive, untouched tables keep their order, and only the intended block changed (mirror `preserves_comments_and_only_changes_one_value`, `config_write.rs:601`). Re-parse the output to prove validity.

## Notes

- This CLI is the dependency for **ITERATION-200** (skills install — consumes `config --json` at runtime) and **ITERATION-202** (`/configure-type` meta-skill — drives `add-type`/`set-lifecycle`/`add-gate`). Keep the JSON shape and subcommand surface stable.
- **`config --json` serialization status (verified):** `types`, `relationships`, `rules` are `#[serde(skip_deserializing)]` (`config.rs:189,268,274`) — they serialize OUT correctly, so AC1/AC2 need NO extra serialization work. No change required there. By contrast `github`/`coordination`/`sqids`/`reserved`/`ref_count_ceiling` are `#[serde(skip)]` and will be absent — out of scope for STORY-146, leave alone.
- **STORY-145 boundary in `config_write.rs`:** the writer's `update_type_table` (`config_write.rs:71`) and `update_rule_table` (`config_write.rs:331`) must learn to write the new axes (`lifecycle`) and the `require_parent_status` key. STORY-145 ideally adds these as it adds the fields. If STORY-145 lands the struct fields but NOT the writer branches, Changes 4 and 5 must add them — verify at build time and adjust scope.
- Mutators load the config strictly (`Config::load`), so they require an already-valid `.lazyspec.toml`; that is fine — `fix --config` remains the migration path for legacy configs.
