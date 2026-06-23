---
name: co-write
description: Use when drafting a document of a configured type collaboratively -- AI proposes a draft body, the human edits, iterate -- up to the type's authorship ceiling.
---

```
PROPOSE A DRAFT, THE HUMAN EDITS, ITERATE
```

<HARD-GATE>
Do NOT proceed when the target type's `authorship` ceiling is `human` -- that type tops out at scaffold. Read the ceiling from `lazyspec config --json` and refuse, naming the ceiling.
Co-write proposes a draft for human editing; it does not finalise a body unilaterally.
</HARD-GATE>

<NEVER>
- Do NOT hand-edit document files. The CLI is the only writer: `lazyspec create` (seed with `--body`/`--body-file`), `lazyspec link`, and `lazyspec update <id> --body`/`--body-file` to change body content. This holds for EVERY store, filesystem included.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured `parent_type` chain and `rules`.
</NEVER>

<BODY-CONTENT>
Set body at creation: `lazyspec create <type> "<title>" --body "content"` or `--body-file <path>` (`-` reads stdin). Change it later: `lazyspec update <ID> --body "content"` or `--body-file <path>`. Prefer `--body`/`--body-file` over any direct file edit, for ALL stores (filesystem and github-issues alike).
GitHub-issues docs additionally: never edit `.lazyspec/cache/` mirrors (read-only); always reference docs by shorthand ID (e.g. STORY-095), not cache paths.
</BODY-CONTENT>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. Read parent/relation/gate facts from the CLI, never from `.lazyspec/` graph files directly. On failure, check `--help` before retrying.

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

Scaffold, interview, then propose:

1. **Create + link** as in /scaffold: `lazyspec create <type> "<title>" --author <name>`, then `lazyspec link <new-id> <relation> <parent-id>` using the configured relation when a parent exists.
2. **Interview the human before drafting.** Co-write captures intent from the human, so grill before you write. Interview them relentlessly about every decision this document must resolve, walking each branch of the design tree and resolving dependencies between decisions one at a time. Ask ONE question at a time. For each question, give your recommended answer. If a question can be answered by exploring the codebase or reading `config --json` / parent docs / `@ref` targets, explore and answer it yourself instead of asking. Continue until every open branch the type's `intent` and section guidance imply is resolved -- do not start the draft with unresolved decisions.
3. **Propose a draft body** toward the type's `intent` and section guidance from config, incorporating the interview answers. Write the proposal to a file.
4. **Present for human edits.** The human revises; iterate the proposal with them.
5. **Apply the accepted draft:** `lazyspec update <id> --body-file <path>`.
