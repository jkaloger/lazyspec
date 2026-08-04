---
name: execute
description: Use when carrying out the work a delivery document describes -- the build loop -- against its task breakdown and acceptance criteria.
---

```
DO THE WORK THE DOCUMENT DESCRIBES
```

Execute is the build loop: it orchestrates the task breakdown of a delivery document, dispatching a subagent per task and verifying each against its acceptance criteria.

<HARD-GATE>
Do NOT begin execution without a delivery document that carries a task breakdown. If the document lacks one, author it first (route to the appropriate authoring verb).
Confirm from `lazyspec config --json` that the document's type is a delivery type in this DAG before starting.
ALWAYS use subagents for the work. The orchestrator dispatches; it does not implement.
NO SIZE EXCEPTION. A one-line change, a typo, a single-function edit, a "trivial" fix -- all dispatched to a subagent. The orchestrator NEVER edits implementation, test, or documentation files itself, no matter how small the task looks or how fast it would be to do inline. "Too small to dispatch" is not a carve-out; it is the most common way this gate is broken.
Each task must carry enough detail for a zero-context subagent. If it does not, fix the breakdown first.
</HARD-GATE>

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` and `lazyspec link`.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured `parent_type` chain and `rules`.
- Do NOT implement tasks yourself, regardless of size. Dispatch a subagent per task. Do NOT dispatch parallel implementers.
- Do NOT read, grep, or explore implementation files to build a dispatch prompt. The READ FIRST slot sends the subagent to the source at full fidelity.
</NEVER>

<RED-FLAGS>
STOP and dispatch a subagent if you catch yourself thinking:
- "This is only ~N lines, dispatching is overkill"
- "Faster to just edit it inline, then I'll dispatch the rest"
- "It's a trivial / mechanical / obvious change"
- "I already know the exact diff, no need for a subagent"
- "I'll implement it and have a subagent review afterwards"
- "Let me grep the source first so I can write a precise prompt"

All of these mean: fill the prompt slots, dispatch the implementer, run the review loop. The orchestrator's hands stay off the files, including reading them. Violating the letter of this gate is violating its spirit.

| Rationalization                           | Reality                                                                                           |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------- |
| "Change is tiny, ~N lines"                | Tiny changes break things too, and the review loop is cheap. Size is not a dispatch criterion.    |
| "I already have the diff in mind"         | Then the task text is trivial to write. Dispatch it.                                              |
| "Momentum -- just do it"                  | Momentum is the pressure this gate exists to resist. Dispatch.                                    |
| "I'll dispatch the non-trivial ones only" | Every task is dispatched. Selective dispatch is no dispatch discipline at all.                    |
| "I need to read the code to scope it"     | The subagent reads the code. Your slots are chain intent, what landed, environment, return shape. |

</RED-FLAGS>

<GITHUB-ISSUES-DOCUMENTS>
Documents stored in GitHub Issues (store = "github-issues") are managed through the GitHub API. The `.lazyspec/cache/` directory contains read-only mirrors.
- Never edit files under `.lazyspec/cache/`. Use `lazyspec update <ID> --body` to modify content.
- Always use shorthand IDs (e.g. STORY-095) not cache file paths when referencing documents in `lazyspec link`, `lazyspec update`, `lazyspec show`, etc.
- To set body content at creation: `lazyspec create <type> <title> --body "content"`.
- To modify after creation: `lazyspec update <ID> --body "new content"`.
</GITHUB-ISSUES-DOCUMENTS>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. Read type/chain facts from the CLI, never from `.lazyspec/` graph files directly. On failure, check `--help` before retrying.

## No Ceiling

Execute is **work, not authoring.** The `scaffold < co-write < generate` ceiling does not apply here -- it governs who writes a document's prose, not who does the work the document describes. Do not confuse execute with the authoring trio.

## Status bracketing

A document's status should track reality: the work-active status (in the shipped default lifecycle, `in-progress`) means the build loop is running right now, not "queued" and not "finished". Because execute is the only step that runs the build loop, it is the only step positioned to open and close that status. So execute brackets its work with two status moves, both dispatched through /advance (the sole status writer -- execute never writes status frontmatter directly):

- **Open (before the first task).** The delivery doc arrives at the status that precedes work (the default DAG calls it `accepted`; read the edge from `lazyspec config --json`, do not assume the name). Advance it across the edge into the work-active status (`accepted -> in-progress`) so the status reflects that work has started. If the doc is already in the work-active status, or no such edge exists, skip.
- **Close (at Final Review).** After /review passes, advance the work-active status to its completion status (`in-progress -> complete`).

This is why the previous flow left `in-progress` meaningless: nothing opened it at the start, so the doc sat at `accepted` through the whole loop and only passed through `in-progress` in a single tick at the very end. Opening it here keeps `in-progress` true exactly while the loop runs.

## Preflight

1. `lazyspec config --json` -- confirm the document's `<type>` is a delivery type in this DAG (the type whose breakdown describes implementation work; in the shipped default config that is the `iteration` type, but read it -- do not assume the name).
2. `lazyspec show <id> --json` -- read the task breakdown and acceptance criteria.
3. `lazyspec context --json` -- pull the full chain (parent and grandparent docs) for intent and ACs.
4. `lazyspec convention --json` -- load codebase conventions. Non-empty results go in the CONVENTION slot of every dispatch prompt.
5. Extract every task from the breakdown before dispatching any subagent.

## Subagent Dispatch

| Operation                 | Agent type      | Model tier   |
| ------------------------- | --------------- | ------------ |
| Implement task            | general-purpose | capable      |
| Review task (both stages) | general-purpose | capable      |
| Final review              | general-purpose | most capable |

Model tier is by capability, not product name: "most capable" for implementation and the correctness-bearing reviews. The orchestrator picks the concrete model.

Every prompt is built from the Dispatch Prompt Contract and ends with a Report Contract.

## Dispatch Prompt Contract

A dispatch prompt is exactly these slots, in this order, at these lengths:

1. **ROLE** — one line: task ID, repo path, branch, skills to load.
2. **READ FIRST** — `lazyspec show <id> -e --json` for the delivery doc, plus one command per ancestor doc carrying ACs. Commands and IDs only.
3. **WHY** — parent intent, 1-2 sentences.
4. **INHERITED** — one line per completed task: `<task>: <what landed> -> <files>`.
5. **CONVENTION** — non-empty `lazyspec convention --json` output, verbatim.
6. **ENVIRONMENT** — only the invocations that differ from the obvious ones (sandbox vars, toolchain wrappers).
7. **SCOPE EDGE** — one line naming what a later task owns, when an adjacent task could be mistaken for this one's.
8. **VERIFY** — the scoped check command for this task.
9. **RETURN** — the Report Contract, pasted verbatim.

Requirements, task steps, and acceptance criteria reach the subagent through slot 2. The doc is the source, read at full fidelity by the agent that acts on it, so the prompt carries only what the doc does not: chain intent, what already landed, environment, and the return shape. A well-formed prompt runs 15-25 lines.

## Report Contract

Paste this into slot 9 of every implementer prompt. It is the whole report — first line to last.

```
Return exactly these lines, under 200 words total:
VERDICT: DONE | BLOCKED
FILES: paste `git diff --stat` output, nothing else
CHECKS: the one summary line from each command you ran (e.g. "test result: ok. 1173 passed; 0 failed")
HANDOFF: up to 3 lines — facts a later task cannot read off the diff: names introduced that other code calls, invariants established, signatures changed
DEVIATIONS: up to 3 lines, each "<what you did instead> — <why>", or "none"
BLOCKERS: what stopped you, or "none"
```

Reviewer prompts get this instead:

```
Return exactly these lines, under 150 words total:
VERDICT: GREEN | RED
UNMET: one line per unmet acceptance criterion, "<AC> — <what is missing>" (omit when GREEN)
BLOCKING: one line per defect that must be fixed before this task closes (omit when none)
NOTED: up to 3 non-blocking observations worth carrying forward
```

Your evidence gathering, file reads, and reasoning stay in your own session; the verdict lines are what the orchestrator acts on.

## Per-Task Loop

**First, open the bracket** (see Status bracketing): advance the delivery doc into its work-active status (`accepted -> in-progress` in the default DAG) before dispatching any implementer. This is the one status move that opens the loop.

Iterate the breakdown's tasks **sequentially**. For each task, dispatch an implementer subagent (general-purpose, most capable) with a prompt built from the Dispatch Prompt Contract, its ROLE slot naming this skill among the skills to load — the standing rules below are how the subagent gets its working method, so the prompt does not restate them.

Handle implementer questions before letting them proceed: answer, then re-dispatch.

After the implementer reports, dispatch a **separate** reviewer subagent. Its prompt uses the same contract, with the implementer's report pasted into INHERITED and both review stages named in ROLE:

- **Stage 1 (AC compliance):** Run the scoped checks covering this task's ACs, verify each claimed AC has a passing test, check for missing requirements or scope creep.
- **Stage 2 (code quality, only if Stage 1 passes):** Correctness, clarity, YAGNI, DRY, security. Evaluate test properties (behavioral, structure-insensitive, isolated, deterministic, readable, specific). Flag unjustified tradeoffs.

On RED: dispatch a fresh implementer whose INHERITED slot carries the reviewer's UNMET and BLOCKING lines, then re-review. Repeat until GREEN. Mark the task complete.

## Standing Rules for Dispatched Subagents

These apply to every subagent this skill dispatches. Follow them from this skill; the prompt will not repeat them.

- Use the `lazyspec` CLI for doc ops, `--json` always, `--help` before unfamiliar commands, `lazyspec show -e <id>` to expand `@ref` directives.
- Before you begin: ask questions about unclear requirements. Don't guess.
- TDD: failing test first, then implementation, then verify.
- Run **scoped** verification only -- just the tests/checks this task touches, never the full suite. The full check runs once, at the orchestrator's Final Review.
- Self-review before reporting: completeness (ACs met?), YAGNI (only what was asked?), test quality (behavioral, isolated, deterministic, readable, specific).
- Do not commit and do not change document status. Those belong to the orchestrator.
- Report in the Report Contract shape given in your RETURN slot.

## Context Refresh

Re-read the chain (`lazyspec context --json`, `lazyspec show` for the delivery document and its parent) when one of these is true:

- a reviewer reported scope drift, or cited an AC you do not currently hold
- an implementer asked a question you cannot answer from what you hold
- the next task's INHERITED or SCOPE EDGE slot would be a guess
- your context was compacted since the last read

Otherwise the Preflight read stands for the whole loop.

## Final Review

The orchestrator runs this gate itself after all tasks complete. Subagents only ran scoped checks; this is the one place the full check runs.

1. Verify every task in the breakdown is complete, no out-of-scope work.
2. **Run the full check once.** It must pass -- required gate, no acceptance on failure. On failure, dispatch a targeted fix subagent, then re-run.
3. Run `lazyspec validate --json`.
4. Dispatch a final reviewer (most capable model). Its prompt uses the Dispatch Prompt Contract -- READ FIRST sends it to the delivery doc and its ancestors for the ACs, INHERITED carries one line per completed task, RETURN carries the reviewer Report Contract.
5. On pass, route to /review for critique, then to /advance to close the bracket: move the work-active status to its completion status (`in-progress -> complete` in the default DAG). **/advance owns the status write** -- execute dispatches it at the work boundaries (open into `in-progress` at loop start, close to `complete` here) but never writes status frontmatter itself.
