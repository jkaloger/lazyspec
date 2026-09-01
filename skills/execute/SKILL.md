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
- Do NOT skip the workflow pipeline. Respect the configured `parent_type` chain and `rules`.
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
