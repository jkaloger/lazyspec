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
If ACs fail, return to create-iteration.
</HARD-GATE>

# Review Iteration

## Workflow Position

```d2
write-rfc -> create-story -> resolve-context -> create-iteration -> review-iteration

review-iteration.style.fill: "#4A9EFF"
review-iteration.style.font-color: "#FFFFFF"
write-rfc.style.opacity: 0.4
create-story.style.opacity: 0.4
resolve-context.style.opacity: 0.4
create-iteration.style.opacity: 0.4
```

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
All ACs satisfied? -> Return to create-iteration: no
All ACs satisfied? -> Code quality review: yes

Code quality review -> Critical issues?

Critical issues?.shape: diamond
Critical issues? -> Return to create-iteration: yes
Critical issues? -> Approve: no

Approve.shape: double_circle
```

## Stage 1: AC Compliance

1. Run `lazyspec show <iteration-id>` to read the iteration.
2. Follow the `implements` link to get the parent Story.
3. Run `lazyspec show <story-id>` to read the Story's ACs.
4. Run the full test suite. Show the output.
5. For each AC the iteration claims to cover: verify the test exists and passes.
6. If any AC is not satisfied, state which ACs are unmet and return to create-iteration.

## Stage 2: Code Quality

Only enter this stage if all ACs are satisfied.

1. Review the code changes for correctness and clarity.
2. Check that tests are meaningful (not just asserting true).
3. Verify no unnecessary complexity was introduced.
4. Check for security issues.

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

## Rules

- Never review code quality before AC compliance
- The Story is the spec -- if the code satisfies the ACs, it's correct by definition
- If ACs are ambiguous, that's a Story problem, not an Iteration problem
