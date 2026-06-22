---
title: Binary owns data, skill owns prose
type: adr
status: accepted
author: jkaloger
date: 2026-06-21
tags:
- skills
- cli
- architecture
related:
- related-to: RFC-048
---

## Context

Config-driven workflow skills (RFC-048) need two things to drive spec-driven development over an arbitrary DAG: the structural facts (what types exist, their relations, statuses, gates, intent) and the authoring methodology (how to write each type well, how to run the build loop). The facts are derivable from config; the methodology is prose that iterates often and cannot be derived.

Three altitudes were considered for where the routing/guidance logic lives: a binary command that emits ready-to-act guidance (prose in Rust), skills that hold everything (logic re-authored per agent runtime), or a split.

## Decision

The binary owns data; the skill owns prose. The binary serves config and document state as JSON (`config --json`, plus existing `status`/`context --json`) and enforces config invariants on the existing mutation commands. It does **not** decide what verb to run next. The skill reads that data, computes eligibility/ceiling/gate state, and applies authoring methodology held in markdown.

For v1 there is no routing/eligibility "brain" command (`next`/`guide`). The derivation lives in skill prose.

## Consequences

- The data layer is identical across runtimes and testable in Rust; prose stays fast to iterate.
- Eligibility/ceiling/gate derivation in prose is untestable and is the one real per-runtime drift vector. Mitigated because the skill prose is single-sourced portable markdown (skill ≈ AGENTS.md).
- Escape valve: if prose eligibility proves fragile, promote a `next`/`guide` command into the binary. The data contract already exists, so promotion is additive.

Locked for RFC-048.
