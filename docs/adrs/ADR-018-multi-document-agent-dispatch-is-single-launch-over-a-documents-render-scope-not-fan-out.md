---
title: Multi-document agent dispatch is single-launch over a documents render scope, not fan-out
type: adr
status: draft
author: jkaloger
date: 2026-06-18
tags: []
related:
- related-to: RFC-047
---

## Context

RFC-047 lets a user mark a set of documents and dispatch an agent over the set. RFC-046 dispatches over the single cursor document. Two shapes could carry a set to agents. Fan-out spawns one agent per marked document: N background processes, N `AgentRecord`s, N lifecycles to poll and reconcile. Single-launch spawns one agent with the whole set in its prompt scope: one process, the same lifecycle as a single-document run.

The fan-out shape is the orchestration model this project already rejected. RFC-036 and RFC-041 (agent orchestration daemon) proposed a long-lived scheduler that leases work across many concurrent agents; both are superseded/rejected, and lazyspec's scope is a structured-markdown document tool, not an agent scheduler.

## Decision

Multi-document agent dispatch is single-launch. Marking a set and selecting an action spawns one agent over the set; it never spawns one agent per document. The run is identical in lifecycle to RFC-046's single-document run -- one `AgentRecord` for headless, none for interactive.

A multi-document run renders against a `documents` list (each entry exposing the same fields as RFC-046's `document`), with no singular `document`. Template arity is declared in frontmatter (`multi: bool`, default `false`); the dialog offers `multi: true` templates only when the marked set is non-empty and single-document templates only when it is empty, so every offered action is renderable. A heterogeneous set resolves to the intersection of its types' allowed templates. Headless carries the set as `doc_paths: Vec<PathBuf>` on `AgentContext` and spawns once; interactive exports the set as `$LAZYSPEC_DOC_PATHS` and hands over the terminal once.

Rejected: fan-out (one agent per document). It reintroduces the multi-agent lifecycle -- per-document records, polling, partial-failure reconciliation -- that is the rejected daemon line (RFC-036 / RFC-041), and it makes a set dispatch structurally different from a single dispatch rather than a generalisation of it. Rejected: a per-template `scope: each|all` selector. With single-launch as the only shape, there is one unit of work (the set) and nothing to select. Rejected: binding both `document` and `documents` in every run so any template renders. The singular `document` would silently bind to one member of the set, so a single-document template run over five marked documents would operate on one and ignore four -- a silent-wrong-result footgun; arity declaration makes the mismatch impossible to select instead of silently wrong.

## Consequences

- A multi-document run costs exactly what a single-document run costs: one spawn, one record (headless) or one handover (interactive). No scheduler, no reconciliation.
- The agent decides how to treat the set from the prompt the template author wrote; lazyspec does not impose a per-document iteration or a goal protocol.
- A template is authored for one arity. A project that wants both a single-document and a set version of an action authors two templates (`multi: false` and `multi: true`).
- The marked-set selection primitive (RFC-047) is independent of this decision; bulk non-agent operations over the set remain admissible later without revisiting dispatch shape.
