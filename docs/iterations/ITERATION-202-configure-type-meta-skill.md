---
title: Configure-type meta-skill
type: iteration
status: draft
author: agent
date: 2026-06-21
tags: []
related:
- implements: STORY-149
---

## Changes

One iteration, all five ACs of STORY-149. Deliverable is a single prose SKILL — no
Rust, no tests. Spine: engine ships no methodology (ADR-011); a grill-me-style
meta-skill co-authors one type's methodology, recording it as an enriched template
plus a `[[types]]` block written through the config-write CLI (ADR-019). v1 = one
type per run. Depends on the CLI from ITERATION-198 and the template format from
ITERATION-201 — assume both exist; this iteration only calls them.

1. **Author `skills/configure-type/SKILL.md` — the skill scaffold + frontmatter.** [AC1, AC5]
   - Create the file at `skills/configure-type/SKILL.md` (one dir, one file — mirrors
     `skills/create-story/`, `skills/write-rfc/`).
   - YAML frontmatter, matching the canonical two-field shape used by every existing skill:
     ```yaml
     ---
     name: configure-type
     description: Use when adding a new custom document type to a lazyspec project. Runs a grill-me-style interview to co-author the type's methodology — intent, authorship, lifecycle, gates, relations — then writes its enriched template and `[[types]]` config via the config-write CLI. One type per run.
     ---
     ```
   - Open the body with the one-type-per-run contract: the skill configures exactly
     ONE type per invocation; more types means running `/configure-type` again. State
     this up front so the interview stays scoped (AC5).

2. **Write the interview script — one question at a time, recommended answer each.** [AC1]
   - Mirror the grill-me pattern verbatim in spirit: "Ask the questions one at a time.
     For each question, provide your recommended answer. If a question can be answered
     by exploring the project (existing types, `lazyspec config --json`), explore
     instead of asking." Walk the axes as a decision tree, resolving dependencies
     between them one-by-one.
   - Elicit these axes, in this order (each its own question, each with a recommendation):
     1. **name** — the type's identifier (and plural / dir / prefix that fall out of it).
        Recommend a name; reject collisions by first reading `config --json`.
     2. **intent** — one line: what this document type is FOR. Drives the template's
        `<!-- intent: ... -->` header.
     3. **authorship** — one of `human` / `assisted` / `generated`. Recommend based on intent.
     4. **lifecycle** — the per-type status DAG: `states` (e.g. draft → done) and `edges`
        (`FROM:TO`, `*` allowed as source). Recommend a minimal DAG; per ADR-021 each
        type declares its own.
     5. **status gates** — `require_parent_status`: when must the parent sit at a given
        status before a child of this type may exist/advance. Ask only if the type has a
        parent relation.
     6. **relations to existing types** — which existing types this one links to and how
        (e.g. `implements`, `related-to`). Read `config --json` to enumerate candidates.
   - The interview ends when every axis is resolved and the user confirms the summary.

3. **Produce the enriched per-type template (STORY-148 / ITERATION-201 format).** [AC2]
   - After the interview, write the type's template file to the project's templates dir:
     `.lazyspec/templates/{name}.md` (the configured `[templates].dir`, one file per type).
   - Emit the enriched format from ITERATION-201: an `<!-- intent: ... -->` header (the
     elicited intent) at the top, the standard `{title}/{author}/{date}/{type}` frontmatter
     placeholders, and one `## Section` per logical part of the doc, each followed by a
     `<!-- guidance: ... -->` comment describing what belongs there. Sections come from the
     interview (the skill asks what sections this type needs, recommending defaults).
   - The comments render invisibly; they are the methodology the user supplied, recorded as prose.

