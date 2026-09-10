---
title: Adopt a shared doc set with a one-line config
type: story
status: complete
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: RFC-072
reviewed: 4fe4f87707975ac0780871beb31ecbc3a3973b4e
---

## Context

A code repo participates in a shared doc set by declaring one line. Everything else -- types, DAG, documents -- comes from the extended location.

The trap is what "root" means. `Store` carries two: the doc root, and `governs_root` from `config.governs.root`, defaulting to `"."` (`src/engine/store.rs:265`). Moving both would point every git question about *code* at the shared spec repo -- `src/cli/why.rs:45`, `validation.rs:985` and `:1138`, `staleness.rs:173`, and the `reviewed` anchors themselves. RFC-068 solved the docs/code split with `[governs] root`, and this story's exclusivity rule makes that key illegal, so the split has to be structural. RFC-072 Decision 4 records it.

As the maintainer of a service repo, I want to point my repo at a shared doc set with one line of config, so that it participates in that set -- its types, its DAG, its documents -- without redeclaring any of it.

## Acceptance Criteria

- **Given** a `.lazyspec.toml` whose only key is `extends` pointing at a local directory
  **When** I run `config --json`
  **Then** the types, relationships and edges reported are those declared at the extended location, and none are declared locally.

- **Given** the same repo
  **When** I run `list <type>` and `show <ID>`
  **Then** the documents listed and shown are those under the extended root's `dir` for that type.

- **Given** a `.lazyspec.toml` containing only `extends`
  **When** it loads
  **Then** no "missing required `[[types]]`" or "`[[relationships]]` is required" error is raised: the `extends` branch short-circuits before `parse_inner` (`src/engine/config.rs:1817-1832`).

- **Given** a repo using `extends`
  **When** `governs`, staleness anchors, `@ref` expansion and `.lazyspec/cache/` resolve
  **Then** they resolve against the *local* repo root; only `[[types]].dir` and `[templates].dir` follow the extended root.

- **Given** a config declaring `extends` alongside one or more other keys
  **When** it loads
  **Then** it exits non-zero with an error listing every top-level key declared alongside `extends`, in file order.

- **Given** an extended config that itself declares `extends`
  **When** it loads
  **Then** it errors rather than following the chain.

- **Given** `extends` is a relative path
  **When** it resolves
  **Then** it resolves against the directory containing `.lazyspec.toml`, and `config --json` reports the resolved absolute path.

- **Given** `extends` is a clone URL
  **When** a command first runs
  **Then** the remote is cloned under the *local* repo's `.lazyspec/cache/config/` and the config at that clone is loaded; a subsequent run reads the existing clone.

- **Given** `extends` is a URL and the remote has moved ahead
  **When** I run `lazyspec fetch`
  **Then** the config clone is brought current and subsequent commands see the new types and documents.

- **Given** `extends` is a URL with no branch
  **When** it resolves
  **Then** the remote's default branch is used; a branch is selected by URL fragment (`<url>#<branch>`) and by no other key, since the exclusivity rule forbids a sibling `branch`.

- **Given** a repo using `extends`
  **When** I run `config --json`
  **Then** each type's `resolved_dir` is under the extended root.

- **Given** a repo using `extends`
  **When** I open the TUI or the web view
  **Then** documents load from the extended root, and an edit to the *extended* config triggers the reload that a local config edit does today (`src/tui/infra/event_loop.rs:493`, `src/web/watch.rs:105`).

- **Given** I run `lazyspec config add-type --help`
  **When** I read the `--store` line
  **Then** it lists every backend including `git` -- correcting `src/cli/config.rs:51`, which omits `github-milestones`, `github-projects` and `clickup-tasks` today -- and the README store table (README.md:675-682) gains a `git` row reading "Files in another repo's worktree, cloned under `.lazyspec/cache/`", distinct from the existing `git-ref` row.

## Scope

### In Scope

- `extends` on `RawConfig`, resolving a local dir or a URL.
- The exclusivity error and the no-recursion error.
- The doc-root / local-root split.
- `fetch` for the config clone.
- TUI and web reload parity; `--help` and README documentation of both git backends.

### Out of Scope

- Merge semantics between two configs. RFC-072 Decision 2: a config declaring `extends` declares nothing else.
- Per-repo `[governs] root` beside `extends`. The structural split above is the answer.

## Notes

`RawConfig` (`src/engine/config.rs:1428`) has no `deny_unknown_fields`, so unknown keys are dropped silently today. The exclusivity error is new machinery, not a tightening of an existing check.

Ordering inversion worth planning for: the clone must exist *before* the config parses, whereas the `git` store clones during `Store::load` with a `Config` already in hand. STORY-281's machinery does not literally serve this case -- there is no type -- which is why the config clone gets its own path under `.lazyspec/cache/config/`.

`main.rs` threads `&cwd` to 73 call sites, so "the root moves" is not a one-line change. The cheap shape is that `cwd` never moves: the resolved extended root rides on `Config` and only `doc_root`, the template loader and the watch set consult it.

Accepted surprise: someone reading the repo sees a one-line config and no `docs/`. That is the point of the feature, and it is still a surprise.
