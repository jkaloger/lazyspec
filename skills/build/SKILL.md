---
name: build
description: Use when an Iteration has a task breakdown and is ready for implementation. Dispatches subagent per task with review between tasks.
---

```
NO IMPLEMENTATION WITHOUT A PLAN
```

If the Iteration doesn't have a numbered task breakdown in `## Changes`, use `/create-iteration` first.

<HARD-GATE>
Do NOT begin implementation without a complete Iteration document with numbered
task breakdown. Each task must have enough detail for a zero-context subagent.
ALWAYS use subagents for development.
</HARD-GATE>

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` and `lazyspec link`.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Features need RFC -> Story -> Iteration. Bug fixes need Iteration.
- Do NOT implement tasks yourself. Dispatch a subagent per task. Do NOT dispatch parallel implementers.
</NEVER>

<GITHUB-ISSUES-DOCUMENTS>
Documents stored in GitHub Issues (store = "github-issues") are managed through the GitHub API. The `.lazyspec/cache/` directory contains read-only mirrors.
- Never edit files under `.lazyspec/cache/`. Use `lazyspec update <ID> --body` to modify content.
- Always use shorthand IDs (e.g. STORY-095) not cache file paths when referencing documents in `lazyspec link`, `lazyspec update`, `lazyspec show`, etc.
- To set body content at creation: `lazyspec create <type> <title> --body "content"` or `--body-file <path>`.
- To modify after creation: `lazyspec update <ID> --body "new content"` or `--body-file <path>`.
</GITHUB-ISSUES-DOCUMENTS>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. On failure, check `--help` before retrying.

## Workflow

```d2
Read iteration -> Extract all tasks -> Create task tracking

Per task {
  Dispatch implementer subagent -> Questions? -> Answer, re-dispatch: yes
  Questions? -> Implementer works + self-reviews: no
  Implementer works + self-reviews -> Dispatch reviewer subagent
  Dispatch reviewer subagent -> AC compliance passes?
  AC compliance passes? -> Implementer fixes -> Dispatch reviewer subagent: no
  AC compliance passes? -> Code quality passes?: yes
  Code quality passes? -> Implementer fixes quality -> Dispatch reviewer subagent: no
  Code quality passes? -> Mark task complete: yes
}

Mark task complete -> More tasks?

More tasks?.shape: diamond
More tasks? -> Per task: yes
More tasks? -> Final full review: no

Final full review -> All Story ACs met?

All Story ACs met?.shape: diamond
All Story ACs met? -> Done: yes
All Story ACs met? -> Fix gaps: no

Done.shape: double_circle
```

## Preflight

0. Load convention context: `lazyspec convention --tags build,testing,architecture --json`. Include non-empty results in subagent prompts under `## Convention Context`.
1. Resolve: `lazyspec context <iteration-id> --json`
2. Read iteration tasks: `lazyspec show <iteration-id> --json`
3. Read Story ACs: `lazyspec show <story-id> --json`
4. Read RFC intent: `lazyspec show <rfc-id> --json`
5. Extract all tasks from `## Changes` before dispatching any subagent

## Subagent Dispatch

| Operation | Agent Type | Model | Context to provide |
|-----------|-----------|-------|-------------------|
| Implement task | general-purpose | Opus | Full task text, RFC intent, Story ACs, prior task results |
| Review task (AC compliance) | general-purpose | Opus | Task text, Story ACs, implementer report |
| Review task (code quality) | general-purpose | Sonnet | Changed files, test output, quality criteria |
| Final review | general-purpose | Opus | All Story ACs, full implementation summary |

## Per-Task Loop

For each task, dispatch an implementer subagent (general-purpose, Opus) with:

- **Full task text** copied from iteration (not a file reference)
- Scene-setting: RFC intent (1-2 sentences), relevant Story ACs, prior task results
- Lazyspec workflow rules: use `lazyspec` CLI for doc ops, `--json` always, `--help` before unfamiliar commands, `lazyspec show -e <id>` to expand `@ref` directives
- "Before you begin: ask questions about unclear requirements. Don't guess."
- TDD: failing test first, then implementation, then verify
- Run **scoped** tests during the loop, never the full suite: `cargo test <module>` (e.g. `cargo test engine::document`) or `cargo test --test integration <name>`. Full suite is ~20s/run; scoped is sub-second. The full suite runs once, at the orchestrator's Final Review.
- Self-review: completeness (ACs met?), YAGNI (only what was asked?), test quality (behavioral, isolated, deterministic, readable, specific)
- Report: what was implemented, test results, files changed, concerns

Handle implementer questions before letting them proceed.

After the implementer reports, dispatch a **separate** reviewer subagent with task text, Story ACs, and implementer report:

- **Stage 1 (AC compliance):** Run the tests covering this task's ACs (scoped: `cargo test <module>` / `cargo test --test integration <name>`), verify each claimed AC has a passing test, check for missing requirements or scope creep. If any AC unmet: report specifics. Do NOT run the full suite here — that is the orchestrator's Final Review gate.
- **Stage 2 (code quality, only if Stage 1 passes):** Correctness, clarity, YAGNI, DRY, security. Evaluate test properties (behavioral, structure-insensitive, isolated, deterministic, readable, specific). Flag unjustified tradeoffs.

On failure: dispatch fresh implementer with specific issues, then re-review. Repeat until both stages pass. Mark task complete.

## Context Refresh

Every 2 completed tasks, re-read the chain (`lazyspec context`, `lazyspec show` for iteration and story) to prevent drift.

## Final Review

The orchestrator runs this gate itself after all tasks complete. Subagents only ran scoped tests; this is the one place the full suite runs.

1. Verify all tasks in `## Changes` completed, no out-of-scope work
2. **Run the full test suite once: `cargo test`. All tests must pass — required gate, no acceptance on failure.** On failure, dispatch a targeted fix subagent, then re-run.
3. Run `lazyspec validate --json`
4. Dispatch final reviewer (Opus) with all Story ACs and implementation summary
5. On pass: `lazyspec update <path> --status accepted` for iteration, then story (if all ACs covered), then RFC (if all stories accepted)

## Rules

- Fresh subagent per task (no context pollution)
- Reviewer is always a separate subagent from the implementer
- Stage 1 (AC compliance) MUST pass before Stage 2 (code quality)
- Subagents receive full task text, not file references
- One task, one review cycle. No batching tasks.
- Sequential dispatch only. No parallel implementers.
- Subagents run scoped tests only. Full `cargo test` runs once, by the orchestrator, at Final Review.
- Update document statuses after successful final review
