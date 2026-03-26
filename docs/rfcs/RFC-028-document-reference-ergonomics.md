---
title: "Document Reference Ergonomics"
type: rfc
status: accepted
author: "jkaloger"
date: 2026-03-20
tags:
  - cli
  - tui
  - ergonomics
related:
  - related to: docs/rfcs/RFC-018-tui-interaction-enhancements.md
  - related to: docs/rfcs/RFC-027-sqids-document-numbering.md
---


## Problem

Referencing documents in lazyspec requires too much friction. Three pain points:

1. The `link` command requires full relative paths for both arguments, while `show` and `context` already accept shorthand IDs like `RFC-001`. Running `lazyspec link docs/rfcs/RFC-027-sqids-document-numbering.md related-to docs/stories/STORY-064-sqids-numbering-and-config.md` is tedious when `lazyspec link RFC-027 related-to STORY-064` should work.

2. Shell users get no tab completion. Document IDs are short but still need to be remembered or looked up. Tab completion for document IDs on commands like `show`, `link`, and `context` would eliminate that lookup step.

3. The TUI displays relationships in a read-only Relations panel but provides no way to add or remove links on existing documents. The only option is pressing `e` to open `$EDITOR` and manually editing frontmatter YAML.

## Intent

Make document references effortless across the CLI and TUI. A user should be able to reference any document by its shorthand ID in any command, get tab completion for those IDs in their shell, and manage links visually in the TUI.

## Design

### Universal ID Resolution

`Store::resolve_shorthand` already handles prefix-based ID matching:

@ref src/engine/store.rs#resolve_shorthand

The `show` command wraps this with a path-first fallback:

@ref src/cli/show.rs#resolve_shorthand_or_path

Commands that currently accept only paths (`link`, `unlink`, `delete`, `update`, `ignore`, `unignore`) should use the same resolution. The `resolve_shorthand_or_path` pattern (try exact path first, fall back to shorthand) is the right default for all of them.

For `link` specifically, there's a subtlety: the `to` value is stored as a raw string in frontmatter. Today it's a relative path. After this change, resolution happens at write time: the user types `RFC-027`, the CLI resolves it to the canonical path, and that path is what gets written to frontmatter. This preserves the existing path-based storage format and avoids changing how `store.get` works at read time.

### Shell Completions via clap_complete

Add a `completions` subcommand that generates shell completion scripts:

```
lazyspec completions zsh > _lazyspec
lazyspec completions bash > lazyspec.bash
lazyspec completions fish > lazyspec.fish
```

Static completions (subcommands, flags, relationship types) come from clap_complete's derive integration. Dynamic completions for document IDs require a custom completer that:

1. Runs `lazyspec list --json` (or reads the store directly)
2. Extracts shorthand IDs (e.g. `RFC-001`, `STORY-064`)
3. Returns them as completion candidates for arguments that accept document references

clap_complete 4.x supports custom completers via `CompleteEnv`. The completer registers which arguments accept document IDs and provides candidates at completion time. This means completions are live: as new documents are created, they appear in tab completion without regenerating scripts.

### TUI Link Management

Add a link editor overlay, triggered by `r` (for "relations") when viewing a document's detail. The interaction follows the same pattern as RFC-018's tag editor:

1. `r` opens a modal overlay on the Relations panel
2. The overlay has two fields: relationship type (cycle with `Tab`) and target document (fuzzy search)
3. Typing in the target field filters the document list in real time
4. `Enter` adds the link, `Esc` cancels
5. Existing links are shown with `d` to delete

The fuzzy search uses the same shorthand ID format as the CLI. Documents are displayed as `TYPE-NNN: Title` so users can match on either the ID or the title.

Link removal is also exposed: when the Relations panel is focused (existing behaviour), pressing `d` on a selected relationship prompts for confirmation and removes it.

## Stories

### Story 1: Universal ID Resolution

Extend `resolve_shorthand_or_path` to all path-accepting CLI commands. The `link` command resolves shorthand IDs to canonical paths at write time. This is a prerequisite for Stories 2 and 3.

### Story 2: Shell Completions

Add a `completions` subcommand using `clap_complete` with static completions for subcommands/flags and dynamic completions for document IDs. Supports zsh, bash, and fish.

### Story 3: TUI Link Editor

Add an `r`-triggered link editor overlay in the TUI with fuzzy document search, relationship type selection, and link removal via `d`. Follows RFC-018's overlay interaction pattern.
