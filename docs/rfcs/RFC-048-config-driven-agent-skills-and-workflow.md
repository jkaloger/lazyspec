---
title: Config-driven agent skills and workflow
type: rfc
status: accepted
author: jkaloger
date: 2026-06-21
tags:
- skills
- config
- agents
- workflow
- templates
related:
- related-to: RFC-042
- related-to: RFC-046
- related-to: RFC-007
- related-to: RFC-002
---

## Summary

Lazyspec's config defines an arbitrary DAG of document types, relations, and rules (RFC-042), but the agent skills that drive spec-driven development are hand-authored markdown hard-coupled to `RFC → Story → Iteration`. A user with a custom DAG gets no skills that match it.

This RFC makes the workflow config-driven. Skills become a small, static, DAG-agnostic set of generic verbs that read the config at runtime. The config gains three per-type axes that carry the per-type intent and constraints: `intent`, `authorship`, and a per-type status DAG, plus status-conditioned gates. A grill-me-style meta-skill (`/configure-type`) helps users author the per-type artifacts for custom types. The skill prose is portable markdown that serves Claude (as skills) and any other agent (as `AGENTS.md`), so the workflow is not Claude-only.

The spine: **the binary owns data, the skill owns prose.** The binary serves config and state as JSON and enforces config invariants on mutations; the skill computes what-to-do-next from that data and holds the authoring methodology.

## Motivation

Three problems, in order.

**1. The DAG is configurable; the workflow is not.** Per RFC-042 the engine ships zero baked-in types; users declare arbitrary `[[types]]`, `parent_type`, `[[relationships]]`, and `[[rules]]`. But `skills/` hard-codes `write-rfc`, `create-story`, `create-iteration`, each assuming the default three-type chain. A team whose DAG is `spec → feature → story → iteration` has no skill that knows their types exist.

**2. Workflow modes vary and the skills aggressively push toward building.** Some teams hand-write docs; some run an up-front planning phase (write `spec`/`rfc`/`adr`, refine into `feature`/`story`, only then break a story into an `iteration`); some plan and execute inline. The current skills drive from planning into building rather than letting a planning phase settle. There is no structural notion of "stop here, a human decides whether to proceed."

**3. Status is the one closed axis.** Types are open (RFC-042), relations are a validated string newtype with a config registry (ADR-010). `@ref src/engine/document.rs#Status` is a fixed 7-variant enum with no transition model. Once status carries workflow weight (gating), that rigidity forces lazyspec's vocabulary (`accepted`, `in-progress`) onto every user and leaves the binary unable to say what the sensible next status is.

The constraint that shapes the whole design: for a user's **arbitrary** type, neither a generic skill nor a code generator can invent authoring methodology the engine doesn't ship (ADR-011 forbids baked-in knowledge). Per-type methodology for arbitrary types can only come from config data the user wrote. So the design puts that methodology in config (intent + enriched template) and parameterizes one static skill set over it, rather than generating per-type skill files.

## Goals

- Workflow skills work for any user-configured DAG, not just `RFC → Story → Iteration`.
- Support hand-written, up-front-planning, and inline plan-execute modes without nudging users out of a planning phase.
- Express per-type purpose and authoring guidance as config + template data.
- Make "this doc type is human-written" an enforced ceiling, not a suggestion.
- Make a planning→delivery handoff structural: the binary refuses gated transitions; the skill never auto-crosses a type boundary.
- Keep the skill prose portable across agent runtimes (Claude skills, `AGENTS.md`, others).

## Non-goals

- A routing/eligibility "brain" command in the binary (`next`/`guide`). Eligibility is computed in skill prose for v1; promotion to the binary is a documented future move if prose drifts.
- Generated per-type skill files. Documented as a future escape hatch (see Future work).
- A whole-DAG bootstrap wizard. `/configure-type` is per-type; greenfield is running it per type.
- A new phase/banding axis. Banding is implicit in the status gates and DAG topology.
- Status transition *constraints* beyond the declared edges (the DAG edges already give this).

## Design

### Spine: binary owns data, skill owns prose

The binary exposes the config and document state as JSON and enforces config invariants on the existing mutation commands. It does **not** decide what verb to run next. The skill reads `config --json` + `status --json` + `context --json`, locates the user in the DAG, and applies authoring methodology held in prose.

Rationale, and the main accepted risk, are in ADR-019. The eligibility/ceiling/gate derivation living in prose is untestable and is the one real per-runtime drift vector; the escape valve is promoting it into a binary command later.

### Per-type config axes

Extend `@ref src/engine/config.rs#TypeDef` with three axes:

```
@draft
// added to TypeDef
intent: Option<String>,          // one line: why this type exists
authorship: Authorship,          // ceiling on AI autonomy; default Assisted
lifecycle: Lifecycle,            // inline status DAG for this type
```

