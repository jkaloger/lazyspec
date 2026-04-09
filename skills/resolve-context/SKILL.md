---
name: resolve-context
description: Use when an agent needs full context before beginning work on a Story or Iteration. Gathers the document chain from iteration through story to RFC.
---

```
NO IMPLEMENTATION WITHOUT FULL CONTEXT
```

If you haven't read the RFC -> Story -> existing Iteration chain, you cannot write code.

<HARD-GATE>
Do NOT begin implementation without completing this skill. Read the full
RFC -> Story -> existing Iteration chain before writing any code.
After completion: use `/create-iteration`.
</HARD-GATE>

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` and `lazyspec link`.
- Do NOT edit a document you haven't read. Always `lazyspec show <id> --json` or `Read` first.
- Do NOT skip the workflow pipeline. Features need RFC -> Story -> Iteration.
</NEVER>

<GITHUB-ISSUES-DOCUMENTS>
Documents stored in GitHub Issues (store = "github-issues") are managed through the GitHub API. The `.lazyspec/cache/` directory contains read-only mirrors.
- Never edit files under `.lazyspec/cache/`. Use `lazyspec update <ID> --body` to modify content.
- Always use shorthand IDs (e.g. STORY-095) not cache file paths when referencing documents in `lazyspec link`, `lazyspec update`, `lazyspec show`, etc.
- To set body content at creation: `lazyspec create <type> <title> --body "content"` or `--body-file <path>`.
- To modify after creation: `lazyspec update <ID> --body "new content"` or `--body-file <path>`.
</GITHUB-ISSUES-DOCUMENTS>

Always run `lazyspec help <subcommand>` before using unfamiliar commands. Always pass `--json`. On failure, check `--help` before retrying.

## Steps

1. **Identify the document:** `lazyspec list --json` or `lazyspec search <query> --json`
2. **Resolve the chain:** `lazyspec context <id> --json` (shows RFC -> Story -> Iteration frontmatter)
3. **Read bodies:** `lazyspec show <id> --json` on Story (for ACs) and RFC (for design intent). Use `lazyspec show -e <id>` to expand `@ref` directives inline.
4. **Check existing work:** `lazyspec status --json` for existing iterations, ADRs, or related documents covering the same ground.
5. **Discover code:** Use file paths and symbols from spec documents as starting points.
6. **State context back:** Summarise the chain: RFC intent, Story ACs, prior iteration work, relevant codebase locations. This confirms you understood it.

## Red Flags

| Red Flag | Reality |
|----------|---------|
| "I already know this codebase" | Prior iterations may have changed assumptions. |
| "I'll read the Story and skip the RFC" | The RFC explains *why*. Without it you're implementing without understanding intent. |
| "I'll look things up as I go" | Ad-hoc context gathering misses the big picture. Resolve the full chain. |

## Verification

- [ ] `lazyspec context <id> --json` run on target document
- [ ] `lazyspec show --json` run on Story ACs and RFC design
- [ ] Existing iterations and ADRs checked
- [ ] Context chain stated back (RFC intent, Story ACs, prior work)
