---
name: scaffold
description: Use when creating a new document of a configured type at the most manual authorship level -- AI creates the file and frontmatter, surfaces intent and section guidance, then hands the body back to the human.
---

```
CREATE THE SHELL, HAND THE BODY BACK
```

<HARD-GATE>
Do NOT write the document body. Scaffold creates the file, frontmatter, and links, then surfaces the type's intent and section guidance for the human to fill in.
Read the target type's config from `lazyspec config --json` before creating anything; the type is a parameter, never assumed.
</HARD-GATE>

<NEVER>
- Do NOT hand-edit document files to create or link them. Use `lazyspec create` (seed with `--body`) and `lazyspec link`. To change body content, use `lazyspec update <id> --body` -- for EVERY store, filesystem included. (Scaffold itself writes no body; it hands that back to the human.)
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured DAG -- type boundaries come from the `edges` table and from nothing else; honor every edge.
</NEVER>

<BODY-CONTENT>
Set body at creation: `lazyspec create <type> "<title>" --body "content"`. Change it later: `lazyspec update <ID> --body "content"`. Prefer `--body` over any direct file edit, for ALL stores (filesystem and github-issues alike).
GitHub-issues docs additionally: never edit `.lazyspec/cache/` mirrors (read-only); always reference docs by shorthand ID (e.g. STORY-095), not cache paths.
</BODY-CONTENT>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. Read parent/relation/gate facts from the CLI, never from `.lazyspec/` graph files directly. On failure, check `--help` before retrying.

## Authorship Ceiling

The authorship order is `scaffold < co-write < generate`. A type's `authorship` value in config (`human`, `assisted`, `generated`) is the *ceiling* -- the highest verb permitted for that type.

Scaffold is the floor of that order, so it is permitted on **every** `authorship` value. **Scaffold never refuses on ceiling grounds.** Even a type whose ceiling is `human` can be scaffolded; that is exactly the manual case scaffold exists for.

## Preflight

1. `lazyspec config --json` -- read the target `<type>` entry: its `intent` (what the doc is for), its `authorship` ceiling (for confirmation only -- scaffold proceeds regardless), and the section guidance available from its template. `parent_type` decides containment only -- the directory this type's documents live under and the store backend they share -- and declares no link.
2. `lazyspec status --json` -- see what already exists and locate the parent document to link to.
3. `lazyspec context --json` -- understand the chain around the user's current position so the new document lands in the right place.

## Workflow

1. **Create the shell:** `lazyspec create <type> "<title>" --author <name>`, where `<type>` is the parameter read from config (e.g. in the shipped default config a type named `rfc`, but never assume that name -- read it).
2. **Link by edge:** find the `edges` rows whose `from` admits this type. A row reads child-to-parent, so the new document sits on the `from` side: the row's `via` is the relation to pass to `lazyspec link`, and its `to` names the types a target document may be. `lazyspec link <new-id> <via> <target-id>`, with `<via>` read off the row -- never bake a relation name into the call. Take the type vocabulary from `types`; a `"*"` filters rather than lists, so never expand one into a type name. When no row admits this type, or no document of a type its `to` admits exists, link nothing and say so.
3. **Surface intent + guidance:** show the human the type's `intent` from config and the per-section `<!-- guidance -->` comments from the scaffolded body. Tell the human these are the sections to fill in.
4. **Hand back:** stop. The human writes the body. Scaffold does not draft prose.

---

---
name: co-write
description: Use when drafting a document of a configured type collaboratively -- AI proposes a draft body, the human edits, iterate -- up to the type's authorship ceiling.
---

```
PROPOSE A DRAFT, THE HUMAN EDITS, ITERATE
```

<HARD-GATE>
Do NOT proceed when the target type's `authorship` ceiling is `human` -- that type tops out at scaffold. Read the ceiling from `lazyspec config --json` and refuse, naming the ceiling.
Co-write proposes a draft for human editing; it does not finalise a body unilaterally.
</HARD-GATE>

<NEVER>
- Do NOT hand-edit document files. The CLI is the only writer: `lazyspec create` (seed with `--body`), `lazyspec link`, and `lazyspec update <id> --body` to change body content. This holds for EVERY store, filesystem included.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured DAG -- type boundaries come from the `edges` table and from nothing else; honor every edge.
</NEVER>

<BODY-CONTENT>
Set body at creation: `lazyspec create <type> "<title>" --body "content"`. Change it later: `lazyspec update <ID> --body "content"`. Prefer `--body` over any direct file edit, for ALL stores (filesystem and github-issues alike).
GitHub-issues docs additionally: never edit `.lazyspec/cache/` mirrors (read-only); always reference docs by shorthand ID (e.g. STORY-095), not cache paths.
</BODY-CONTENT>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. Read parent/relation/gate facts from the CLI, never from `.lazyspec/` graph files directly. On failure, check `--help` before retrying.

## Authorship Ceiling

The authorship order is `scaffold < co-write < generate`. A type's `authorship` value in config is the ceiling.

Co-write is the middle rung. It is permitted when the type's `authorship` is `assisted` or `generated`.

**Refuse when the type's `authorship` is `human`.** Read the ceiling from `lazyspec config --json` and report it. Refusal text reads the ceiling out of config -- there is no hardcoded type-to-ceiling table:

> Type `<type>` is human-authored (ceiling = scaffold); drop to /scaffold.

where `<type>` and the ceiling are the actual values read from config for that run.

## Preflight

1. `lazyspec config --json` -- read the target `<type>`: its `intent`, its `authorship` ceiling (gate the verb on this), section guidance from its template, and the relation names in `relationships`. `parent_type` decides containment only -- the directory this type's documents live under and the store backend they share -- and declares no link.
2. `lazyspec status --json` -- locate the parent document to link to.
3. `lazyspec context --json` -- understand the chain around the user's position.

## Workflow

Scaffold, interview, then propose:

