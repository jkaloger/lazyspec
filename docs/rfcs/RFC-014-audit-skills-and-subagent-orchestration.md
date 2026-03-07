---
title: "Audit Skills and Subagent Orchestration"
type: rfc
status: draft
author: "jkaloger"
date: 2026-03-08
tags: [skills, audits, subagents, orchestration]
---

## Summary

Two changes to the lazyspec skill suite:

1. A new `create-audit` skill for running criteria-based reviews (health checks, security audits, accessibility reviews, pen tests, bug bashes, spec compliance checks). Audits produce findings that the user triages into iterations.

2. Subagent orchestration inside `create-story` and `create-iteration` so that multiple stories or iterations can be created in parallel from a single parent document, following the same dispatch pattern that `build` uses for tasks.

## Context

The current skill pipeline handles the happy path well: one RFC produces one story at a time, one story produces one iteration at a time. In practice, an RFC often identifies 3-5 vertical slices, and a story often has ACs that split naturally into 2-3 iterations. Creating these sequentially wastes time and loses context between invocations.

Separately, the `audit` document type is already registered in `.lazyspec.toml` but has no skill to drive it. Audits are a common need -- reviewing code against criteria and producing actionable findings -- but currently require ad-hoc prompting.

## Design

### Part 1: Audit Skill

An audit is a general-purpose "review against criteria" document. The criteria vary by audit type.

**Workflow position:** Audits sit outside the main RFC -> Story -> Iteration pipeline. They link to stories they audit (when available) and produce iterations as output.

```d2
audit -> findings -> user triage -> create-iteration

audit -> story: "audits against"
```

**Audit lifecycle:**

1. Agent receives audit scope and type (e.g. "security audit of the CLI module")
2. Agent creates an audit document using `lazyspec create audit`
3. Agent reviews the codebase against the criteria for that audit type
4. Agent writes findings with severity ratings into the document
5. Agent presents findings to the user
6. User decides which findings become iterations

**Template:** A single generic audit template in `.lazyspec/templates/` with flexible sections. The template provides minimum structure (scope, criteria, findings with severity, recommendations) while allowing the agent to adapt sections to the audit type.

```markdown
---
title: "{title}"
type: audit
status: draft
author: "{author}"
date: {date}
tags: []
---

## Scope

What is being audited and why.

## Criteria

The standards or checklist being audited against.

## Findings

### Finding 1: [title]

**Severity:** critical | high | medium | low | info
**Location:** file path or component
**Description:** What was found.
**Recommendation:** What should be done.

## Summary

Overall assessment and prioritised recommendations.
```

**Linking:** Audits use `related-to` links to stories or RFCs they audit against. Iterations created from findings use `implements` links back to the relevant story (if one exists) or stand alone.

### Part 2: Subagent Orchestration

Both `create-story` and `create-iteration` gain internal subagent dispatch, following the pattern established by `build`.

**Principle:** The orchestrating skill partitions scope upfront. Each subagent gets a clear, non-overlapping slice definition. Subagents don't coordinate with each other.

#### create-story changes

When `create-story` is invoked for an RFC that identifies multiple vertical slices:

1. Read the RFC and extract the identified stories/slices
2. For each slice, define the exact scope boundary (what's in, what's out)
3. Present the partition to the user for approval
4. Dispatch N subagents in parallel, each creating one story
5. Each subagent receives: the RFC context, its specific slice definition, and the scope boundaries of adjacent slices (so it knows what to exclude)
6. Collect results, validate with `lazyspec validate`, present to user

```d2
Read RFC -> Extract slices -> Define partitions -> User approves? -> Dispatch N subagents

User approves?.shape: diamond
User approves? -> Revise partitions: no
Revise partitions -> Define partitions

Dispatch N subagents -> Collect results -> Validate -> Present to user
```

#### create-iteration changes

When `create-iteration` is invoked for a story with multiple ACs that split into separate iterations:

1. Read the story ACs
2. Group ACs into iteration-sized chunks (each iteration covers a coherent subset)
3. For each group, define the scope and which ACs it addresses
4. Present the partition to the user for approval
5. Dispatch N subagents in parallel, each creating one iteration
6. Each subagent receives: the story context, its AC group, and the boundaries of other groups
7. Collect results, validate, present to user

The key constraint: each AC belongs to exactly one iteration. No overlap.

#### Overlap prevention

Upfront partitioning eliminates overlap by construction:

- The orchestrator defines non-overlapping scope for each subagent before dispatch
- Each subagent receives explicit "in scope" and "out of scope" boundaries
- Adjacent slice definitions are included so subagents can actively avoid crossing boundaries
- No runtime coordination or locking needed

This mirrors how `build` works: tasks are defined upfront in the iteration document, and each implementer subagent gets exactly one task.

## Stories

### Story A: Create-Audit Skill

New `create-audit` skill with generic template, audit lifecycle, finding severity ratings, and user-driven triage to iterations.

### Story B: Subagent Orchestration in create-story

Update `create-story` to partition RFC slices and dispatch parallel subagents for multi-story creation.

### Story C: Subagent Orchestration in create-iteration

Update `create-iteration` to partition story ACs and dispatch parallel subagents for multi-iteration creation.
