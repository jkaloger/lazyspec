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
After completion: invoke create-iteration.
</HARD-GATE>

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
Identify target doc -> Show target -> Walk related links -> Show each linked doc -> Search for existing work -> State context back -> Context complete

Context complete.shape: double_circle
```

## Steps

1. **Identify the document:** Use `lazyspec list` or `lazyspec search <query>` to find the target document.

2. **Read the document:** Run `lazyspec show <id>` to get its full content and frontmatter.

3. **Walk the chain:** Check the `related` frontmatter for linked documents. For each link, run `lazyspec show <path>` to read the linked document.

4. **Check for existing work:** Run `lazyspec search <story-title>` to find any existing iterations, ADRs, or related documents.

5. **Discover relevant code:** Use `lazyspec search` and `lazyspec list` to find documents that reference the types, modules, or features you'll be working with. The spec documents often name exact files and symbols -- use those as starting points rather than guessing at file paths.

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

- [ ] `lazyspec show` has been run on the target document
- [ ] `lazyspec show` has been run on the parent Story and RFC
- [ ] Existing iterations and ADRs have been checked
- [ ] Context chain has been stated back (RFC intent, Story ACs, prior work)

## Rules

- Always resolve context before starting implementation
- Read the full Story ACs before writing any code
- Check for existing iterations to avoid duplicating work
- Search for types and symbols mentioned in the Story before creating new ones