1. **Create + link** as in /scaffold: `lazyspec create <type> "<title>" --author <name>`, then `lazyspec link <new-id> <relation> <parent-id>` using the configured relation when a parent exists.
2. **Interview the human before drafting.** Co-write captures intent from the human, so grill before you write. Interview them relentlessly about every decision this document must resolve, walking each branch of the design tree and resolving dependencies between decisions one at a time. Ask ONE question at a time. For each question, give your recommended answer. If a question can be answered by exploring the codebase or reading `config --json` / parent docs / `@ref` targets, explore and answer it yourself instead of asking. Continue until every open branch the type's `intent` and section guidance imply is resolved -- do not start the draft with unresolved decisions.
   2.1 **Propose 2-3 designs per unknown** Never propose only one option, always provide a range of design iterations/options to choose.
3. **Propose a draft body** toward the type's `intent` and section guidance from config, incorporating the interview answers. Do not write verbose prose, only outline a summary of the key design/decisions made during the interview into the document.
4. **Present for human edits.** After writing the document, the human revises; iterate the proposal with them.
5. **Apply the accepted draft:** `lazyspec update <id>`.

---

---
name: generate
description: Use when authoring a full document body of a configured type from context -- AI writes the complete body, then asks for review -- only permitted when the type's authorship ceiling is `generated`.
---

```
WRITE THE WHOLE BODY, THEN ASK FOR REVIEW
```

<HARD-GATE>
Do NOT proceed unless the target type's `authorship` ceiling is `generated`. For `human` and `assisted` types, refuse and name the permitted verb. Read the ceiling from `lazyspec config --json`.
Generate writes the full body, then routes to /review -- it does not self-approve.
</HARD-GATE>

<NEVER>
- Do NOT hand-edit document files. The CLI is the only writer: `lazyspec create` (seed with `--body`), `lazyspec link`, and `lazyspec update <id> --body` to change body content. This holds for EVERY store, filesystem included.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured DAG -- type boundaries come from the `edges` table and from nothing else; honor every edge.
</NEVER>

<BODY-CONTENT>
Set body at creation: `lazyspec create <type> "<title>" --body "content"`. Change it later: `lazyspec update <ID> --body "content"`. Prefer `--body` over any direct file edit, for ALL stores (filesystem and github-issues alike).
GitHub-issues docs additionally: never edit `.lazyspec/cache/` mirrors (read-only); always reference docs by shorthand ID (e.g. STORY-095), not cache paths.
</BODY-CONTENT>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. Read parent/relation/gate facts from the CLI, never from `.lazyspec/` graph files directly. On failure, check `--help` before retrying.

## Authorship Ceiling

The authorship order is `scaffold < co-write < generate`. A type's `authorship` value in config is the ceiling.

Generate is the top rung. It is permitted **only** when the type's `authorship` is `generated`.

**Refuse for `human` and `assisted` types.** This is the headline ceiling-refusal case. Read the ceiling from `lazyspec config --json` and report it together with the permitted verb. The refusal text reads the ceiling string out of config; there is no baked type-to-ceiling table:

> Type `<type>` ceiling = co-write; drop to /co-write.

or, for a human-authored type:

> Type `<type>` ceiling = scaffold; drop to /scaffold.

where `<type>` and the ceiling are the actual values read from config for that run. Map ceiling to verb by the order itself: `human` -> scaffold, `assisted` -> co-write, `generated` -> generate.

## Preflight

1. `lazyspec config --json` -- read the target `<type>`: its `intent`, its `authorship` ceiling (gate the verb on this), section guidance, and relation names. `parent_type` decides containment only -- the directory this type's documents live under and the store backend they share -- and declares no link.
2. `lazyspec context --json` -- assemble source material: parent docs, related docs, and referenced code. Expand `@ref` directives and pull referenced code with `lazyspec show -e <id>`.

## Workflow

1. **Create + link:** `lazyspec create <type> "<title>" --author <name>`, then `lazyspec link <new-id> <relation> <parent-id>` with the configured relation when a parent exists.
2. **Resolve residual gaps before writing.** Generate is context-first: it leans on parent docs, related docs, `@ref` targets, and the codebase, not on the human. So capture lightly -- resolve every decision you can from gathered context yourself, then surface ONLY the decisions the context cannot settle. Ask those as a short batch (one at a time, each with your recommended answer); skip the question entirely when the context already answers it. This is a lighter touch than /co-write's full interview -- you are filling gaps, not eliciting the whole design.
3. **Interview about any remaining gaps** Interview the user (as below)
4. **Write the full body** from gathered context and resolved gaps toward the type's `intent` and section guidance. Write to a file.
5. **Apply:** `lazyspec update <id> --body "content"`.
6. **Request review:** route to /review. Generate never approves its own output.

## Interview

When interviewing, grill me relentlessly about every aspect of this plan until we reach a shared understanding. Walk down each branch of the design tree, resolving dependencies between decisions one-by-one. For each question, provide your recommended answer.

Ask the questions one at a time.

If a question can be answered by exploring the codebase, explore the codebase instead.

---

---
name: advance
description: Use when moving a document to its next status along the type's lifecycle DAG, maintaining links and checking gates at the transition.
---

```
TRAVERSE ONE OUT-EDGE OF THE LIFECYCLE GRAPH
```

A type's lifecycle is a directed graph: the nodes are its statuses, the edges are the transitions config permits. A document sits on one status. Advance reads the out-edges from that status, picks the successor, confirms the gate on that edge holds, and writes the move. One document, one edge.

## The command

Advance is a skill, not a subcommand. The move is written by `update`:

```
lazyspec update <id> --status <next>
```

`lazyspec advance` does not exist. `lazyspec help` lists every subcommand there is.

