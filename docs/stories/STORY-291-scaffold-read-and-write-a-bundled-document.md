---
title: Scaffold, read and write a bundled document
type: story
status: accepted
author: Jack Kaloger
date: 2026-09-28
tags: []
related:
- implements: RFC-074
- blocks: STORY-292
- blocks: STORY-293
reviewed: a6983d9cb6cbf9c5c2a647cfca0703ff0b38da8b
---

## Value

As a lazyspec user with a bundle-shaped workflow (OpenSpec, Kiro, Autobots), I run one `create` and get the whole folder of parts, read it back as one document, and edit a single part through the CLI. No four `create` calls per change, no parse error on a frontmatter-less part.

## Acceptance Criteria

- AC1: Template resolution checks `{type}/` in the templates dir before `{type}.md`. `{type}/index.md` is the parent template. Directory without `index.md` → config error. `subdirectory = false` on a type whose template is a directory → config error. `config --json` reports per-type `template: "file" | "directory"`.
- AC2: `create <type> <title>` on a directory template scaffolds every file in it: `.md` files with `{title}`, `{author}`, `{date}`, `{type}` substituted, everything else copied verbatim. `--json` returns the parent record plus `parts: [{name, path}]` and `sidecars: [path]`. TUI create path scaffolds the same directory.
- AC3: Loader classifies files inside a document folder: `index.md` → the document; `*.md` with frontmatter → child document (unchanged); `*.md` without frontmatter → part; anything else → sidecar. `DocMeta` gains `parts: Vec<Part { name, path }>` and `sidecars: Vec<PathBuf>`. A part has no id, status or relations; staleness, `governs`, `reviewed` apply from the parent.
- AC4: Frontmatter-less `.md` inside a document folder is no longer a parse error. A top-level frontmatter-less `.md` in a type's `dir` still is, reported once, not three times.
- AC5: Part order is template-directory order (sorted filename) for declared parts, then extra parts alphabetically. Extra parts are allowed and produce no finding.
- AC6: `show <id>` prints a `Parts:` block after `Children:`, one line per part and sidecar. `show <id> --parts` prints the `index.md` body then each part body under `## <name>` in part order; `-e` expands `@ref` in parts too. `show --json` always carries `parts` and `sidecars`; `--parts` adds `body` to each part entry. `list --json` and `context --json` carry `parts` and `sidecars`. TUI preview shows the concatenated form.
- AC7: `update <id> --part <name> --body|--body-file` writes that part, creating it if absent. `update --body` without `--part` writes `index.md` as today.
- AC8: `search` indexes part bodies under the parent id; a hit inside a part reports the parent document.
- AC9: README documents template directories, parts and sidecars beside `subdirectory` and `create --parent`.

## Out of scope

`missing-part` validation and `init --template` are sibling stories. Per-part status or relations (make it a child with `create --parent`). Multi-level nesting. BUG-034 child addressing. Packed `context` body (RFC-070). Migrating existing documents.
