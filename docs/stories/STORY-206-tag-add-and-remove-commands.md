---
title: Tag add and remove commands
type: story
status: accepted
author: jkaloger
date: 2026-04-13
tags: []
related:
- implements: RFC-037
---



## Context

RFC-037 specifies bidirectional tag/label sync: "tags added via lazyspec are pushed as labels" and "Labels are created automatically if they don't exist on the repo." The `update` command handles title, status, and body, but there is no CLI command to add or remove tags from a document. Tags can only be changed by hand-editing YAML frontmatter (filesystem) or manually applying labels on GitHub (github-issues). A `tag` command with `add` and `remove` subcommands closes this gap across both backends.

## Acceptance Criteria

### AC: Add tag to filesystem document

**Given** a filesystem document `RFC-001` with `tags: [architecture]`
**When** the user runs `lazyspec tag add RFC-001 security`
**Then** the document's frontmatter `tags` field contains `[architecture, security]` and the command outputs the updated document metadata

### AC: Add multiple tags at once

**Given** a filesystem document with `tags: []`
**When** the user runs `lazyspec tag add RFC-001 auth refactor cleanup`
**Then** all three tags are present in the document's `tags` field

### AC: Add tag is idempotent

**Given** a document with `tags: [auth]`
**When** the user runs `lazyspec tag add <ID> auth`
**Then** the tags field remains `[auth]` (no duplicate) and the command succeeds without error

### AC: Remove tag from filesystem document

**Given** a filesystem document with `tags: [auth, refactor]`
**When** the user runs `lazyspec tag remove <ID> auth`
**Then** the tags field becomes `[refactor]`

### AC: Remove nonexistent tag is a no-op

**Given** a document with `tags: [auth]`
**When** the user runs `lazyspec tag remove <ID> cleanup`
**Then** the command succeeds without error and tags remain `[auth]`

### AC: Add tag to github-issues document creates label if needed

**Given** a github-issues document `ITERATION-042`
**When** the user runs `lazyspec tag add ITERATION-042 security`
**Then** the label `security` is created on the repo if it does not exist, and the label is applied to the issue

### AC: Remove tag from github-issues document

**Given** a github-issues document with labels `[security, auth, lazyspec:iteration]`
**When** the user runs `lazyspec tag remove ITERATION-042 security`
**Then** the `security` label is removed from the issue, and the `lazyspec:iteration` label is unaffected

### AC: JSON output

**Given** any document
**When** the user runs `lazyspec tag add <ID> foo --json`
**Then** the output is a JSON object containing the updated document metadata including the new tags list

### AC: Tag command updates local cache for github-issues

**Given** a github-issues document
**When** the user runs `lazyspec tag add <ID> foo`
**Then** the local cache file for that document reflects the updated tags

## Scope

### In Scope

- `lazyspec tag add <DOC_ID> <TAG>...` subcommand
- `lazyspec tag remove <DOC_ID> <TAG>...` subcommand
- Filesystem backend: YAML `tags:` field rewriting
- GitHub-issues backend: label add/remove via `gh` CLI, auto-creation of missing labels
- `--json` flag on both subcommands
- Local cache update after github-issues tag mutations

### Out of Scope

- `tag list` subcommand (use `lazyspec show` instead)
- TUI tag editing (existing keybindings cover this)
- Tag rename or bulk tag operations
- Git-ref backend tag support (deferred until needed)
- Tag validation or controlled vocabulary enforcement
