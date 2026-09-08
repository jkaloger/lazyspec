---
title: Resolve every type's doc root to one path shape
type: story
status: complete
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: RFC-072
- blocks: STORY-281
- blocks: STORY-284
reviewed: ead6302e2ac8d5f3cfc0277ede6907ec26112f68
---

## Context

Lazyspec resolves a type's doc root in one place (`src/engine/store.rs:128-133`), but what comes back out of `list --json` is not one shape. Verified against HEAD: an absolute out-of-project `dir` reports document paths as absolute, a relative one that escapes the root (`../shared-specs`) reports them with the `..` intact, and everything under the root reports relative -- because `src/engine/store/loader.rs:75` falls back to absolute only when `strip_prefix` fails. A missing external directory is worse: `store.rs:136` skips a non-existent doc root and moves on, so a typo in an absolute `dir` is indistinguishable from a shared spec repo with no documents in it.

None of this needs the `git` store. Six backends exist today and the inconsistency is live in all of them, which is why RFC-072's resolution contract came out of its `git` story and became the first slice.

As someone wiring lazyspec into a tool outside the process -- a hook, an editor plugin, a CI check -- I want `config --json` to report each type's resolved absolute doc root, so that I can decide whether a given file is a lazyspec document without reimplementing lazyspec's path resolution.

## Acceptance Criteria

- **Given** a `filesystem` type with a relative `dir`
  **When** I run `config --json`
  **Then** the type carries a `resolved_dir` absolute path alongside the raw `dir`, equal to the project root joined with `dir`.

- **Given** a `filesystem` type whose `dir` is absolute and outside the project root
  **When** I run `config --json`
  **Then** `resolved_dir` is that directory, normalised.

- **Given** a `filesystem` type whose `dir` is relative but escapes the project root (`../shared-specs`)
  **When** I run `list <type> --json`
  **Then** the reported document paths are normalised absolute paths, matching the shape returned for an absolute `dir` -- one path shape per type, whatever the config spelling.

- **Given** a type on a cache-backed store (`github-issues`, `github-milestones`, `github-projects`, `git-ref`, `clickup-tasks`)
  **When** I run `config --json`
  **Then** `resolved_dir` is `<root>/.lazyspec/cache/<type name>` and the raw `dir` is not used, matching `store.rs:128-132`.

- **Given** a document path from `list --json` or `show --json`
  **When** it is resolved against the project root
  **Then** it is under its type's `resolved_dir`; a path already absolute is under it directly.

- **Given** a `filesystem` type whose `dir` is absolute and does not exist
  **When** a command reads that type
  **Then** it warns naming the resolved absolute path, instead of reporting zero documents indistinguishably from an empty directory.

- **Given** a type whose `dir` is relative and does not exist
  **When** a command reads that type
  **Then** it is skipped silently as it is today -- an unpopulated local docs dir between `init` and the first `create` is not an error.

- **Given** a child type declaring an external-`dir` type as its `parent_type`
  **When** I run `validate`
  **Then** no parent-type violation is raised: the rule at `src/engine/validation.rs:1367`, which string-compares `doc.path` against `parent_type_def.dir`, compares like-for-like path shapes.

## Scope

### In Scope

- `resolved_dir` on `config --json`, for all six existing backends.
- Normalising `list`/`show` document paths to one shape per type.
- The missing-external-directory warning, and preserving today's silent skip for relative dirs.
- The `validation.rs:1367` parent-type comparison.

### Out of Scope

- `StoreBackend::Git` and its clone -- STORY-281 adds the `git` arm to `resolved_dir` as its own criterion.
- `extends` -- STORY-284.
- Any edit guard. RFC-072's Risks section records that no such guard exists; this field is what one would need, not a fix to one.

## Notes

Absorbs STORY-280, which was deleted. Review proved that story's criteria already passed on an unmodified binary -- external dirs list, show, validate, create and link today -- so what survived was the path-shape bug and the missing-directory warning, and both belong here with the resolution contract.

`src/cli/config.rs:207` `run_show_json(config: &Config)` is a pure serialisation and receives no project root. Adding `resolved_dir` means threading the root through it, which changes `config --json` from "dump the config" to "resolve against the project". Cheap, but a category change worth knowing before starting.
