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
If ACs fail during a `/build` per-task review, dispatch a fix subagent.
If ACs fail during a standalone review, use `/create-iteration` to re-plan.
</HARD-GATE>

## Forbidden Actions

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` to create documents and `lazyspec link` to create relationships.
- Do NOT edit a document you haven't read. Always `lazyspec show <id>` or `Read` a file before modifying it.
- Do NOT skip the workflow pipeline. Features need RFC -> Story -> Iteration. Bug fixes need Iteration.
- Do NOT approve without running tests in the current session. Do NOT trust prior test reports.
</NEVER>

# Review Iteration

## Workflow Position

```d2
plan -> write-rfc -> create-story -> resolve-context -> create-iteration -> build -> review-iteration

review-iteration.style.fill: "#4A9EFF"
review-iteration.style.font-color: "#FFFFFF"
plan.style.opacity: 0.4
write-rfc.style.opacity: 0.4
create-story.style.opacity: 0.4
resolve-context.style.opacity: 0.4
create-iteration.style.opacity: 0.4
build.style.opacity: 0.4
```

## Modes

This skill operates in two modes:

**Per-task review** (dispatched as a reviewer subagent by `/build` after each task):
- Scoped to the ACs relevant to the completed task
- Same two-stage process (AC compliance first, code quality second)
- On failure: report back to `/build` orchestrator, which dispatches a fix subagent

**Full review** (dispatched by `/build` as final gate, or used standalone):
- Checks ALL Story ACs against the complete implementation
- On failure during `/build`: targeted fix subagents for specific gaps
- On failure standalone: report to user

## Preflight

1. Resolve the chain with `lazyspec context <iteration-id>` to see RFC -> Story -> Iteration
2. Read the iteration body with `lazyspec show <iteration-id>`
3. Read the parent Story ACs with `lazyspec show <story-id>`
4. Do NOT begin review until both documents are loaded into context

## The Gate

```
BEFORE claiming review passes:

1. IDENTIFY: Which Story ACs does this iteration cover?
2. RUN: Execute the FULL test suite (fresh, in this session)
3. READ: Full output, check exit code, count failures
4. VERIFY: Does each claimed AC have a passing test?
   - If NO: State which ACs are unmet. Return to create-iteration.
   - If YES: Proceed to Stage 2 (code quality)
5. ONLY THEN: Approve

Skip any step = not a review
```

## Workflow

```d2
Read iteration doc -> Read parent story ACs -> Run full test suite -> All ACs satisfied?

All ACs satisfied?.shape: diamond
All ACs satisfied? -> Fix (see failure handling below): no
All ACs satisfied? -> Code quality review: yes

Code quality review -> Critical issues?

Critical issues?.shape: diamond
Critical issues? -> Fix (see failure handling below): yes
Critical issues? -> Approve: no

Approve.shape: double_circle
```

## Stage 1: AC Compliance

1. Run `lazyspec context <iteration-id>` to see the full chain.
2. Run `lazyspec show <iteration-id>` to read the iteration body.
3. Run `lazyspec show <story-id>` to read the Story's ACs.
4. Run the full test suite. Show the output.
5. For each AC the iteration claims to cover: verify the test exists and passes.
6. If any AC is not satisfied, state which ACs are unmet. See Failure Handling below for what to do next.

## Stage 2: Code Quality

Only enter this stage if all ACs are satisfied.

1. Review the code changes for correctness and clarity.
2. Verify no unnecessary complexity (YAGNI -- only what was asked for).
3. Check for real duplication worth extracting (DRY -- three instances, not two).
4. Check for security issues.
5. Evaluate test quality against these properties:

   | Property              | What to check                                             |
   | --------------------- | --------------------------------------------------------- |
   | Behavioral            | Tests assert on behavior, not implementation details      |
   | Structure-insensitive | A refactor preserving behavior shouldn't break tests      |
   | Isolated              | No order dependence, no shared mutable state between tests|
   | Deterministic         | No flaky results from timing, randomness, or global state |
   | Readable              | Motivation for each test is obvious to the reader         |
   | Specific              | When a test fails, the cause is obvious                   |
   | Writable              | Test complexity is proportional to code complexity        |

   These properties conflict. If a test trades one for another (e.g. an
   integration test that sacrifices Fast/Isolated for Predictive/Inspiring),
   the tradeoff should be noted. Flag unjustified tradeoffs to the collaborator.

## Failure Handling

The response to a failed review depends on context:

**During `/build` per-task review:** Report the specific failures back to the `/build` orchestrator. The build skill will dispatch a fresh implementer subagent with the failure details. Do NOT re-plan the iteration or use `/create-iteration`.

**During `/build` final review:** If individual ACs are unmet, the build skill dispatches targeted fix subagents. If the failures indicate a fundamental planning problem (wrong approach, missing tasks), escalate to the user.

**Standalone review (not during build):** Report failures to the user. The user decides whether to use `/create-iteration` to re-plan or fix directly.

## Red Flags

| Red Flag | Reality |
|----------|---------|
| "The agent says tests pass" | Run them yourself. Trust is not evidence. |
| "I ran them earlier" | Earlier is stale. Run them now, in this session. |
| "The code looks right to me" | Code review before AC compliance is backwards. Check ACs first. |
| "It mostly works" | Mostly = some ACs aren't met. Return to implementation. |

## Verification

Before claiming this review is approved:

- [ ] Test suite has been run in this session with full output shown
- [ ] Every claimed AC has a corresponding passing test
- [ ] Code quality review completed (only after Stage 1 passes)
- [ ] `lazyspec validate` passes

## Status Updates

When a review passes (both stages), update document statuses:

```bash
lazyspec update <iteration-path> --status accepted
```

Then check whether the parent Story and RFC should also be promoted:
- If all iterations under a Story are accepted and all Story ACs are covered, mark the Story as accepted.
- If all Stories under an RFC are accepted, mark the RFC as accepted.

## Rules

- Never review code quality before AC compliance
- The Story is the spec -- if the code satisfies the ACs, it's correct by definition
- If ACs are ambiguous, that's a Story problem, not an Iteration problem
- Always update document statuses after a successful review
