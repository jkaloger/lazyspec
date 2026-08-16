---
title: Read hydra interviews as documents
type: story
status: accepted
author: Jack Kaloger
date: 2026-08-17
tags: []
related:
- implements: RFC-066
- blocks: STORY-253
---
As a developer who designs with hydra interviews, I want my `.hydra` trees to appear as lazyspec documents, so that the decisions behind a design are visible in the same place as the design.

## Context

Design decisions are made in hydra interviews stored as `.hydra/<slug>.json`, one directory away from the docs they produce and structurally invisible to lazyspec — absent from `list`, unfindable by `search`, unreadable in the TUI and web view.

This is the walking skeleton for RFC-066: the thinnest end-to-end path that makes an interview a first-class document. It covers the `StoreBackend::Hydra` variant, direct JSON parsing with no `.lazyspec/cache` mirror, body rendering, derived status, and the failure modes. Linking interviews to other documents is a separate slice.

## Acceptance Criteria

- **AC1** — **Given** a `.lazyspec.toml` with a `[[types]]` entry `name = "hydra"`, `store = "hydra"` and a `.hydra/hydra-store.json` tree
  **When** I run `lazyspec list --json`
  **Then** a document with id `HYDRA-HYDRA-STORE` and type `hydra` is listed

- **AC2** — **Given** that tree has 20 answered heads and 0 open
  **When** I run `lazyspec show HYDRA-HYDRA-STORE`
  **Then** the body contains the interview's intent, an ASCII tree, a `## Decisions` section carrying each question with its answer, rationale and rejected alternatives, and a `## Open questions` section

- **AC3** — **Given** a tree with at least one open head
  **When** I run `lazyspec show --json` on it
  **Then** its status is `in-progress`; **and given** all heads are answered **then** its status is `complete`; **and given** the tree has no heads **then** its status is `draft`

- **AC4** — **Given** a cauterised head
  **When** the body renders
  **Then** it appears under `## Decisions` with its `cauterised_by` slug noted, not under `## Open questions`

- **AC5** — **Given** the hydra type is configured
  **When** lazyspec loads
  **Then** no file is written under `.lazyspec/cache/`, and `.hydra` is not modified

- **AC6** — **Given** a running TUI and a live interview
  **When** `hydra cut` answers a head
  **Then** the file watch reloads and the document body and status reflect the cut without restarting

- **AC7** — **Given** the same repo
  **When** I open the web view
  **Then** the hydra document renders with the same id and body as the CLI shows — this requires `src/web/render.rs:316` and `src/engine/graph.rs:446` to resolve ids through `DocMeta.id` rather than re-deriving from the path stem, which yields `hydra` for `.hydra/hydra-store.json`

- **AC8** — **Given** no `.hydra` directory exists
  **When** any lazyspec command runs
  **Then** it succeeds with zero hydra documents and no error

- **AC9** — **Given** one tree file is unparseable or written by a newer hydra schema
  **When** lazyspec loads
  **Then** that file becomes a `ParseError` in `store.parse_errors`, every other document still loads, and the command still succeeds

- **AC10** — **Given** the `hydra` binary is not installed
  **When** any lazyspec command runs
  **Then** behaviour is unchanged — lazyspec never invokes it

- **AC11** — **Given** a hydra document
  **When** I run `lazyspec update`, `create`, or `link` against it
  **Then** the command fails with an error naming the `hydra` command to use instead, and `.hydra` is untouched

## Scope

### In Scope

- `StoreBackend::Hydra` variant and its config deserialization (`store = "hydra"`), with `dir` defaulting to `.hydra` and `prefix` to `HYDRA`
- A JSON loader reading every `*.json` in the configured dir through the existing `FileSystem` trait
- Id derivation: `"HYDRA-" + slug.to_uppercase()`
- Body rendering from parsed heads, including the ASCII tree rendered by lazyspec
- Status derivation from open-head count
- Fixing the two path-derived id sites so graph and web resolve hydra ids correctly
- File-watch coverage of the hydra dir
- Read-only enforcement on write commands
- Failure modes: missing dir, unparseable JSON
- README section for the hydra store

### Out of Scope

- Linking other documents to hydra interviews and validation behaviour (next slice)
- Any write path into `.hydra` — no cut, sprout, reopen or reword from lazyspec
- Invoking the `hydra` binary
- Per-head documents, a dedicated TUI mode, new CLI subcommands
- Reading `.hydra/HEAD`
- Enabling the type in the shipped default config

