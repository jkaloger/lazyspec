---
title: Link documents to hydra interviews
type: iteration
status: accepted
author: Jack Kaloger
date: 2026-08-17
tags: []
related:
- implements: STORY-253
---

## Objective

An RFC can link to the hydra interview that produced it, the link shows on the interview, and validate treats read-only documents correctly.

## Satisfies

STORY-253 AC1, AC2, AC3, AC4, AC5, AC6. Depends on ITERATION-364.

## Context

- Story + ACs: STORY-253
- Why inbound-only, and the validation-exemption rationale: RFC-066 §Links and validation
- Touch: `src/engine/store.rs` (`build_links`, reverse-link resolution by id), `src/engine/context.rs`, `src/engine/validation.rs`

## Tasks

1. Confirm reverse links resolve to hydra documents by id through the existing `build_links`. If resolution goes via path rather than id, fix it there rather than special-casing hydra.
2. Exempt hydra documents from authoring rules (`parent-child`, `relation-existence`) in `validation.rs`. Key the exemption off the store being read-only, not off the type name, so a future read-only store inherits it.
3. Leave dangling-link validation firing on the referencing document.
4. Add the linking note to the README section added in ITERATION-364.
5. Tests: inbound relation appears on the hydra document in `show --json` and in `context --json`; a link to a nonexistent `HYDRA-*` id reports against the linking document only; no authoring-rule finding names a hydra document.
6. Link RFC-066 to `HYDRA-HYDRA-STORE` in this repo, dogfooding the result.

## Out of scope

- Outbound relations from a hydra document, and any sidecar mapping file — RFC-066 §Links and validation.
- Auto-linking by naming convention.

## Principles/conventions

`lazyspec convention`. The exemption is a property of the store, not a hardcoded type name.

## Verification

`cargo run -- context RFC-066 --json` includes `HYDRA-HYDRA-STORE`, and `cargo run -- validate --json` reports no finding naming it.

