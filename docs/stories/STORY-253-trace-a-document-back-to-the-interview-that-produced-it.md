---
title: Trace a document back to the interview that produced it
type: story
status: accepted
author: Jack Kaloger
date: 2026-08-17
tags: []
related:
- implements: RFC-066
---
As a developer reading an RFC, I want to follow a link to the hydra interview that produced it, so that I can see the rejected options and rationale behind its decisions without knowing which `.hydra` file to open.

## Context

Once hydra interviews load as documents (previous slice), they are still isolated: an RFC cannot point at the interview that shaped it, so the reader of a design has no path back to the reasoning. This slice adds traceability in the direction that the data supports, and settles how validation treats a read-only document.

Only inbound links are possible. The hydra JSON has no field naming a lazyspec document, so there is nothing to source an outbound relation from.

## Acceptance Criteria

- **AC1** — **Given** `RFC-066` and the document `HYDRA-HYDRA-STORE`
  **When** I run `lazyspec link RFC-066 related-to HYDRA-HYDRA-STORE`
  **Then** the relation is written to `RFC-066` and `.hydra` is not modified

- **AC2** — **Given** that link exists
  **When** I run `lazyspec show HYDRA-HYDRA-STORE --json`
  **Then** `RFC-066` appears as an inbound relation on the hydra document

- **AC3** — **Given** that link exists
  **When** I run `lazyspec context RFC-066 --json`
  **Then** the hydra document appears in the resolved neighbourhood with its correct `HYDRA-` id

- **AC4** — **Given** a document links to `HYDRA-NO-SUCH-TREE`, which does not exist
  **When** I run `lazyspec validate --json`
  **Then** a dangling-link finding is reported against the linking document, not against any hydra document

- **AC5** — **Given** a hydra document exists and the config declares parent-child or relation-existence rules
  **When** I run `lazyspec validate --json`
  **Then** no authoring-rule finding names a hydra document

- **AC6** — **Given** a hydra document
  **When** I try to add an outbound relation from it with `lazyspec link HYDRA-HYDRA-STORE related-to RFC-066`
  **Then** the command fails with the read-only error and `.hydra` is untouched

## Scope

### In Scope

- Inbound relations resolving to hydra documents by id, in `show`, `context` and the graph
- Exempting hydra documents from authoring validation rules (parent-child, relation-existence)
- Keeping dangling-link validation active for documents that reference a `HYDRA-*` id
- README note on linking to interviews

### Out of Scope

- Outbound relations declared by a hydra document
- Any sidecar file mapping hydra trees to lazyspec documents
- Auto-linking an interview to documents by naming convention

