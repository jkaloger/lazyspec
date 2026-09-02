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

Completing a parent is a human or downstream decision. Advance a parent only when all of its in-batch children are done; if it owns children outside this batch, leave it and note that in the report. Order within a unit is commit, then the delivery document, then the parent -- /advance proposes only a status the type's own `lifecycle` edges permit.

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
