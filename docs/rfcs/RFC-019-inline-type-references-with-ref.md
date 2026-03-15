---
title: "Inline type references with @ref"
type: rfc
status: accepted
author: "jkaloger"
date: 2026-03-10
tags: [refs, types, rendering]
---


## Summary

Documents should be able to reference types from source files using `@ref` directives. When rendered (via `lazyspec show` or TUI), these directives expand into fenced code blocks showing the extracted type definition. An optional git SHA pins the reference to a specific commit, creating permalink-style references.

## Problem

Spec documents frequently refer to types defined in code. Today, authors copy-paste type definitions into documents, which drift out of sync as code evolves. There's no way to create a living reference that stays current, or a pinned reference that captures a type at a specific point in time.

## Syntax

```
@ref src/tui/agent.rs#AgentRecord@HEAD
```

```
@ref <path>#<symbol>
@ref <path>#<symbol>@<sha>
```

- `path` -- relative file path from the repo root (e.g. `src/types/user.ts`)
- `symbol` -- the named type, interface, struct, or enum to extract
- `sha` (optional) -- git commit SHA to resolve against. When omitted, resolves against HEAD.

### Examples

```markdown
See the user profile shape:

@ref src/types/user.ts#UserProfile

And the version we shipped in v1:

@ref src/types/user.ts#UserProfile@a1b2c3d
```

Expands to:

````markdown
See the user profile shape:

```ts
type UserProfile = {
  id: string;
  email: string;
}
```

And the version we shipped in v1:

```ts
type UserProfile = {
  id: string;
}
```
````

## Design

### Ref expansion as a body transform

The current rendering pipeline is:

```
Store::get_body() -> raw markdown -> render (CLI print / tui_markdown)
```

This becomes:

```
Store::get_body() -> expand_refs() -> rendered markdown -> render
```

`expand_refs()` scans the body for `@ref` directives, resolves each one, and replaces it with a fenced code block. The language tag on the fence is derived from the file extension (`.ts` -> `ts`, `.rs` -> `rust`, etc.).

### File resolution

| Ref form | Resolution |
|----------|-----------|
| `@ref path#symbol` | `git show HEAD:<path>`, extract symbol |
| `@ref path#symbol@sha` | `git show <sha>:<path>`, extract symbol |

Using `git show` rather than reading the working tree means refs are reproducible -- they resolve against committed state. This also makes SHA-scoped refs work uniformly: same mechanism, different revision.

If git is unavailable or the path doesn't exist at the given revision, the ref is unresolvable.

### Symbol extraction with tree-sitter

Tree-sitter parses the file into an AST. A query then extracts the named symbol's full definition. This gives accurate extraction regardless of formatting, comments, or nesting.

Initial language support: TypeScript and Rust. Each language needs:
- A tree-sitter grammar (crate dependency)
- A symbol query pattern (e.g. "find `type_alias_declaration` where name matches")

The extraction interface should be a trait so new languages can be added by implementing the trait and registering a grammar + query.

```
@draft SymbolExtractor {
  fn extract(source: &str, symbol: &str) -> Option<String>
}

@draft RefExpander {
  fn expand_refs(body: &str, repo_root: &Path) -> String
}
```

### Unresolved refs

When a ref can't be resolved (file not found, symbol not found, bad SHA), it renders as a warning block:

```markdown
> ⚠️ [unresolved: src/types/user.ts#UserProfile]
> Could not find symbol UserProfile in src/types/user.ts
```

This is non-blocking -- the rest of the document renders normally.

### Crate dependencies

| Crate | Purpose |
|-------|---------|
| `tree-sitter` | Core parsing runtime |
| `tree-sitter-typescript` | TypeScript grammar |
| `tree-sitter-rust` | Rust grammar |

These are compile-time dependencies. The grammars include C source that gets compiled during build, which will increase build time.

## Stories

1. **Ref parsing and expansion pipeline** -- Parse `@ref` directives from markdown bodies, wire up `expand_refs()` as a transform step in `get_body()`, resolve file content via `git show`.

2. **Tree-sitter symbol extraction** -- Integrate tree-sitter with TypeScript and Rust grammars. Implement the extractor trait. Extract named types, interfaces, structs, and enums from source.

3. **Rendering integration** -- Expand refs in CLI `show` output and TUI preview. Handle unresolved refs with warning placeholders. Detect language from file extension for code fence tags.
