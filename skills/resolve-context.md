---
name: resolve-context
description: Use when an agent needs full context before beginning work on a Story or Iteration. Gathers the document chain from iteration through story to RFC.
---

# Resolve Context

## Workflow

```d2
Identify target doc -> Show target -> Walk related links -> Show each linked doc -> Search for related types -> Context complete

Context complete.shape: double_circle
```

## Steps

1. **Identify the document:** Use `lazyspec list` or `lazyspec search <query>` to find the target document.

2. **Read the document:** Run `lazyspec show <id>` to get its full content and frontmatter.

3. **Walk the chain:** Check the `related` frontmatter for linked documents. For each link, run `lazyspec show <path>` to read the linked document.

4. **Check for existing work:** Run `lazyspec search <story-title>` to find any existing iterations, ADRs, or related documents.

5. **Assemble context:** You now have the full chain: RFC (intent) -> Story (ACs) -> existing Iterations (prior work).

## Rules

- Always resolve context before starting implementation
- Read the full Story ACs before writing any code
- Check for existing iterations to avoid duplicating work
- Search for types and symbols mentioned in the Story before creating new ones
