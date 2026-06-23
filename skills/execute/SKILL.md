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
</NEVER>

<RED-FLAGS>
STOP and dispatch a subagent if you catch yourself thinking:
- "This is only ~N lines, dispatching is overkill"
- "Faster to just edit it inline, then I'll dispatch the rest"
- "It's a trivial / mechanical / obvious change"
- "I already know the exact diff, no need for a subagent"
- "I'll implement it and have a subagent review afterwards"

All of these mean: write the task text, dispatch the implementer, run the review loop. The orchestrator's hands stay off the files. Violating the letter of this gate is violating its spirit.

| Rationalization                           | Reality                                                                                        |
| ----------------------------------------- | ---------------------------------------------------------------------------------------------- |
| "Change is tiny, ~N lines"                | Tiny changes break things too, and the review loop is cheap. Size is not a dispatch criterion. |
| "I already have the diff in mind"         | Then the task text is trivial to write. Dispatch it.                                           |
| "Momentum -- just do it"                  | Momentum is the pressure this gate exists to resist. Dispatch.                                 |
| "I'll dispatch the non-trivial ones only" | Every task is dispatched. Selective dispatch is no dispatch discipline at all.                 |

</RED-FLAGS>

<GITHUB-ISSUES-DOCUMENTS>
Documents stored in GitHub Issues (store = "github-issues") are managed through the GitHub API. The `.lazyspec/cache/` directory contains read-only mirrors.
- Never edit files under `.lazyspec/cache/`. Use `lazyspec update <ID> --body` to modify content.
- Always use shorthand IDs (e.g. STORY-095) not cache file paths when referencing documents in `lazyspec link`, `lazyspec update`, `lazyspec show`, etc.
- To set body content at creation: `lazyspec create <type> <title> --body "content"` or `--body-file <path>`.
- To modify after creation: `lazyspec update <ID> --body "new content"` or `--body-file <path>`.
</GITHUB-ISSUES-DOCUMENTS>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. Read type/chain facts from the CLI, never from `.lazyspec/` graph files directly. On failure, check `--help` before retrying.

## No Ceiling

Execute is **work, not authoring.** The `scaffold < co-write < generate` ceiling does not apply here -- it governs who writes a document's prose, not who does the work the document describes. Do not confuse execute with the authoring trio.

## Preflight

1. `lazyspec config --json` -- confirm the document's `<type>` is a delivery type in this DAG (the type whose breakdown describes implementation work; in the shipped default config that is the `iteration` type, but read it -- do not assume the name).
2. `lazyspec show <id> --json` -- read the task breakdown and acceptance criteria.
3. `lazyspec context --json` -- pull the full chain (parent and grandparent docs) for intent and ACs.
4. `lazyspec convention --json` -- load codebase conventions. Include non-empty results in subagent prompts under `## Convention Context`.
5. Extract every task from the breakdown before dispatching any subagent.

## Subagent Dispatch

| Operation                   | Agent type      | Model tier   | Context to provide                                                                         |
| --------------------------- | --------------- | ------------ | ------------------------------------------------------------------------------------------ |
| Implement task              | general-purpose | most capable | Full task text, parent intent, acceptance criteria, prior task results, convention context |
| Review task (AC compliance) | general-purpose | most capable | Task text, acceptance criteria, implementer report                                         |
| Review task (code quality)  | general-purpose | lighter      | Changed files, scoped test output, quality criteria                                        |
| Final review                | general-purpose | most capable | All acceptance criteria, full implementation summary                                       |

Model tier is by capability, not product name: "most capable" for implementation and the correctness-bearing reviews, a "lighter" model for the code-quality pass. The orchestrator picks the concrete model.

## Per-Task Loop

Iterate the breakdown's tasks **sequentially**. For each task, dispatch an implementer subagent (general-purpose, most capable model) with:

- **Full task text** copied from the document (not a file reference).
- Scene-setting: parent intent (1-2 sentences), relevant acceptance criteria, prior task results.
- Lazyspec workflow rules: use the `lazyspec` CLI for doc ops, `--json` always, `--help` before unfamiliar commands, `lazyspec show -e <id>` to expand `@ref` directives.
- "Before you begin: ask questions about unclear requirements. Don't guess."
- TDD: failing test first, then implementation, then verify.
- Run **scoped** verification only -- just the tests/checks this task touches, never the full suite mid-loop. The full check runs once, at the orchestrator's Final Review.
- Self-review: completeness (ACs met?), YAGNI (only what was asked?), test quality (behavioral, isolated, deterministic, readable, specific).
- Report: what was implemented, verification results, files changed, concerns.

Handle implementer questions before letting them proceed: answer, then re-dispatch.

After the implementer reports, dispatch a **separate** reviewer subagent with task text, acceptance criteria, and the implementer report:

- **Stage 1 (AC compliance):** Run the scoped checks covering this task's ACs, verify each claimed AC has a passing test, check for missing requirements or scope creep. If any AC is unmet, report specifics. Do NOT run the full suite here -- that is the orchestrator's Final Review gate.
- **Stage 2 (code quality, only if Stage 1 passes):** Correctness, clarity, YAGNI, DRY, security. Evaluate test properties (behavioral, structure-insensitive, isolated, deterministic, readable, specific). Flag unjustified tradeoffs.

On failure: dispatch a fresh implementer with the specific issues, then re-review. Repeat until both stages pass. Mark the task complete.

## Context Refresh

Every 2 completed tasks, re-read the chain (`lazyspec context`, `lazyspec show` for the delivery document and its parent) to prevent drift.

## Final Review

The orchestrator runs this gate itself after all tasks complete. Subagents only ran scoped checks; this is the one place the full check runs.

1. Verify every task in the breakdown is complete, no out-of-scope work.
2. **Run the full check once.** It must pass -- required gate, no acceptance on failure. On failure, dispatch a targeted fix subagent, then re-run.
3. Run `lazyspec validate --json`.
4. Dispatch a final reviewer (most capable model) with all acceptance criteria and the implementation summary.
5. On pass, route to /review for critique, then to /advance for the status move. **/advance owns the status transition** -- execute does not move statuses itself.
