---
title: 'Interactive bootstrap: full DAG designer from scratch'
type: story
status: in-progress
author: jkaloger
date: 2026-07-19
tags: []
related:
- implements: RFC-062
---> As a person setting up a bespoke lazyspec project, I want the `init` wizard to let me design the entire type DAG from scratch — every type, lifecycle, gate, and relation — review it, and confirm before writing, so that I can bootstrap a config that owes nothing to the starter defaults.

The full DAG designer (RFC-062). Layers a blank-slate path onto the STORY-227 bootstrap wizard, resolving the ordering dependencies between DAG design steps (types must exist before lifecycles reference them, before relations/gates reference both).

## Scope

- `init` wizard offers "blank slate" alongside STORY-227's "start from starter". Blank slate walks: **types → per-type lifecycle → parent types / parent-child rules / gates → relation vocabulary → review → write**.
- Each step only offers targets that earlier steps have defined (a gate's parent, a relation's endpoints).
- A rendered **DAG summary** (types, edges, gates, relations) is shown for confirmation before anything is written.

## Acceptance criteria

- **Given** the blank-slate path, **when** I add types then design lifecycles, **then** a lifecycle edge or gate can only reference a type/status already defined; invalid references are re-prompted.
- **Given** I define a parent-child rule, **when** I pick the child and parent, **then** only already-defined types are offered, and I set severity (warning/error).
- **Given** I finish designing, **when** the wizard renders the DAG summary, **then** it lists every type, its lifecycle, gates, and relations, and asks me to confirm before writing.
- **Given** I confirm, **then** the written `.lazyspec.toml` passes `lazyspec validate` and the scaffolded dirs/templates match the designed DAG.
- **Given** I decline at the summary, **then** nothing is written and no directories are created.
- **Given** non-TTY / `--non-interactive`, **then** the blank-slate path is unreachable (STORY-227's non-interactive rule holds — writes `starter_config()`).

## Non-functional / constraints

- Ordering is enforced by the prompt sequence, not by post-hoc validation; the wizard resolves DAG dependencies as it goes.
- Reuses the STORY-225/226/227 `Prompter` seam and type-authoring flow; still one config writer.
- README: document the blank-slate DAG designer.