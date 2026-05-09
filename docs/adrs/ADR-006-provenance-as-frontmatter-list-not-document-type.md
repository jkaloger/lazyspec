---
title: Provenance as frontmatter list of free-form strings
type: adr
status: accepted
author: jkaloger
date: 2026-04-29
tags: []
related:
- related-to: RFC-039
---


## Context

Provenance feature (RFC-039) needs a place to record source-of-truth citations on documents. Three shapes were on the table:

A. **Frontmatter list of free-form strings**: `provenance: ["GDPR Art. 17", "2026-Q1 Workshop"]`. Modeled on `author`. No structure beyond text.

B. **Frontmatter list of typed entries**: `provenance: [{kind: legislation, name: ..., ref: ...}]`. Closed `kind` enum (person/workshop/legislation), required `name`, optional `ref`.

C. **Source-as-document-type**: introduce `person`, `workshop`, `legislation` doc types and link via relationships.

## Decision

Frontmatter list of free-form strings (A).

## Rationale

- KISS. `Vec<String>` on `DocMeta`, no new module, no enum, no struct, no kind validation.
- Mirrors existing `author` field. Same shape, same TUI column treatment, same mental model.
- `kind` enum (B) buys nothing concrete: nothing in the engine branches on it, the citation text already conveys what kind of source it is, and a closed enum forces awkward fits ("standards body? regulator? court ruling?").
- Source-as-document (C): sources have no independent lifecycle. A person isn't versioned or audited the way an RFC is. Doc-type churn (numbering, templates, validation, TUI handling) without payoff.
- Reversible. If structure is later needed, migrate strings into typed entries; the field name stays.

## Consequences

- Same workshop cited from ten docs is repeated ten times. Acceptable: short string, no canonical store to keep in sync.
- No structured filtering (`--kind legislation`). Substring match on the strings is enough; users can prefix their citations if they want grep-friendly conventions.
- TUI shows provenance as a column alongside author. Comma-joined, truncated.
- Future per-requirement attribution remains independent of this decision.
