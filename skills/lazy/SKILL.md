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
confirm: Confirm the work plan {
  shape: hexagon
  tooltip: "present units, order, route -- STOP for explicit approval"
}
execute: /execute {tooltip: "one ready unit -- build pass, terminal"}
orchestrate: /orchestrate {tooltip: "several ready units -- ordered batch, commits, closes"}
review: /review {tooltip: "critique a document before its next status"}
reviewwork: /review-work {tooltip: "critique landed code against the doc's ACs"}

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
dispatch -> reviewwork: "work landed, awaiting critique"
dispatch -> confirm: "work is the next step"
dispatch -> boundary: "next step crosses a type boundary (a chain row in edges)"

confirm -> execute: "approved, one ready unit"
confirm -> orchestrate: "approved, several ready units"
execute -> reviewwork: "build reported"
orchestrate -> locate: "chunk done -- it reviewed, committed and closed its own units"

advance -> validate: "graph mutated"
author -> validate: "graph mutated"
validate -> locate: "loop within document"
review -> locate
reviewwork -> locate: "GREEN -> advance to completion"
```

<HARD-GATE>
CONFIRM THE PLAN BEFORE MUTATING. Before the FIRST graph-mutating dispatch of a turn (`create`, `link`, `/advance`, or any authoring verb) AND before `/execute` or `/orchestrate`, present the planned commands and the direction (which doc, which type, which parent link, what the fix/feature is), then STOP for explicit user approval. A prior "do it", "go ahead", "use /lazy", or the user naming the fix is approval of the WORK -- never of THIS specific plan (the parent link, the scope, the type choice are decisions to surface). General go-ahead is not step approval. This binds the actor: it holds whether `/lazy` is the entry router OR you are acting inline as the orchestrator -- running a verb directly does not exempt you. Violating the letter of this gate is violating its spirit.
A **type-boundary edge** is a row in `config --json`'s `edges` whose `traversal` is `chain`, whose `to` admits the type of the document you are on, and whose `from` names a different type. `edges` is the only source of a boundary: a type's `parent_type` declares none, and no other key does either.
**A row reads child-to-parent.** `from = "iteration"`, `to = "story"`, `via = "implements"` says an iteration implements a story. So the child types of the document you are on are a REVERSE lookup -- the `from` side of the rows whose `to` admits its type, never the `to` side of the rows whose `from` does. Run it:

```
lazyspec config --json | jq -r --arg t <doc-type> '
  [ .edges[]
    | select(.traversal == "chain")
    | select([.to] | flatten | any(. == $t or . == "*"))
    | [.from] | flatten[] | select(. != "*") ] | unique | .[]'
