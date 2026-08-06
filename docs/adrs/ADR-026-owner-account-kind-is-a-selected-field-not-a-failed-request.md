---
title: Owner account kind is a selected field, not a failed request
type: adr
status: draft
author: Jack Kaloger
date: 2026-08-06
tags: []
related:
- implements: RFC-065
---

## Context

Resolving a Projects v2 board's field schema needed the owner's account kind,
and the code learned it by guessing: `gh_schema::try_org_then_user` fired an
`organization(login:)`-rooted query and, if the response's node pointer came
back null, fired the `user(login:)`-rooted one. A user-owned repo therefore
paid two requests per board -- one of them known in advance to fail -- and the
discriminator was a failed request rather than a value.

That cost multiplied. `refresh_schema_snapshot` runs once per `github-issues`
type, so a config with ten such types and one authority board issued ten to
twenty board-schema probes per fetch, on both the CLI and the TUI poll.

GraphQL will merge two selections that share an alias when both resolve the
same type. `projectV2(number:)` resolves `ProjectV2` under `Organization` and
under `User` alike, so the same alias may be written into both inline fragments
of one `owner { ... }` selection. Verified against the live API in a single
request.

## Decision

The owner subtree selects `__typename` and carries every authority board as
`b<n>: projectV2(number: n)` inside **both** the `... on Organization` and the
`... on User` fragment, from one shared `fields` selection constant. Whichever
fragment applies answers; the other contributes nothing. The account kind is a
selected field of the response, never something inferred from a request that
failed.

`issueTypes` stays inside the Organization fragment alone, because that is
where GitHub defines it. Its absence on a user-owned repo is an answer -- that
account has no issue types -- not a failure, and so resolves to an empty set
with no warning. Boards differ: their alias is present under either account
kind, so an absent or null `b<n>` is a failure and keeps the board's prior ids
(ADR-025).

`try_org_then_user`, `fetch_project_fields`, `OwnerKind` and the two
`PROJECT_FIELDS_*` query constants are deleted.

## Consequences

Board schemas cost zero additional requests: they ride the round that already
fetches milestones and issue types, whatever the number of boards or types.

Issue types are now keyed on `repository.owner.issueTypes` rather than on
`repository.owner`, so an `errors[]` entry naming one board no longer condemns
its sibling selections.

A bare-owner node-id lookup (`resolve_project_id_live`, `resolve_owner_node_id`)
still probes org-then-user. The round is rooted at a repository and cannot
answer for an owner login alone, so that probe moved into `store_dispatch` as a
private helper rather than being removed.

