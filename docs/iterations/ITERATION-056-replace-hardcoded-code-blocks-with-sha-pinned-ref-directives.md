---
title: Replace hardcoded code blocks with SHA-pinned @ref directives
type: iteration
status: accepted
author: agent
date: 2026-03-12
tags: []
related:
- related-to: RFC-019
---




## Context

12 doc files contain hardcoded Rust code blocks defining types that still exist
in source. Replace each with an `@ref path#Symbol@sha` directive so the code
stays accurate to the document's era. The SHA should be the commit closest to
(but not after) the document's creation date where the symbol existed.

Early design logs (2026-03-04-lazyspec-design, 2026-03-04-lazyspec-implementation,
2026-03-05-ai-workflow-implementation) are excluded -- those are historical records
where the code blocks are part of the narrative.

### SHA lookup strategy

For each document, use `git log --until=<date> -1 --format=%H -- <source-file>`
to find the last commit before or on the document's date that touched the source
file. If the type didn't exist at that commit, use the earliest commit where it
does exist instead (`git log --diff-filter=A --format=%H -- <file>`).

## Changes

### Task 1: Replace code blocks in RFC docs (2 files, 4 refs)

**Files:**
- Modify: `docs/rfcs/RFC-006-tui-progressive-disclosure.md`
- Modify: `docs/rfcs/RFC-013-custom-document-types.md`

**What to implement:**

1. `RFC-006` (date: 2026-03-06): Find the fenced `rust` block defining `ViewMode`.
   Look up SHA: `git log --until=2026-03-06 -1 --format=%H -- src/tui/app.rs`.
   Replace the code block with `@ref src/tui/app.rs#ViewMode@<sha>`.

2. `RFC-013` (date: 2026-03-07): Find fenced `rust` blocks defining `DocType`,
   `TypeDef`, and `Config`. Look up SHAs for each source file
   (`src/engine/document.rs`, `src/engine/config.rs`). Replace each block with
   its corresponding `@ref` directive.

**How to verify:**
```
cargo run -- show RFC-006 -e --json | jq '.body' | grep -c 'enum ViewMode'
cargo run -- show RFC-013 -e --json | jq '.body' | grep -c 'struct TypeDef'
```

### Task 2: Replace code blocks in iterations (10 files, 14 refs)

**Files:**
- Modify: `docs/iterations/ITERATION-001-tui-enhancements-design.md`
- Modify: `docs/iterations/ITERATION-015-test-infrastructure.md`
- Modify: `docs/iterations/ITERATION-024-graph-mode-tree-rendering-and-navigation.md`
- Modify: `docs/iterations/ITERATION-028-config-driven-type-definitions.md`
- Modify: `docs/iterations/ITERATION-029-config-driven-validation-rules.md`
- Modify: `docs/iterations/ITERATION-037-store-parse-error-collection.md`
- Modify: `docs/iterations/ITERATION-038-frontmatter-fix-command.md`
- Modify: `docs/iterations/ITERATION-044-status-picker-overlay.md`
- Modify: `docs/iterations/ITERATION-047-agent-management-screen.md`
- Modify: `docs/iterations/ITERATION-049-forward-and-backward-context-with-related-records.md`

**What to implement:**

For each file, repeat the same pattern: read the doc, find each fenced `rust`
code block containing a type definition, look up the appropriate SHA using the
document's frontmatter date, replace the block with `@ref path#Symbol@sha`.

| Doc | Date | Symbol(s) | Source path |
|-----|------|-----------|-------------|
| ITERATION-001 | 2026-03-05 | `PreviewTab` | `src/tui/app.rs` |
| ITERATION-015 | 2026-03-06 | `TestFixture` | `tests/common/mod.rs` |
| ITERATION-024 | 2026-03-06 | `GraphNode` | `src/tui/app.rs` |
| ITERATION-028 | 2026-03-07 | `DocType` | `src/engine/document.rs` |
| ITERATION-029 | 2026-03-07 | `ValidationRule` | `src/engine/config.rs` |
| ITERATION-037 | 2026-03-08 | `ParseError` | `src/engine/store.rs` |
| ITERATION-038 | 2026-03-08 | `FixResult` | `src/cli/fix.rs` |
| ITERATION-044 | 2026-03-09 | `StatusPicker` | `src/tui/app.rs` |
| ITERATION-047 | 2026-03-09 | `AgentRecord`, `AgentStatus`, `AgentSpawner` | `src/tui/agent.rs` |
| ITERATION-049 | 2026-03-10 | `ResolvedContext` | `src/cli/context.rs` |

**How to verify:**
```
for id in ITERATION-001 ITERATION-015 ITERATION-024 ITERATION-028 ITERATION-029 ITERATION-037 ITERATION-038 ITERATION-044 ITERATION-047 ITERATION-049; do
  echo "$id: $(cargo run -- show $id -e --json 2>/dev/null | jq '.body' | grep -c '```')"
done
```
Each should show expanded code blocks (count > 0), confirming the @ref resolved.

## Test Plan

- **Expansion smoke test**: For each modified doc, run `cargo run -- show <id> -e --json`
  and confirm the body contains fenced code blocks (not raw `@ref` directives).
  This is behavioral and predictive -- if refs expand, the SHAs are valid.

- **Raw mode preserves refs**: Run `cargo run -- show <id> --json` (without `-e`)
  and confirm the body still contains the raw `@ref` directive. Verifies we
  didn't accidentally inline code.

- **No broken refs**: Run `cargo run -- show <id> -e --json` and check that no
  `[unresolved:` warnings appear. If one does, the SHA or symbol name is wrong.

> No new test code is needed. These are manual verification steps using existing
> CLI functionality. The @ref expansion pipeline is already tested in
> `tests/expand_refs_test.rs`.

## Notes

This is a mechanical refactor. The only risk is picking a SHA where the type
doesn't exist yet (e.g. ITERATION-001 references PreviewTab but it may not have
existed at its creation date). In that case, use the first commit that introduced
the type instead of the doc date.
