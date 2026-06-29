---
title: Constrain relation vocabulary by store for github-milestones
type: story
status: accepted
author: jkaloger
date: 2026-06-29
tags: []
related:
- implements: RFC-050
---

## Context

Link editor offers every relation type for every doc, and `link_inner` accepts any `(source, rel, target)` triple. No store-aware guardrail. So a user can pick "implements MILESTONE-x", or make a milestone the source of a relation. Frontmatter gets written, but the write is meaningless: a `github-milestones`-store doc is a REST milestone, not an issue, so it carries no native edge for these relations and has no body to hold frontmatter relations. Result is silently wrong — frontmatter says one thing, GitHub has no corresponding edge, and the relation does nothing useful.

Dogfooding hit this directly. In the TUI link editor the rel-type picker cycles the full `rel_types` list (keys.rs:165) regardless of the viewed doc, and the search results (`update_link_search`) list every doc, milestones included. Pick "implements" against a milestone, confirm, and `confirm_link` → `link_with_config` → `link_inner` writes the `related` entry and returns Ok. `apply_native_milestone` only fires for the `targets` rel (`github_native == "milestone"`), so any other rel against a milestone is a pure frontmatter write with no native counterpart. The user thinks they associated something; nothing happened on GitHub.

The constraint derives from the doc's store. A `github-milestones`-store doc has exactly one legitimate role in the relation vocabulary: the TARGET of `targets` (the `github_native = "milestone"` relation, STORY-158's issue→milestone edge). Stated as rules:

- A `github-milestones` doc may be the TARGET only of a relation whose `github_native == "milestone"` (i.e. `targets`). No other relation may target it.
- A `github-milestones` doc may never be the SOURCE of any relation. Its store is REST-only; there is no issue body to carry frontmatter relations.
- The `targets` relation requires its target to be a `github-milestones` doc — `targets` pointed at a non-milestone is invalid.

Enforcement is dual, deliberately. Core validation in `link_inner` (and symmetrically `unlink_inner`) checks the triple against these rules and rejects invalid combos with a clear error. This covers CLI and TUI uniformly, since both funnel through `link_with_config`. The validation reads the source/target docs' store backend via config (`type_by_name` → `store`) and the relation's `github_native` (`relationship_by_name`). On rejection the TUI surfaces the message through the link-editor error field added in the sibling link-path-reconcile work, so the user sees why the pick was refused instead of a silent no-op.

On top of core, the TUI filters before the user can pick a bad triple. The rel-type picker is scoped to the viewed doc's type: a milestone doc is never offered a relation-source role. When the selected rel-type is `targets`, `update_link_search` shows only `github-milestones` targets; for any other rel-type it excludes milestone docs from results. The filter is a usability layer; the core check is the authority. Both must agree.

This is a guardrail slice, sibling to the native-relation/conflict-guard reconcile story (STORY-167) — same dogfooding lineage, both tightening the milestone link path so the local view and GitHub cannot silently diverge.

## Scope

### In Scope

- Core triple validation in `link_inner`/`unlink_inner` enforcing the three store-derived rules, with a clear rejection error.
- TUI rel-type picker scoped by viewed doc type; never offer a milestone doc a source role.
- TUI search filter: `targets` shows only milestone targets, other rels exclude milestone docs.
- Error surfaced in the link editor via the existing error field.
- Tests at the core seam covering each rejected combo and the one allowed `targets` edge.

### Out of Scope

- Generalizing the guardrail to `github-projects`/membership or any other store — milestones only this slice.
- The native-write / conflict-guard reconcile (STORY-167).
- New relation types or changes to the `[[relationships]]` vocabulary itself.
- Validating existing on-disk frontmatter (this guards new `link`/`unlink`, not a back-scan/migration).

