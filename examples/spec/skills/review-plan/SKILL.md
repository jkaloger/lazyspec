---
name: review-plan
description: Use when a Plan is complete and ready for review. Two-stage review -- spec contract compliance first, code quality second. Block on contract failure before reviewing code.
---

```
NO APPROVAL WITHOUT FRESH VERIFICATION EVIDENCE
```

If you haven't run the tests in this session, you cannot claim they pass.

<HARD-GATE>
Do NOT approve without running verification commands in this session.
Stage 1 (contract compliance) MUST pass before entering Stage 2 (code quality).
If contracts fail during a `/build` per-task review, dispatch a fix subagent.
If contracts fail during a standalone review, use `/create-plan` to re-plan.
</HARD-GATE>

> [!IMPORTANT]
> Read `_common.md` in the skills directory for CLI usage, forbidden actions, and subagent tiers.

<NEVER>
- Do NOT approve without running tests in the current session. Do NOT trust prior test reports.
</NEVER>

# Review Plan

## Workflow Position

```d2
lazy -> write-rfc -> create-spec -> resolve-context -> create-plan -> build -> review-plan

review-plan.style.fill: "#4A9EFF"
review-plan.style.font-color: "#FFFFFF"
lazy.style.opacity: 0.4
write-rfc.style.opacity: 0.4
create-spec.style.opacity: 0.4
resolve-context.style.opacity: 0.4
create-lazy.style.opacity: 0.4
build.style.opacity: 0.4
```

## Modes

This skill operates in two modes:

**Per-task review** (dispatched as a reviewer subagent by `/build` after each task):

- Scoped to the contracts relevant to the completed task
- Same two-stage process (contract compliance first, code quality second)
- On failure: report back to `/build` orchestrator, which dispatches a fix subagent

**Full review** (dispatched by `/build` as final gate, or used standalone):

- Checks ALL Spec contracts against the complete implementation
- On failure during `/build`: targeted fix subagents for specific gaps
- On failure standalone: report to user

## Preflight

1. Resolve the chain with `lazyspec context <plan-id> --json` to see RFC -> Spec -> Plan
2. Read the plan body with `lazyspec show <plan-id> --json`
3. Read the parent Spec contracts with `lazyspec show <spec-id> --json`
4. Do NOT begin review until both documents are loaded into context

## The Gate

```
BEFORE claiming review passes:

1. IDENTIFY: Which Spec contracts does this plan cover?
2. RUN: Execute the FULL test suite (fresh, in this session)
3. READ: Full output, check exit code, count failures
4. VERIFY: Does each claimed contract have a passing test?
   - If NO: State which contracts are unmet. Return to create-plan.
   - If YES: Proceed to Stage 2 (code quality)
5. ONLY THEN: Approve

Skip any step = not a review
```

## Workflow

```d2
Read plan doc -> Read parent spec contracts -> Run full test suite -> All contracts satisfied?

All contracts satisfied?.shape: diamond
All contracts satisfied? -> Fix (see failure handling below): no
All contracts satisfied? -> Code quality review: yes

Code quality review -> Critical issues?

Critical issues?.shape: diamond
Critical issues? -> Fix (see failure handling below): yes
Critical issues? -> Approve: no

Approve.shape: double_circle
```

## Stage 1: Contract Compliance

1. Run `lazyspec context <plan-id> --json` to see the full chain.
2. Run `lazyspec show <plan-id> --json` to read the plan body.
3. Run `lazyspec show <spec-id> --json` to read the Spec's contracts.
4. Run the full test suite. Show the output.
5. For each contract the plan claims to cover: verify the test exists and passes.
6. If any contract is not satisfied, state which contracts are unmet. See Failure Handling below for what to do next.

## Stage 2: Code Quality

Only enter this stage if all contracts are satisfied.

1. Review the code changes for correctness and clarity.
2. Verify no unnecessary complexity (YAGNI -- only what was asked for).
3. Check for real duplication worth extracting (DRY).
4. Check for security issues.
5. Evaluate test quality against these properties:

   | Property              | What to check                                              |
   | --------------------- | ---------------------------------------------------------- |
   | Behavioral            | Tests assert on behavior, not implementation details       |
   | Structure-insensitive | A refactor preserving behavior shouldn't break tests       |
   | Isolated              | No order dependence, no shared mutable state between tests |
   | Deterministic         | No flaky results from timing, randomness, or global state  |
   | Readable              | Motivation for each test is obvious to the reader          |
   | Specific              | When a test fails, the cause is obvious                    |
   | Writable              | Test complexity is proportional to code complexity         |

   These properties conflict. If a test trades one for another (e.g. an
   integration test that sacrifices Fast/Isolated for Predictive/Inspiring),
   the tradeoff should be noted. Flag unjustified tradeoffs to the collaborator.

## Failure Handling

The response to a failed review depends on context:

**During `/build` per-task review:** Report the specific failures back to the `/build` orchestrator. The build skill will dispatch a fresh implementer subagent with the failure details. Do NOT re-plan or use `/create-plan`.

**During `/build` final review:** If individual contracts are unmet, the build skill dispatches targeted fix subagents. If the failures indicate a fundamental planning problem (wrong approach, missing tasks), escalate to the user.

**Standalone review (not during build):** Report failures to the user. The user decides whether to use `/create-plan` to re-plan or fix directly.

## Red Flags

| Red Flag                     | Reality                                                              |
| ---------------------------- | -------------------------------------------------------------------- |
| "The agent says tests pass"  | Run them yourself. Trust is not evidence.                            |
| "I ran them earlier"         | Earlier is stale. Run them now, in this session.                     |
| "The code looks right to me" | Code review before contract compliance is backwards. Check contracts first. |
| "It mostly works"            | Mostly = some contracts aren't met. Return to implementation.        |

## Verification

Before claiming this review is approved:

- [ ] Test suite has been run in this session with full output shown
- [ ] Every claimed contract has a corresponding passing test
- [ ] Code quality review completed (only after Stage 1 passes)
- [ ] `lazyspec validate --json` passes

## Status Updates

When a review passes (both stages), follow the status promotion steps in `_common.md`.

## Rules

- Never review code quality before contract compliance
- The Spec is the source of truth -- if the code satisfies the contracts, it's correct by definition
- If contracts are ambiguous, that's a Spec problem, not a Plan problem
- Always update document statuses after a successful review
