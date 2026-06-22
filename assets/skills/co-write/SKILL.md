---
name: co-write
description: Use when drafting a document of a configured type collaboratively -- AI proposes a draft body, the human edits, iterate -- up to the type's authorship ceiling.
---

```
PROPOSE A DRAFT, THE HUMAN EDITS, ITERATE
```

Co-write is the middle rung: AI scaffolds, then proposes a body toward the type's intent; the human revises and the loop repeats.

<HARD-GATE>
Do NOT proceed when the target type's `authorship` ceiling is `human` -- that type tops out at scaffold. Read the ceiling from `lazyspec config --json` and refuse, naming the ceiling.
Co-write proposes a draft for human editing; it does not finalise a body unilaterally.
</HARD-GATE>

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` and `lazyspec link`.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured `parent_type` chain and `rules`.
</NEVER>

<GITHUB-ISSUES-DOCUMENTS>
Documents stored in GitHub Issues (store = "github-issues") are managed through the GitHub API. The `.lazyspec/cache/` directory contains read-only mirrors.
- Never edit files under `.lazyspec/cache/`. Use `lazyspec update <ID> --body` to modify content.
- Always use shorthand IDs (e.g. STORY-095) not cache file paths when referencing documents in `lazyspec link`, `lazyspec update`, `lazyspec show`, etc.
- To set body content at creation: `lazyspec create <type> <title> --body "content"` or `--body-file <path>`.
- To modify after creation: `lazyspec update <ID> --body "new content"` or `--body-file <path>`.
</GITHUB-ISSUES-DOCUMENTS>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. On failure, check `--help` before retrying.

## Authorship Ceiling

The authorship order is `scaffold < co-write < generate`. A type's `authorship` value in config is the ceiling.

Co-write is the middle rung. It is permitted when the type's `authorship` is `assisted` or `generated`.

**Refuse when the type's `authorship` is `human`.** Read the ceiling from `lazyspec config --json` and report it. Refusal text reads the ceiling out of config -- there is no hardcoded type-to-ceiling table:

> Type `<type>` is human-authored (ceiling = scaffold); drop to /scaffold.

where `<type>` and the ceiling are the actual values read from config for that run.

## Preflight

1. `lazyspec config --json` -- read the target `<type>`: its `intent`, its `authorship` ceiling (gate the verb on this), section guidance from its template, its `parent_type`, and the relation names in `relationships`.
2. `lazyspec status --json` -- locate the parent document to link to.
3. `lazyspec context --json` -- understand the chain around the user's position.

## Workflow

Scaffold-then-propose:

1. **Create + link** as in /scaffold: `lazyspec create <type> "<title>" --author <name>`, then `lazyspec link <new-id> <relation> <parent-id>` using the configured relation when a parent exists.
2. **Propose a draft body** toward the type's `intent` and section guidance from config. Write the proposal to a file.
3. **Present for human edits.** The human revises; iterate the proposal with them.
4. **Apply the accepted draft:** `lazyspec update <id> --body-file <path>`.

## Rules

- The `<type>` is always a parameter read from `config --json`. No type name is load-bearing in this prose.
- Refuse when `authorship` is `human`; the refusal reports the config-read ceiling, not a baked table.
- Always propose-for-edit; never finalise a body without the human in the loop.
- Read parent/relation/gate facts from config, never from `.lazyspec/` graph files directly.
