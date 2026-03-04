---
name: create-iteration
description: Use when implementing against a Story. Creates an Iteration document, links it to the Story, and drives TDD.
---

# Create Iteration

## Workflow

```d2
Gather context -> Create iteration doc -> Link to story -> Write tests from ACs -> Implement -> Update iteration doc -> Validate

Validate.shape: double_circle
```

## Steps

1. **Gather context:** Run `lazyspec show <story-id>` to read the Story and its ACs. Check existing iterations: `lazyspec list iteration`.

2. **Create the iteration:** Run `lazyspec create iteration "<title>" --author agent`

3. **Link to story:** Run `lazyspec link <iteration-path> implements <story-path>`

4. **Write tests first:** For each AC this iteration covers, write a failing test before any implementation code. Document the test plan in the iteration's `## Test Plan` section.

5. **Implement:** Write minimal code to make tests pass.

6. **Document:** Update `## Changes` with what was implemented. Add any discoveries or decisions to `## Notes`. If a significant decision was made, create an ADR: `lazyspec create adr "<decision>"`.

7. **Validate:** Run `lazyspec validate`.

## Rules

- Tests before implementation, always
- One iteration should cover a subset of Story ACs, not all of them
- If you discover a contract needs to change, emit an ADR
- Keep iterations small and committable
