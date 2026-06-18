---
title: Strict config load with no engine-baked defaults
type: adr
status: accepted
author: jkaloger
date: 2026-06-18
tags:
- config
- architecture
related:
- related-to: RFC-042
---

## Context

The engine carries default types, rules, and (via the enum) relationships in code: `default_types()`, `default_rules()` (`src/engine/config.rs`). These defaults are applied as a silent fallback when config is absent. For the tool to be unopinionated about taxonomy, the load path must not inject an ontology the project did not declare.

## Decision

Load is strict. Missing `[[types]]` or `[[relationships]]` is a hard error, not a fallback. The engine carries no default types, relationships, or rules. The sole home for defaults is the config that `init` writes (and `fix` injects on migration). The strict-load error names the remedy (`init` for fresh projects, `fix` for existing ones).

Rejected: lenient fallback to builtins (weakest form of unopinionated — engine still owns a full default set) and a split policy (types strict, relationships lenient — inconsistent, two rules to reason about).

## Consequences

- Behavior of a fresh `init` project is unchanged, because `init` writes the same starter set the engine used to hardcode.
- Existing projects break on upgrade until migrated (none declare `[[relationships]]` yet) — handled by ADR-012.
- Config is the single source of truth for taxonomy; no hidden defaults to reconcile against.
