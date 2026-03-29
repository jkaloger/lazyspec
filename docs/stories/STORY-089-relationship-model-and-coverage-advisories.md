---
title: Relationship Model and Coverage Advisories
type: story
status: draft
author: jkaloger
date: 2026-03-24
tags:
- certification
- relationships
- coverage
related:
- implements: RFC-038
---





## Context

The spec certification system (RFC-031) introduces a two-axis model: a contract layer (specs) and a delivery layer (RFCs, iterations). These layers need to cross-reference each other through relationships. Iterations link to specs via `implements` (intentional conformance work) and `affects` (incidental code overlap). Coverage advisories surface qualitative signals about which parts of the codebase lack spec references, without reducing coverage to a percentage.

## Acceptance Criteria

### AC: affects-relationship-type

Given a new relationship type `affects` is defined
When a user runs `lazyspec link <iteration-path> affects <spec-path>`
Then the relationship is persisted in the iteration's frontmatter
And the relationship is distinct from `implements` in semantics and output

### AC: link-supports-affects

Given the `lazyspec link` command
When invoked with `affects` as the relationship type
Then the command succeeds and writes the relationship to the source document
And `lazyspec validate --json` accepts the relationship as valid

### AC: implements-iteration-to-spec

Given an iteration document and a spec document
When a user runs `lazyspec link <iteration-path> implements <spec-path>`
Then the relationship is persisted in the iteration's frontmatter
And `lazyspec status --json` reflects the link between the iteration and the spec

### AC: coverage-advisories-unref-symbols

Given a codebase with public symbols not referenced by any spec's `@ref` directives
When the user runs `lazyspec status --json`
Then the output includes a `coverage_advisories` field
And entries identify files with public symbols that have zero spec coverage

### AC: coverage-advisories-unref-files

Given non-code files (config, schemas, templates) that no spec references
When the user runs `lazyspec status --json`
Then the `coverage_advisories` field includes entries for those unreferenced files

### AC: coverage-advisories-qualitative

Given coverage advisories are reported
When the user reads `lazyspec status --json` output
Then no percentage or ratio is present in the coverage data
And advisories are qualitative signals with descriptive notes

### AC: cross-layer-display-rfc-to-spec

Given an RFC linked to a spec via `related-to`
When the user runs `lazyspec status --json`
Then the output shows the relationship between the RFC and the spec

### AC: cross-layer-display-iteration-to-spec

Given iterations linked to a spec via `implements` and `affects`
When the user runs `lazyspec status --json`
Then the output shows both relationship types between the iterations and the spec
And `implements` and `affects` are distinguishable in the output

### AC: expected-drift-tagging

Given a draft iteration with an `affects` relationship to a spec
When `lazyspec status --json` reports drift on that spec
Then the drift is tagged as expected, referencing the in-progress iteration

## Scope

### In Scope

- `affects` relationship type: "this iteration touched symbols within this spec's scope"
- `lazyspec link` support for the `affects` relationship type
- Iteration-to-spec linking via both `implements` and `affects`
- `coverage_advisories` field in `lazyspec status --json` output
- Coverage advisories: public symbols with no spec reference, modules with zero spec coverage, non-code files not referenced by any spec
- Coverage advisories as qualitative signals (not percentages)
- Cross-layer relationship display: RFC-to-spec (`related-to`), iteration-to-spec (`implements`, `affects`)

### Out of Scope

- The `spec` document type itself (Story 1)
- Blob pinning / `@{blob:hash}` syntax (Story 2)
- Drift detection / `lazyspec drift` (Story 3)
- Certification workflow / `lazyspec certify` (Story 4)
- The `/write-spec` skill
- Coverage percentages or coverage as a metric to optimize
- Auto-creation of documents based on signals
