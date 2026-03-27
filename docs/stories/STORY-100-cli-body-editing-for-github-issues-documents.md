---
title: CLI body editing for github-issues documents
type: story
status: accepted
author: jkaloger
date: 2026-03-27
tags: []
related:
- implements: RFC-037
---



## Context

RFC-037 defines `lazyspec update` as capable of updating issue body and labels, and the TUI `e` flow for `$EDITOR`-based editing. STORY-095 implemented the store dispatch for `update`, but the CLI only exposes `--status` and `--title` flags. There is no way to update document body content from the CLI without the TUI. This blocks agent workflows and scripted pipelines that need to write document content to github-issues backed types.

## Acceptance Criteria

### AC: Update body via --body flag

- Given a github-issues document that exists in the issue map
  When `lazyspec update <id> --body "new content"` is executed
  Then the issue body on GitHub is updated with the new content (preserving the HTML comment frontmatter block), and the issue map timestamp is refreshed

### AC: Update body via --body-file flag

- Given a github-issues document that exists in the issue map
  When `lazyspec update <id> --body-file path/to/file.md` is executed
  Then the issue body is updated with the file contents, and `--body-file -` reads from stdin

### AC: Body update respects optimistic lock

- Given a github-issues document whose remote `updated_at` differs from the local issue map
  When `lazyspec update <id> --body "content"` is executed
  Then the update is rejected with a conflict error, same as existing status/title updates

### AC: Body update works with other flags

- Given a github-issues document
  When `lazyspec update <id> --body "content" --status accepted` is executed
  Then both the body and status are updated in a single operation

### AC: Filesystem documents ignore body flags

- Given a filesystem-backed document
  When `lazyspec update <id> --body "content"` is executed
  Then an error is returned indicating body editing via flags is not supported for filesystem documents (use your editor directly)

## Scope

### In Scope

- `--body` and `--body-file` flags on `lazyspec update`
- Routing body updates through the github-issues store dispatch
- Optimistic lock enforcement for body updates
- Combining body updates with existing frontmatter updates
- Error messaging for unsupported backends

### Out of Scope

- TUI editor integration (covered by STORY-098)
- `$EDITOR` launch from CLI (could be a separate story)
- Body editing for git-ref backend documents
- Fetch or cache refresh logic
