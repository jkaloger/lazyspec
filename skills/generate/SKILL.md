---
name: generate
description: Use when authoring a full document body of a configured type from context -- AI writes the complete body, then asks for review -- only permitted when the type's authorship ceiling is `generated`.
---

```
WRITE THE WHOLE BODY, THEN ASK FOR REVIEW
```

<HARD-GATE>
Do NOT proceed unless the target type's `authorship` ceiling is `generated`. For `human` and `assisted` types, refuse and name the permitted verb. Read the ceiling from `lazyspec config --json`.
Generate writes the full body, then routes to /review -- it does not self-approve.
</HARD-GATE>

<NEVER>
- Do NOT hand-edit document files. The CLI is the only writer: `lazyspec create` (seed with `--body`), `lazyspec link`, and `lazyspec update <id> --body` to change body content. This holds for EVERY store, filesystem included.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured DAG -- type boundaries come from the `edges` table and from nothing else; honor every edge.
</NEVER>

<BODY-CONTENT>
Set body at creation: `lazyspec create <type> "<title>" --body "content"`. Change it later: `lazyspec update <ID> --body "content"`. Prefer `--body` over any direct file edit, for ALL stores (filesystem and github-issues alike).
GitHub-issues docs additionally: never edit `.lazyspec/cache/` mirrors (read-only); always reference docs by shorthand ID (e.g. STORY-095), not cache paths.
</BODY-CONTENT>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. Read parent/relation/gate facts from the CLI, never from `.lazyspec/` graph files directly. On failure, check `--help` before retrying.

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
2. **Resolve residual gaps before writing.** Generate is context-first: it leans on parent docs, related docs, `@ref` targets, and the codebase, not on the human. So capture lightly -- resolve every decision you can from gathered context yourself, then surface ONLY the decisions the context cannot settle. Ask those as a short batch (one at a time, each with your recommended answer); skip the question entirely when the context already answers it. This is a lighter touch than /co-write's full interview -- you are filling gaps, not eliciting the whole design.
3. **Interview about any remaining gaps** Interview the user (as below)
4. **Write the full body** from gathered context and resolved gaps toward the type's `intent` and section guidance. Write to a file.
5. **Apply:** `lazyspec update <id> --body "content"`.
6. **Request review:** route to /review. Generate never approves its own output.

## Interview

When interviewing, grill me relentlessly about every aspect of this plan until we reach a shared understanding. Walk down each branch of the design tree, resolving dependencies between decisions one-by-one. For each question, provide your recommended answer.

Ask the questions one at a time.

If a question can be answered by exploring the codebase, explore the codebase instead.