<HARD-GATE>
Propose only a successor: a status the current one has an out-edge to in `lifecycle.edges`. Read the edge set from config. The binary rejects any pair that is not an edge.
Advance writes status only. It never creates a child document, even when the move satisfies a gate that makes a child creatable.
</HARD-GATE>

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` and `lazyspec link`.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured DAG -- type boundaries come from the `edges` table and from nothing else; honor every edge.
</NEVER>

<GITHUB-ISSUES-DOCUMENTS>
Documents stored in GitHub Issues (store = "github-issues") are managed through the GitHub API. The `.lazyspec/cache/` directory contains read-only mirrors.
- Never edit files under `.lazyspec/cache/`. Use `lazyspec update <ID> --body` to modify content.
- Always use shorthand IDs (e.g. STORY-095) not cache file paths when referencing documents in `lazyspec link`, `lazyspec update`, `lazyspec show`, etc.
- To set body content at creation: `lazyspec create <type> <title> --body "content"`.
- To modify after creation: `lazyspec update <ID> --body "new content"`.
</GITHUB-ISSUES-DOCUMENTS>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. Read lifecycle and gate facts from the CLI, never from `.lazyspec/` graph files directly. On failure, check `--help` before retrying.

## Preflight

1. `lazyspec config --json` gives the type's `lifecycle`: its `states` (the nodes) and `edges` (the transitions). The edge set decides which moves exist. Every status name comes from config; this skill names none.
2. `lazyspec show <id> --json` gives the document's current status.
3. `lazyspec context --json` gives the parent and child statuses a gate may depend on.

## Workflow

1. Find the successors. Keep the edges in `lifecycle.edges` whose `from` is the current status; their `to` values are the statuses you can move to. An edge with `from: "*"` applies from every status, so the default config's `* -> superseded` is always available.
2. Test the gate. A gate is a predicate on the target status, such as `require_parent_status`. Read the parent's status from `context --json` and check the predicate. If it fails, stop and report which status the parent must reach first.
3. Write the move. `lazyspec update <id> --status <next>`. The binary rejects any pair that is not an edge, so offer only successors.
4. Preserve the links across the move.

## Gates and the type boundary

A gate can make a child of another type creatable once the parent reaches a status. When that happens, advance writes the status move and stops. It does not create the child.

Two conditions separate a move within the lifecycle from crossing into a child type. The gate makes the child creatable; starting it is a second, human step, handled by /lazy's stop-at-boundary rule. Satisfying the gate is necessary, not sufficient.

---

---
name: execute
description: Use when carrying out the work a delivery document describes -- building against its task breakdown and acceptance criteria in one pass.
---

```
ONE DOCUMENT, ONE AGENT, ONE PASS
```

Execute is the build pass. The agent running it does every task in the delivery document's breakdown **itself**, in order, TDD, and reports. It does not dispatch, does not review its own work, does not commit, and does not close the document.

Execute is terminal. It ends with a report saying the work is ready for review. Whoever ran it decides what happens next.

<HARD-GATE>
**Do NOT dispatch subagents.** You are the implementer. Every task in the breakdown is yours -- not in parallel, not one at a time, not "just a helper for this one task". If the breakdown is too large for your context, stop at a task boundary and report what landed and what remains. Running short is a reason to report, never a reason to delegate.
Do NOT begin without a delivery document that carries a task breakdown. If it lacks one, stop and report that the plan must be authored first.
Confirm from `lazyspec config --json` that the document's type is a delivery type in this DAG before starting.
Do NOT commit. Do NOT advance the document to a completion status. Do NOT review your own tasks and do NOT dispatch a reviewer.
Never `sleep` to wait for anything. If you are waiting, you are doing it wrong.
</HARD-GATE>

<NEVER>
- Do NOT write document files directly. Use `lazyspec create`, `lazyspec link`, `lazyspec update <id> --body`.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured DAG -- type boundaries come from the `edges` table and from nothing else; honor every edge.
- Do NOT do work the document did not ask for. Out-of-scope changes are a review finding against you.
</NEVER>

<RED-FLAGS>
STOP and do the task yourself if you catch yourself thinking:

| Rationalization | Reality |
|---|---|
| "This task is big enough to warrant a subagent" | The unit was sized for one agent. If it wasn't, that is a sizing defect -- report it, don't paper over it with fan-out. |
| "I'll dispatch just the mechanical parts" | Selective dispatch is fan-out. Depth is 0 from here. |
| "A helper agent would explore faster" | Exploration is yours. You already hold the chain. |
| "I'll have a reviewer check this before I report" | Review is your caller's, exactly once, after you report. A review you commissioned is the duplication this skill exists to remove. |
| "I'm nearly out of context, one agent finishes it" | Stop at the task boundary and report. A partial report is useful; a fanned-out finish is not. |
| "I'll commit so the work isn't lost" | Reporting does not lose work. The commit belongs to your caller. |

Violating the letter of this gate is violating its spirit.
</RED-FLAGS>

<GITHUB-ISSUES-DOCUMENTS>
Documents stored in GitHub Issues (store = "github-issues") are managed through the GitHub API. The `.lazyspec/cache/` directory contains read-only mirrors.
- Never edit files under `.lazyspec/cache/`. Use `lazyspec update <ID> --body` to modify content.
- Always use shorthand IDs (e.g. STORY-095) not cache file paths when referencing documents in `lazyspec link`, `lazyspec update`, `lazyspec show`, etc.
- To set body content at creation: `lazyspec create <type> <title> --body "content"`.
</GITHUB-ISSUES-DOCUMENTS>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. Read type/chain facts from the CLI, never from `.lazyspec/` graph files directly. On failure, check `--help` before retrying.

## No ceiling

Execute is **work, not authoring.** The `scaffold < co-write < generate` ceiling governs who writes a document's prose, not who does the work the document describes. It does not apply here.

## Preflight

1. `lazyspec config --json` -- confirm the document's type is a delivery type in this DAG (the type whose breakdown describes implementation work). Read it; never assume the name.
2. `lazyspec show <id> -e --json` -- the task breakdown and acceptance criteria, `@ref` directives expanded.
3. `lazyspec context --json` -- the full chain, for the intent and ACs you are building against.
4. `lazyspec convention --json` -- the project's convention and dictums. You build to these; a reviewer will check them by name.
5. **The gate command.** The repo's chained verification -- typecheck, tests, lint, format in one invocation returning one summary. If your caller gave you the exact string, use it. If not, find it (package scripts, justfile, Makefile, CI config). If none exists, say so in your report; do not invent one mid-run.
6. Extract every task from the breakdown before starting.

## Open the bracket

The delivery doc arrives at the status that precedes work (read the edge from `lazyspec config --json`; the shipped default calls it `accepted`). Advance it across that edge into the work-active status via /advance, so the status is true exactly while the loop runs. If it is already there, or no such edge exists, skip.

This is the **only** status write execute makes. Closing the bracket belongs to whoever reviews.

## The build loop

Work the tasks **sequentially**, in breakdown order. For each:

- Ask before guessing. Unclear requirement -> ask your caller, do not invent.
- TDD: failing test first, then implementation, then verify.
- Run **scoped** verification only -- the tests and checks this task touches. Not the full suite.
- Self-check before moving on: AC met, only what was asked (YAGNI), tests behavioural, isolated, deterministic, readable, specific.

Re-read the chain (`lazyspec context --json`, `lazyspec show`) when your context was compacted, or when the next task's scope would be a guess. Otherwise the preflight read stands for the whole pass.

## Close out

1. Run the gate command **once**, at the end. Report its result whether it passed or failed. Do not re-run it to make it look better; a failing gate is a fact your reviewer needs.
2. Run `lazyspec validate --json`, scoped to the documents you touched.
3. Report, and stop. The document sits at the work-active status, work ready for review.

## Report contract

Return exactly these lines, under 200 words total:

```
VERDICT: DONE | PARTIAL | BLOCKED
FILES: paste `git diff --stat` output, nothing else
GATE: the command you ran and its one summary line, or "none found"
HANDOFF: up to 3 lines -- facts a reader cannot get off the diff: names introduced that other code calls, invariants established, signatures changed
DEVIATIONS: up to 3 lines, each "<what you did instead> — <why>", or "none"
REMAINING: tasks not done and why, or "none"
BLOCKERS: what stopped you, or "none"
```

PARTIAL is a legitimate verdict. Report it and stop; do not delegate to finish.

---

---
name: orchestrate
description: Use when handed a batch of delivery documents to drive to done in one run -- several IDs to build, ordered by their dependency edges, each with review, commit, and status transitions, until the whole batch is done.
---

```
YOU ORDER, DISPATCH, REVIEW, COMMIT, CLOSE. YOU DO NOT BUILD.
```

Orchestrate drives a **chunk** -- the set of delivery documents you were handed -- to done. You own ordering, dispatch, review, commits, status transitions, and the done check. Each unit's building is a dispatched agent's, running /execute.

The chunk is whatever set you were given. It may span several parent documents, or sit under one. Nothing here assumes a type name; read every type, relation, and status from `lazyspec config --json`.

**REQUIRED SUB-SKILLS:** /execute (loaded by each build agent), /review-work (both passes), /advance (every status move).

<HARD-GATE>
**Depth 1. You are the only agent that spawns agents.** Every agent you dispatch does its own work alone. Check each dispatch against the fan-out table before you send it; if it is not a row, you do not need it.
**Never `sleep` to wait for an agent.** You are re-invoked when it finishes. Dispatch, then end your turn. That is the whole protocol.
Do NOT build, edit, or fix anything yourself. Dispatch it.
Do NOT re-run the gate command to confirm what an agent reported.
Order comes from the documents' dependency edges, never from the order you were handed.
</HARD-GATE>

<RED-FLAGS>
STOP and end your turn if you catch yourself:

- about to run `sleep`, for any reason
- about to run `git diff --stat` or `git status` to find out whether an agent is done
- writing "continuing to wait", "checking in on it", or counting how long an agent has run
- writing a brief that hands one unit's tasks to more than one agent
- reading a build agent's report and finding reviewer verdicts, a RED/GREEN loop, or a status it closed -- that agent broke /execute; treat its output as unreviewed
- editing a file yourself because the fix is one line

| Rationalization | Reality |
|---|---|
| "A short sleep is cheaper than a round trip" | `sleep 570` costs 570 seconds whether the agent took 20 seconds or 20 minutes, and buys nothing the notification would not have handed you. |
| "I have nothing else to do while it runs" | Nothing else to do is the signal to end the turn, not to sleep. |
| "The remaining units are queued on me" | They are queued on the agent. The notification unblocks them. |
| "One nested helper would speed this up" | One nested build can outspend three units put together. Depth 1. |
| "This fix is faster than dispatching" | Dispatching is the whole job. Your hands stay off the files. |
</RED-FLAGS>

If an agent has genuinely gone silent -- no notification well past anything plausible -- use one `ScheduleWakeup` of 1200s or more. Never a `sleep`, never a poll loop.

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`.

