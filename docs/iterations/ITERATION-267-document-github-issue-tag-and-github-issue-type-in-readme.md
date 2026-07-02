---
title: Document github_issue_tag and github_issue_type in README
type: iteration
status: complete
author: jkaloger
date: 2026-07-03
tags: []
related:
- implements: STORY-196
---

## Objective

Document `github_issue_tag`/`github_issue_type` in README's `github-issues` store auth section -- doc-only, no code.

## Context

- Story: STORY-196 (assumes STORY-191/192/193/194/195 shipped -- ITERATION-262 through 266 -- so documented behavior matches reality)
- Exact proposed text (four matching cases, dual-materialization/last-write-wins callout, `create` behavior, placement rationale): STORY-196 body verbatim -- copy the proposed addition as written, don't re-derive.
- Touch: README.md, existing `#### \`github-issues\` store auth` section (README.md:537-549) only -- insert after `GH_TOKEN` paragraph, before `#### \`github-milestones\` store`.

## Satisfies

STORY-196 AC1-AC4 (all -- single README insertion).

## Tasks

1. Insert STORY-196's proposed addition (prose + `toml` example + four-case bullet list + dual-materialization/last-write-wins paragraph + `create` behavior paragraph) verbatim into README.md at the specified location.
2. Confirm no other README section is touched.
3. Confirm the added text matches the section's existing prose-plus-`toml`-example convention (Custom Types, Numbering, Validation Rules sections) -- same heading level, same code-fence style.

## Out of scope

- Editing config schema, matching logic, discovery, materialization, or write-side behavior -- STORY-191/192/193/194/195.
- Documenting `github_label`/`label_override` (STORY-190) or its overlap with `github_issue_tag` -- open RFC-055 decision, not documented until resolved.

## Principles/conventions

Documentation-only (CONVENTION.md L1 -- codebase produces/serves structured markdown, this is the README instance of that).

## Verification

- README's `github-issues` auth section documents both fields, the four matching cases (neither/tag-only/type-only/both, AND semantics), dual materialization, and last-write-wins.
- `git diff README.md` touches only the one section -- no other section changed.
- Documented semantics match RFC-055's Design section exactly -- no drift from the four cases as specified.

