---
title: CLI next graph critical-path commands
type: story
status: accepted
author: jkaloger
date: 2026-04-30
tags: []
related:
- implements: RFC-041
- blocks: STORY-121
- blocks: STORY-118
priority: should
---






## Context

RFC-041 introduces a graph layer in `engine` that treats `blocks` relationships as a DAG and exposes ready-work, full-graph, and critical-path queries. This Story is the CLI slice: wire three new subcommands (`next`, `graph`, `critical-path`) over the engine primitives, extend `validate` with the three new graph-aware diagnostics, and update `--help` plus the README so the surface is discoverable.

The engine internals (graph construction, smart traversal, cycle detection, critical-path weighting) and the priority field/TOML config land in earlier stories; this slice consumes them. TUI sequencing and the consuming skills (`/sequence`, `/next-work`) come later.

## Acceptance Criteria

- **Given** a project with a clean DAG
  **When** the user runs `lazyspec next --json`
  **Then** the output is a JSON object with `ready`, `bottlenecks`, and `warnings` array fields, where each `ready` entry includes `id`, `kind` (`claimable` | `needs-children` | `needs-status-update`), and a `leased_by` field that is null for unleased docs.

- **Given** a project with leased ready candidates
  **When** the user runs `lazyspec next --json` without `--include-leased`
  **Then** leased docs are omitted from `ready`; **and when** the user reruns with `--include-leased`, the leased docs appear in `ready` with `leased_by` populated.

- **Given** the user passes both `--scope` and `--after` to `next`, `graph`, or `critical-path`
  **When** the command is parsed
  **Then** the CLI exits non-zero with a message stating the two flags are mutually exclusive and does not invoke the engine.

- **Given** the user passes an iteration id to `--scope`
  **When** the command runs
  **Then** the CLI rejects the input with a non-zero exit, names the offending id, and hints that `--scope` only accepts document types with implements-descendants (RFC, Story).

- **Given** an RFC id passed to `--scope`
  **When** `lazyspec next --scope <rfc-id> --json` runs
  **Then** the `ready` set is constrained to that RFC's implements-subtree (and its blocks-ancestors per engine semantics) and the JSON shape is unchanged.

- **Given** the user runs `lazyspec next --type story --json`
  **When** the engine returns ready candidates
  **Then** only docs of type `story` appear in `ready`.

- **Given** a project with a `blocks` cycle
  **When** the user runs `lazyspec next --json`
  **Then** cycle-affected docs are skipped from `ready` and the `warnings` array contains an entry naming the cycle members.

- **Given** a non-empty graph
  **When** the user runs `lazyspec graph --format d2`
  **Then** stdout contains a valid d2 document; **and** `--format json` emits a JSON graph; **and** `--format dot` emits a Graphviz dot document.

- **Given** a project with priorities configured
  **When** the user runs `lazyspec critical-path --json`
  **Then** the output is an ordered JSON array of document ids representing the longest weighted path through the DAG.

- **Given** a project with a `blocks` cycle
  **When** the user runs `lazyspec validate --json`
  **Then** the output contains a cycle error naming the cycle members and exits non-zero.

- **Given** an RFC with status `accepted` whose implementing stories are all in a terminal state
  **When** the user runs `lazyspec validate --json`
  **Then** the output contains a warning suggesting the RFC be promoted to `complete`.

- **Given** a doc whose upstream `blocks` is in status `rejected`
  **When** the user runs `lazyspec validate --json`
  **Then** the output contains a warning that the upstream blocker is rejected and the downstream may be stale.

- **Given** the user runs `lazyspec next --help`, `lazyspec graph --help`, or `lazyspec critical-path --help`
  **When** help renders
  **Then** every flag listed above is documented with its accepted values.

- **Given** the README is checked into the repo
  **When** a reader scans the CLI section
  **Then** `next`, `graph`, and `critical-path` are documented with flags, and the validate section mentions cycle / accepted-but-children-done / rejected-upstream diagnostics.

## Scope

### In Scope

- New CLI subcommands: `lazyspec next`, `lazyspec graph`, `lazyspec critical-path`, with the flag set specified above.
- Mutual-exclusion check for `--scope` / `--after`.
- Type guard on `--scope` rejecting docs without implements-descendants (e.g. iterations) with a hint.
- `--json` output for `next` and `critical-path`; `--format d2|json|dot` for `graph`.
- `next` JSON payload shape: `ready[]`, `bottlenecks[]`, `warnings[]`.
- `--include-leased` toggle on `next`.
- `--type <doc-type>` filter on `next`.
- Three new `validate` diagnostics: cycle error, RFC-accepted-but-children-complete warning, rejected-upstream-blocker warning.
- `--help` text for the new commands and flags.
- README updates documenting the new commands and validate diagnostics.

### Out of Scope

- Engine graph layer, smart-traversal, cycle detection, critical-path computation (Story 1).
- `priority` frontmatter field and `[priorities.*]` TOML parsing (Story 2).
- TUI sequencing screen (Stories 4a/4b/4c).
- `/sequence` and `/next-work` skills (Stories 5 and 6).
- Retiring the existing graph mode (Story 7).
- Daemon-driven autonomous selection (RFC-036).
