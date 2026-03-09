---
name: create-audit
description: Use when running a criteria-based review (health check, security audit, accessibility review, pen test, bug bash, spec compliance). Creates an Audit document with findings and presents them to the user for triage.
---

```
AUDITS DOCUMENT FINDINGS. THEY DON'T FIX THEM.
```

Present findings to the user. Let them decide what to act on.

<HARD-GATE>
Do NOT create plans from findings. The audit produces a findings report
that the user triages. Only after the user selects findings to act on should
`/create-plan` be used, and that is a separate skill invocation.
</HARD-GATE>

> [!IMPORTANT]
> Read `_common.md` in the skills directory for CLI usage, forbidden actions, and subagent tiers.

<NEVER>
- Do NOT create plans from audit findings. Present findings to the user for triage.
- Do NOT fix issues found during the audit. Document them only.
</NEVER>

# Create Audit

## Workflow Position

Audits sit outside the main pipeline. They feed findings into the pipeline
via user triage.

```d2
create-audit -> findings -> user triage -> create-plan

create-audit.style.fill: "#4A9EFF"
create-audit.style.font-color: "#FFFFFF"
findings.style.opacity: 0.4
user triage.style.opacity: 0.4
create-plan.style.opacity: 0.4
```

## Workflow

```d2
Define scope and criteria -> Create audit doc -> Review codebase -> Document findings -> Validate -> Present to user

Present to user -> User triages findings -> Use /create-plan skill: for selected findings

Present to user.shape: diamond
Use /create-plan skill.shape: double_circle
```

## Preflight

1. Understand what's being audited (scope) and against what criteria
2. Search for existing audits on the same topic: `lazyspec search "<topic>" --json`
3. If auditing against specs, read them: `lazyspec show <spec-id> --json`

## Subagent Dispatch

| Operation | Agent Type | Tier | Context to provide |
|-----------|-----------|------|-------------------|
| Discover relevant code | Explore | Medium | Audit scope, criteria, file paths |
| Review code against criteria | Explore | Medium | Specific criterion, file paths to check |

## Steps

1. **Define scope and criteria:** Work with the user to establish what is being audited and the criteria to audit against. This could be a checklist, a set of standards, or specific spec contracts.

2. **Create the audit:** Run `lazyspec help create` to confirm usage, then: `lazyspec create audit "<title>" --author <name>`

3. **Link to related documents:** If auditing against existing specs or RFCs, run `lazyspec help link` to confirm usage, then: `lazyspec link <audit-path> related-to <target-path>`. Link to every document the audit references.

4. **Review the codebase:** Use Explore subagents to discover and review code against the criteria. Dispatch one subagent per area of the codebase or per criterion, depending on audit scope.

5. **Document findings:** Edit the audit document. Each finding must include:
   - **Severity:** critical, high, medium, low, or info
   - **Location:** file path or component
   - **Description:** what was found
   - **Recommendation:** what should be done

6. **Validate:** Run `lazyspec validate --json` to ensure all links resolve.

7. **Present to user:** Show the complete findings to the user, grouped by severity. Do NOT create plans. The user decides which findings to act on and when.

## Red Flags

| Red Flag | Reality |
|----------|---------|
| "I'll fix this while I'm in here" | Audits document. They don't fix. |
| "This finding is obvious, I'll skip documenting it" | If it's worth noticing, it's worth recording. |
| "I'll create a plan for the critical findings" | The user triages findings. Not you. |
| "I don't need to link to the specs I'm auditing" | Link to everything the audit references. Traceability matters. |

## Checklist

Before claiming this skill is complete:

- [ ] `lazyspec validate --json` passes
- [ ] Every finding has severity, location, description, recommendation
- [ ] Audit links to relevant specs/RFCs (if applicable)
- [ ] Findings presented to user, grouped by severity
- [ ] No plans created without user direction
