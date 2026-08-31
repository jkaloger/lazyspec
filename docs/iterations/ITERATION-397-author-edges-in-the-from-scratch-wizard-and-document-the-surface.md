---
title: Author edges in the from-scratch wizard, and document the surface
type: iteration
status: draft
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-261
---

## Objective

Both `init` designers hand back a config whose DAG loads: the from-scratch wizard prompts for edges where it used to prompt for parent-child rules, the starter designer stops leaving edges that name a type the user dropped, and the README documents the three new commands and the `[[edges]]` schema.

## Satisfies

STORY-261 AC7. Also closes the wizard gap ITERATION-383 §Out of scope recorded and deferred to this story ("between here and STORY-261 the wizard designs types and lifecycles but not the DAG"), which carries no AC on either story, and the dangling-edge hazard in Context, which carries none either. AC1, AC4 landed in ITERATION-392, AC2 in ITERATION-393, AC3 in ITERATION-394, AC5 in ITERATION-395, AC6 in ITERATION-396. Last slice on the story.

## Context

- Story + ACs: STORY-261
- What each position means and why `to` is a set: ADR-030 §Decision. `"*"` on any position: ADR-031 §Decision
- Touch:
  - `src/cli/init.rs:216-260` `design_config_from_scratch` -- ITERATION-383 removed the `while prompter.confirm("Add a parent-child rule", ...)` loop at `:248-251` and replaced it with nothing. The comment above it (`:244-246`) explains why rules were prompted only once two types existed and why endpoints came from the defined types; that reasoning transfers to edges intact
  - `src/cli/config.rs:550-601` `collect_parent_child_rule` and `:523-542` `parent_child_rule_name`, both removed by ITERATION-383. `collect_edge` is their replacement, and the dedup-guarded `-2`/`-3` name generator is the part worth reviving verbatim: nothing at load rejects two edges sharing a `name` (`src/engine/config.rs:1307-1345`), a hole recorded in ITERATION-388, worked around in ITERATION-390 and again in ITERATION-392
  - `src/cli/wizard.rs:8-32` `Prompter` -- `multi_select` (`:26-31`) is the affordance a `to` set needs: it renders an arrow-key multi-chooser over the declared type names and the scripted fake splits a queued line on `,`
  - `src/cli/init.rs:159-189` `design_config_interactive` -- the starter designer. It sets `config.documents.types = kept` (`:177`) and never touches `config.rules` or `config.edges`
  - `src/cli/init.rs:266-330` `render_dag_summary`
  - `README.md:286` (the `init` row), `:311-315` (the `config` table), `:438` (the wizard description), `:440` (the sources-of-truth sentence), `:469-476` (the migration section), `:483-513` (the config examples and the mutator paragraph), `:650-711` §"Edges"
- **The hazard ITERATION-383 introduced and nobody has caught.** The starter designer lets the user drop any starter type, and never prunes the declarations that name it. Today the survivors are `[[rules]]`, and strict load does not check a rule's type names at all -- `parse_inner` takes `raw.rules.unwrap_or_default()` (`src/engine/config.rs:1305`) and validates nothing -- so dropping the `story` type produces a config that loads with a dangling rule. Under `[[edges]]` the same answer produces a config that **fails to load**: `:1327-1336` bails on an edge naming a type absent from `[[types]]`. So after ITERATION-383, `init --template starter` plus one `no` writes a `.lazyspec.toml` that every subsequent command refuses. `write_project` (`src/cli/init.rs:78-100`) serialises through `to_toml` and never re-parses, so nothing catches it on the way out. Fix it by pruning: drop every edge whose `from` or `to` names a dropped type, and report the pruned rows in the summary so the user sees the DAG they actually chose. Pruning beats refusing the drop, because the drop is the feature.
- **`"*"` is a variant, not a member, and `multi_select` cannot say so.** `TypeSelector` is `Any | Types(Vec<String>)` (`config.rs:98-121`), so offering `*` in the same list as the type names lets the user check `*` and `story` together -- a selection with no representation. Validate at the callsite and re-ask, in the idiom `collect_parent_child_rule` used for an unknown type name (`config.rs:561-569`), rather than teaching `Prompter` about exclusivity. ITERATION-391 makes the same call for the panel's picker and ITERATION-392 for the CLI's repeated `--to`; this is the third surface and the rule must be the one engine-side constructor ITERATION-392 added, not a third reading of it.
- **What the wizard must not prompt for.** `traversal` and `required` are the two fields a new user has no basis to answer, and both have a safe absence: no `required` means "legal, not demanded" (RFC-067 §Design) and no `traversal` means the edge does not walk. Prompt for them with unset as the default, and put `chain` in front of the user only once at least one edge exists -- a from-scratch project whose every edge has no traversal has a `context` of one document, which is the `blank`-path regression ITERATION-396 §Context recorded and this loop is the place it gets fixed.
- The from-scratch designer gates the loop on two or more types, for the same reason the rules loop did: an edge's endpoints are chosen from the defined types, so no edge can dangle. `to = "*"` is the one exception -- it names no type and is legal with one type declared. Decide whether to offer the loop at one type and say which.

