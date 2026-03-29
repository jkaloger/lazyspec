---
title: "Lazyspec Codebase Convention"
type: convention
status: accepted
author: "jack"
date: 2026-03-29
tags: []
---

Lazyspec is a Rust CLI/TUI tool for managing structured project documentation as version-controlled markdown. This convention governs the codebase. Individual dictums elaborate on each principle; when dictums conflict, these principles are the tiebreaker, read in order.

## Principles

1. The codebase exists to produce, validate, and serve structured markdown. Features are justified by how they serve that function.
2. Every command supports `--json`. Convention content is retrievable programmatically. Agents consume the same interfaces humans do.
3. The binary contains three layers: engine (core logic, no I/O assumptions), CLI (command dispatch, output formatting), TUI (state and rendering). Dependencies flow inward. CLI and TUI depend on engine; they never depend on each other.
4. I/O boundaries are defined by traits so that production code and test code share the same interface. Real implementations by default; fakes only at trait seams.
5. Follow Rust's idioms and ecosystem norms. When a Rust convention exists for something, use it rather than inventing a project-specific alternative.
6. Add indirection when there are two concrete uses for it, not before. One implementation does not need a trait. Three similar lines do not need a helper.

## Governance

Dictums are amended by updating the document and recording the rationale in the commit message. If a rule no longer reflects how the codebase works, change the rule or change the code; don't leave them in disagreement.
