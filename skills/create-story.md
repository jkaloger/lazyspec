---
name: create-story
description: Use when starting a new feature, card, or vertical slice of work. Creates a Story document with given/when/then acceptance criteria linked to an RFC.
---

```
NO WORK WITHOUT ACCEPTANCE CRITERIA
```

If you can't state given/when/then, you don't understand the work yet.

<HARD-GATE>
Do NOT create a Story without a parent RFC. If no RFC exists for this work,
invoke write-rfc first.
After completion: invoke resolve-context before any implementation.
</HARD-GATE>

# Create Story

## Workflow Position

```d2
write-rfc -> create-story -> resolve-context -> create-iteration -> review-iteration

create-story.style.fill: "#4A9EFF"
create-story.style.font-color: "#FFFFFF"
write-rfc.style.opacity: 0.4
resolve-context.style.opacity: 0.4
create-iteration.style.opacity: 0.4
review-iteration.style.opacity: 0.4
```

## Workflow

```d2
Find parent RFC -> RFC exists?

RFC exists?.shape: diamond
RFC exists? -> Create story: yes
RFC exists? -> Invoke write-rfc: no

Invoke write-rfc.shape: hexagon

Create story -> Write acceptance criteria -> Link to RFC -> Define scope -> Validate -> Invoke resolve-context

Invoke resolve-context.shape: double_circle
```

## Steps

1. **Find the parent RFC:** Run `lazyspec list rfc` to find the relevant RFC. Use `lazyspec show <id>` to verify it's the right one. If no RFC exists, invoke write-rfc first.

2. **Create the story:** Run `lazyspec create story "<title>" --author <name>`

3. **Write acceptance criteria:** Edit the created file. Each AC must follow given/when/then:
   - **Given** a precondition that establishes context
   - **When** an action is taken
   - **Then** an observable outcome occurs

4. **Link to RFC:** Run `lazyspec link <story-path> implements <rfc-path>`

5. **Define scope:** Fill in the In Scope and Out of Scope sections. Be explicit about what this story does NOT cover.

6. **Validate:** Run `lazyspec validate` to ensure all links resolve.

## Red Flags

| Red Flag | Reality |
|----------|---------|
| "The RFC covers it, I don't need a Story" | RFCs describe intent. Stories describe acceptance. Different audiences. |
| "I'll write the ACs after I see what the code looks like" | That's testing after. ACs define the work, not the other way around. |
| "This AC is obvious, I don't need given/when/then" | Obvious to you. Given/when/then forces you to be explicit about preconditions. |

## Verification

Before claiming this skill is complete:

- [ ] `lazyspec validate` passes (story links to RFC)
- [ ] Every AC has given/when/then structure
- [ ] Scope sections are filled (not TODO)
- [ ] Story is readable without implementation specifics

## Rules

- A Story must be readable by a client without referencing implementation details
- If you can't write the Story without mentioning implementation specifics, it's scoped wrong
- Each AC must be independently testable
- Keep stories small enough to complete in 1-3 iterations