```
@draft
enum Authorship { Human, Assisted, Generated }   // monotone: scaffold < cowrite < generate

struct Lifecycle {
    states: Vec<String>,                 // node set, e.g. ["draft","review","ratified"]
    edges: Vec<(String, String)>,        // directed transitions; may include "*" source
}
```

`intent` is the headline an authoring skill writes toward. `authorship` is the autonomy ceiling (next section). `lifecycle` is a per-type status DAG, declared inline on the type. Per-type inline DAGs duplicate lifecycle definitions across similar types; this is an accepted cost of each type self-describing (ADR-021). A shared `lifecycle =` reference is the future de-dup if duplication bites.

### Status as a validated newtype over a per-type DAG

Replace the closed `@ref src/engine/document.rs#Status` enum with a validated string newtype (the ADR-010 pattern already used for `RelationType`), validated against the owning type's `lifecycle.states`. Transitions are the declared edges; `update --status` rejects any move not on an edge, which turns "unconstrained any→any" into "transitions are the edges you declared" and yields transition validation for free. The current 7 statuses become the starter config's default lifecycle, and `fix --config` injects that default DAG into pre-existing configs (migration).

### Authorship ceiling

Authoring is three verbs, monotone in AI autonomy: `scaffold < co-write < generate`. The type's `authorship` names the ceiling:

- `human` → scaffold only (AI never writes the body; it creates the file from template, fills frontmatter, surfaces intent + section guidance, hands back).
- `assisted` → up to co-write (AI proposes a draft, human edits, iterates).
- `generated` → up to generate (AI writes the full body from context, then asks for review).

A verb above the ceiling is refused; a verb at or below is always allowed, so a human can choose a more manual mode on any type. The ceiling is reported in `config --json` and honored by the skill; `validate` carries a detective rule for violations. True prevention at the binary is not possible (prose bodies are written by file edits, not a CLI mutation), so enforcement is ceiling-in-data plus the validate backstop (ADR-020).

### Gates: status-conditioned, no phase axis

Non-aggression is structural without a new banding axis. Extend the parent-child rule (`@ref src/engine/config.rs#ValidationRule`) with an optional `require_parent_status`: a child type is creatable only once its parent reaches a named status.

```
@draft
// added to ValidationRule::ParentChild
require_parent_status: Option<String>,   // e.g. story creatable only when rfc is "ratified"
```

`create <child>` refuses when the gate is unmet. Combined with the rule that the skill never auto-crosses a type boundary (spawning a child is always a human-initiated step; only within-doc progression flows automatically), this gives the planning→delivery handoff the user wants: the agent settles the planning docs and stops, rather than racing into iterations. Banding is implicit in the gates plus the existing DAG topology; no `[[phases]]` (ADR-022).

**Amended 2026-08-31 (ADR-033):** the refusal is withdrawn. Status-conditioned `create` gating is abandoned, so the planning→delivery handoff now rests solely on the second fact above — the skill never auto-crosses a type boundary. `require_parent_status` dies with `[[rules]]` (RFC-067 STORY-259) and has no successor.

### CLI surface

New and extended commands, all `--json` (principle 2):

- `lazyspec config --json` — read the full config: types (with `intent`/`authorship`/`lifecycle`), relations, rules, gates. The DAG-introspection the router reads. *New.*
- `lazyspec config add-type | set-lifecycle | add-gate ...` — config mutation, reusing `@ref src/engine/config_write.rs` (in-place TOML editing, today only driven by TUI settings and `fix --config`). Lets `/configure-type` author config instead of hand-editing TOML. *New.*
- `update --status` — reject transitions outside the type's lifecycle DAG. *Extended.*
- `create <child>` — honor `require_parent_status` gates. *Extended.*
- `init` — materialize default templates to disk (today the dir is created empty and defaults live only as embedded fallbacks). *Extended.*
- `skills install [--runtime claude|agents-md]` — drop the generic verb skills + `AGENTS.md`, set `[skills] entry`. Decoupled from `init`. *New.*

No `next`/`guide` command (non-goal).

### Generic verb skills

One static, portable, DAG-agnostic set. The verbs read config + templates at runtime; the type is a parameter, not a baked name:

- `scaffold` / `co-write` / `generate` — authoring, ceiling-ordered (`refine` folds into re-invoking the mode).
- `advance` — transition status along the lifecycle DAG and maintain links; the point where gates are checked.
- `execute` — carry out the work a delivery doc describes (the build loop).
- `review` — critique a doc against its intent/AC, or review completed work.
- `/lazy` — the entry router (name configurable via `[skills] entry`, default `lazy`). Reads config + status + context, locates the user, dispatches; stops at type boundaries.

These collapse today's `write-rfc`/`create-story`/`create-iteration`/`build`/`review-iteration`/`plan-work`; `resolve-context` folds into `context --json`.

### Skill ≈ AGENTS.md

A Claude `SKILL.md` and an `AGENTS.md` are both instruction markdown; the delta is frontmatter and filename, not content. Because the verbs read config at runtime, the prose is identical for every user and every DAG. So there is one portable source, placed (not transformed) as `.claude/skills/` for Claude and concatenated into `AGENTS.md` for other agents. No per-runtime transformer (principle 6).

