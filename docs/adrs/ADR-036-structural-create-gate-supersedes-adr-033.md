---
title: Structural create gate supersedes ADR-033
type: adr
status: review
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- supersedes: ADR-033
- related-to: RFC-071
---

## Context

ADR-033 removed `require_to_status` and let `require_parent_status` die: no `create` refuses because of another document's status. Its consequence was explicit: an agent that ignores its instructions can author an iteration against a `draft` story and nothing stops it. It named two revisit triggers: agents observed crossing the boundary with the after-the-fact finding proving too weak, and `create` gaining a real target argument so a gate checks one named document rather than an existence query across the project.

The second trigger is the one this decision satisfies. `create --parent <id>` exists today, but on the filesystem store it means directory placement (a subdir child, promoting a flat parent) plus a same-store check, and writes no frontmatter relation. On github-issues it creates a native sub-issue. The link that satisfies the edge is a second command, and it is the command agents forget: the dogfood repo carries 30 `iterations-need-stories` errors, nearly all from before the verb skills, and the after-the-fact finding has cleared none of them.

The failure ADR-033 left open is not "wrong status on the parent". It is "no parent named".

## Decision

`create <type>` refuses when the type sits on the `from` side of a chain edge whose `required` is `error` and neither `--parent <id>` nor `--orphan` is given. The refusal names the edge, its `via`, and the candidate parent types.

`--parent <id>` writes the relation at create time: the parent's type selects the matching edge when the child type has several, and the first entry of that edge's `via` is the relation written. Existing `--parent` placement and same-store behaviour are unchanged.

`--orphan` creates without a parent and without a link. The unsatisfied edge remains a `validate` error, as today.

No status is read. The gate is structural: it asks whether a parent was named, not what state it is in. ADR-033's reasoning against status conditions stands untouched.

## Consequences

An agent cannot create an unlinked delivery document by omission. It can only do so by typing `--orphan`, which is a decision the human can see in the transcript and the reviewer can grep for.

A fresh project still writes its first iteration without a story: `--orphan` is the escape hatch ADR-033 wanted to avoid inventing, but it gates omission rather than state, so it costs one flag and no coordination.

Edges with `required = "warning"` gate nothing. `stories-need-rfcs` stays advisory.

`--parent` now carries two meanings on the filesystem store, placement and relation. They were always meant to travel together; the relation was the missing half.

## Revisit when

- `--orphan` appears in transcripts at a rate that says the gate is friction rather than a check.
- A store backend cannot write the relation at create time, so the gate would refuse what the store cannot then satisfy.