4. **Write the config via the config-write CLI — never hand-edit TOML.** [AC3]
   - The skill writes ALL config through the ITERATION-198 subcommands, in this order:
     - `lazyspec config add-type <name> <plural> <dir> <prefix> --intent "<intent>" --authorship <human|assisted|generated>` (plus `--parent-type` / `--icon` etc. when elicited) — appends the `[[types]]` block.
     - `lazyspec config set-lifecycle <name> --state <s> [--state <s> …] --edge <FROM:TO> [--edge … ]` — sets the per-type DAG.
     - `lazyspec config add-gate <rule-name> --status <required-parent-status>` — sets `require_parent_status` (only when a gate was elicited).
     - Relations to existing types are added through the same `config` mutation surface (the parent/relationship args of `add-type`, or the dedicated relation mutator if present at build time).
   - State explicitly, as a NEVER rule, that the skill MUST NOT open or hand-edit
     `.lazyspec.toml` — all writes go through `config …`, which preserves comments and
     formatting (same guarantee as TUI settings / `fix --config`).

5. **Verify and close the loop with `config --json`.** [AC4, AC5]
   - After writing, the skill runs `lazyspec config --json` and confirms the new type
     appears with its `intent`, `authorship`, `lifecycle` (states + edges), and gates
     populated as supplied. Use `jq '.types[] | select(.name=="<name>")'` to show the
     user the result.
   - Add a verification checklist to the skill (mirroring the existing skills' close-out
     sections): template file written; type present in `config --json` with all axes;
     no direct TOML edit occurred; exactly one type configured. Reiterate AC5: to add
     another type, run `/configure-type` again.

6. **Register the skill in `skills/README.md`.** [all]
   - Add a `configure-type` row to the Reference table and a one-line mention that it
     runs independently of the main `plan-work → … → review-iteration` pipeline (like
     `create-audit`), since configuring a type is a setup/meta action, not a lifecycle step.

## Test Plan

The deliverable is a prose SKILL — there is no code, so nothing here is unit-testable.
Verification is by reading the authored `skills/configure-type/SKILL.md` and confirming
it instructs the agent to do the right things, end-to-end:

- **AC1 — interview shape.** The skill script asks one question at a time, gives a
  recommended answer for each, and elicits all six axes (name, intent, authorship,
  lifecycle states+edges, gates, relations). Confirm it tells the agent to explore
  `config --json` / existing types rather than ask when the answer is discoverable.
- **AC2 — template produced.** The skill writes `.lazyspec/templates/{name}.md` in the
  ITERATION-201 enriched format: an `<!-- intent: ... -->` header and a
  `<!-- guidance: ... -->` comment per section.
- **AC3 — CLI-only writes.** The skill writes config exclusively via `config add-type`,
  `config set-lifecycle`, and `config add-gate`, and carries an explicit NEVER rule
  against hand-editing `.lazyspec.toml`.
- **AC4 — config reflects the type.** The skill closes by running `lazyspec config --json`
  and confirming the new type shows its intent, authorship, lifecycle, and gates.
- **AC5 — one type per run.** The skill states the one-type-per-run contract at the top
  and in the close-out, directing the user to re-run for additional types.

End-to-end smoke (manual, once ITERATION-198 + ITERATION-201 land): invoke
`/configure-type`, answer the interview for a throwaway type, then confirm (a) a
template file exists, (b) `config --json` shows the type with every axis populated, and
(c) `.lazyspec.toml`'s existing comments/order are intact (proving the CLI did the write).

## Notes

- **Hard dependency on ITERATION-198 + ITERATION-201.** This skill only calls them: the
  config-write CLI (`config add-type` / `set-lifecycle` / `add-gate` / `--json`) and the
  enriched per-type template format. Both are TODO stubs at authoring time — do NOT plan
  or build them here; if their surface shifts during build, align the skill's commands
  and template shape to match what actually landed.
- **Why a skill, not engine logic.** Per ADR-011 the engine ships no methodology for an
  arbitrary type, and per ADR-019 the binary owns data while the skill owns prose. So the
  knowledge comes from the user via a grill-me-style interview; the skill merely records
  it. This is deliberate — there is nothing to "compute".
- **Per-type DAGs.** Each configured type carries its own inline `states`/`edges`
  (ADR-021); the interview elicits a DAG per type, not a shared global lifecycle.
- **Out of scope — whole-DAG bootstrap.** v1 configures exactly one type per run. A
  guided bootstrap that stands up an entire multi-type ontology (all types + relations +
  gates) in a single pass is explicitly future work, not planned here.
