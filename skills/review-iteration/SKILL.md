---
name: review-iteration
description: Use when an Iteration is complete and ready for review. Two-stage review -- AC compliance first, code quality second. Block on AC failure before reviewing code.
---

```
NO APPROVAL WITHOUT FRESH VERIFICATION EVIDENCE
```

If you haven't run the tests in this session, you cannot claim they pass.

<HARD-GATE>
Do NOT approve without running verification commands in this session.
Stage 1 (AC compliance) MUST pass before entering Stage 2 (code quality).
If ACs fail during `/build` per-task review, dispatch a fix subagent.
If ACs fail during standalone review, report to user.
</HARD-GATE>

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` and `lazyspec link`.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT approve without running tests in the current session. Do NOT trust prior test reports.
</NEVER>

<GITHUB-ISSUES-DOCUMENTS>
Documents stored in GitHub Issues (store = "github-issues") are managed through the GitHub API. The `.lazyspec/cache/` directory contains read-only mirrors.
- Never edit files under `.lazyspec/cache/`. Use `lazyspec update <ID> --body` to modify content.
- Always use shorthand IDs (e.g. STORY-095) not cache file paths when referencing documents in `lazyspec link`, `lazyspec update`, `lazyspec show`, etc.
- To set body content at creation: `lazyspec create <type> <title> --body "content"` or `--body-file <path>`.
- To modify after creation: `lazyspec update <ID> --body "new content"` or `--body-file <path>`.
</GITHUB-ISSUES-DOCUMENTS>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. On failure, check `--help` before retrying.

## Modes

**Per-task review** (dispatched by `/build` after each task): scoped to task ACs. On failure: report to build orchestrator for fix dispatch.

**Full review** (final `/build` gate or standalone): checks ALL Story ACs. On failure during build: targeted fix subagents. On failure standalone: report to user.

## Workflow

```d2
Read iteration doc -> Read parent story ACs -> Run full test suite -> All ACs satisfied?

All ACs satisfied?.shape: diamond
All ACs satisfied? -> Fix (see failure handling): no
All ACs satisfied? -> Code quality review: yes

Code quality review -> Critical issues?

Critical issues?.shape: diamond
Critical issues? -> Fix (see failure handling): yes
Critical issues? -> Approve: no

Approve.shape: double_circle
```

## Preflight

1. `lazyspec context <iteration-id> --json` to see the chain
2. `lazyspec show <iteration-id> --json` for iteration body
3. `lazyspec show <story-id> --json` for Story ACs
4. Do NOT begin review until both documents are loaded

## Stage 1: AC Compliance

1. Run the full test suite. Show output.
2. For each AC the iteration claims to cover: verify a passing test exists.
3. If any AC unmet: state which ACs failed. See failure handling in Modes section.

## Stage 2: Code Quality

Only enter if all ACs are satisfied.

1. Review code for correctness and clarity.
2. YAGNI: only what was asked for. DRY: real duplication worth extracting. Security issues.
3. Evaluate test quality:

   | Property | Check |
   |----------|-------|
   | Behavioral | Asserts on behavior, not implementation details |
   | Structure-insensitive | Refactor preserving behavior shouldn't break tests |
   | Isolated | No order dependence or shared mutable state |
   | Deterministic | No flaky results from timing, randomness, global state |
   | Readable | Motivation for each test is obvious |
   | Specific | Failure cause is obvious |

   Flag unjustified property tradeoffs.

## Red Flags

| Red Flag | Reality |
|----------|---------|
| "The agent says tests pass" | Run them yourself. Trust is not evidence. |
| "I ran them earlier" | Earlier is stale. Run them now, in this session. |
| "The code looks right to me" | Check ACs first. Code review before AC compliance is backwards. |

## Status Updates

On approval: `lazyspec update <iteration-path> --status accepted`. Then check whether parent Story (all ACs covered?) and RFC (all stories accepted?) should also be promoted.

## Rules

- Never review code quality before AC compliance
- The Story is the spec -- if code satisfies the ACs, it's correct by definition
- If ACs are ambiguous, that's a Story problem, not an Iteration problem
- Always update document statuses after a successful review
