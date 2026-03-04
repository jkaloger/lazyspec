---
name: create-story
description: Use when starting a new feature, card, or vertical slice of work. Creates a Story document with given/when/then acceptance criteria linked to an RFC.
---

# Create Story

## Workflow

```d2
Identify parent RFC -> Run lazyspec create -> Write acceptance criteria -> Link to RFC -> Validate

Validate.shape: double_circle
```

## Steps

1. **Find the parent RFC:** Run `lazyspec list rfc` to find the relevant RFC. Use `lazyspec show <id>` to verify it's the right one.

2. **Create the story:** Run `lazyspec create story "<title>" --author <name>`

3. **Write acceptance criteria:** Edit the created file. Each AC must follow given/when/then:
   - **Given** a precondition that establishes context
   - **When** an action is taken
   - **Then** an observable outcome occurs

4. **Link to RFC:** Run `lazyspec link <story-path> implements <rfc-path>`

5. **Define scope:** Fill in the In Scope and Out of Scope sections. Be explicit about what this story does NOT cover.

6. **Validate:** Run `lazyspec validate` to ensure all links resolve.

## Rules

- A Story must be readable by a client without referencing implementation details
- If you can't write the Story without mentioning implementation specifics, it's scoped wrong
- Each AC must be independently testable
- Keep stories small enough to complete in 1-3 iterations
