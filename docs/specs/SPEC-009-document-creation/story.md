---
title: "Document Creation"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: []
related: []
---

## Acceptance Criteria

### AC: type-lookup-valid

Given a config with type definitions for "rfc", "story", and "iteration"
When the user runs `lazyspec create rfc "My Title"`
Then the document is created in the directory specified by the "rfc" type definition

### AC: type-lookup-invalid

Given a config with type definitions for "rfc" and "story"
When the user runs `lazyspec create bogus "My Title"`
Then the command fails with an error listing "rfc" and "story" as valid types

### AC: incremental-numbering

Given a type configured with `numbering = "incremental"` and the target directory contains files up to RFC-003
When the user runs `lazyspec create rfc "Next Doc"`
Then the created file is numbered RFC-004

### AC: sqids-numbering

Given a type configured with `numbering = "sqids"` and a valid `[numbering.sqids]` section with salt and min_length
When the user runs `lazyspec create rfc "My Feature"`
Then the filename contains a lowercase sqids-encoded ID instead of a zero-padded integer

### AC: create-sqids-collision-retry

Given the target directory already contains a file whose prefix matches the first sqids candidate
When `next_sqids_id` generates an ID
Then the input is incremented until a non-colliding ID is produced

### AC: reserved-numbering-success

Given a type configured with `numbering = "reserved"` and a reachable remote
When the user runs `lazyspec create`
Then the command queries remote reservation refs, creates a local ref, atomically pushes it, and uses the reserved number in the filename

### AC: reserved-push-rejected-retry

Given the atomic push is rejected because another reservation claimed the same number
When the push fails
Then the local ref is cleaned up, the candidate is incremented, and the push is retried up to `max_retries` times

### AC: reserved-exhausted-retries

Given all push attempts are rejected
When the retry loop exhausts `max_retries`
Then the command fails with an error message stating the prefix and attempted number range, and no file is written

### AC: create-reserved-remote-unreachable

Given the remote is unreachable (offline, auth failure, DNS failure)
When `lazyspec create` runs with `numbering = "reserved"`
Then it fails immediately with an error suggesting `--numbering incremental` or `--numbering sqids` as overrides

### AC: template-from-file

Given a template file exists at `{templates_dir}/rfc.md`
When the user creates an rfc document
Then the template file content is used, with `{title}`, `{author}`, `{date}`, and `{type}` variables substituted

### AC: template-default-fallback

Given no template file exists for the requested type
When the user creates a document
Then the built-in default template for that type is used (story, iteration, spec each have distinct defaults; unknown types use a generic default)

### AC: subdirectory-mode

Given a type definition with `subdirectory = true`
When the user runs `lazyspec create`
Then a directory is created containing `index.md` (from the standard template) and `story.md` (with an Acceptance Criteria skeleton), and the returned path points to `index.md`

### AC: json-output

Given the user passes `--json` to the create command
When the document is created successfully
Then the output is pretty-printed JSON containing the document's frontmatter fields and its path relative to the repo root

### AC: reservations-list

Given reservation refs exist on the remote under `refs/reservations/*`
When the user runs `lazyspec reservations list`
Then each reservation is displayed with its prefix, number, and ref path (tab-separated in human mode, structured JSON with `--json`)

### AC: reservations-prune

Given reservation refs exist on the remote, some with matching local documents and some without
When the user runs `lazyspec reservations prune`
Then refs with matching local documents are deleted from the remote, refs without matches are reported as orphans, and `--dry-run` previews actions without deleting