```

What it prints is the far side of the crossing: the child type to create. `$t` is the near side, the document you already have. Read a row the other way and you propose creating the parent that exists.
`"*"` sits on any position and filters rather than lists. On `to` it admits every type, so such a row applies to whatever document you are on. On `from` it names no type at all, so such a row yields no child type and no crossing to report -- a config that means concrete children names them in `from`. Take the type vocabulary from `types`; never expand a `"*"` into one. A row's `via` is the relation to pass to `lazyspec link`.
Do NOT auto-run `create <child-type>` across a type-boundary edge. Crossing into a different type is always human-initiated -- even when a `require_parent_status` gate is already satisfied. Within-document progression is automatic; crossing a type boundary is not.
**No work without a reviewed plan -- the PLAN->EXECUTE gate.** Authoring and advancing a delivery document's plan (task breakdown, AC) is automatic within that document. *Starting the work* is not: it requires an explicit, separate approval of THIS work plan -- which units, in which order, by which route. Present it and STOP. Never begin work off a general go-ahead, and never off a plan that has not been through /review.
Compute the dispatch table from `lazyspec config --json` at runtime. There is no fixed chain in this prose.
A reported bug, defect, or unexpected behaviour is investigated to root cause FIRST -- via systematic-debugging -- before any fix document is authored. No fix doc before root cause.
After every graph-mutating dispatch (/advance and the authoring verbs), run `lazyspec validate --json` scoped to the touched document before looping.
</HARD-GATE>

<NEVER>
- Do NOT hand-edit document files. The CLI is the only writer: `lazyspec create` (seed with `--body`), `lazyspec link`, and `lazyspec update <id> --body` to change body content. This holds for EVERY store, filesystem included.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured DAG -- type boundaries come from the `edges` table and from nothing else; honor every edge.
- Do NOT author, link, advance, or execute before the user approves the direction for THIS step -- even when they already authorized the work, named the fix, or said "use /lazy".
</NEVER>

<RED-FLAGS>
STOP and present the plan for approval if you catch yourself rationalizing past the gate:

| Rationalization | Reality |
|---|---|
| "User pre-authorized the work" | Authorizing the work is not approving this create+link+parent choice. Present it, get the nod. |
| "They said use /lazy, so route and go" | Using /lazy includes its stops. Going through a boundary without approval is not using /lazy. |
| "The fix is named, the plan is obvious" | Obvious to you is not confirmed by them. The parent link and scope are decisions -- surface them. |
| "Gate is satisfied, so it's automatic" | Gate-clear makes the next step eligible, not approved. Eligibility is not consent. |
| "Inline orchestration is exempt" | The gate binds the actor, not the invocation path. Inline does not skip it. |
</RED-FLAGS>

<BODY-CONTENT>
Set body at creation: `lazyspec create <type> "<title>" --body "content"`. Change it later: `lazyspec update <ID> --body "content"`. Prefer `--body` over any direct file edit, for ALL stores (filesystem and github-issues alike).
GitHub-issues docs additionally: never edit `.lazyspec/cache/` mirrors (read-only); always reference docs by shorthand ID (e.g. STORY-095), not cache paths.
</BODY-CONTENT>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. Read DAG/gate/status facts from the CLI, never from `.lazyspec/` graph files directly. On failure, check `--help` before retrying.

## Preflight (the routing read)

This is the resolve-context fold-in: `/lazy` reads context from the CLI rather than calling a separate skill.

1. `lazyspec config --json` -- the full DAG, in three keys: `types` for the type vocabulary and each type's `intent`, `authorship` ceiling and `lifecycle`; `edges` for the parent-child DAG, one row per declared edge (`name`, `from`, `to`, `via`, `required`, `traversal`); `relationships` for the link verbs a row's `via` names.
2. `lazyspec status --json` -- what documents exist and each one's current status.
3. `lazyspec context --json` -- the chain around the user's current document.

## Entry triage: bug or defect

When the user arrives with a **bug, defect, test failure, or unexpected behaviour** rather than positioned on a document, handle it here before routing. The whole branch is DAG-agnostic: it reads the fix-doc type and its links from config, never assuming a type name.

1. **Root cause first.** REQUIRED SUB-SKILL: systematic-debugging. Complete its Phase 1 (root-cause investigation) BEFORE authoring any fix document. No fix doc before root cause -- that is the systematic-debugging Iron Law, and it gates this branch.
2. **Pick the fix-doc type from config.** Read `config --json`. If a type's `intent` describes defects/bugs/fixes (a user may have a dedicated `bug` type), use that type. Otherwise use the delivery type -- the type whose breakdown describes implementation work (in the shipped default config that is `iteration`, but read it; never hardcode the name).
3. **Find the document the bug touches.** `lazyspec search "<area>" --json` plus `context --json` to locate the story/spec/feature covering the buggy area.
4. **Propose a create+link that satisfies the type's edges.** The fix-doc type may sit on the `from` side of a required edge (e.g. the `iterations-need-stories` edge, whose `required` is `error`). Propose the `create` plus the `link` (using that row's `via` relation) that satisfies those edges -- linking the fix doc to the doc it touches. If no document satisfies a required edge, report that the human must pick or create the parent first. NEVER create a standalone doc that bypasses a required edge, and never invent a link the user did not confirm.
5. **Crossing into the fix-doc type is a type boundary -- human-initiated.** Lazy proposes the exact `create` + `link` commands and stops (see Stop-at-Type-Boundary). It does not auto-create the fix doc.

## Locate-in-DAG

From config + status + context, determine which document and type the user is on and where it sits in its lifecycle (current status, outgoing edges, gates).

## Dispatch (computed from config)

Build the dispatch table at runtime from config. No chain is hardcoded here; the chain is whatever `edges` says at runtime. (The shipped default config happens to define a chain among types named `rfc`, `story`, and `iteration` -- treat that only as the shipped default, never as a routing assumption.)

**Within-document progression is automatic.** If the current document has an eligible outgoing `lifecycle` edge (the edge exists and its gate, if any, is met), dispatch the matching verb WITHOUT asking:

- a status move with no authoring/work needed -> /advance
- an authoring step appropriate to the type's `authorship` and current status -> the authoring verb at the type's ceiling (/scaffold, /co-write, or /generate)
- a critique step before the next status -> /review

**Authoring submits into review.** A body-producing authoring verb (/co-write, /generate) writes the body but not the status -- it leaves the document at its initial status (`draft` in the default lifecycle). It does NOT leave the document review-ready by itself. After such a verb completes and the body exists, advance the document across the edge into its review status (`draft -> review`) BEFORE dispatching /review, so /review critiques a document that is actually in its review status and its pass-route (the onward edge, `review -> accepted`) is available. Skipping this into-review advance is the common failure: /review fires while the doc is still at `draft`, and the `review -> ...` edge it expects to traverse on pass does not exist from `draft`. /scaffold is exempt -- it hands the body back to the human, so the document stays at its initial status until the human writes the body and re-enters /lazy, which then advances it into review.

**The work-open edge belongs to the build pass, not /lazy.** The edge from the work-ready status into the work-active status (`accepted -> in-progress` in the default DAG) is ungated, but /lazy does NOT traverse it. That edge means "the build has started", and /execute writes it as its first act. /lazy stops at the work-ready status and asks.

**Work is confirm-then-run.** When the next step is the work itself, /lazy does not stop dead and it does not just go. It presents the work plan -- which delivery documents are ready, the order their dependency edges imply, and which route -- then STOPS for explicit approval of that plan. On approval it dispatches:

| Ready units | Route | What it does |
|---|---|---|
| One | /execute | Builds that unit and reports. Terminal -- it does not review, commit, or close. /lazy then routes to /review-work, and on GREEN to /advance. |
| Several | /orchestrate | Orders them by their dependency edges and drives the whole chunk: build, review, commit, close, plus the end-of-chunk pass. Returns when the chunk is done. |

Approval of the work is not approval of this plan. "Go ahead", "use /lazy", or the user naming the feature authorises the work; the units, the order, and the route are still decisions to surface. Present them, get the nod, then dispatch.

**Reviewing work is /review-work, not /review.** After /execute reports, the delivery document sits at its work-active status with a diff and no verdict. Dispatch /review-work (depth blocking-only) against that diff, not /review -- /review critiques documents. On GREEN, /advance the document to its completion status. On RED, route the findings to a fix pass. On STOP, halt and report: the plan, not the code, is wrong.

**Authorship-aware dispatch.** When routing to an authoring action, pick the verb at or below the type's `authorship` ceiling. Default to the ceiling verb (`human` -> /scaffold, `assisted` -> /co-write, `generated` -> /generate) and allow the human to drop lower. Never dispatch an above-ceiling verb.

## Stop-at-Type-Boundary

When the only remaining next step would cross into a **different type** -- traversing a type-boundary edge (a `chain` row in `edges`, per the HARD-GATE) -- `/lazy` **STOPS.** The boundary is the edge, not one type: a row's far side is a set of types, and any one member satisfies the row. So the report names every type the edge admits and leaves the choice among them to the human; `/lazy` never auto-runs `create <child-type>` for a member of that set.

This holds **even when a `require_parent_status` gate is already satisfied.** Gate-clear makes the crossing _eligible_, not _automatic_. Crossing a type boundary is always human-initiated. A ceiling belongs to the type, not to the edge, so a three-member set can carry three ceilings and therefore three different verbs (per Authorship-aware dispatch: `human` -> /scaffold, `assisted` -> /co-write, `generated` -> /generate). That is why the report is a list: one line per type, carrying that type's own verb.

**Two commands assemble it.** `validate --json` says *that* an edge is unsatisfied. `config --json` says *which* types satisfy it. `validate`'s `errors` are flat strings -- one rendered finding each -- so a finding's type set is prose; do not string-parse it back out, however direct a source it looks. Read the set structured, from `config --json`: `edges[].to` when the crossing goes up (the document you hold needs a parent), and the `from` sides of the rows admitting this type when it goes down (the HARD-GATE's reverse lookup, which already collects them as a set).

When the row names its types:

> `<doc>` (type `<type>`) is at status `<status>`; edge `<edge-name>` is now eligible to cross. Any one of these satisfies it -- crossing types is human-initiated, so pick one and run its verb:
> - `<type-a>` -- run <ceiling-verb-a>
> - `<type-b>` -- run <ceiling-verb-b>

When the far side is `"*"` there is no list to name, because the row declined to name one:

> `<doc>` (type `<type>`) is at status `<status>`; edge `<edge-name>` goes `to a document of any type`. Pick the type from `types` in `config --json`, then run that type's ceiling verb.

Never expand a `"*"` into the type vocabulary. Eleven names offered as equal options claim a choice the config did not make.

**Multi-hop:** when the type at the far side has no document to link to either, report the whole chain the human must author, nearest hop first -- each hop is a separate human-initiated crossing. **Enumerate at the nearest hop only.** The hop being crossed now gets one line per type; each hop beyond it gets a single line naming its edge and its set in the finding's own wording -- `to one of: spike, story, bug`, or `to a document of any type` for a `"*"` row -- and no lines of its own. The choice at a later hop is not live until the earlier one is made, and three types at this hop against two at the next is six chains nobody asked to read.

with every value read from config + status for that run.

## Validate after each mutation

`/lazy` is the chokepoint for graph integrity. After every dispatched verb that **mutates the graph** -- `/advance` (status move plus relations) and the authoring verbs `/scaffold`, `/co-write`, `/generate` (create plus link) -- run `lazyspec validate --json` before looping back to locate.

- **Scope to the doc just touched.** `validate` is a whole-repo check and will report pre-existing findings across unrelated documents. Filter its output to findings naming the document this mutation created, linked, or advanced. Fix only the broken or dangling relation **this mutation introduced** before continuing. Do not block on pre-existing repo-wide findings.
- `/review` and `/review-work` are not graph mutators, so they need no validate step here. `/execute` runs its own `validate` at close-out, and `/orchestrate` runs one at its done check.
- **Known limitation:** invoking a mutating verb standalone -- outside `/lazy` -- skips this check. `/lazy` is the canonical entry router; that is where graph integrity is enforced.
