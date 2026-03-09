---
name: create-spec
description: Use when locking down technical contracts for a vertical slice. Creates Spec documents with data models, API surface, validation rules, error handling, edge cases, and optional acceptance criteria. Specs can be standalone or linked to an RFC. Supports parallel subagent dispatch for RFCs with multiple slices.
---

```
NO IMPLEMENTATION WITHOUT LOCKED CONTRACTS
```

If you can't state the data models, API surface, and error handling, the contracts aren't locked down yet.

> [!IMPORTANT]
> Read `_common.md` in the skills directory for CLI usage, forbidden actions, and subagent tiers.

<HARD-GATE>
Specs can exist standalone or linked to a parent RFC. A parent RFC is NOT required.
After identifying multiple slices, partition upfront and get user approval
before dispatching subagents.
After completion: use the `/create-plan` skill to plan the first implementation.
You already have the Spec context (and RFC context if one exists) from writing
this Spec, so resolve-context is not needed when continuing in the same session.
</HARD-GATE>

<NEVER>
- Do NOT write contract sections without reading the parent RFC first (if one exists).
- Do NOT dispatch subagents without user approval of the partition.
</NEVER>

# Create Spec

## Workflow Position

```d2
lazy -> write-rfc -> create-spec -> resolve-context -> create-plan -> build

create-spec.style.fill: "#4A9EFF"
create-spec.style.font-color: "#FFFFFF"
lazy.style.opacity: 0.4
write-rfc.style.opacity: 0.4
resolve-context.style.opacity: 0.4
create-lazy.style.opacity: 0.4
build.style.opacity: 0.4
```

## Workflow

```d2
Check for parent RFC -> RFC exists?

RFC exists?.shape: diamond
RFC exists? -> Read RFC and extract slices: yes
RFC exists? -> Create standalone spec: no

Read RFC and extract slices -> Multiple slices?

Multiple slices?.shape: diamond
Multiple slices? -> Define partitions -> User approves partitions?: yes
Multiple slices? -> Create single spec (inline): no

User approves partitions?.shape: diamond
User approves partitions? -> Dispatch N subagents: yes
User approves partitions? -> Revise partitions: no
Revise partitions -> Define partitions

Dispatch N subagents -> Collect results -> Validate -> Present to user -> Use /create-plan skill
Create standalone spec -> Write contracts -> Validate -> Use /create-plan skill
Create single spec (inline) -> Write contracts -> Link to RFC -> Validate -> Use /create-plan skill

Use /create-plan skill.shape: double_circle
```

## Preflight

1. Read relevant documents using `lazyspec show --json` before modifying anything
2. Check for existing artifacts using `lazyspec search --json` and `lazyspec list --json`
3. If a parent RFC exists, read it with `lazyspec show <rfc-id> --json`
4. If a parent RFC exists, confirm you understand the design intent before writing contracts
5. Check for existing specs: `lazyspec search "<topic>" --json`

## Partitioning

Before dispatching subagents, the orchestrator must:

1. Read the RFC with `lazyspec show <rfc-id> --json`
2. Extract the identified vertical slices from the Specs section
3. For each slice, define: title, scope boundary (in/out), which RFC sections it addresses
4. Verify slices are non-overlapping (no shared scope)
5. Present the partition table to the user for approval

The partition table should clearly show each slice's title, what is in scope, what is out of scope, and which RFC sections it maps to. The user must approve or request revisions before any subagents are dispatched.

## Subagent Dispatch

| Operation   | Agent Type      | Tier  | Context to provide                                       |
| ----------- | --------------- | ----- | -------------------------------------------------------- |
| Create spec | general-purpose | Heavy | RFC context, slice definition, adjacent slice boundaries |

Each subagent receives: full RFC body (not a file reference), its slice definition, and the scope boundaries of all other slices. Read the prompt template from `prompts/spec-writer.md` (relative to this skill directory).

Subagents are dispatched in parallel using the Agent tool.

## Contract Sections

The template suggests these sections, but adapt per-spec. Not every spec needs all sections -- include what's relevant and skip what isn't.

### Data Models
Types, schemas, and structures that this slice introduces or modifies. Use `@draft` for new types and `@ref` for existing ones.

