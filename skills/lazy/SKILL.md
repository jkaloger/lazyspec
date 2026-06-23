---
name: lazy
description: Use as the entry point for any work, including reported bugs, defects, and unexpected behaviour. Reads the configured DAG and the user's position, then dispatches the right verb -- advancing within the current document automatically but stopping at type boundaries.
---

```
ADVANCE WITHIN A DOCUMENT, STOP AT THE BOUNDARY
```

Lazy is the entry router: it reads the configured DAG and where the user is, then dispatches the right verb -- progressing within the current document automatically, but never crossing a type boundary on its own.

```d2
direction: down

preflight: Preflight {
  shape: rectangle
  config: "config --json (DAG)"
  status: "status --json (docs + statuses)"
  context: "context --json (chain)"
}

triage: Entry intent? {
  shape: diamond
  tooltip: "bug/defect reported, or positioned on a doc?"
}

debug: systematic-debugging {
  tooltip: "root cause FIRST -- no fix doc before Phase 1 done"
}

locate: Locate-in-DAG {
  shape: rectangle
  tooltip: "current type, status, outgoing edges, gates"
}

dispatch: Dispatch (computed from config) {
  shape: diamond
}

advance: /advance {tooltip: "status move, no authoring"}
author: authoring verb at ceiling {
  tooltip: "human -> /scaffold, assisted -> /co-write, generated -> /generate"
}
execute: /execute {tooltip: "work the document describes -- HUMAN-INITIATED, never auto-dispatched"}
review: /review {tooltip: "critique before next status"}

validate: validate touched doc {
  shape: rectangle
  tooltip: "validate --json scoped to the doc just mutated; fix introduced breakage"
}

boundary: STOP at type boundary {
  shape: hexagon
  tooltip: "child of a different type -- human-initiated only, even if gate is met"
}

preflight -> triage
triage -> debug: "bug / defect reported"
triage -> locate: "positioned on a doc"
debug -> boundary: "root cause found -- author config-driven fix doc, human-initiated"

locate -> dispatch

dispatch -> advance: "eligible status edge"
dispatch -> author: "authoring step due"
dispatch -> review: "critique due"
dispatch -> boundary: "next step crosses a type boundary (parent_type or parent-child rule), or only work remains"

advance -> validate: "graph mutated"
author -> validate: "graph mutated"
validate -> locate: "loop within document"
review -> locate

boundary -> execute: "human runs /execute" {style.stroke-dash: 3}
```

<HARD-GATE>
CONFIRM THE PLAN BEFORE MUTATING. Before the FIRST graph-mutating dispatch of a turn (`create`, `link`, `/advance`, or any authoring verb) AND before `/execute`, present the planned commands and the direction (which doc, which type, which parent link, what the fix/feature is), then STOP for explicit user approval. A prior "do it", "go ahead", "use /lazy", or the user naming the fix is approval of the WORK -- never of THIS specific plan (the parent link, the scope, the type choice are decisions to surface). General go-ahead is not step approval. This binds the actor: it holds whether `/lazy` is the entry router OR you are acting inline as the orchestrator -- running a verb directly does not exempt you. Violating the letter of this gate is violating its spirit.
A **type-boundary edge** is any edge to a different type, declared EITHER via `parent_type` OR via a parent-child `rule` (`shape: parent-child`, carrying `child`/`parent`/`link`). Both express the DAG; a config may use one, the other, or both. Derive boundaries from the UNION. Never assume `parent_type` is populated -- many configs encode the entire DAG in `rules` with every `parent_type` null. Null `parent_type` everywhere does NOT mean "no boundaries"; read the boundaries from the rules.
Do NOT auto-run `create <child-type>` across a type-boundary edge. Crossing into a different type is always human-initiated -- even when a `require_parent_status` gate is already satisfied. Within-document progression is automatic; crossing a type boundary is not.
**No work without a plan -- the PLAN->EXECUTE wall.** Authoring and advancing a delivery document's plan (task breakdown, AC) is automatic within that document; *executing* that plan is a separate, human-initiated step. `/lazy` NEVER auto-runs `/execute`. It authors and reviews the delivery doc, then STOPS and reports that the plan is ready to execute. It does not start implementing.
Compute the dispatch table from `lazyspec config --json` at runtime. There is no fixed chain in this prose.
A reported bug, defect, or unexpected behaviour is investigated to root cause FIRST -- via systematic-debugging -- before any fix document is authored. No fix doc before root cause.
After every graph-mutating dispatch (/advance and the authoring verbs), run `lazyspec validate --json` scoped to the touched document before looping.
</HARD-GATE>

