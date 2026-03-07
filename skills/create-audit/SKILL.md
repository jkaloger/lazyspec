---
name: create-audit
description: Use when running a criteria-based review (health check, security audit, accessibility review, pen test, bug bash, spec compliance). Creates an Audit document with findings and presents them to the user for triage.
---

```
AUDITS DOCUMENT FINDINGS. THEY DON'T FIX THEM.
```

Present findings to the user. Let them decide what to act on.

<HARD-GATE>
Do NOT create iterations from findings. The audit produces a findings report
that the user triages. Only after the user selects findings to act on should
`/create-iteration` be used, and that is a separate skill invocation.
</HARD-GATE>

## Forbidden Actions

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` to create documents and `lazyspec link` to create relationships.
- Do NOT edit a document you haven't read. Always `lazyspec show <id>` or `Read` a file before modifying it.
- Do NOT create iterations from audit findings. Present findings to the user for triage.
- Do NOT fix issues found during the audit. Document them only.
</NEVER>

## CLI Reference

Before using any `lazyspec` command, run `lazyspec help` to see all available
commands, and `lazyspec help <subcommand>` to see the full usage for that
command. Do not assume you know the flags or arguments -- verify with `--help`.

Always pass `--json` when the command supports it. This gives you structured,
parseable output. Only omit `--json` when presenting output directly to the user.

If a `lazyspec` command fails, run `lazyspec help <subcommand>` to check
the correct usage before retrying. Do not guess at fixes or retry the same
command blindly.

# Create Audit

## Workflow Position

Audits sit outside the main pipeline. They feed findings into the pipeline
via user triage.

```d2
create-audit -> findings -> user triage -> create-iteration

create-audit.style.fill: "#4A9EFF"
create-audit.style.font-color: "#FFFFFF"
findings.style.opacity: 0.4
user triage.style.opacity: 0.4
create-iteration.style.opacity: 0.4
```

## Workflow

```d2
Define scope and criteria -> Create audit doc -> Review codebase -> Document findings -> Validate -> Present to user

Present to user -> User triages findings -> Use /create-iteration skill: for selected findings

Present to user.shape: diamond
Use /create-iteration skill.shape: double_circle
```

## Preflight

1. Understand what's being audited (scope) and against what criteria
2. Search for existing audits on the same topic: `lazyspec search "<topic>" --json`
3. If auditing against stories, read them: `lazyspec show <story-id> --json`

## Subagent Dispatch

| Tier | Model | Use for |
|------|-------|---------|
| Light | Haiku | Parsing frontmatter, extracting structured data, simple validation |
| Medium | Sonnet | Codebase exploration, searching for patterns, reading and summarizing documents |
| Heavy | Opus | Implementation, complex reasoning, multi-file changes, review |

| Operation | Agent Type | Tier | Context to provide |
|-----------|-----------|------|-------------------|
| Discover relevant code | Explore | Medium | Audit scope, criteria, file paths |
| Review code against criteria | Explore | Medium | Specific criterion, file paths to check |

## Steps

1. **Define scope and criteria:** Work with the user to establish what is being audited and the criteria to audit against. This could be a checklist, a set of standards, or specific story ACs.

2. **Create the audit:** Run `lazyspec help create` to confirm usage, then: `lazyspec create audit "<title>" --author <name>`

3. **Link to related documents:** If auditing against existing stories or RFCs, run `lazyspec help link` to confirm usage, then: `lazyspec link <audit-path> related-to <target-path>`. Link to every document the audit references.

4. **Review the codebase:** Use Explore subagents to discover and review code against the criteria. Dispatch one subagent per area of the codebase or per criterion, depending on audit scope.

5. **Document findings:** Edit the audit document. Each finding must include:
   - **Severity:** critical, high, medium, low, or info
   - **Location:** file path or component
   - **Description:** what was found
   - **Recommendation:** what should be done

6. **Validate:** Run `lazyspec validate --json` to ensure all links resolve.

7. **Present to user:** Show the complete findings to the user, grouped by severity. Do NOT create iterations. The user decides which findings to act on and when.

## Red Flags

| Red Flag | Reality |
|----------|---------|
| "I'll fix this while I'm in here" | Audits document. They don't fix. |
| "This finding is obvious, I'll skip documenting it" | If it's worth noticing, it's worth recording. |
| "I'll create an iteration for the critical findings" | The user triages findings. Not you. |
| "I don't need to link to the stories I'm auditing" | Link to everything the audit references. Traceability matters. |

## Verification

Before claiming this skill is complete:

- [ ] `lazyspec validate --json` passes
- [ ] Every finding has severity, location, description, recommendation
- [ ] Audit links to relevant stories/RFCs (if applicable)
- [ ] Findings presented to user
- [ ] No iterations created without user direction

## Rules

- Audits document findings, they don't fix them
- Present findings to the user for triage, not automatic iteration creation
- Each finding must have a severity rating (critical, high, medium, low, info)
- Link to stories/RFCs being audited when they exist
- Group findings by severity when presenting to the user
