---
title: Config axes and status newtype data model
type: iteration
status: complete
author: agent
date: 2026-06-21
tags: []
related:
- implements: STORY-145
---

## Changes

Group A of STORY-145: the DATA MODEL only. Covers AC1 (parse `intent`/`authorship`/`lifecycle`), AC2 (`authorship` defaults Assisted), AC3 (status validated against the type's lifecycle states). NO transition enforcement, NO `require_parent_status`, NO `fix --config` migration — those are ITERATION-197.

Design grounded in RFC-048, ADR-021 (per-type inline status DAG), ADR-023 (Status as validated newtype), ADR-010 (the `RelationType` newtype pattern this mirrors).

### Task 1 — Add `Authorship` enum and `Lifecycle` struct to `src/engine/config.rs` (AC1, AC2)

File: `src/engine/config.rs`.

Add a new enum mirroring the existing `NumberingStrategy` shape (`src/engine/config.rs:34-41`), placed near the other config enums (above `TypeDef`, ~line 132):

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Authorship {
    Human,
    #[default]
    Assisted,
    Generated,
}
```

The `#[default]` on `Assisted` is what delivers AC2 — combined with `#[serde(default)]` on the `TypeDef` field (Task 2), an absent `authorship` key resolves to `Assisted`.

Add the `Lifecycle` struct. `states` is a node list; `edges` is a directed transition list. ADR-021 permits `*` as an edge source (wildcard, e.g. `* -> superseded`); for THIS iteration `*` is stored verbatim as a string and carried through — no edge-traversal logic exists yet (that is 197). Represent an edge as a 2-tuple `(from, to)`; TOML has no tuple literal, so model the wire form as a struct with `from`/`to` fields and expose `edges` as `Vec<Edge>` (cleaner than `Vec<(String, String)>` for TOML and serde; the design intent's `Vec<(String,String)>` is satisfied semantically):

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Edge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Lifecycle {
    #[serde(default)]
    pub states: Vec<String>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}
```

`Default` (empty states + edges) lets a type omit `lifecycle` without a parse error; whether an empty lifecycle is later legal is a 197 concern, out of scope here.

Verification: `cargo build` compiles. The structs derive `Serialize`/`Deserialize`/`PartialEq` consistent with sibling config types.

### Task 2 — Add the three new fields to `TypeDef` (AC1, AC2)

File: `src/engine/config.rs`, struct `TypeDef` at `src/engine/config.rs:133-152`.

Add three fields, following the `#[serde(default)]` convention used by every optional `TypeDef` field (e.g. `parent_type`, `agents` at lines 149-151):

```rust
    #[serde(default)]
    pub intent: Option<String>,
    #[serde(default)]
    pub authorship: Authorship,
    #[serde(default)]
    pub lifecycle: Lifecycle,
```

- `intent: Option<String>` — one-line "why this type exists"; absent -> `None`.
- `authorship: Authorship` — `#[serde(default)]` + the enum's `#[default]` (Task 1) gives the Assisted default (AC2).
- `lifecycle: Lifecycle` — `#[serde(default)]` -> empty `Lifecycle` when absent.

Because `TypeDef` is deserialized through `RawConfig` (`raw.types`, `src/engine/config.rs:359` / `523-528`) and NOT via a hand-written path, the serde `default` attributes are sufficient for parsing — no change to `parse_inner` is required for the new fields to load. (Contrast: `relationships`/`rules` use `skip_deserializing` and are pulled from `RawConfig` manually; `types` is not, so the derive does the work.)

Then fix every other construction site of `TypeDef` so the project still compiles (these are exhaustive struct literals, so each must add the three fields):
- `starter_types()` closure `simple` and the two literal blocks — `src/engine/config.rs:380-433` (Task 4 details the starter VALUES).
- `TypeDef::test_fixture` — `src/engine/config.rs:716-730`: add `intent: None, authorship: Authorship::default(), lifecycle: Lifecycle::default()`.
- `tests/integration/cli_create_test.rs:5-19` `singleton_type` helper: same three defaults.

Verification: `cargo build` and `cargo test --no-run` compile; no "missing field" errors.

### Task 3 — Convert `Status` from a closed enum to a validated string newtype (AC3)

File: `src/engine/document.rs`. Mirror the `RelationType` pattern (`src/engine/document.rs:115-153`) exactly — that is the established ADR-010 model.

Replace the `Status` enum (`src/engine/document.rs:88-99`), its `Display` impl (`101-113`), and its `FromStr` impl (`162-176`) with a newtype:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct Status(String);

impl Status {
    pub fn new(s: &str) -> Self { Status(s.to_lowercase()) }
    pub fn as_str(&self) -> &str { &self.0 }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

impl<'de> Deserialize<'de> for Status {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where D: serde::Deserializer<'de> {
        let s = String::deserialize(deserializer)?;
        Ok(Status(s.to_lowercase()))
    }
}

impl std::str::FromStr for Status {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> { Ok(Status::new(s)) }
}
```

Key point (matches `RelationType`): `FromStr`/`Deserialize` are PURE — any string parses. Validation against the lifecycle is a separate, explicit step (Task 3b), NOT done in deserialization. This is required because `RawFrontmatter` (`src/engine/document.rs:200-216`) deserializes a `Status` with no access to the config/type.

This is a large blast radius — every `Status::Variant` match site must be rewritten. The conversion is mechanical; the affected sites (verified):
- `src/cli/style.rs:7-20` (`status_style`) and `src/tui/views/colors.rs:5-15` (`status_color`): both `match` on the seven variants. Rewrite to match on `status.as_str()` with the seven string literals (`"draft"`, `"review"`, `"accepted"`, `"in-progress"`, `"complete"`, `"rejected"`, `"superseded"`) and a `_ =>` arm for unknown user-defined statuses (default style/color). Add the `_` arm — it did not exist for the closed enum and is now required.
- `src/engine/validation.rs` (`339`, `347`, `364-365`, `507`, `517`, `536`): comparisons like `parent_doc.status == Status::Rejected`. Rewrite as `parent_doc.status == Status::new("rejected")` (or compare `.as_str()`). Behaviour is unchanged for the starter statuses.
- `src/engine/store/loader.rs` (`146`, `148`): constructs `Status::Accepted` / `Status::Draft` for virtual docs -> `Status::new("accepted")` / `Status::new("draft")`.
- `src/engine/document.rs:348` test helper `make_doc` uses `Status::Draft` -> `Status::new("draft")`.
- Any other site surfaced by the build (`grep -rn "Status::" src/ tests/` after the edit). The git-ref / issue-body / issue-cache stores reference status as a string already and should be unaffected, but compile-check confirms.

#### Task 3b — Lifecycle-membership validation helper (AC3)

Add a validation function that accepts a `Status` iff it names one of the owning type's lifecycle states. Two choices; prefer (a) for testability and to keep `document.rs` config-free:

(a) Method on `TypeDef` in `src/engine/config.rs` (engine, no I/O — consistent with the file-as-module / no-I/O convention):
```rust
impl TypeDef {
    /// True iff `status` names one of this type's declared lifecycle states.
    pub fn accepts_status(&self, status: &Status) -> bool {
        self.lifecycle.states.iter().any(|s| s == status.as_str())
    }
}
```
(`config.rs` would need to import `Status` from `document.rs`; that dependency direction already exists elsewhere in the engine.)

Then expose a `Result`-returning validator (for the load path / `update --status` to call) that bails naming the type, the rejected status, and the legal set — mirroring `resolve_relationship`'s error style (`src/engine/config.rs:701-704`):
```rust
pub fn validate_status(type_def: &TypeDef, status: &Status) -> anyhow::Result<()> {
    if type_def.accepts_status(status) { return Ok(()); }
    anyhow::bail!(
        "status \"{}\" is not a valid state for type \"{}\" (allowed: {})",
        status, type_def.name, type_def.lifecycle.states.join(", ")
    )
}
```

Scope boundary: this iteration only ADDS the validator and proves it accepts/rejects (AC3 tests, Task below). WIRING it into the document load path and `update --status` enforcement of transitions is ITERATION-197. If a minimal call site is needed to satisfy AC3 "a document of that type is loaded ... is rejected", call `validate_status` from the existing per-doc load in `src/engine/store/loader.rs` (around the `DocMeta::parse` at `loader.rs:54`) where the owning `TypeDef` is resolvable — but keep transition-edge logic out. If load-path wiring proves to entangle with 197's transition work, satisfy AC3 at the helper level (unit tests on `validate_status`) and record the deferral in Notes. Decide during build; do not pull 197 scope in.

Verification: `cargo build`; targeted `grep -rn "Status::" src/ tests/` returns only `Status::new(...)` calls; full suite compiles.

### Task 4 — Default lifecycle in starter config + `to_toml` round-trip (AC1)

File: `src/engine/config.rs`, `starter_types()` at `src/engine/config.rs:380-433`.

The current seven statuses become the starter default lifecycle (ADR-023). Define one shared default `Lifecycle` and attach it to the default types so their `[[types]]` blocks ship a valid lifecycle. Add a helper near `starter_relationships()` (`src/engine/config.rs:169`):

```rust
fn default_lifecycle() -> Lifecycle {
    let edge = |from: &str, to: &str| Edge { from: from.into(), to: to.into() };
    Lifecycle {
        states: ["draft", "review", "accepted", "in-progress", "complete", "rejected", "superseded"]
            .iter().map(|s| s.to_string()).collect(),
        edges: vec![
            edge("draft", "review"),
            edge("review", "accepted"),
            edge("review", "rejected"),
            edge("accepted", "in-progress"),
            edge("in-progress", "complete"),
            edge("*", "superseded"),
        ],
    }
}
```

The exact edge set is "sensible edges" per STORY-145/ADR-023; the `states` set must be exactly the prior seven so existing documents' statuses remain valid (AC3, and 197's migration). Note `*` is carried as a literal source string only — no traversal this iteration.

Wire `default_lifecycle()` (and `intent`/`authorship` defaults) into every `TypeDef` produced by `starter_types()`:
- `simple` closure (`src/engine/config.rs:381-393`): add `intent: None` (or a short per-type string if trivially available — `None` is acceptable for this slice), `authorship: Authorship::default()`, `lifecycle: default_lifecycle()`.
- the `convention` and `dictum` literal blocks (`406-431`): same three fields.

Round-trip: `Config::to_toml` (`src/engine/config.rs:655-657`) serializes via the derives. Because `TypeDef`, `Lifecycle`, and `Edge` all derive `Serialize`, the lifecycle is emitted under each `[[types]]` block (TOML renders `Vec<Edge>` as a `[[types.lifecycle.edges]]` array-of-tables and `states` as an inline array). Confirm a parse->to_toml->parse round-trip preserves `states` and `edges` (Test Plan).

Verification: `cargo test` for the new round-trip test passes; `Config::default()` (`src/engine/config.rs:462-489`) still builds (it calls `starter_types()`, so no separate edit there).

## Test Plan

No production sequencing here — tests only. Follow the existing `config_test.rs` style: build a `[[types]]` + `[[relationships]]` TOML string, `Config::parse`, assert on the parsed `TypeDef`. Reuse the `RELATIONSHIPS` const pattern (`tests/integration/config_test.rs:9-16`) so strict load is satisfied. Unit tests for `Status` go in `src/engine/document.rs#tests` alongside the `RelationType` tests (`document.rs:550-561`); config-axis tests in `tests/integration/config_test.rs`.

### AC1 — intent / authorship / lifecycle parse and are readable

- `type_with_all_three_axes_parses`: a `[[types]]` block declaring `intent = "..."`, `authorship = "human"`, and a `[lifecycle]` with `states = [...]` and `[[types.lifecycle.edges]]` entries. Assert `intent == Some(...)`, `authorship == Authorship::Human`, `lifecycle.states` and `lifecycle.edges` match what was declared (including a `from = "*"` edge to prove `*` is carried verbatim). One test, three assertions — exercises all three axes loading together as AC1 states.
- `to_toml_round_trips_lifecycle`: `Config::default()` (or a parsed config) -> `to_toml()` -> `Config::parse()`; assert a default type's `lifecycle.states` and `lifecycle.edges` survive unchanged. Proves the starter default lifecycle (Task 4) serializes and re-parses.

### AC2 — authorship defaults to Assisted

- `authorship_defaults_to_assisted_when_absent`: a `[[types]]` block with NO `authorship` key. Assert `type.authorship == Authorship::Assisted`. Mirror the `singleton_field_defaults_to_false` test shape (`config_test.rs:580-592`).
- `authorship_parses_each_variant` (optional, low cost): assert `"human"`/`"assisted"`/`"generated"` parse to the three variants — guards the `rename_all = "lowercase"` wiring.

### AC3 — status accepted iff in the type's lifecycle states

Two layers; pick per Task 3b's decision.

- Newtype purity (always): `status_newtype_fromstr_is_pure_and_lowercases` in `document.rs#tests` — mirror `relation_type_fromstr_is_pure_and_never_errors` (`document.rs:556-561`): `"In-Progress".parse::<Status>()` is `Ok`, displays `in-progress`; any arbitrary string parses. Proves validation is NOT in `FromStr`.
- Membership accept/reject (the AC): `status_in_lifecycle_states_is_accepted` and `status_outside_lifecycle_states_is_rejected` — build a `TypeDef` (via `test_fixture` + a `lifecycle` with a known `states` set, or parse a config), then assert `validate_status(&type_def, &Status::new("review"))` is `Ok` and `validate_status(&type_def, &Status::new("frozen"))` is `Err` whose message names the offending status and the type. Mirror `resolve_relationship`'s unknown-keyword assertion (`config.rs:1351-1352`).
- If load-path wiring is done (Task 3b option), add an integration test: a document whose `status:` frontmatter names a state outside its type's lifecycle fails to load with an error naming the status; a document with an in-set status loads. If wiring is deferred to 197, OMIT this and record the deferral in Notes — the helper tests still satisfy AC3 at the data-model layer.

Test-properties tradeoff: AC3's strongest test is the load-path integration test (behavioural, end-to-end), but it risks reaching into 197's enforcement wiring. The helper-level tests (`validate_status` accept/reject) are isolated and fast but test the unit, not the user-visible reject. Prefer the helper tests as the AC3 floor; add the integration test only if Task 3b wires the load path without pulling transition logic in.

## Notes

This iteration is the FOUNDATION (STORY-145 group A, the data model). It deliberately ships the three config axes, the `Status` newtype, and the starter default lifecycle, and proves they parse / default / validate — but wires NO enforcement.

ITERATION-197 depends directly on what this lands and owns the behaviour:
- `update --status` edge-transition rejection (consuming `lifecycle.edges` and the `*` wildcard source carried here as literal strings).
- `require_parent_status` on the parent-child rule (field + parse + `create`-gate enforcement) — entirely 197.
- `fix --config` migration that injects `default_lifecycle()` into pre-existing configs lacking one. The migration reuses the `default_lifecycle()` helper added in Task 4, so keep it `pub(crate)` if 197 needs it cross-module.

Status-newtype blast radius (flag for 197 and any concurrent work): converting `Status` from a closed enum to an open newtype touches every construction/match site. Verified sites that 197/others must keep in mind:
- serialize/parse/display: `src/engine/document.rs` (newtype itself), `RawFrontmatter` deserialization.
- TUI color/style: `src/tui/views/colors.rs:5` and `src/cli/style.rs:7` — both gain a `_ =>` fallback arm; user-defined statuses outside the starter seven render with a default color/style until per-status config (out of RFC-048 scope) exists.
- validation comparisons: `src/engine/validation.rs` (six sites) and `src/engine/store/loader.rs` — now `== Status::new("...")`; semantically identical for the starter statuses but no longer compiler-exhaustive, so a typo'd status string would silently never match. 197's transition work should prefer the lifecycle/edges as the source of truth over hard-coded status-string comparisons where possible.
- `list --status` / `update --status` (`src/cli.rs:96-125`): take `Option<String>` already and parse via `FromStr`, which stays pure — they will accept any string now; rejection of unknown statuses must come from the lifecycle validator (this iteration's `validate_status`), not from `FromStr`. 197 owns enforcing that on `update`.

Decision recorded above (Task 3b): if load-path status validation entangles with 197's enforcement, AC3 is satisfied at the `validate_status` helper level and load-path wiring defers to 197. State which path was taken in the build PR.

### Build outcome (Task 3b path taken)

Took the **helper-level path**: AC3 is satisfied by unit/integration tests on `validate_status` (`status_in_lifecycle_states_is_accepted` / `status_outside_lifecycle_states_is_rejected`). Load-path wiring (calling `validate_status` from `loader.rs`) was DEFERRED to ITERATION-197 — wiring it now would require deciding rejection-vs-warning semantics and resolving the owning `TypeDef` mid-load, which entangles with 197's transition enforcement. No load-path integration test was added (per the Test Plan's deferral instruction).

Tests added (all pass):
- `src/engine/document.rs#tests::status_newtype_fromstr_is_pure_and_lowercases`
- `tests/integration/config_test.rs`: `type_with_all_three_axes_parses`, `to_toml_round_trips_lifecycle`, `authorship_defaults_to_assisted_when_absent`, `authorship_parses_each_variant`, `status_in_lifecycle_states_is_accepted`, `status_outside_lifecycle_states_is_rejected`

Deviations from the iteration:
- `default_lifecycle()` made `pub(crate)` (not private) — used cross-module by a new-type creation site in the TUI settings screen (`src/tui/state/app.rs`) that the iteration's site list pre-dated. The Notes already anticipated `pub(crate)` for 197's migration, so this is consistent.
- Status-newtype blast radius was larger than the iteration's verified list: additional exhaustive `TypeDef { .. }` and `Status::Variant` construction sites existed in `src/tui/state/app.rs` (settings screen, status-picker index/cycle fns), `src/engine/store.rs`, `src/engine/store_dispatch.rs`, `src/engine/config_write.rs`, `src/engine/issue_cache.rs`, `src/cli/lease.rs`, `src/cli/link.rs`, and several integration test files. All rewritten mechanically (`Status::new("..")`, `.as_str()` matches with `_` arms, three new `TypeDef` field defaults). `cargo build` clean, `cargo test --lib` (786) and `cargo test --test integration` (721) all green.