<NEVER>
- Do NOT hand-edit document files. The CLI is the only writer: `lazyspec create` (seed with `--body`/`--body-file`), `lazyspec link`, and `lazyspec update <id> --body`/`--body-file` to change body content. This holds for EVERY store, filesystem included.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured DAG -- type boundaries come from `parent_type` edges AND parent-child `rules` (the union); honor every `rule`.
- Do NOT author, link, advance, or execute before the user approves the direction for THIS step -- even when they already authorized the work, named the fix, or said "use /lazy".
</NEVER>

<RED-FLAGS>
STOP and present the plan for approval if you catch yourself thinking:
- "The user already said 'do it' / 'use /lazy', so I'll create + link now"
- "They named the fix, the plan is obvious, just build it"
- "The boundary gate / require_parent_status is satisfied, so I can proceed"
- "I'm the orchestrator running inline -- the stop only applies to the dispatched verb"
- "Confirming is a formality, I'll show them after it's done"

| Rationalization | Reality |
|---|---|
| "User pre-authorized the work" | Authorizing the work is not approving this create+link+parent choice. Present it, get the nod. |
| "They said use /lazy, so route and go" | Using /lazy includes its stops. Going through a boundary without approval is not using /lazy. |
| "The fix is named, the plan is obvious" | Obvious to you is not confirmed by them. The parent link and scope are decisions -- surface them. |
| "Gate is satisfied, so it's automatic" | Gate-clear makes the next step eligible, not approved. Eligibility is not consent. |
| "Inline orchestration is exempt" | The gate binds the actor, not the invocation path. Inline does not skip it. |

## Confirm the plan before mutating

Before the first mutation or `/execute` in a turn, run this checklist. If any answer is "no", STOP and complete the missing step:

- [ ] Ran preflight (`config`/`status`/`context`) and located the position in the DAG
- [ ] Presented the planned commands + direction to the user (doc, type, parent link, what the fix/feature is)
- [ ] User approved THIS direction in THIS turn (not a prior general go-ahead)
- [ ] Dispatching the correct verb for the work and the type's authorship ceiling
- [ ] Not skipping to `/execute` without a delivery doc that carries a task breakdown
</RED-FLAGS>

<BODY-CONTENT>
Set body at creation: `lazyspec create <type> "<title>" --body "content"` or `--body-file <path>` (`-` reads stdin). Change it later: `lazyspec update <ID> --body "content"` or `--body-file <path>`. Prefer `--body`/`--body-file` over any direct file edit, for ALL stores (filesystem and github-issues alike).
GitHub-issues docs additionally: never edit `.lazyspec/cache/` mirrors (read-only); always reference docs by shorthand ID (e.g. STORY-095), not cache paths.
</BODY-CONTENT>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. On failure, check `--help` before retrying.

## Preflight (the routing read)

This is the resolve-context fold-in: `/lazy` reads context from the CLI rather than calling a separate skill.

1. `lazyspec config --json` -- the full DAG: `types` (with `intent`, `authorship`, `lifecycle`, `parent_type`), `relationships`, and `rules` (including any `require_parent_status` gates).
2. `lazyspec status --json` -- what documents exist and each one's current status.
3. `lazyspec context --json` -- the chain around the user's current document.

## Entry triage: bug or defect

When the user arrives with a **bug, defect, test failure, or unexpected behaviour** rather than positioned on a document, handle it here before routing. The whole branch is DAG-agnostic: it reads the fix-doc type and its links from config, never assuming a type name.

1. **Root cause first.** REQUIRED SUB-SKILL: systematic-debugging. Complete its Phase 1 (root-cause investigation) BEFORE authoring any fix document. No fix doc before root cause -- that is the systematic-debugging Iron Law, and it gates this branch.
2. **Pick the fix-doc type from config.** Read `config --json`. If a type's `intent` describes defects/bugs/fixes (a user may have a dedicated `bug` type), use that type. Otherwise use the delivery type -- the type whose breakdown describes implementation work (in the shipped default config that is `iteration`, but read it; never hardcode the name).
3. **Find the document the bug touches.** `lazyspec search "<area>" --json` plus `context --json` to locate the story/spec/feature covering the buggy area.
4. **Propose a create+link that satisfies the type's relation rules.** The fix-doc type may carry a `parent_type` or a `relation-existence` rule (e.g. `iterations-need-stories`). Propose the `create` plus the `link` (using the configured relation) that satisfies those rules -- linking the fix doc to the doc it touches. If no document satisfies a required relation, report that the human must pick or create the parent first. NEVER create a standalone doc that bypasses a rule, and never invent a link the user did not confirm.
5. **Crossing into the fix-doc type is a type boundary -- human-initiated.** Lazy proposes the exact `create` + `link` commands and stops (see Stop-at-Type-Boundary). It does not auto-create the fix doc.

