---
title: A traversal row suppresses the relationship's global marker for both roles
type: adr
status: accepted
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- related-to: RFC-067
- related-to: ADR-030
- related-to: ADR-031
- related-to: STORY-259
---

## Context

RFC-067 moves traversal onto `[[edges]]` rows, but the pre-table declaration is still standing: `traversal` on a `[[relationships]]` row marks a relationship NAME as chain or related for every pair of types it will ever join. Two declarations therefore speak about the same relationship, and one of them has to give way.

They cannot union. This project marks `targets` chain globally, so a union would keep every `targets` link hierarchy no matter what a row said, and STORY-257 AC1 -- a `targets` link no row declares must drop out of a story's chain -- could not hold under any config that also carries a marker. ITERATION-373 took the narrower rule: a row that states a `traversal` for a relationship decides that relationship's walk membership outright and the global marker stops applying to it, while a relationship no row states a traversal for keeps its marker as a blanket fallback (`traversal::states_a_traversal_for`).

What that slice did not argue, and what the code decides anyway, is the ROLE question. `states_a_traversal_for` asks only whether SOME row states a traversal for the relationship, never which role that row assigns. So a row saying `traversal = "related"` about relationship X silences X's global `traversal = "chain"`, and a chain row silences a global related marker the same way. The behaviour is documented in the README and pinned by two unit tests, but the only place it was argued is a work slice -- and it outlives the slice, because it constrains what STORY-258's migration may emit and what STORY-259 may delete.

## Decision

Suppression is role-blind and keyed by relationship name. One `[[edges]]` row whose `via` matches relationship X and whose `traversal` is set -- whatever role it sets, and whatever its `from` and `to` select -- suppresses X's global `[[relationships]].traversal` for BOTH roles. The rows are then the only declaration either walk reads for X.

- The table has spoken about the relationship. A row carries one `traversal` field and RFC-067 gives it no spelling for "state chain here and keep the global related elsewhere", so partial suppression is unsayable. A rule that suppressed only the stated role would leave the other role governed by the very declaration the author was in the act of replacing.
- Suppression is keyed by NAME because the marker is keyed by name. A global marker names a relationship and no types whatsoever, so there is no triple to subtract it from: it is suppressed whole or not at all.
- The rule reaches the WALKS only. `parent-child` rules keep reading `traversal = "chain"` off `[[relationships]]`, so a row that suppresses a marker changes what `context` walks and leaves what `validate` reports alone.

## Consequences

- **A wildcard `via` row suppresses every relationship's marker, silently.** Suppression is broader than a row's own selectors suggest: a row with `via = "*"` and any `traversal` matches every declared relationship, so it suppresses all of their global markers at once. The obvious starter row -- `from = "*"`, `to = "*"`, `via = "*"`, `traversal = "related"`, written to reproduce a blanket related neighbourhood in one line -- also silences `implements`'s global `traversal = "chain"`, and the whole hierarchy flattens: `context` reports every document as a root, both graph views draw a flat list, and `validate` still passes because rules read the marker directly. The config loads, the chain walk is empty, and nothing at load time tells the author. This is RECORDED here, not fixed: a diagnostic is a load-time check with its own design (which rows count as over-broad, error or warning, how a deliberately blanket-suppressing config opts out), and it belongs with the config-authoring surfaces. It is an input to STORY-259, which removes the hazard outright by removing the thing suppressed, and to STORY-261, whose `config edges` editing surface is where a warning would be cheapest to raise.
- Migration must translate per relationship. `fix --config` cannot render a set of markers as one wildcard-`via` row, because that row would suppress every marker it did not translate. Behaviour preservation under STORY-258 is therefore a row per marked relationship, and the hazard above is what makes the shortcut wrong rather than merely untidy.
- This rule expires with the marker. STORY-259 retires `[[relationships]].traversal`, and with nothing left to suppress, `states_a_traversal_for` and the `blanket` list beside it are deleted rather than kept as no-ops.
- A config can lose a role it never meant to touch. Saying "iterations relate to milestones" about a relationship that is otherwise blanket chain costs the author the blanket chain marker, and the chain rows must then be written out by hand. Nobody has asked for the other spelling, and expressing it would need a second field on the row or a per-role opt-out.

## Revisit when

- A row gains a way to name a role it does NOT decide: a second `traversal` field, a list of roles, or an explicit keep-the-blanket opt-out. That makes partial suppression sayable, which is the thing this decision currently forecloses.
- `[[relationships]].traversal` is retired (STORY-259). The rule then governs nothing and should be removed, not preserved.
- Someone builds the load-time diagnostic. The decision stands either way; the silence around it does not, and the hazard consequence becomes a pointer to the check rather than a warning to readers.
