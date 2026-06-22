---
name: execute
description: Use when carrying out the work a delivery document describes -- the build loop -- against its task breakdown and acceptance criteria.
---

```
DO THE WORK THE DOCUMENT DESCRIBES
```

Execute is the build loop: it carries out the task breakdown of a delivery document, verifying each task against its acceptance criteria.

<HARD-GATE>
Do NOT begin execution without a delivery document that carries a task breakdown. If the document lacks one, author it first (route to the appropriate authoring verb).
Confirm from `lazyspec config --json` that the document's type is a delivery type in this DAG before starting.
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

## No Ceiling

Execute is **work, not authoring.** The `scaffold < co-write < generate` ceiling does not apply here -- it governs who writes a document's prose, not who does the work the document describes. Do not confuse execute with the authoring trio.

## Preflight

1. `lazyspec config --json` -- confirm the document's `<type>` is a delivery type in this DAG (the type whose breakdown describes implementation work; in the shipped default config that is the `iteration` type, but read it -- do not assume the name).
2. `lazyspec show <id> --json` -- read the task breakdown and acceptance criteria.
3. `lazyspec context --json` -- pull the full chain (parent and grandparent docs) for intent and ACs.
4. `lazyspec convention --json` -- load codebase conventions to inform the work.

## Workflow

Iterate the document's tasks:

1. For each task in the breakdown, do the implementation work it describes.
2. Keep the per-task discipline: run **scoped** verification per task (just the tests/checks the task touches), never the full suite mid-loop.
3. Self-review each task against its acceptance criteria before moving on.
4. After all tasks complete, run the **full check once** at the end.
5. On completion, route to /review for critique, then to /advance for the status move.

Express the loop over "the delivery document's tasks" and "its acceptance criteria" -- generically, against whatever delivery type config defines.

## Rules

- The delivery `<type>` is read from `config --json`. No type name is load-bearing in this prose.
- No ceiling concept -- execute is work, not authoring.
- Scoped verification per task; full check once at the end.
- Route to /review then /advance on completion; advance owns the status move.
- Read type/chain facts from config and the CLI, never from `.lazyspec/` graph files directly.
