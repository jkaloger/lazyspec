---
title: Document the git backend and extends
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-09
tags: []
related:
- implements: STORY-284
reviewed: 4fe4f87707975ac0780871beb31ecbc3a3973b4e
---

## Objective

`--help` and the README name every store backend including `git`, distinguish `git` from `git-ref`, and document `extends`.

## Satisfies

STORY-284 AC13.

## Context

- Story + ACs: STORY-284. Contract: RFC-072 Risks ("two git backends will be confused ... mitigated by documenting the split in `--help` and the README"). STORY-281 Out of Scope deferred this documentation here so it lands once.
- **`--store` help** (`src/cli/config.rs:51`): `Storage backend: filesystem, github-issues, or git-ref` -> list all seven as `parse_store` accepts them (`:1199-1209`): `filesystem, github-issues, github-milestones, github-projects, git-ref, git, or clickup-tasks`. `fetch`'s subcommand doc (`src/cli.rs:342`) omits `git` and `github-milestones`; correct it in the same pass.
- **README store table** (`README.md:675-682`): a `git` row after `git-ref`, Documents column "Files in another repo's worktree, cloned under `.lazyspec/cache/`", Auth "a readable git remote (writable to push)". The paragraph at `:686` ("Remote-backed types cache into ... `git-ref` mutations push live") gains one sentence: a `git` type commits and pushes each write to its `remote`, a rejected push exits non-zero and names `lazyspec fetch` (STORY-282, Decision 5). Config keys `remote`/`branch` are in `config schema` already; do not duplicate them.
- **`extends`**: a new `### Sharing a doc set` subsection after `### Store backends` (before `### Custom types`, `:704`). Cover, briefly: the one-line file, dir vs URL, `#branch`, exclusivity (any other key is an error naming it), no chains, what follows the extended root (`[[types]].dir`, `[templates].dir`) versus what stays local (`governs`, `reviewed`/staleness, `@ref`, `.lazyspec/cache/`), the clone at `.lazyspec/cache/config/`, `fetch` refreshing it, and that config mutators refuse under `extends`. Point `config --json` readers at `.extends`. Also one line in `### Inspecting and editing the config` (`:617-620`) beside the `resolved_dir` sentence.
- Style: the `writing-reference-docs` skill governs README prose; match the table's existing register.
- Test shape: `tests/integration/cli_no_config_test.rs:11` drives the binary; a `--help` assertion is one `Command` run.

## Tasks

1. Test-first, a test in `tests/integration/cli_config_schema_test.rs` or a sibling: `config add-type --help` stdout contains `git,` and `clickup-tasks`; `fetch --help` mentions `git`.
2. The two doc-comment edits. Green.
3. README: table row, the fetch paragraph sentence, the `extends` subsection, the config-section line. Read the rendered sections back once for a broken table.
4. `clippy -D warnings`; `cargo run -q -- config add-type --help` by eye.

## Out of scope

- `lazyspec config schema` wording for `remote`/`branch`/`extends`; the `RawConfig`/`TypeDef` doc comments already feed it (ITERATION-441 added `extends`).
- A `--help` line for `extends` on any subcommand; it is a config key, not a flag.
- Renaming either backend. RFC-072 Risks: documentation, not a longer name.

## Principles/conventions

`cargo run -q -- convention`. Principle 2: the README and `--help` are interfaces agents read; keep the lists identical to `parse_store`. Project CLAUDE.md: CLI change -> README change. `writing-reference-docs` for prose.

## Verification

`cargo run -q -- config add-type --help | grep -- --store -A1` lists seven backends; `grep -n '| \`git\`' README.md` shows the row; `grep -c extends README.md` is non-zero.
