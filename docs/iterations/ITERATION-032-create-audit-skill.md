---
title: Create-Audit Skill
type: iteration
status: draft
author: agent
date: 2026-03-08
tags: []
related:
- implements: docs/stories/STORY-040-create-audit-skill.md
---


## Changes

### Task 1: Create generic audit template

**ACs addressed:** AC-2, AC-3

**Files:**
- Create: `.lazyspec/templates/audit.md`

**What to implement:**

Create the directory `.lazyspec/templates/` and add `audit.md` with this content:

```markdown
---
title: "{title}"
type: audit
status: draft
author: "{author}"
date: {date}
tags: []
related: []
---

## Scope

What is being audited and why. Include the audit type (e.g. health check, security, accessibility, spec compliance, bug bash).

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

**How to verify:**
- File exists at `.lazyspec/templates/audit.md`
- Template has sections: Scope, Criteria, Findings, Summary
- Finding structure includes severity, location, description, recommendation

### Task 2: Create the create-audit skill

**ACs addressed:** AC-1, AC-3, AC-4, AC-5

**Files:**
- Create: `skills/create-audit/SKILL.md`

**What to implement:**

Create `skills/create-audit/SKILL.md` following the conventions of existing skills (see `skills/create-story/SKILL.md` and `skills/create-iteration/SKILL.md` for structure).

The skill should include:

**Frontmatter:**
```yaml
---
name: create-audit
description: Use when running a criteria-based review (health check, security audit, accessibility review, pen test, bug bash, spec compliance). Creates an Audit document with findings and presents them to the user for triage.
---
```

**Hard gate:** Do NOT create iterations from findings. Present findings to user and let them decide.

**Forbidden actions:** Same pattern as other skills (no direct file writes, read before edit, use lazyspec CLI).

**Workflow position:** Audits sit outside the main pipeline. Show a d2 diagram:
```
create-audit -> findings -> user triage -> create-iteration
```

**Workflow d2 diagram:**
```
Define scope and criteria -> Create audit doc -> Review codebase -> Document findings -> Validate -> Present to user

Present to user -> User triages findings -> Use /create-iteration skill: for selected findings

Present to user.shape: diamond
Use /create-iteration skill.shape: double_circle
```

**Preflight:**
1. Understand what's being audited (scope) and against what criteria
2. Search for existing audits on the same topic: `lazyspec search "<topic>" --json`
3. If auditing against stories, read them: `lazyspec show <story-id> --json`

**Steps:**
1. Define scope and criteria with the user
2. Create the audit: `lazyspec create audit "<title>" --author <name>`
3. If auditing against existing stories/RFCs, link via: `lazyspec link <audit-path> related-to <target-path>`
4. Review the codebase against the criteria. Use Explore subagents for codebase discovery.
5. Document findings in the audit document. Each finding must have severity (critical/high/medium/low/info), location, description, and recommendation.
6. Validate: `lazyspec validate --json`
7. Present findings to user. Do NOT create iterations. The user decides which findings to act on.

**Subagent dispatch table:** Same tier structure as other skills. Use Explore agents for codebase review against criteria.

**Verification checklist:**
- `lazyspec validate --json` passes
- Every finding has severity, location, description, recommendation
- Audit links to relevant stories/RFCs (if applicable)
- Findings presented to user
- No iterations created without user direction

**Rules:**
- Audits document findings, they don't fix them
- Present findings to the user for triage, not automatic iteration creation
- Each finding must have a severity rating
- Link to stories/RFCs being audited when they exist

**How to verify:**
- File exists at `skills/create-audit/SKILL.md`
- Skill has frontmatter with name and description
- Has hard gate, forbidden actions, CLI reference, workflow, preflight, steps, verification, rules sections
- Follows same conventions as `skills/create-story/SKILL.md`

### Task 3: Update skills README

**ACs addressed:** AC-1

**Files:**
- Modify: `skills/README.md`

**What to implement:**

Add `create-audit` to the workflow description and reference table. The workflow section should note that audits sit outside the main pipeline:

Add to the workflow section after the existing flow:
```
`create-audit` runs independently of the main pipeline. It produces findings that the user can triage into iterations.
```

Add to the reference table:
```
| `create-audit`   | Run a criteria-based review and document findings for user triage            |
```

**How to verify:**
- `skills/README.md` mentions `create-audit` in both the workflow section and reference table

## Test Plan

These are skill files (markdown), not code. Verification is manual:

- **AC-1:** Invoke `/create-audit` and confirm the skill guides through the full audit lifecycle
- **AC-2:** Run `lazyspec create audit "test"` and confirm the template is used with correct sections
- **AC-3:** Confirm the skill enforces severity/location/description/recommendation on each finding
- **AC-4:** Confirm the skill presents findings and does not auto-create iterations
- **AC-5:** Confirm the skill prompts for `lazyspec link` to related stories when applicable

> Note: AC-2 depends on lazyspec supporting custom templates from `.lazyspec/templates/`. If the engine doesn't pick up the template automatically, the template file still serves as documentation of the expected structure and the skill instructions will reference it directly.

## Notes

The audit type is already registered in `.lazyspec.toml` (line 30-34, prefix AUDIT, icon 🔍, dir docs/audits). No engine changes needed for `lazyspec create audit` to work.
