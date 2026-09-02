---
name: review
description: Use when critiquing a document -- its prose, its intent, its acceptance criteria -- before advancing its status.
---

```
CONFORMANCE FIRST, QUALITY SECOND
```

Review critiques **documents**. Its sibling /review-work critiques **code** against the document that specified it. If you are reading a diff rather than a document body, you are in the wrong skill.

<HARD-GATE>
Do NOT review quality before conformance. The document's acceptance criteria and declared intent come first; block on any conformance failure before looking at quality.
Do NOT approve without fresh verification evidence gathered in this session.
Do NOT review landed code here. Route to /review-work, which carries the convention stage and the diff verdicts.
</HARD-GATE>

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` and `lazyspec link`.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured DAG -- type boundaries come from the `edges` table and from nothing else; honor every edge.
</NEVER>

<GITHUB-ISSUES-DOCUMENTS>
Documents stored in GitHub Issues (store = "github-issues") are managed through the GitHub API. The `.lazyspec/cache/` directory contains read-only mirrors.
- Never edit files under `.lazyspec/cache/`. Use `lazyspec update <ID> --body` to modify content.
- Always use shorthand IDs (e.g. STORY-095) not cache file paths when referencing documents in `lazyspec link`, `lazyspec update`, `lazyspec show`, etc.
- To set body content at creation: `lazyspec create <type> <title> --body "content"`.
- To modify after creation: `lazyspec update <ID> --body "new content"`.
</GITHUB-ISSUES-DOCUMENTS>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. Read type/lifecycle facts from the CLI, never from `.lazyspec/` graph files directly. On failure, check `--help` before retrying.

## Preflight

1. `lazyspec config --json` -- read the type's `intent` (the bar to critique against) and its `lifecycle` (to know which status review precedes, so the pass-route to /advance targets the right edge).
2. `lazyspec show <id> --json` -- read the document and its acceptance criteria.
3. `lazyspec context --json` -- read the chain (parent intent and ACs) so conformance is judged against the right spec.

## Workflow

Two-stage critique:

**Stage 1 -- Conformance.** Does the document satisfy its declared intent and its acceptance criteria? Does it satisfy the `edges` its type sits on -- the rows whose `from` admits it, at the `required` severity each carries? Block on any conformance failure.

**Stage 2 -- Quality.** Only after conformance passes: critique quality -- clarity, correctness, cohesion, whether the acceptance criteria are actually checkable, whether a delivery document's task breakdown is sized for one agent pass. Flag unjustified tradeoffs.

Express targets generically: "the document's acceptance criteria", "its declared intent". No type name is baked in.

## Routing

- **On pass:** route to /advance to move status along the lifecycle edge that review precedes.
- **On fail:** route back to the appropriate authoring verb, one at or below the type's ceiling: /scaffold, /co-write, or /generate.
- **Reviewing landed work rather than a document:** route to /review-work.