## Locate-in-DAG

From config + status + context, determine which document and type the user is on and where it sits in its lifecycle (current status, outgoing edges, gates).

## Dispatch (computed from config)

Build the dispatch table at runtime from config. No `parent_type` chain is hardcoded here. (The shipped default config happens to define a chain among types named `rfc`, `story`, and `iteration` -- treat that only as the shipped default, never as a routing assumption.)

**Within-document progression is automatic.** If the current document has an eligible outgoing `lifecycle` edge (the edge exists and its gate, if any, is met), dispatch the matching verb WITHOUT asking:

- a status move with no authoring/work needed -> /advance
- an authoring step appropriate to the type's `authorship` and current status -> the authoring verb at the type's ceiling (/scaffold, /co-write, or /generate)
- a critique step before the next status -> /review

**`/execute` is never automatic.** Work described by a delivery document is implementation -- the EXECUTE band. `/lazy` does NOT dispatch `/execute` on its own. It brings the delivery doc to a reviewed, ready-to-execute plan and STOPS (treat this like a type boundary: human-initiated). Report that the plan is ready and the human runs `/execute` to begin work. No work without a reviewed plan.

**Authorship-aware dispatch.** When routing to an authoring action, pick the verb at or below the type's `authorship` ceiling. Default to the ceiling verb (`human` -> /scaffold, `assisted` -> /co-write, `generated` -> /generate) and allow the human to drop lower. Never dispatch an above-ceiling verb.

## Stop-at-Type-Boundary

When the only remaining next step would create a child of a **different type** -- crossing a type-boundary edge (a `parent_type` edge OR a parent-child `rule`, per the HARD-GATE union) -- `/lazy` **STOPS.** It reports the boundary and what the human can do next; it never auto-runs `create <child-type>`.

This holds **even when a `require_parent_status` gate is already satisfied.** Gate-clear makes the child _eligible_, not _automatic_. Crossing a type boundary is always human-initiated. Report it with the ceiling verb for the child type (per Authorship-aware dispatch: `human` -> /scaffold, `assisted` -> /co-write, `generated` -> /generate), like:

> `<doc>` (type `<type>`) is at status `<status>`; its child type `<child-type>` is now eligible to create. Crossing types is human-initiated -- run <ceiling-verb> to start one.

**Multi-hop:** if the required parent type is itself empty (e.g. an iteration needs a story, but no story exists), report the FULL chain the human must author in order -- each hop is a separate human-initiated crossing -- not just the nearest one.

with every value read from config + status for that run.

## Validate after each mutation

`/lazy` is the chokepoint for graph integrity. After every dispatched verb that **mutates the graph** -- `/advance` (status move plus relations) and the authoring verbs `/scaffold`, `/co-write`, `/generate` (create plus link) -- run `lazyspec validate --json` before looping back to locate.

- **Scope to the doc just touched.** `validate` is a whole-repo check and will report pre-existing findings across unrelated documents. Filter its output to findings naming the document this mutation created, linked, or advanced. Fix only the broken or dangling relation **this mutation introduced** before continuing. Do not block on pre-existing repo-wide findings.
- `/execute` and `/review` are not graph mutators in this loop, so they need no validate step here (`/execute` runs its own `validate` at Final Review).
- **Known limitation:** invoking a mutating verb standalone -- outside `/lazy` -- skips this check. `/lazy` is the canonical entry router; that is where graph integrity is enforced.

## Rules

- All routing reads from `config --json` / `status --json` / `context --json` at runtime. No type name and no chain are load-bearing in this prose.
- Type boundaries are the UNION of `parent_type` edges and parent-child `rules`. A config with every `parent_type` null still has boundaries -- read them from the rules.
- Within-document progression is automatic; crossing a type boundary is never automatic. If the parent type the human must cross into is itself empty, report the full chain of crossings, not just the nearest.
- `/execute` is never auto-dispatched. Bring the delivery doc to a reviewed, ready-to-execute plan and STOP; the human runs `/execute`. No work without a reviewed plan.
- Dispatch only verbs at or below the type's authorship ceiling.
- A reported bug/defect goes through root cause (systematic-debugging) before any fix doc is authored; the fix-doc type and its links are read from config and must satisfy the type's relation rules -- never create a standalone doc that bypasses a rule.
- After each mutating dispatch (`/advance`, `/scaffold`, `/co-write`, `/generate`), validate the touched doc and fix only the relation breakage this mutation introduced; standalone verb invocation outside `/lazy` skips this.
- Read DAG/gate/status facts from the CLI, never from `.lazyspec/` graph files directly.
