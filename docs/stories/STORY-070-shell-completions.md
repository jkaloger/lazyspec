---
title: Shell Completions
type: story
status: draft
author: jkaloger
date: 2026-03-20
tags: []
related:
- implements: docs/rfcs/RFC-028-document-reference-ergonomics.md
---


## Context

Users working with lazyspec in the terminal have no tab-completion support. This means typing full subcommand names, flags, and document paths by hand. Adding shell completions via `clap_complete` would reduce friction, especially for document references where shorthand IDs like `RFC-028` or `STORY-070` could be completed dynamically from the current store.

## Acceptance Criteria

- **Given** a user runs `lazyspec completions zsh`
  **When** the command completes
  **Then** a valid zsh completion script is written to stdout

- **Given** a user runs `lazyspec completions bash`
  **When** the command completes
  **Then** a valid bash completion script is written to stdout

- **Given** a user runs `lazyspec completions fish`
  **When** the command completes
  **Then** a valid fish completion script is written to stdout

- **Given** a user runs `lazyspec completions` with an unsupported shell name
  **When** the command completes
  **Then** a clear error message is returned

- **Given** a user has sourced the generated completion script
  **When** they type `lazyspec ` and press tab
  **Then** all subcommands (create, show, list, link, validate, delete, context, completions, etc.) are suggested

- **Given** a user has sourced the generated completion script
  **When** they type `lazyspec show --` and press tab
  **Then** available flags for the `show` subcommand are suggested

- **Given** a user has sourced the generated completion script and documents exist in the store
  **When** they type `lazyspec show ` and press tab on an argument that accepts a document reference
  **Then** shorthand document IDs (e.g. `RFC-028`, `STORY-070`) are listed as completion candidates

- **Given** a user has sourced the generated completion script
  **When** they type `lazyspec link <from> ` and press tab on the relationship type argument
  **Then** valid relationship types (`implements`, `supersedes`, `blocks`, `related-to`) are suggested

- **Given** documents exist in the store and the user creates a new document
  **When** they trigger tab completion for a document reference argument
  **Then** the newly created document's shorthand ID appears without regenerating the completion script

- **Given** the document store is unreadable or corrupted
  **When** the user triggers dynamic document ID completion
  **Then** completions degrade gracefully (static completions still work, dynamic completions return empty rather than erroring)

## Scope

### In Scope

- A `completions` subcommand that accepts a shell name (zsh, bash, fish) and outputs the corresponding completion script
- Static completions for all subcommands and flags, derived from clap's existing CLI definition
- Dynamic completions for document shorthand IDs on arguments that accept document references (`show`, `link`, `context`, `delete`, etc.)
- Dynamic completions for relationship types on the `link` command's `rel_type` argument
- Use of `clap_complete` 4.x with `CompleteEnv` for live dynamic completions

### Out of Scope

- ID resolution changes in other commands (covered by Story 1: Universal ID Resolution)
- TUI changes (covered by Story 3: TUI Link Editor)
- Changes to document storage format
- Installation or shell configuration automation (users source the script themselves)
