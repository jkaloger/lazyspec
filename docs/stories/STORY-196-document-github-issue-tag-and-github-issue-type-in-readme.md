---
title: Document github_issue_tag and github_issue_type in README
type: story
status: accepted
author: jkaloger
date: 2026-07-02
tags: []
related:
- implements: RFC-055
---## Problem

RFC-055 adds two new `[[types]]` fields for `github-issues`-store types --
`github_issue_tag` and `github_issue_type` -- as classification signals
alongside the existing `lazyspec:{type}` label. STORY-191 lands the schema for
both fields; STORY-192/193/194/195 land the matching, discovery,
materialization, and write-side behavior. None of that is discoverable from
the README: the `github-issues` store auth section (README.md:537-549)
documents `gh` auth setup only, with no mention of either field once they
ship.

## Goal

Document `github_issue_tag` and `github_issue_type` in README.md's
`github-issues` store auth section, in the same prose-plus-`toml`-example
style already used by sibling config-field sections (Custom Types, Numbering,
Validation Rules).

## Design

This is a documentation-only story: no code changes. The proposed addition
sits in the existing `#### \`github-issues\` store auth` section
(README.md:537-549), appended after the current `GH_TOKEN` paragraph and
before `#### \`github-milestones\` store`. Field semantics and names are as
specified by RFC-055's Design and Goals sections and STORY-191's schema; this
story does not redefine them, only documents them.

Proposed README addition:

> A `[[types]]` entry may also declare `github_issue_tag` and/or
> `github_issue_type` to control which issues classify as that type,
> independent of (or instead of) the default `lazyspec:{type}` label:
>
> ```toml
> [[types]]
> name = "feature"
> prefix = "FEAT"
> store = "github-issues"
> github_issue_tag = "customer-facing"
> github_issue_type = "Feature"
> ```
>
> - Neither set: unchanged default -- an issue matches when it carries the
>   `lazyspec:{type}` label.
> - Only `github_issue_tag` set: matches every issue carrying that tag; the
>   `lazyspec:{type}` label is not checked.
> - Only `github_issue_type` set: matches every issue whose native GitHub
>   Issue Type equals that value; the label is not checked.
> - Both set: an issue must carry the tag **and** have the native issue type --
>   AND, not OR.
>
> Because these are independent per-type rules, two types may legitimately
> match the same issue (e.g. both set `github_issue_type = "Feature"`). `fetch`
> then materializes one document per matching type from that single issue --
> same issue number, separate cache entries under each type's own prefix and
> numbering. Both documents stay independently `update`-able; each write goes
> to the same issue, so whichever update runs last wins on any field the two
> types share.
>
> `create` on a type with `github_issue_type` set also pushes that value onto
> the new issue's native issue type field (needs the `project` scope described
> above); `create` with `github_issue_tag` set applies that value as a label
> the same way the default `lazyspec:{type}` label is applied today.

Rationale for placement and framing:

- Sits under "auth" because issue-type discovery is GraphQL-based and needs
  the same `project` scope already documented there -- the callout to that
  scope in the last paragraph ties the new fields back to the auth
  requirement instead of repeating it.
- The bullet list mirrors RFC-055's four matching cases (neither / tag-only /
  type-only / both) verbatim, so a reader can map config directly to
  behavior without re-deriving it.
- Dual materialization and last-write-wins are called out briefly, in prose,
  because they are surprising consequences a reader needs before opting in --
  not because this story re-specifies STORY-194's or RFC-055's design (it
  doesn't; it summarizes the user-visible outcome only).
- `github_label` (STORY-190's proposed field, unbuilt, and its overlap with
  `github_issue_tag`) is deliberately not mentioned -- RFC-055 leaves that
  reconciliation as an open decision, so documenting it here would describe
  behavior that doesn't exist yet.

## Non-goals

- Editing README.md itself -- that happens when this story is executed, not
  when it's authored.
- Config schema, matching logic, discovery, materialization, or write-side
  behavior -- STORY-191, STORY-192, STORY-193, STORY-194, and STORY-195
  respectively. This story only documents their combined user-visible
  contract once shipped.
- Resolving `github_label` / `github_issue_tag` overlap -- an open RFC-055
  decision, out of scope here.

## Acceptance criteria

- README.md's `github-issues` store auth section documents `github_issue_tag`
  and `github_issue_type`, including the four matching cases (neither / tag
  only / type only / both, AND semantics) and the dual-materialization /
  last-write-wins consequence of overlap.
- The added text follows the section's existing prose-plus-`toml`-example
  convention (as used by Custom Types, Numbering, and Validation Rules), not a
  new format.
- No other README section is touched.
- The documented semantics match RFC-055's Design section exactly (matching
  rules, dual materialization, `create` write-side behavior) -- no drift.