## Resolve order from edges, not from the list

The list you were handed is **not** the execution order.

1. Read the config's `relationships` and find the dependency relation -- the one whose name and inverse express blocking (in the shipped default that pair is `blocks` / `blocked-by`, but read it).
2. Read the real edges off the documents: `lazyspec list <type> --json` and `lazyspec context --json`.
3. Read the docs in **one** call, not one per doc: `for id in DOC-1 DOC-2 DOC-3; do lazyspec show "$id" --json; done`. Same for every later sweep over the batch.
4. Topologically sort. A unit is **eligible** iff every unit it depends on is already at its completion status.
5. Run **sequentially**, even where branches are independent. Batched units share files, and parallel agents collide and break one-commit-per-unit.

If the config declares no dependency relation, the order you were handed is the only order there is -- say so in your opening report rather than inferring one from IDs.

## Pre-flight

**Find the one gate command.** The repo's chained verification -- typecheck, tests, lint, format in a single invocation returning one summary. Every brief carries the exact string. If there is none, write one and commit it as the chunk's first commit.

**Split bundled scaffolding.** A new package, workspace member, build config, exports map, or generator sitting alongside feature work carries a different kind of risk and attracts most of the review. Give it its own unit, first in the order. If you cannot split it, say so in the report.

## Fan-out budget

For a chunk of N units, the whole run is:

| Agent | Count |
|---|---|
| Build | N |
| Per-unit review | N |
| Per-unit fix | only where that review returned RED |
| Chunk review | 1 |
| Chunk fix | 1 |

The rule is about **who spawns**, not how many run at once. A build agent that dispatches its tasks one at a time has broken depth 1 exactly as much as one that fans out six in parallel -- and pays a wait on every one. A unit with six tasks is six tasks for one agent, not six agents. /execute states this to the agent that loads it; you do not need to restate it.

## The per-unit loop

For each eligible unit, in topo order:

