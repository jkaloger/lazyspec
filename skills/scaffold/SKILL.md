---
name: scaffold
description: Use when creating a new document of a configured type at the most manual authorship level -- AI creates the file and frontmatter, surfaces intent and section guidance, then hands the body back to the human.
---

```
CREATE THE SHELL, HAND THE BODY BACK
```

<HARD-GATE>
Do NOT write the document body. Scaffold creates the file, frontmatter, and links, then surfaces the type's intent and section guidance for the human to fill in.
Read the target type's config from `lazyspec config --json` before creating anything; the type is a parameter, never assumed.
</HARD-GATE>

<NEVER>
- Do NOT hand-edit document files to create or link them. Use `lazyspec create` (seed with `--body`) and `lazyspec link`. To change body content, use `lazyspec update <id> --body` -- for EVERY store, filesystem included. (Scaffold itself writes no body; it hands that back to the human.)
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Respect the configured DAG -- type boundaries come from the `edges` table and from nothing else; honor every edge.
</NEVER>

<BODY-CONTENT>
Set body at creation: `lazyspec create <type> "<title>" --body "content"`. Change it later: `lazyspec update <ID> --body "content"`. Prefer `--body` over any direct file edit, for ALL stores (filesystem and github-issues alike).
GitHub-issues docs additionally: never edit `.lazyspec/cache/` mirrors (read-only); always reference docs by shorthand ID (e.g. STORY-095), not cache paths.
</BODY-CONTENT>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. Read parent/relation/gate facts from the CLI, never from `.lazyspec/` graph files directly. On failure, check `--help` before retrying.

## Authorship Ceiling

The authorship order is `scaffold < co-write < generate`. A type's `authorship` value in config (`human`, `assisted`, `generated`) is the *ceiling* -- the highest verb permitted for that type.

Scaffold is the floor of that order, so it is permitted on **every** `authorship` value. **Scaffold never refuses on ceiling grounds.** Even a type whose ceiling is `human` can be scaffolded; that is exactly the manual case scaffold exists for.

## Preflight

1. `lazyspec config --json` -- read the target `<type>` entry: its `intent` (what the doc is for), its `authorship` ceiling (for confirmation only -- scaffold proceeds regardless), and the section guidance available from its template. `parent_type` decides containment only -- the directory this type's documents live under and the store backend they share -- and declares no link.
2. `lazyspec status --json` -- see what already exists and locate the parent document to link to.
3. `lazyspec context --json` -- understand the chain around the user's current position so the new document lands in the right place.

## Workflow

1. **Create the shell:** `lazyspec create <type> "<title>" --author <name>`, where `<type>` is the parameter read from config (e.g. in the shipped default config a type named `rfc`, but never assume that name -- read it).
2. **Link by edge:** find the `edges` rows whose `from` admits this type. A row reads child-to-parent, so the new document sits on the `from` side: the row's `via` is the relation to pass to `lazyspec link`, and its `to` names the types a target document may be. `lazyspec link <new-id> <via> <target-id>`, with `<via>` read off the row -- never bake a relation name into the call. Take the type vocabulary from `types`; a `"*"` filters rather than lists, so never expand one into a type name. When no row admits this type, or no document of a type its `to` admits exists, link nothing and say so.
3. **Surface intent + guidance:** show the human the type's `intent` from config and the per-section `<!-- guidance -->` comments from the scaffolded body. Tell the human these are the sections to fill in.
4. **Hand back:** stop. The human writes the body. Scaffold does not draft prose.
