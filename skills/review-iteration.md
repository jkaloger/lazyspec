---
name: review-iteration
description: Use when an Iteration is complete and ready for review. Two-stage review -- AC compliance first, code quality second. Block on AC failure before reviewing code.
---

# Review Iteration

## Workflow

```d2
Read iteration doc -> Read parent story ACs -> Check each AC -> All ACs satisfied?

All ACs satisfied?.shape: diamond
All ACs satisfied? -> Block: return to implementation: no
All ACs satisfied? -> Code quality review: yes

Code quality review -> Critical issues?

Critical issues?.shape: diamond
Critical issues? -> Block: return to implementation: yes
Critical issues? -> Approve: no

Approve.shape: double_circle
```

## Stage 1: AC Compliance

1. Run `lazyspec show <iteration-id>` to read the iteration.
2. Follow the `implements` link to get the parent Story.
3. Run `lazyspec show <story-id>` to read the Story's ACs.
4. For each AC the iteration claims to cover: verify the test exists and passes.
5. If any AC is not satisfied, block the review and return to implementation.

## Stage 2: Code Quality

Only enter this stage if all ACs are satisfied.

1. Review the code changes for correctness and clarity.
2. Check that tests are meaningful (not just asserting true).
3. Verify no unnecessary complexity was introduced.
4. Check for security issues.

## Rules

- Never review code quality before AC compliance
- The Story is the spec -- if the code satisfies the ACs, it's correct by definition
- If ACs are ambiguous, that's a Story problem, not an Iteration problem