1. **Dispatch the build agent** (/execute). It opens the unit's work-active status itself, does every task, runs the gate once, and reports. You do not open the status; it does.
2. **Dispatch one review agent** (/review-work, depth **blocking-only**), handing it the diff range, the unit's ID, and the gate result the build agent reported.
3. **Act on the verdict.**
   - GREEN: go to 4.
   - RED: dispatch **one** fix agent with the UNMET and CONVENTION lines. Its gate is the gate command, not another reviewer. If it reports a finding it cannot satisfy, stop the chunk and report -- do not commit around it, do not loop.
   - STOP: halt the chunk and report. STOP means the unit is not what its document asked for; that is a planning problem, not a fix pass.
4. **Commit.** One per unit. Stage only that unit's changes. If on the default branch, branch first.
5. **Close the bracket.** /advance the unit to its completion status.
6. **Advance the parent, if appropriate.** See the status contract below.
7. Recompute the eligible set.

**Nothing returns to review.** The fix agent's output goes straight to the commit. A fix that introduces a fresh convention violation is caught by the chunk pass.

## The two review passes

Review happens at two points and only two.

| | **Per-unit** | **End of chunk** |
|---|---|---|
| When | Once, after the build agent reports | Once, after the last unit reaches its completion status |
| Depth | blocking-only | comprehensive |
| Scope | That unit's diff vs that unit's ACs | The chunk's whole combined diff, `git diff <base>..HEAD`, plus every document in the batch |
| Re-review | None | None |

Neither reviewer is capped -- /review-work emits everything it finds, ranked. The **fix** is budgeted, not the finding list: on the chunk pass, hand the fix agent the top 8 and record the remainder as follow-up documents. Truncating the reviewer hides drift; truncating the fix list is a scheduling decision you can see.

## Dispatch brief contract

Every brief has these parts, in this order.

1. **ROLE** -- build, review, or fix. One of the three, named, plus the skill to load (`/execute`, `/review-work`).
2. **READ FIRST** -- the commands, not the contents: `lazyspec show <id> -e --json` for the unit, one per ancestor carrying ACs. The agent reads the documents at full fidelity; you do not paste them, and you do not grep the source to write a better prompt.
3. **WHY** -- the parent's intent, one or two sentences.
4. **INHERITED** -- one line per unit already landed in this chunk: `<unit>: <what landed> -> <files>`. For a review brief, the build agent's report. For a fix brief, the reviewer's UNMET and CONVENTION lines verbatim.
5. **GATE** -- the gate command verbatim, and the last gate result you hold. Reviewers do not run it.
6. **SCOPE** -- for review, the depth and the diff range. For build, nothing: the document is the scope.
7. **ENVIRONMENT** -- only the invocations that differ from the obvious ones (sandbox vars, toolchain wrappers).
8. **CLOSING BLOCK** -- pasted verbatim, last. Paraphrasing it is the same as omitting it.

A well-formed brief runs 15-25 lines.

````markdown
## Rules for this run

Do not spawn subagents — not in parallel, not one at a time, not "just a helper for this one task".
This work is yours. If it is too large for your context, stop at a task boundary and report what
landed and what remains. Running short is a reason to report, never a reason to delegate.

Do not commit, and do not advance any document past the status your skill tells you to open.
The orchestrator that dispatched you owns the commit, the review and the close.

Never `sleep` to wait for anything. If you are waiting, you are doing it wrong.

Read files with `Read`, change them with `Edit` or `Write`. Not `cat -n`, `sed -n`, or `cat >` heredocs.
Structural queries in `.ts`, `.tsx`, `.js`, `.rs`, `.py` use `ast-grep`. `grep` is for file types
`ast-grep` does not parse, such as `.astro`.
JSON goes through `jq`. Not `python3`.
A command that would run more than about three times with different arguments becomes one shell loop.
````

## Status-transition contract

