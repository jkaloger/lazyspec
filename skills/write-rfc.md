---
name: write-rfc
description: Use when proposing a design or significant change. Creates an RFC document with intent, interface sketches, and identifies the Stories that fall out of it.
---

# Write RFC

## Workflow

```d2
Understand the problem -> Create RFC -> Write intent and context -> Sketch interfaces -> Identify stories -> Validate

Validate.shape: double_circle
```

## Steps

1. **Understand the problem:** Search existing docs with `lazyspec search <topic>` to avoid duplicating prior work. Check for superseded RFCs.

2. **Create the RFC:** Run `lazyspec create rfc "<title>" --author <name>`

3. **Write intent:** Describe the problem being solved and why. This is design intent, not implementation detail.

4. **Sketch interfaces:** Use `@draft` syntax for types that don't exist yet:
   ```
   @draft UserProfile { id: string; email: string }
   ```
   Use `@ref` for types that already exist in the codebase:
   ```
   @ref src/types/user.ts#UserProfile
   ```

5. **Identify Stories:** List the vertical slices that fall out of this RFC. Each should be independently shippable.

6. **Emit ADRs:** For significant decisions made during RFC writing, create ADRs: `lazyspec create adr "<decision>"` and link them: `lazyspec link <adr-path> related-to <rfc-path>`.

7. **Validate:** Run `lazyspec validate`.

## Rules

- RFCs describe intent, not implementation
- An RFC is a design record -- it captures thinking at the time of writing
- Sketch interfaces in prose or TypeScript, not as live code
- Every RFC should identify at least one Story
