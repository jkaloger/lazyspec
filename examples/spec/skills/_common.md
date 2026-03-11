# Shared Skill Rules

These rules apply to all skills in this workflow. Read this before proceeding.

## CLI Usage

Use `lazyspec` for all document operations.

- Run `lazyspec help <subcommand>` to verify flags and arguments before any command
- Always pass `--json` for structured output (omit only when presenting directly to the user)
- If a command fails, check `lazyspec help <subcommand>` before retrying

## Universal Forbidden Actions

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` and `lazyspec link`.
- Do NOT edit a document you haven't read. Use `lazyspec show <id> --json` or the Read tool first.
- Do NOT skip the workflow pipeline. Features need Spec -> Plan (RFC optional for heavy work). Bug fixes need Plan.
</NEVER>

## Subagent Tiers

| Tier   | Model  | Use for                                                |
| ------ | ------ | ------------------------------------------------------ |
| Light  | Haiku  | Parsing, extracting structured data, simple validation |
| Medium | Sonnet | Codebase exploration, searching, summarizing documents |
| Heavy  | Opus   | Implementation, complex reasoning, multi-file changes  |

## Subagent Preamble

Include this block at the start of every subagent prompt:

```
IMPORTANT: You are working within the lazyspec workflow.
- Use `lazyspec` CLI commands for document operations. Do NOT write document files directly.
- Read files before editing them. Use the Read tool or `lazyspec show --json` before any modification.
- Implement ONLY what the task specifies. Do not add features, refactor surrounding code, or "improve" things not in the task.
- Before using any `lazyspec` command, run `lazyspec help <subcommand>` to verify usage.
- Always pass `--json` when the command supports it.
```

## Status Promotion

After completing work, promote document statuses up the chain. Run `lazyspec help update` to confirm usage.

1. Mark plan as accepted: `lazyspec update <plan-path> --status accepted`
2. If all plans under a Spec are accepted, mark the Spec as accepted
3. If an RFC exists and all Specs under it are accepted, mark the RFC as accepted

Run `lazyspec validate --json` after updates.

## Anti-patterns

| Pattern | Problem | Fix |
|---|---|---|
| Code blocks in specs | Couples contract to implementation | Move to plan; spec references types only |
| ACs at the bottom | Buried under implementation detail | ACs go in the top half |
| "Evaluate during implementation" | Unresolved design decision | Decide during spec review |
| One plan for the whole spec | Too much in flight | Split by vertical slice |
| Tests without AC mapping | No traceability | Tag each test with AC(s) it covers |
| Notes scattered inline | Hard to find rationale | Collect in Notes section |
| Spec > 100 lines | Doing the plan's job | Extract implementation detail |
