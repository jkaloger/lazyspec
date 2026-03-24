---
title: Create spec directory scaffolding
type: iteration
status: accepted
author: agent
date: 2026-03-24
tags: []
related:
- implements: docs/stories/STORY-091-story-to-spec-migration-and-create-scaffolding.md
---



## Context

STORY-091 AC `create-spec-scaffolds-directory` requires `lazyspec create spec` to produce a directory structure (`docs/specs/SPEC-NNN-slug/index.md` + `story.md`) rather than a single flat file. The loader already handles subdirectory-based documents with `index.md`, so no loader changes are needed. The change is entirely in the create command and its config.

## ACs Addressed

- `create-spec-scaffolds-directory`

## Changes

### Task 1: Add `subdirectory` flag to TypeDef

ACs addressed: `create-spec-scaffolds-directory`

Files:
- Modify: `src/engine/config.rs`
- Modify: `.lazyspec.toml`

What to implement:

Add `#[serde(default)] pub subdirectory: bool` to the `TypeDef` struct (after the `numbering` field). Set `subdirectory: true` for the `spec` type in both `default_types()` and `.lazyspec.toml`. All other types default to `false`.

How to verify:
- `cargo test` passes (no breaking changes to existing config parsing)
- `lazyspec status --json` still loads all document types correctly

### Task 2: Modify create command for directory scaffolding

ACs addressed: `create-spec-scaffolds-directory`

Files:
- Modify: `src/cli/create.rs`

What to implement:

In `run()`, after resolving the filename, check `type_def.subdirectory`. When true:

1. Strip `.md` from the resolved filename to use as a directory name
2. Create the directory at `target_dir.join(dir_name)`
3. Write `index.md` inside it using the existing template logic (the template should have `type: spec` frontmatter)
4. Write `story.md` inside it with a spec story template containing `type: spec` frontmatter and an empty `### AC:` template
5. Return the path to `index.md` (this is what the loader expects as the canonical path)

Add a `"spec"` branch to `default_template()` for `index.md` content, and a new `default_story_template()` function (or a second match arm) for the `story.md` content. The story template should include:

```markdown
---
title: "{title}"
type: spec
status: draft
author: "{author}"
date: {date}
tags: []
related: []
---

## Acceptance Criteria

### AC: example-criterion

Given a precondition
When an action is taken
Then an expected outcome occurs
```

How to verify:
- `lazyspec create spec "Test Spec" --author agent` creates `docs/specs/SPEC-NNN-test-spec/index.md` and `docs/specs/SPEC-NNN-test-spec/story.md`
- Both files have `type: spec` in frontmatter
- `story.md` has the `### AC:` template
- `lazyspec show` and `lazyspec list` correctly pick up the new spec
- `lazyspec validate --json` passes

## Test Plan

### test: create spec produces directory with index.md and story.md
Invoke `create::run()` with `doc_type = "spec"` using a `TestFixture` that has the spec type configured with `subdirectory: true`. Assert that the returned path ends in `index.md`, that both `index.md` and `story.md` exist in the same directory, and that the directory name follows `SPEC-NNN-slug` format. Behavioural, isolated, fast.

### test: created index.md has correct frontmatter
Parse the `index.md` produced by `create::run()` with `DocMeta::parse()`. Assert `type: spec`, `status: draft`, correct title and author. Behavioural, isolated, fast.

### test: created story.md has correct frontmatter and AC template
Parse the `story.md` produced by `create::run()`. Assert `type: spec` in frontmatter and that the body contains `### AC:`. Behavioural, isolated, fast.

### test: created spec loads correctly in store
Create a spec via `create::run()`, then load a `Store` from the same root. Assert the spec appears in `store.all_docs()` with the correct type, and that `story.md` appears as a child. Integration-level (trades Fast for Predictive), isolated.

### test: non-subdirectory types still produce flat files
Invoke `create::run()` with `doc_type = "rfc"` (which has `subdirectory: false`). Assert the result is a flat `.md` file, not a directory. Regression guard, behavioural, fast.

## Notes

The naming pattern `{type}-{n:03}-{title}.md` is shared across all types. For subdirectory types, the `.md` suffix is stripped to form the directory name. This avoids needing a separate naming pattern per type.
