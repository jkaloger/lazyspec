---
title: "CLI Framework and Output"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: [cli, json, styling, completions]
related: []
---

## Acceptance Criteria

### AC: init-creates-project

Given a directory without a `.lazyspec.toml` file
When `lazyspec init` is run
Then a default config file is created, all document-type directories exist, and the template directory exists

### AC: init-rejects-existing

Given a directory that already contains `.lazyspec.toml`
When `lazyspec init` is run
Then the command fails with an error and does not overwrite the existing config

### AC: json-document-schema

Given any command that outputs document information with `--json`
When the output is parsed
Then each document object contains: path, title, type, status, author, date, tags, related, and validate_ignore

### AC: json-family-fields

Given a document with parent and child relationships in the store
When `doc_to_json_with_family` serializes the document
Then the output includes `parent` (with path and title) and `children` (array of path/title objects) fields

### AC: styled-status-colors

Given a terminal that supports ANSI colors
When any human-readable command displays a document status
Then Accepted renders green, Draft renders yellow, Review renders blue, Rejected renders red, and Superseded renders gray

### AC: style-fallback-plain-text

Given a terminal that does not support ANSI colors (piped output or dumb terminal)
When any human-readable command runs
Then output contains no ANSI escape sequences and uses plain-text prefixes for errors and warnings

### AC: resolve-shorthand-id

Given a shorthand ID like `RFC-001` that matches exactly one document in the store
When any command that accepts a document reference is run with that shorthand
Then the command operates on the resolved document, identical to passing the full relative path

### AC: resolve-ambiguous-id

Given a shorthand ID prefix that matches multiple documents
When any command attempts resolution
Then the command fails with an error listing all ambiguous matches

### AC: resolve-not-found-id

Given a shorthand ID that matches no documents in the store
When any command attempts resolution
Then the command fails with a "document not found" error message

### AC: delete-removes-file

Given a valid document path or shorthand ID
When `lazyspec delete <id>` is run
Then the resolved file is removed from disk

### AC: update-modifies-frontmatter

Given a document and a field update (e.g. `--status accepted`)
When `lazyspec update <id> --status accepted` is run
Then the frontmatter field is updated in-place and the body content is preserved unchanged

### AC: link-adds-relationship

Given two documents and a relationship type
When `lazyspec link <from> <rel_type> <to>` is run
Then a new entry is appended to the source document's `related` YAML sequence with the resolved canonical path

### AC: unlink-removes-relationship

Given an existing relationship between two documents
When `lazyspec unlink <from> <rel_type> <to>` is run
Then the matching entry is removed from the source document's `related` sequence

### AC: completions-generates-script

Given a supported shell name (bash, zsh, fish)
When `lazyspec completions <shell>` is run
Then a valid completion script for that shell is written to stdout

### AC: completions-dynamic-doc-ids

Given a sourced completion script and documents in the store
When the user triggers tab completion on a document reference argument
Then shorthand document IDs from the current store are offered as candidates, and if the store fails to load, completion returns empty rather than erroring
