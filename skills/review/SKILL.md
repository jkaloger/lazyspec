---
name: review
description: Use when critiquing a document against its intent and acceptance criteria, or reviewing completed work, before advancing status.
---

```
CONFORMANCE FIRST, QUALITY SECOND
```

<HARD-GATE>
Do NOT review quality before conformance. The document's acceptance criteria and declared intent come first; block on any conformance failure before looking at quality.
Do NOT approve without fresh verification evidence gathered in this session.
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

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. Read type/lifecycle facts from the CLI, never from `.lazyspec/` graph files directly. On failure, check `--help` before retrying.

## Preflight

1. `lazyspec config --json` -- read the type's `intent` (the bar to critique against) and its `lifecycle` (to know which status review precedes, so the pass-route to /advance targets the right edge).
2. `lazyspec show <id> --json` -- read the document and its acceptance criteria.
3. `lazyspec context --json` -- read the chain (parent intent and ACs) so conformance is judged against the right spec.

## Workflow

Two-stage critique:

**Stage 1 -- Conformance.** Does the document satisfy its declared intent and its acceptance criteria? For work being reviewed, verify each acceptance criterion with fresh evidence run in this session. Block on any conformance failure.

**Stage 2 -- Quality.** Only after conformance passes: critique quality -- clarity, correctness, cohesion, and (for work) test quality. Flag unjustified tradeoffs.

Express targets generically: "the document's acceptance criteria", "its declared intent". No type name is baked in.

## Routing

- **On pass:** route to /advance to move status along the lifecycle edge that review precedes.
- **On fail:** route back to the appropriate authoring verb (one at or below the type's ceiling: /scaffold, /co-write, or /generate) for a document, or to /execute for work.
