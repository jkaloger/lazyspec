---
title: Generic verb skill set
type: iteration
status: complete
author: agent
date: 2026-06-21
tags: []
related:
- implements: STORY-147
---

## Changes

This iteration authors **skill markdown only** -- the prose half of RFC-048's "binary owns data, skill owns prose" spine. No Rust, no test code. Each verb is a portable `SKILL.md` that reads the DAG from `config --json` / `status --json` / `context --json` at runtime and acts on a **named type passed as a parameter**, never a baked DAG-specific type name. Eligibility, authorship ceiling, and gate logic live entirely in this prose (ADR-019: no routing brain command in the binary).

Shared conventions every file below MUST follow (match the existing `skills/*/SKILL.md`):
- Frontmatter is two fields only: `name` and `description` (description starts "Use when…").
- Open with an all-caps slogan in a fenced block, a one-line restatement, a `<HARD-GATE>` block, a `<NEVER>` block (reuse the three standard bullets: don't write doc files directly, don't edit unread docs, don't skip the pipeline), the verbatim `<GITHUB-ISSUES-DOCUMENTS>` block, and the CLI-discipline line ("Always run `lazyspec help <subcommand>` … Always pass `--json` …").
- Invoke the bare `lazyspec` binary (on PATH), always with `--json` for machine reads.
- Read DAG state through the CLI only: never read `.lazyspec/` graph files directly.
- The type is **always a parameter** read from config; no prose names `rfc`/`story`/`iteration` as if fixed. Where examples help, mark them explicitly as examples drawn from the default config.

---

1. **Author `skills/scaffold/SKILL.md`** (AC1, AC2).
   - Frontmatter `name: scaffold`; description: "Use when creating a new document of a configured type at the most manual authorship level -- AI creates the file and frontmatter, surfaces intent and section guidance, then hands the body back to the human."
   - Preflight: `lazyspec config --json` to read the target type's `intent`, `authorship` ceiling, and template guidance; `lazyspec status --json` / `lazyspec context --json` to locate the user and find the parent to link to.
   - Behavior: `lazyspec create <type> "<title>" --author <name>`, then `lazyspec link <new> implements <parent>` (or the configured relation) where a parent exists. Surface the type's `intent` and per-section `<!-- guidance -->` to the human; do NOT write the body. Prose reads `<type>` from config; carries zero baked type names.
   - Ceiling: scaffold is the floor of the `scaffold < co-write < generate` order, so it is allowed on every `authorship` value (`human`, `assisted`, `generated`). State this explicitly: scaffold never refuses on ceiling grounds.

2. **Author `skills/co-write/SKILL.md`** (AC1, AC2).
   - Frontmatter `name: co-write`; description: "Use when drafting a document of a configured type collaboratively -- AI proposes a draft body, the human edits, iterate -- up to the type's authorship ceiling."
   - Preflight identical to scaffold: read type `intent`/`authorship`/guidance from `lazyspec config --json`; locate parent via `status`/`context --json`.
   - Behavior: scaffold-then-propose. Create + link as in task 1, then write a proposed body toward `intent` and section guidance and present it for human edits; iterate. Use `lazyspec update <id> --body-file <path>` to apply an accepted draft.
   - Ceiling: co-write is the middle rung. Allowed when the type's `authorship` is `assisted` or `generated`. **Refuse when `authorship` is `human`**, and report the ceiling: "type `<type>` is human-authored (ceiling = scaffold); drop to /scaffold." Read the ceiling from `config --json`; do not hardcode which types are human.

3. **Author `skills/generate/SKILL.md`** (AC1, AC2 -- primary ceiling-refusal case).
   - Frontmatter `name: generate`; description: "Use when authoring a full document body of a configured type from context -- AI writes the complete body, then asks for review -- only permitted when the type's authorship ceiling is `generated`."
   - Preflight: `lazyspec config --json` for `intent`/`authorship`/guidance; `lazyspec context --json` to assemble source material (parent docs, related docs, referenced code via `lazyspec show -e <id>`).
   - Behavior: create + link, write the full body from gathered context toward `intent`, apply via `lazyspec update <id> --body-file <path>`, then request review (route to /review).
   - Ceiling: generate is the top rung. **Allowed only when `authorship` is `generated`. Refuse for `human` and `assisted`**, reporting the ceiling and the permitted verb (e.g. "type `<type>` ceiling = co-write; drop to /co-write"). This is the headline AC2 case. The refusal text reads the ceiling string out of `config --json`; no baked type-to-ceiling table.

4. **Author `skills/advance/SKILL.md`** (AC1, AC3 support).
   - Frontmatter `name: advance`; description: "Use when moving a document to its next status along the type's lifecycle DAG, maintaining links and checking gates at the transition."
   - Preflight: `lazyspec config --json` to read the type's `lifecycle` (states + edges); `lazyspec show <id> --json` for the document's current status; `lazyspec context --json` for parent/child state relevant to gates.
   - Behavior: compute the eligible next status from `lifecycle.edges` (the prose derives the next move from the edge set -- no baked status names). Apply with `lazyspec update <id> --status <next>`; the binary rejects any off-edge transition, so the skill proposes only declared edges. Maintain configured relations on transition.
   - Gates: `advance` is the point where status-conditioned gates matter (ADR-022). When a transition would make a child type creatable (a `require_parent_status` gate clears), advance does the status move ONLY and STOPS; it never spawns the child. State that crossing into a child type is a human-initiated step handled by /lazy's stop-at-boundary rule.

5. **Author `skills/execute/SKILL.md`** (AC1).
   - Frontmatter `name: execute`; description: "Use when carrying out the work a delivery document describes -- the build loop -- against its task breakdown and acceptance criteria."
   - Preflight: `lazyspec config --json` to confirm the type is a delivery type in this DAG; `lazyspec show <id> --json` for the task breakdown / ACs; `lazyspec context --json` for the full chain; `lazyspec convention --json` for codebase conventions.
   - Behavior: iterate the document's tasks, doing the implementation work, verifying against ACs. This is the generalization of today's `build`. Keep the per-task discipline (scoped verification per task, full check once at the end) but express it over "the delivery document's tasks", not over a baked iteration type. On completion, route to /review and then to /advance for the status move.
   - No ceiling concept (execute is work, not authoring); note that explicitly so it is not confused with the authoring trio.

6. **Author `skills/review/SKILL.md`** (AC1).
   - Frontmatter `name: review`; description: "Use when critiquing a document against its intent and acceptance criteria, or reviewing completed work, before advancing status."
   - Preflight: `lazyspec config --json` for the type's `intent` and lifecycle (to know what status review precedes); `lazyspec show <id> --json` for the document and its ACs; `lazyspec context --json` for the chain.
   - Behavior: two-stage critique generalized from today's `review-iteration` -- conformance to intent/ACs first, quality second; block on conformance failure. Express targets generically ("the document's acceptance criteria", "its declared intent"), no baked type names. On pass, route to /advance to move status; on fail, route back to the appropriate authoring verb.

7. **Author `skills/lazy/SKILL.md`** -- the entry router (AC1, AC3 -- primary).
   - Frontmatter `name: lazy`; description: "Use as the entry point for any lazyspec work. Reads the configured DAG and the user's position, then dispatches the right verb -- advancing within the current document automatically but stopping at type boundaries."
   - Preflight (the routing read): `lazyspec config --json` (the full DAG: types, relations, rules, `authorship`, `lifecycle`, gates), `lazyspec status --json` (what exists and each doc's status), `lazyspec context --json` (the chain around the user's current document). This is the resolve-context fold-in: `/lazy` reads context from the CLI rather than calling a separate skill.
   - Locate-in-DAG: from config + status + context, determine which document/type the user is on and what its lifecycle position is.
   - Within-document progression flows automatically: if the current document is eligible to advance status (an outgoing `lifecycle` edge exists and its gate, if any, is met), dispatch the matching verb (/advance, or an authoring/execute/review verb appropriate to the type's `authorship` and status) WITHOUT asking.
   - **Stop-at-type-boundary (AC3 core):** when the only remaining next step would create a child of a **different type** (i.e. cross a `parent_type` edge), `/lazy` STOPS. It reports the boundary and what the human can do next; it never auto-runs `create <child-type>`. This holds even when a `require_parent_status` gate is already satisfied -- gate-clear makes the child *eligible*, not *automatic*. Crossing is always human-initiated.
   - Dispatch table is computed from config at runtime; the router prose contains NO fixed `rfc → story → iteration` chain. Where the design's default DAG is mentioned, mark it as the shipped default, not a hardcoded assumption.
   - Authorship-aware dispatch: when routing to an authoring action, pick the verb at or below the type's `authorship` ceiling (default to the ceiling verb, allow the human to drop lower); never dispatch an above-ceiling verb.

8. **Update `skills/README.md`** to describe the generic verb set (AC1 support).
   - Replace the hard-coded `plan-work → write-rfc → create-story → create-iteration → build → review-iteration` workflow description with the generic flow: `/lazy` as entry router dispatching `scaffold`/`co-write`/`generate` (authoring, ceiling-ordered), `advance` (status), `execute` (work), `review` (critique).
   - Add the new skills to the Reference table with one-line descriptions. Note that `resolve-context` folds into `context --json` and that the verbs are DAG-agnostic (type is a parameter, read from `config --json`).
   - Do NOT delete the legacy `skills/*/SKILL.md` files in this iteration unless trivially consistent to do so -- removal/superseding is install-time concern (ITERATION-200). Scope this task to documenting the new set; leave the old files in place.

## Test Plan

These are prose deliverables; eligibility/ceiling/gate logic in prose is **not unit-testable** (RFC-048 / ADR-019 accept this as the one per-runtime drift vector, mitigated by single-sourcing skill ≈ AGENTS.md). Verification is therefore a read-through checklist plus one manual smoke check against a non-default DAG.

**Authoring checklist (per skill file 1-7):**
- [ ] Frontmatter is exactly `name` + `description`; description starts "Use when…".
- [ ] Contains the slogan / `<HARD-GATE>` / `<NEVER>` / verbatim `<GITHUB-ISSUES-DOCUMENTS>` / CLI-discipline preamble matching the existing skills.
- [ ] **No baked DAG-specific type names** used as fixed values: a grep for `rfc`/`story`/`iteration` finds only clearly-marked default-config examples, never load-bearing prose. (Manual: `grep -rinE 'rfc|story|iteration' skills/{scaffold,co-write,generate,advance,execute,review,lazy}/SKILL.md` and confirm every hit is an example, not a hardcoded step.)
- [ ] The verb reads its type/rules from `lazyspec config --json` at runtime (the read is explicit in Preflight).
- [ ] `co-write` encodes ceiling refusal for `human`; `generate` encodes ceiling refusal for `human` and `assisted`; `scaffold` states it never refuses on ceiling. Refusal text reads the ceiling from config, not a hardcoded table.
- [ ] `advance` derives the next status from `lifecycle.edges` and applies via `update --status`; checks gates; does not spawn children.
- [ ] `execute` and `review` express targets generically (no fixed type names) and route on completion.

**`/lazy` router checklist (AC3):**
- [ ] Reads config + status + context in Preflight to locate the user.
- [ ] Advances within the current document automatically when an outgoing lifecycle edge is eligible.
- [ ] **STOPS at a type boundary** -- prose explicitly forbids auto-running `create <child-type>` across a `parent_type` edge, even when a `require_parent_status` gate is satisfied; reports the boundary and the human-initiated next step.
- [ ] Dispatch is computed from config; no fixed chain in prose.

**Smoke check against a non-default DAG fixture:**
1. In a scratch project, hand-write a minimal config with a DAG that is **not** `rfc → story → iteration` (e.g. types `proposal → feature → task` with distinct `authorship` ceilings: `proposal` = `human`, `feature` = `assisted`, `task` = `generated`).
2. Confirm a human can read each verb's prose and, following only the prose against `config --json` for that fixture, correctly: scaffold a `proposal`; have `generate` refuse on `proposal` and `feature` while proceeding on `task`; have `/lazy` advance a `feature`'s status within the doc but STOP rather than auto-create a `task`.
3. This is a manual read-through walkthrough (the prose is the unit under test); record the trace in `## Notes` or the PR description. Note that because `config --json` is not yet shipped (STORY-146 / ITERATION-198), the smoke check uses a hand-written config and reasons about the JSON the verbs will read -- it does not execute `lazyspec config --json` end-to-end in this iteration.

**Validation gate:**
- [ ] `lazyspec validate --json` passes with no errors referencing ITERATION-199.

## Notes

- **Scope boundary with ITERATION-200.** This iteration *authors* the skill files under `skills/`; ITERATION-200 *installs* them (`skills install`, `.claude/skills/` + `AGENTS.md` placement, `[skills] entry` config defaulting to `lazy`). Do NOT plan install, AGENTS.md packaging, or the `[skills] entry` config here.
- **Runtime dependencies (prose targets a not-yet-shipped CLI).** The verbs call `lazyspec config --json` (the full DAG: types with `intent`/`authorship`/`lifecycle`, relations, rules, `require_parent_status` gates). That command and the config axes are built by STORY-145 (config semantics + status DAG + gate/transition enforcement) and STORY-146 (`config --json` + config-write CLI), landing via ITERATION-196/197/198. This iteration writes prose against that contract; it does not require the contract to be implemented to author the markdown, but the smoke check can only be exercised end-to-end once `config --json` ships.
- **Design anchors.** Binary owns data, skill owns prose (ADR-019). Authorship is a ceiling: `scaffold < co-write < generate`, the type's `authorship` is the max (ADR-020). Non-aggression: only within-doc progression is automatic; crossing a type boundary is always human-initiated (ADR-022). Skill ≈ AGENTS.md -- one portable source, identical prose for every DAG (RFC-048).
- **Verb mapping (for reviewers).** New verbs collapse the legacy skills: `scaffold`/`co-write`/`generate` ⟵ `write-rfc`/`create-story`/`create-iteration`; `execute` ⟵ `build`; `review` ⟵ `review-iteration`; `/lazy` ⟵ `plan-work`; `resolve-context` folds into `context --json`. Legacy files stay in place until ITERATION-200's install step supersedes them.
