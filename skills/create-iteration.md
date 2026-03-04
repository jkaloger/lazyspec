---
name: create-iteration
description: Use when implementing against a Story. Creates an Iteration document, links it to the Story, and drives TDD -- tests written against Story ACs before implementation.
---

```
NO CODE WITHOUT A FAILING TEST FIRST
```

Write code before the test? Delete it. Start over.

<HARD-GATE>
Do NOT write production code before writing a failing test derived from
Story ACs. If you haven't resolved context, invoke resolve-context first.
After completion: invoke review-iteration.
</HARD-GATE>

# Create Iteration

## Workflow Position

```d2
write-rfc -> create-story -> resolve-context -> create-iteration -> review-iteration

create-iteration.style.fill: "#4A9EFF"
create-iteration.style.font-color: "#FFFFFF"
write-rfc.style.opacity: 0.4
create-story.style.opacity: 0.4
resolve-context.style.opacity: 0.4
review-iteration.style.opacity: 0.4
```

## Workflow

```d2
Context resolved? -> Gather context: no
Context resolved? -> Create iteration doc: yes

Gather context.shape: hexagon

Create iteration doc -> Link to story -> Write failing test -> Run test (must fail) -> Implement minimal code -> Run test (must pass) -> More ACs?

More ACs?.shape: diamond
More ACs? -> Write failing test: yes
More ACs? -> Update iteration doc: no

Update iteration doc -> Validate -> Invoke review-iteration

Invoke review-iteration.shape: double_circle
```

## Steps

1. **Gather context:** Run `lazyspec show <story-id>` to read the Story and its ACs. Check existing iterations: `lazyspec list iteration`. If you haven't already resolved context, invoke resolve-context first.

2. **Create the iteration:** Run `lazyspec create iteration "<title>" --author agent`

3. **Link to story:** Run `lazyspec link <iteration-path> implements <story-path>`

4. **Write tests first:** For each AC this iteration covers:
   - Write a failing test that asserts the AC's expected outcome
   - Run the test. It MUST fail. If it passes, your test isn't testing anything new.
   - Write minimal code to make the test pass
   - Run the test. It MUST pass.
   - Document the test in the iteration's `## Test Plan` section.

5. **Implement:** Write minimal code to make tests pass. No more.

6. **Document:** Update `## Changes` with what was implemented. Add any discoveries or decisions to `## Notes`. If a significant decision was made, create an ADR: `lazyspec create adr "<decision>"`.

7. **Validate:** Run `lazyspec validate`.

## Red Flags

| Red Flag | Reality |
|----------|---------|
| "I'll write the test after the implementation" | Tests-after = "what does this do?" Tests-first = "what should this do?" |
| "This is too simple to test" | Simple code breaks. The test takes 30 seconds. |
| "Let me just get it working first" | Working without tests = untested. Delete it, start with the test. |
| "I already know how to implement this" | TDD isn't about knowledge. It's about proving correctness before moving on. |

All of these mean: delete code, start over with the test.

## Verification

Before claiming this skill is complete:

- [ ] `lazyspec validate` passes (iteration links to story)
- [ ] Test suite has been run with output shown (not just claimed)
- [ ] All tests pass
- [ ] `## Changes` section is filled in the iteration doc
- [ ] `## Test Plan` section documents the tests written

## Rules

- Tests before implementation, always
- One iteration should cover a subset of Story ACs, not all of them
- If you discover a contract needs to change, emit an ADR
- Keep iterations small and committable
