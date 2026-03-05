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
After completion: use the `/create-iteration` skill.
</HARD-GATE>

## Forbidden Actions

<NEVER>
- Do NOT write document files directly. Use `lazyspec create` to create documents and `lazyspec link` to create relationships.
- Do NOT edit a document you haven't read. Always `lazyspec show <id>` or `Read` a file before modifying it.
- Do NOT skip the workflow pipeline. Features need RFC -> Story -> Iteration. Bug fixes need Iteration.
</NEVER>

# Resolve Context

## Workflow Position

```d2
plan -> write-rfc -> create-story -> resolve-context -> create-iteration -> build

resolve-context.style.fill: "#4A9EFF"
resolve-context.style.font-color: "#FFFFFF"
plan.style.opacity: 0.4
write-rfc.style.opacity: 0.4
create-story.style.opacity: 0.4
create-iteration.style.opacity: 0.4
build.style.opacity: 0.4
```

## Workflow

```d2
Identify target doc -> Resolve chain -> Read bodies if needed -> Check for existing work -> State context back -> Context complete

Context complete.shape: double_circle
```

## Preflight

1. Identify the target document path or ID
2. Confirm the document exists with `lazyspec show <id>`

## Subagent Dispatch

| Tier | Model | Use for |
|------|-------|---------|
| Light | Haiku | Parsing frontmatter, extracting structured data, simple validation |
| Medium | Sonnet | Codebase exploration, searching for patterns, reading and summarizing documents |
| Heavy | Opus | Implementation, complex reasoning, multi-file changes, review |

| Operation | Agent Type | Tier | Context to provide |
|-----------|-----------|------|-------------------|
| Discover relevant codebase files | Explore | Medium | Type names, module paths from spec documents |
| Summarize context | _(inline)_ | - | Main agent synthesizes findings |

## Steps

1. **Identify the document:** Use `lazyspec list` or `lazyspec search <query>` to find the target document.

2. **Resolve the chain:** Run `lazyspec context <id>` to get the full implements chain (RFC -> Story -> Iteration) in one call. Use `lazyspec context <id> --json` if you need structured output for parsing.

3. **Read document bodies:** The context command shows frontmatter only. For documents where you need the full body (typically the Story ACs and RFC design intent), follow up with `lazyspec show <id>` on those specific documents.

4. **Check for existing work:** Run `lazyspec status --json` to get all documents, relationships, and validation results in one call. Look for existing iterations, ADRs, or related documents that cover the same ground.

5. **Discover relevant code:** The spec documents often name exact files and symbols -- use those as starting points rather than guessing at file paths.

6. **Assemble context:** You now have the full chain: RFC (intent) -> Story (ACs) -> existing Iterations (prior work) -> relevant codebase locations.

7. **State it back:** Before proceeding, summarise the context chain: what the RFC intends, what ACs the Story defines, what prior iterations have already done, and which parts of the codebase are involved. This forces you to confirm you actually understood it.

## Red Flags

| Red Flag | Reality |
|----------|---------|
| "I already know this codebase" | Knowledge decays. Prior iterations may have changed assumptions. |
| "I'll read the Story and skip the RFC" | The RFC explains *why*. Without it you're implementing without understanding intent. |
| "I'll look things up as I go" | Ad-hoc context gathering misses the big picture. Resolve the full chain. |

## Verification

Before claiming this skill is complete:

- [ ] `lazyspec context <id>` has been run on the target document
- [ ] `lazyspec show` has been run on documents where the body is needed (Story ACs, RFC design)
- [ ] Existing iterations and ADRs have been checked (via `lazyspec status --json` or search)
- [ ] Context chain has been stated back (RFC intent, Story ACs, prior work)

## Rules

- Always resolve context before starting implementation
- Read the full Story ACs before writing any code
- Check for existing iterations to avoid duplicating work
- Search for types and symbols mentioned in the Story before creating new ones