## Tasks

1. Test-first in `src/cli/init.rs`'s test module: a scripted from-scratch session that defines two types and one edge returns a config whose `edges` carries that row, and `Config::parse(&config.to_toml()?)` is `Ok`. Then a scripted session that adds two edges asserts their names differ.
2. Add `collect_edge` beside the surviving collectors in `src/cli/config.rs`, reviving the dedup-guarded name generator, and wire the loop into `design_config_from_scratch` where the rules loop was. Reuse ITERATION-392's `TypeSelector` constructor for the `to` answer.
3. Test-first the hazard from Context: a scripted starter session answering `no` to "Keep type story" returns a config with no edge naming `story`, and the config strict-loads. Assert `Config::parse` on the rendered TOML, not just that the types list shrank -- the whole failure is that the types list shrinking is exactly what breaks it.
4. Add the prune to `design_config_interactive` and report the pruned rows through `render_dag_summary`. It renders for the from-scratch path only today (`:254`); decide whether the starter path renders a summary too, since a prune the user is not shown is a silent DAG edit.
5. Re-base every scripted answer list in the `init` test module. ITERATION-383 re-based them once by removing a prompt and this slice adds prompts back at a different position; a wrong-length script feeds an answer into the wrong question and still passes, so assert on the resulting config, not on `is_ok()`.
6. Test the `"*"` exclusivity re-ask: a queued `*,story` re-asks and a queued `*` yields `Any`.
7. README, AC7's whole content: the three `config` rows in the table at `:311-315`; `add-edge` / `set-edge` / `remove-edge` examples and the mutator-constraints paragraph at `:508`; the wizard description at `:438`, which still says the from-scratch designer prompts for "parent-child rules"; and §"Edges" at `:650-711` -- `traversal` documented as a row key, the wildcard specificity rule, and the two closing sentences at `:711` corrected, since "per-edge `traversal` is not supported yet" and "`[[edges]]` and `[[rules]]` are enforced independently" are both false by this point.

## Out of scope

- An edge loop in the starter designer. It prunes and does not add: the starter set is already five rows (ITERATION-396) and a user who wants a sixth has `config add-edge`. Recorded because the from-scratch path gets a loop and the asymmetry is deliberate.
- Giving the loader a duplicate-name check, or a check on an edge naming a dropped type at *render* time. `write_project` writing an unloadable config is a general hole -- ITERATION-395's guard covers the `config` mutators and not `init`, because `init` builds a whole `Config` rather than editing one. Worth closing; no AC.
- Prompting for `github_native`, `inverse`, or anything else on `[[relationships]]`. The wizard has never designed the relationship vocabulary and this story does not start.
- The JSON schema. `config schema` derives from the parser (`README.md:443`), so `EdgeDef` documents itself there and nothing in this slice touches it.
- The TUI settings panel -> STORY-260.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 6: `collect_edge` reuses `Prompter::multi_select` and ITERATION-392's selector constructor; a wizard-only selector parser would be the third copy. The project instruction that a CLI change updates the README is what AC7 is, and `writing-reference-docs` governs the §"Edges" rewrite -- it is a schema reference, not a narrative.

## Verification

`lazyspec init` on a TTY, choose `blank`, define two types, add one edge, write: `lazyspec validate` is clean and `context` walks the edge. Then `lazyspec init --template starter` in another empty directory, answer `no` to keeping `story`, write: `lazyspec validate` is clean and no `[[edges]]` block names `story`.