### Enriched templates

Keep plain markdown + `{key}` substitution (`@ref src/engine/template.rs#render_template`, `@ref src/engine/fs_ops.rs#load_template`). Upgrade the per-type template to carry the per-type methodology: an `<!-- intent: ... -->` header and `<!-- guidance: ... -->` per section, in place of bare `TODO:` lines.

```
@draft
// .lazyspec/templates/rfc.md, materialized by init
<!-- intent: fix a design decision and its alternatives before code -->
## Context
<!-- guidance: problem, constraints, why now -->
## Options
<!-- guidance: >=2, tradeoffs each -->
## Decision
<!-- guidance: chosen option + rationale -->
```

Comments render invisibly and guide both the agent and a human opening the file. The template is the per-type methodology, expressed as data the generic verbs consume.

### `/configure-type` meta-skill

A grill-me-style interview for one type. It elicits name, `intent`, `authorship`, lifecycle (`states` + `edges`), gates, and relations to existing types, then writes the enriched template and the `[[types]]` block via the config-write CLI. This is how per-type methodology for an arbitrary type gets authored: the user supplies the knowledge, the skill extracts and records it. Greenfield setup is running it per type; a whole-DAG bootstrap is future composition.

### Defaults

Lazyspec ships sensible defaults so most users never run `/configure-type`: a default config (`rfc`/`story`/`iteration`/…), default enriched templates for those types, the generic verb skills, and an `AGENTS.md`.

## Interfaces

Existing, extended:
- `@ref src/engine/config.rs#TypeDef` — gains `intent`, `authorship`, `lifecycle`.
- `@ref src/engine/config.rs#ValidationRule` — `ParentChild` gains `require_parent_status`.
- `@ref src/engine/document.rs#Status` — becomes a validated string newtype over the type's lifecycle.
- `@ref src/engine/config_write.rs` — backs the new `config` mutation subcommands.
- `@ref src/engine/template.rs#render_template`, `@ref src/engine/fs_ops.rs#load_template` — templates materialized to disk and enriched with guidance comments.

Proposed: `Authorship`, `Lifecycle` (`@draft` above); `config`/`skills install` CLI subcommands; the generic verb skill set.

## Decisions (ADRs to emit)

- ADR-019 — Binary owns data, skill owns prose; no routing brain command for v1.
- ADR-020 — Authorship is a ceiling on AI autonomy, enforced in data + validate, not preventable at the binary.
- ADR-021 — Per-type inline status DAGs (states + edges), duplication accepted over a shared lifecycle registry.
- ADR-022 — Status-conditioned gates instead of a phase/banding axis.
- ADR-023 — Status becomes a validated string newtype over a per-type DAG (extends ADR-010).

## Stories

1. **Config axes + status DAG + enforcement.** Add `intent`/`authorship`/`lifecycle` to `TypeDef`; `Status` newtype over the lifecycle; `update --status` edge validation; `require_parent_status` gate on `create`. Engine + config-write + migration via `fix --config`. (Foundation; others depend on it.)
2. **`config --json` + config-write CLI.** Read the full DAG as JSON; `config add-type|set-lifecycle|add-gate` over `config_write.rs`.
3. **Generic verb skills + `skills install` + `AGENTS.md` + defaults.** The static verb set, the install command, the default config + skills + AGENTS.md.
4. **Enriched templates + `init` materialization.** Guidance-comment template format; `init` writes defaults to disk.
5. **`/configure-type` meta-skill.** The grill-me-style per-type authoring interview driving the config-write CLI.

## Risks and tradeoffs

- **Eligibility in prose is untestable and the one per-runtime drift risk.** Skill ≈ AGENTS.md keeps it single-sourced; if routing prose diverges, promote `next` into the binary (ADR-019 escape valve).
- **Per-type inline DAGs duplicate** lifecycle definitions across similar types; a `lifecycle =` reference is the future de-dup (ADR-021).
- **Ceiling enforcement is not preventive** (prose bodies bypass the CLI); it is ceiling-in-data + a validate rule (ADR-020), matching the existing honor-system trust model for "don't edit files directly."

## Future work

- Per-type skill files as an escape hatch when a type needs workflow prose a template can't hold (principle 6: add on the second concrete use).
- Whole-DAG bootstrap meta-skill composing `/configure-type` plus a DAG-design step.
- `next`/`guide` routing command in the binary if prose eligibility proves fragile.
- Shared `lifecycle =` reference to de-dup per-type DAGs.

## Related

- RFC-042 — Unopinionated document types and relationships (the open-DAG foundation these axes extend).
- RFC-046 — Pluggable agent runtime and user-authored prompt templates (template + runtime mechanism this builds on).
- RFC-007 — Agent-Native CLI (the `--json`, CLI-as-universal-interface basis).
- RFC-002 — AI-Driven Development Workflow (the skills workflow this restructures).
