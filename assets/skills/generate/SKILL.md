---
name: generate
description: Use when authoring a full document body of a configured type from context -- AI writes the complete body, then asks for review -- only permitted when the type's authorship ceiling is `generated`.
---

```
WRITE THE WHOLE BODY, THEN ASK FOR REVIEW
```

Generate is the top rung: AI assembles context and writes the complete body, permitted only when the type's authorship ceiling is `generated`.

<HARD-GATE>
Do NOT proceed unless the target type's `authorship` ceiling is `generated`. For `human` and `assisted` types, refuse and name the permitted verb. Read the ceiling from `lazyspec config --json`.
Generate writes the full body, then routes to /review -- it does not self-approve.
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

Generate is the top rung. It is permitted **only** when the type's `authorship` is `generated`.

**Refuse for `human` and `assisted` types.** This is the headline ceiling-refusal case. Read the ceiling from `lazyspec config --json` and report it together with the permitted verb. The refusal text reads the ceiling string out of config; there is no baked type-to-ceiling table:

> Type `<type>` ceiling = co-write; drop to /co-write.

or, for a human-authored type:

> Type `<type>` ceiling = scaffold; drop to /scaffold.

where `<type>` and the ceiling are the actual values read from config for that run. Map ceiling to verb by the order itself: `human` -> scaffold, `assisted` -> co-write, `generated` -> generate.

## Preflight

1. `lazyspec config --json` -- read the target `<type>`: its `intent`, its `authorship` ceiling (gate the verb on this), section guidance, `parent_type`, and relation names.
2. `lazyspec context --json` -- assemble source material: parent docs, related docs, and referenced code. Expand `@ref` directives and pull referenced code with `lazyspec show -e <id>`.

## Workflow

1. **Create + link:** `lazyspec create <type> "<title>" --author <name>`, then `lazyspec link <new-id> <relation> <parent-id>` with the configured relation when a parent exists.
2. **Write the full body** from gathered context toward the type's `intent` and section guidance. Write to a file.
3. **Apply:** `lazyspec update <id> --body-file <path>`.
4. **Request review:** route to /review. Generate never approves its own output.

## Rules

- The `<type>` is always a parameter read from `config --json`. No type name is load-bearing in this prose.
- Permitted only when `authorship` is `generated`; refuse for `human` and `assisted`, reporting the config-read ceiling and permitted verb -- never a baked table.
- Always route to /review on completion.
- Read parent/relation/gate facts from config, never from `.lazyspec/` graph files directly.
