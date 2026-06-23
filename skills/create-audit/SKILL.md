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
`/lazy` be used to author a delivery document for them, and that is a separate skill invocation.
</HARD-GATE>

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` and `lazyspec link`.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT create iterations from audit findings. Present findings to the user for triage.
- Do NOT fix issues found during the audit. Document them only.
</NEVER>

<GITHUB-ISSUES-DOCUMENTS>
Documents stored in GitHub Issues (store = "github-issues") are managed through the GitHub API. The `.lazyspec/cache/` directory contains read-only mirrors.
- Never edit files under `.lazyspec/cache/`. Use `lazyspec update <ID> --body` to modify content.
- Always use shorthand IDs (e.g. STORY-095) not cache file paths when referencing documents in `lazyspec link`, `lazyspec update`, `lazyspec show`, etc.
- To set body content at creation: `lazyspec create <type> <title> --body "content"` or `--body-file <path>`.
- To modify after creation: `lazyspec update <ID> --body "new content"` or `--body-file <path>`.
</GITHUB-ISSUES-DOCUMENTS>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. On failure, check `--help` before retrying.

## Workflow

```d2
Define scope and criteria -> Create audit doc -> Review codebase -> Document findings -> Validate -> Present to user

Present to user -> User triages findings -> Use /lazy skill: for selected findings

Present to user.shape: diamond
Use /lazy skill.shape: double_circle
```

## Steps

1. **Define scope and criteria** with the user.
2. **Check for existing audits:** `lazyspec search "<topic>" --json`
3. **Create:** `lazyspec create audit "<title>" --author <name>`
4. **Link:** `lazyspec link <audit-path> related-to <target-path>` for every document the audit references.
5. **Review codebase:** Use Explore subagents (Sonnet) per area or criterion.
6. **Document findings** in the audit. Each finding must have:
   - **Severity:** critical, high, medium, low, or info
   - **Location:** file path or component
   - **Description:** what was found
   - **Recommendation:** what should be done
7. **Validate:** `lazyspec validate --json`
8. **Present** findings to user grouped by severity. Do NOT create iterations.