### API Surface
Endpoints, function signatures, message formats. Include request/response shapes. An implementer should be able to build the interface from this section alone.

### Validation Rules
Input constraints, business rules, invariants that must hold. Be specific about boundaries and rejection behavior.

### Error Handling
Error types, codes, messages. How failures propagate and what callers see. Cover both expected errors (validation failures, not-found) and unexpected errors (timeouts, downstream failures).

### Edge Cases
Boundary conditions, race conditions, unusual inputs. Document expected behavior for each. If behavior is undefined, say so explicitly.

### Acceptance Criteria (optional)
For user-story-style specs, express requirements as given/when/then scenarios. This section is optional -- use it when behavior is best described from a user perspective rather than a technical contract perspective.

```
Given [precondition]
When [action]
Then [expected outcome]
```

## Steps

### Multi-slice RFCs

1. **Find the parent RFC:** Run `lazyspec list rfc --json` to find the relevant RFC. Use `lazyspec show <id> --json` to verify it's the right one. Multi-slice partitioning requires an RFC to define the slices.

2. **Read RFC and extract slices:** Read the full RFC body. Identify the vertical slices described in the Specs section or equivalent.

3. **Define partitions:** For each slice, define title, in-scope, out-of-scope, and which RFC sections it addresses. Verify slices are non-overlapping.

4. **Present partition to user for approval:** Show the partition table. Wait for explicit approval. If the user requests changes, revise and re-present.

5. **Dispatch N subagents in parallel:** One subagent per spec, using the Agent tool with the prompt template. Each receives the full RFC body, its slice definition, and the boundaries of all other slices.

6. **Collect results:** Gather reports from all subagents. Run `lazyspec validate --json` to verify all specs link correctly and pass validation.

7. **Present all created specs to the user:** Show a summary of each spec created, its contracts, and the validation result.

### Single spec (standalone or single-slice RFC)

Create the spec directly without subagent dispatch. This is the default path for most features.

1. **Check for parent RFC:** Run `lazyspec search "<topic>" --json` and `lazyspec list rfc --json` to check for an existing RFC. If one exists, read it with `lazyspec show <id> --json`. If none exists, that's fine -- specs can be standalone.

2. **Create the spec:** Run `lazyspec help create` to confirm usage, then: `lazyspec create spec "<title>" --author <name>`

3. **Write contract sections:** Edit the created file. Include the sections relevant to this spec:
   - **Data Models:** types, schemas, structures
   - **API Surface:** endpoints, signatures, request/response shapes
   - **Validation Rules:** constraints, business rules, invariants
   - **Error Handling:** error types, codes, propagation
   - **Edge Cases:** boundaries, race conditions, unusual inputs
   - **Acceptance Criteria (optional):** given/when/then scenarios for user-story-style specs

4. **Link to RFC (if one exists):** Run `lazyspec help link` to confirm usage, then: `lazyspec link <spec-path> implements <rfc-path>`

5. **Define scope:** Fill in the In Scope and Out of Scope sections. Be explicit about what this spec does NOT cover.

6. **Validate:** Run `lazyspec validate --json` to ensure all links resolve.

## Red Flags

| Red Flag | Reality |
|----------|---------|
| "The RFC covers it, I don't need a Spec" | RFCs describe intent. Specs lock down contracts. Different audiences. |
| "I'll figure out the error handling during implementation" | That's a design judgment call. Lock it down in the spec. |
| "This contract is obvious, I don't need to write it out" | Obvious to you. The implementer needs it explicit. |

## Checklist

Before claiming this skill is complete:

- [ ] All created specs link to the parent RFC (if one exists)
- [ ] No overlapping scope between specs
- [ ] Contract sections are specific enough for an implementer to build without design judgment calls
- [ ] Scope sections are filled (not TODO)
- [ ] `lazyspec validate --json` passes
- [ ] If dispatching subagents: have you read the RFC, are slices non-overlapping, has the user approved?

## Rules

- A Spec must be detailed enough that an implementer can build from it without making design judgment calls
- If you can't write the Spec without mentioning implementation specifics, it's scoped wrong
- Each contract should be independently testable
- Keep specs focused on one vertical slice
- Each subagent receives full RFC text, not file references