| Unit | Opened by | Closed by |
|---|---|---|
| **Delivery document** | The build agent, into the work-active status, as /execute's first act | You, to its completion status, after the commit |
| **Its parent document** | You, into the work-active status, when its first in-batch child starts (if the type's lifecycle has one) | **Not by you.** Advance it to its review status only, never to completion |

Completing a parent is a human or downstream decision. Advance a parent only when all of its in-batch children are done; if it owns children outside this batch, leave it and note that in the report. Order within a unit is commit, then the delivery document, then the parent -- /advance checks each type's gates at every transition.

## End of chunk: the comprehensive pass

Every unit at its completion status does **not** mean the chunk is done.

1. Dispatch one review agent (/review-work, depth **comprehensive**) over `git diff <base>..HEAD` plus every document in the batch. This is the pass the per-unit reviews deferred: nits, naming, duplication across units, dead code and scaffolding left behind, inconsistent patterns, missing tests, drifted seams, convention violations that only show at chunk scale.
2. Dispatch one fix agent with the top 8 findings. Tests stay green. Record everything it could not complete, plus every finding past the budget, as follow-up documents.
3. One cleanup commit for the whole chunk.
4. If the pass returns STOP, or surfaces something genuinely blocking (an AC unmet, broken integration between units), fix it in that same cleanup commit and say so. If it is too large to fix here, stop and report it rather than filing it silently.

One review, one fix, one commit. No cycle here either.

## Done

Done iff **every** delivery document in the batch is at its completion status, every in-batch parent has advanced to its review status (or been noted as left alone), **and the chunk pass has run with its cleanup commit landed**. Verify with `lazyspec status --json` and `lazyspec validate --json`, and confirm one commit per unit plus the cleanup commit.

Report the resolved order, the commits, per-unit findings, the chunk pass findings, the follow-up documents you filed, and any sandbox blockers you had to stop on.

## Common mistakes

- **Ordering by the list you were given.** Order comes from the dependency edges read off the documents.
- **Advancing a parent to completion.** Its target is the review status. Completion is downstream.
- **Re-reviewing inside a unit.** One review per unit. The fix agent's gate is the gate command.
- **Letting nits hold up a unit.** Naming, duplication and structure belong to the chunk pass.
- **Stopping at the last unit.** All-complete is not done. The chunk pass and its cleanup commit are part of done.
- **Running the comprehensive pass per unit.** It runs once, over the whole chunk diff.
- **Capping the reviewer.** Cap the fix, not the finding list.
- **Opening the delivery document's status yourself.** /execute does that, at the moment work actually starts.
- **Paraphrasing the closing block.** It is a paste, not a point to cover.
- **Sleeping to wait for an agent.** End the turn. The notification is the wake-up.

---

---
name: review
description: Use when critiquing a document -- its prose, its intent, its acceptance criteria -- before advancing its status.
---

```
CONFORMANCE FIRST, QUALITY SECOND
```

Review critiques **documents**. Its sibling /review-work critiques **code** against the document that specified it. If you are reading a diff rather than a document body, you are in the wrong skill.

<HARD-GATE>
Do NOT review quality before conformance. The document's acceptance criteria and declared intent come first; block on any conformance failure before looking at quality.
Do NOT approve without fresh verification evidence gathered in this session.
Do NOT review landed code here. Route to /review-work, which carries the convention stage and the diff verdicts.
</HARD-GATE>

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` and `lazyspec link`.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured DAG -- type boundaries come from the `edges` table and from nothing else; honor every edge.
</NEVER>

<GITHUB-ISSUES-DOCUMENTS>
Documents stored in GitHub Issues (store = "github-issues") are managed through the GitHub API. The `.lazyspec/cache/` directory contains read-only mirrors.
- Never edit files under `.lazyspec/cache/`. Use `lazyspec update <ID> --body` to modify content.
- Always use shorthand IDs (e.g. STORY-095) not cache file paths when referencing documents in `lazyspec link`, `lazyspec update`, `lazyspec show`, etc.
- To set body content at creation: `lazyspec create <type> <title> --body "content"`.
- To modify after creation: `lazyspec update <ID> --body "new content"`.
</GITHUB-ISSUES-DOCUMENTS>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. Read type/lifecycle facts from the CLI, never from `.lazyspec/` graph files directly. On failure, check `--help` before retrying.

## Preflight

1. `lazyspec config --json` -- read the type's `intent` (the bar to critique against) and its `lifecycle` (to know which status review precedes, so the pass-route to /advance targets the right edge).
2. `lazyspec show <id> --json` -- read the document and its acceptance criteria.
3. `lazyspec context --json` -- read the chain (parent intent and ACs) so conformance is judged against the right spec.

## Workflow

Two-stage critique:

**Stage 1 -- Conformance.** Does the document satisfy its declared intent and its acceptance criteria? Does it satisfy the relation `rules` its type carries? Block on any conformance failure.

**Stage 2 -- Quality.** Only after conformance passes: critique quality -- clarity, correctness, cohesion, whether the acceptance criteria are actually checkable, whether a delivery document's task breakdown is sized for one agent pass. Flag unjustified tradeoffs.

Express targets generically: "the document's acceptance criteria", "its declared intent". No type name is baked in.

## Routing

- **On pass:** route to /advance to move status along the lifecycle edge that review precedes.
- **On fail:** route back to the appropriate authoring verb, one at or below the type's ceiling: /scaffold, /co-write, or /generate.
- **Reviewing landed work rather than a document:** route to /review-work.

---

---
name: review-work
description: Use when critiquing landed code against the document that specified it -- one unit's diff, or a batch's combined diff -- before it is committed or its document is closed.
---

```
CONFORMANCE, CONVENTION, THEN QUALITY
```

Review-work critiques **code that exists** against the acceptance criteria of the document that asked for it. Its sibling /review critiques **documents** against their intent. If you are reading a diff, you are here.

<HARD-GATE>
Run the three stages in order. Do NOT report a quality finding while a conformance or convention finding is open -- the fix will change the code you were about to comment on.
Do NOT approve without evidence you gathered in this session: the diff, the documents, and the gate result you were handed.
Do NOT re-run the gate command. Your caller ran it and hands you the result. If they handed you none, say so and treat the gate as unknown -- do not run it yourself.
Do NOT edit code. You return findings; a separate pass fixes them.
</HARD-GATE>

<NEVER>
- Do NOT edit a document you haven't read. `lazyspec show <id> -e --json` first.
- Do NOT invent acceptance criteria. Every conformance finding cites an AC that is written down.
- Do NOT emit a convention finding without naming the principle or dictum it violates.
</NEVER>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`.

## Depth

Your caller states one. If they state none, assume **comprehensive**.

| | **blocking-only** | **comprehensive** |
|---|---|---|
| When | After one unit's build, before its commit | Once over a batch's whole combined diff |
| Emits | Stage 1 and Stage 2 findings only | All three stages: also nits, naming, duplication across units, dead code, scaffolding left behind, drifted seams, missing tests |
| Stage 3 | Not run | Run, if stages 1 and 2 are clear |
| Dropped findings | Style, naming, structure -- the comprehensive pass catches them | Nothing is dropped |

Neither depth is capped. Emit every finding you have, ranked most severe first. Budgeting *which findings get fixed* is your caller's decision, not yours -- do not truncate to make their job smaller.

## Preflight

1. `lazyspec show <id> -e --json` for the delivery document, and one per ancestor carrying acceptance criteria. These are the bar.
2. `lazyspec context --json` -- the chain, so conformance is judged against the right intent.
3. `lazyspec convention --json` -- the project's principles and dictums. This is Stage 2's bar.
4. The diff. Your caller names the range; read it with `git diff`.

## Stage 1 -- Acceptance conformance

Every acceptance criterion on the delivery document and its ancestors: met, or not. For each unmet one, name the AC and what is missing. A criterion with no test covering it is unmet, however plainly the code appears to do it.

Also flag work the documents did not ask for. Scope creep is a conformance failure in the other direction.

## Stage 2 -- Convention conformance

Read the diff against `lazyspec convention --json`. Every finding **names the principle or dictum it violates, verbatim**, then points at the file and line that violates it. A finding you cannot attribute to a written principle is not a convention finding -- it is Stage 3 quality, and it waits.

Where principles conflict, the convention's own tiebreaker applies; say which one you applied.

Convention findings block. This stage exists because conventions were previously handed to builders and checked by nobody.

## Stage 3 -- Quality

Only when Stages 1 and 2 are clear. Correctness, clarity, YAGNI, DRY, security. Test properties: behavioural, structure-insensitive, isolated, deterministic, readable, specific. Flag unjustified tradeoffs.

## Verdicts

- **GREEN** -- no Stage 1 or Stage 2 findings. Stage 3 notes may still be attached.
- **RED** -- findings a fix pass can clear.
- **STOP** -- the work is not what the document asked for, and fixing it is not a patch job. Use STOP when an AC was not attempted at all, when the gate result you were handed is failing for reasons inside this scope, or when the diff pursues a different design from the one the document specifies. STOP means the caller halts the run and reports; it is not a louder RED.

## Report contract

Return exactly these lines, under 300 words total:

```
VERDICT: GREEN | RED | STOP
UNMET: one line per unmet acceptance criterion, "<AC> — <what is missing>" (omit when none)
CONVENTION: one line per violation, "<principle or dictum, quoted> — <file:line> — <what violates it>" (omit when none)
QUALITY: one line per finding, ranked (omit when not run or none)
NOTED: up to 3 observations worth carrying forward
```

Your evidence gathering and reasoning stay in your own session. The verdict lines are what the caller acts on.

---

---
name: systematic-debugging
description: Use when encountering any bug, test failure, or unexpected behavior, before proposing fixes
---

# Systematic Debugging

## Overview

Random fixes waste time and create new bugs. Quick patches mask underlying issues.

**Core principle:** ALWAYS find root cause before attempting fixes. Symptom fixes are failure.

**Violating the letter of this process is violating the spirit of debugging.**

## The Iron Law

```
NO FIXES WITHOUT ROOT CAUSE INVESTIGATION FIRST
```

If you haven't completed Phase 1, you cannot propose fixes.

## When to Use

Use for ANY technical issue:
- Test failures
- Bugs in production
- Unexpected behavior
- Performance problems
- Build failures
- Integration issues

**Use this ESPECIALLY when:**
- Under time pressure (emergencies make guessing tempting)
- "Just one quick fix" seems obvious
- You've already tried multiple fixes
- Previous fix didn't work
- You don't fully understand the issue

**Don't skip when:**
- Issue seems simple (simple bugs have root causes too)
- You're in a hurry (rushing guarantees rework)
- Manager wants it fixed NOW (systematic is faster than thrashing)

## The Four Phases

You MUST complete each phase before proceeding to the next.

### Phase 1: Root Cause Investigation

**BEFORE attempting ANY fix:**

1. **Read Error Messages Carefully**
   - Don't skip past errors or warnings
   - They often contain the exact solution
   - Read stack traces completely
   - Note line numbers, file paths, error codes

2. **Reproduce Consistently**
   - Can you trigger it reliably?
   - What are the exact steps?
   - Does it happen every time?
   - If not reproducible → gather more data, don't guess

3. **Check Recent Changes**
   - What changed that could cause this?
   - Git diff, recent commits
   - New dependencies, config changes
   - Environmental differences

4. **Gather Evidence in Multi-Component Systems**

   **WHEN system has multiple components (CI → build → signing, API → service → database):**

   **BEFORE proposing fixes, add diagnostic instrumentation:**
   ```
   For EACH component boundary:
     - Log what data enters component
     - Log what data exits component
     - Verify environment/config propagation
     - Check state at each layer

   Run once to gather evidence showing WHERE it breaks
   THEN analyze evidence to identify failing component
   THEN investigate that specific component
   ```

   **Example (multi-layer system):**
   ```bash
   # Layer 1: Workflow
   echo "=== Secrets available in workflow: ==="
   echo "IDENTITY: ${IDENTITY:+SET}${IDENTITY:-UNSET}"

   # Layer 2: Build script
   echo "=== Env vars in build script: ==="
   env | grep IDENTITY || echo "IDENTITY not in environment"

   # Layer 3: Signing script
   echo "=== Keychain state: ==="
   security list-keychains
   security find-identity -v

   # Layer 4: Actual signing
   codesign --sign "$IDENTITY" --verbose=4 "$APP"
   ```

   **This reveals:** Which layer fails (secrets → workflow ✓, workflow → build ✗)

5. **Trace Data Flow**

   **WHEN error is deep in call stack, trace it backward to the source:**
   - Where does bad value originate?
   - What called this with bad value?
   - Keep tracing up until you find the source
   - Fix at source, not at symptom

### Phase 2: Pattern Analysis

**Find the pattern before fixing:**

1. **Find Working Examples**
   - Locate similar working code in same codebase
   - What works that's similar to what's broken?

2. **Compare Against References**
   - If implementing pattern, read reference implementation COMPLETELY
   - Don't skim - read every line
   - Understand the pattern fully before applying

3. **Identify Differences**
   - What's different between working and broken?
   - List every difference, however small
   - Don't assume "that can't matter"

4. **Understand Dependencies**
   - What other components does this need?
   - What settings, config, environment?
   - What assumptions does it make?

### Phase 3: Hypothesis and Testing

**Scientific method:**

1. **Form Single Hypothesis**
   - State clearly: "I think X is the root cause because Y"
   - Write it down
   - Be specific, not vague

2. **Test Minimally**
   - Make the SMALLEST possible change to test hypothesis
   - One variable at a time
   - Don't fix multiple things at once

3. **Verify Before Continuing**
   - Did it work? Yes → Phase 4
   - Didn't work? Form NEW hypothesis
   - DON'T add more fixes on top

4. **When You Don't Know**
   - Say "I don't understand X"
   - Don't pretend to know
   - Ask for help
   - Research more

### Phase 4: Implementation

**Fix the root cause, not the symptom:**

1. **Create Failing Test Case**
   - Simplest possible reproduction
   - Automated test if possible
   - One-off test script if no framework
   - MUST have before fixing
   - Use a test-driven-development skill for writing proper failing tests, if one is available

2. **Implement Single Fix**
   - Address the root cause identified
   - ONE change at a time
   - No "while I'm here" improvements
   - No bundled refactoring

3. **Verify Fix**
   - Test passes now?
   - No other tests broken?
   - Issue actually resolved?

4. **If Fix Doesn't Work**
   - STOP
   - Count: How many fixes have you tried?
   - If < 3: Return to Phase 1, re-analyze with new information
   - **If ≥ 3: STOP and question the architecture (step 5 below)**
   - DON'T attempt Fix #4 without architectural discussion

5. **If 3+ Fixes Failed: Question Architecture**

   **Pattern indicating architectural problem:**
   - Each fix reveals new shared state/coupling/problem in different place
   - Fixes require "massive refactoring" to implement
   - Each fix creates new symptoms elsewhere

   **STOP and question fundamentals:**
   - Is this pattern fundamentally sound?
   - Are we "sticking with it through sheer inertia"?
   - Should we refactor architecture vs. continue fixing symptoms?

   **Discuss with your human partner before attempting more fixes**

   This is NOT a failed hypothesis - this is a wrong architecture.

## Red Flags - STOP and Follow Process

If you catch yourself thinking:
- "Quick fix for now, investigate later"
- "Just try changing X and see if it works"
- "Add multiple changes, run tests"
- "Skip the test, I'll manually verify"
- "It's probably X, let me fix that"
- "I don't fully understand but this might work"
- "Pattern says X but I'll adapt it differently"
- "Here are the main problems: [lists fixes without investigation]"
- Proposing solutions before tracing data flow
- **"One more fix attempt" (when already tried 2+)**
- **Each fix reveals new problem in different place**

**ALL of these mean: STOP. Return to Phase 1.**

**If 3+ fixes failed:** Question the architecture (see Phase 4.5)

## your human partner's Signals You're Doing It Wrong

**Watch for these redirections:**
- "Is that not happening?" - You assumed without verifying
- "Will it show us...?" - You should have added evidence gathering
- "Stop guessing" - You're proposing fixes without understanding
- "Ultrathink this" - Question fundamentals, not just symptoms
- "We're stuck?" (frustrated) - Your approach isn't working

**When you see these:** STOP. Return to Phase 1.

## Common Rationalizations

| Excuse | Reality |
|--------|---------|
| "Issue is simple, don't need process" | Simple issues have root causes too. Process is fast for simple bugs. |
| "Emergency, no time for process" | Systematic debugging is FASTER than guess-and-check thrashing. |
| "Just try this first, then investigate" | First fix sets the pattern. Do it right from the start. |
| "I'll write test after confirming fix works" | Untested fixes don't stick. Test first proves it. |
| "Multiple fixes at once saves time" | Can't isolate what worked. Causes new bugs. |
| "Reference too long, I'll adapt the pattern" | Partial understanding guarantees bugs. Read it completely. |
| "I see the problem, let me fix it" | Seeing symptoms ≠ understanding root cause. |
| "One more fix attempt" (after 2+ failures) | 3+ failures = architectural problem. Question pattern, don't fix again. |

## Quick Reference

| Phase | Key Activities | Success Criteria |
|-------|---------------|------------------|
| **1. Root Cause** | Read errors, reproduce, check changes, gather evidence | Understand WHAT and WHY |
| **2. Pattern** | Find working examples, compare | Identify differences |
| **3. Hypothesis** | Form theory, test minimally | Confirmed or new hypothesis |
| **4. Implementation** | Create test, fix, verify | Bug resolved, tests pass |

## When Process Reveals "No Root Cause"

If systematic investigation reveals issue is truly environmental, timing-dependent, or external:

1. You've completed the process
2. Document what you investigated
3. Implement appropriate handling (retry, timeout, error message)
4. Add monitoring/logging for future investigation

**But:** 95% of "no root cause" cases are incomplete investigation.

## Supporting Techniques

Techniques that compose with the four phases:

- **Root-cause tracing** - trace a bad value backward through the call stack to its original trigger; fix at the source.
- **Defense in depth** - after finding the root cause, add validation at multiple layers so the class of bug cannot recur.
- **Condition-based waiting** - for timing-dependent bugs, replace arbitrary timeouts with polling on the actual condition.

Compose with a test-driven-development skill (Phase 4, Step 1: the failing test) and a verification-before-completion discipline (verify the fix before claiming success) where those are available.

## Real-World Impact

From debugging sessions:
- Systematic approach: 15-30 minutes to fix
- Random fixes approach: 2-3 hours of thrashing
- First-time fix rate: 95% vs 40%
- New bugs introduced: Near zero vs common

---

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

When the only remaining next step would create a child of a **different type** -- crossing a type-boundary edge (a `chain` row in `edges` whose `to` admits this document's type, per the HARD-GATE) -- `/lazy` **STOPS.** It reports the boundary and what the human can do next; it never auto-runs `create <child-type>`.

This holds **even when a `require_parent_status` gate is already satisfied.** Gate-clear makes the child _eligible_, not _automatic_. Crossing a type boundary is always human-initiated. Report it with the ceiling verb for the child type (per Authorship-aware dispatch: `human` -> /scaffold, `assisted` -> /co-write, `generated` -> /generate), like:

> `<doc>` (type `<type>`) is at status `<status>`; its child type `<child-type>` is now eligible to create. Crossing types is human-initiated -- run <ceiling-verb> to start one.

**Multi-hop:** if the required parent type is itself empty (e.g. an iteration needs a story, but no story exists), report the FULL chain the human must author in order -- each hop is a separate human-initiated crossing -- not just the nearest one.

with every value read from config + status for that run.

## Validate after each mutation

`/lazy` is the chokepoint for graph integrity. After every dispatched verb that **mutates the graph** -- `/advance` (status move plus relations) and the authoring verbs `/scaffold`, `/co-write`, `/generate` (create plus link) -- run `lazyspec validate --json` before looping back to locate.

- **Scope to the doc just touched.** `validate` is a whole-repo check and will report pre-existing findings across unrelated documents. Filter its output to findings naming the document this mutation created, linked, or advanced. Fix only the broken or dangling relation **this mutation introduced** before continuing. Do not block on pre-existing repo-wide findings.
- `/review` and `/review-work` are not graph mutators, so they need no validate step here. `/execute` runs its own `validate` at close-out, and `/orchestrate` runs one at its done check.
- **Known limitation:** invoking a mutating verb standalone -- outside `/lazy` -- skips this check. `/lazy` is the canonical entry router; that is where graph integrity is enforced.
