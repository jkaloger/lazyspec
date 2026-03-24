---
title: "Project Status and Validation"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: []
related: []
---

## Acceptance Criteria

### AC: status-human-groups-by-type

Given a project with documents of types RFC, Story, Iteration, and ADR
When `lazyspec status` is run without `--json`
Then documents are displayed grouped by type in the order RFC, Story, Iteration, ADR, each group preceded by a type header, and documents within each group sorted by date ascending

### AC: status-human-empty-project

Given an initialised project with no documents
When `lazyspec status` is run without `--json`
Then the output is "No documents found." and the exit code is 0

### AC: status-json-documents-array

Given a project with documents
When `lazyspec status --json` is run
Then the output is a JSON object containing a `documents` array where each element includes frontmatter fields serialised by `doc_to_json`

### AC: status-json-inline-validation

Given a project with validation errors and warnings
When `lazyspec status --json` is run
Then the output includes a `validation` object with separate `errors` and `warnings` arrays, without requiring a separate `validate` call

### AC: status-json-parse-errors

Given a project containing a markdown file with invalid frontmatter
When `lazyspec status --json` is run
Then the `parse_errors` array contains an entry with `path` and `error` fields for the broken file

### AC: validate-exit-code-errors

Given a project with validation errors or parse errors
When `lazyspec validate` is run
Then the exit code is 2

### AC: validate-exit-code-clean

Given a project with no validation errors and no parse errors (warnings may exist)
When `lazyspec validate` is run
Then the exit code is 0

### AC: validate-human-success-message

Given a project with no validation issues
When `lazyspec validate` is run in human mode
Then the output is "All documents valid." printed to stdout

### AC: validate-human-error-output

Given a project with parse errors and validation errors
When `lazyspec validate` is run in human mode
Then parse errors are listed first, followed by validation errors, each prefixed with an error marker, and output goes to stderr

### AC: validate-warnings-flag

Given a project with both errors and warnings
When `lazyspec validate` is run without `--warnings`
Then only errors and parse errors are displayed
When `lazyspec validate --warnings` is run
Then warnings are also displayed, each prefixed with a warning marker

### AC: validate-json-output-structure

Given a project with validation results
When `lazyspec validate --json` is run
Then the output is a JSON object with `errors`, `warnings`, and `parse_errors` arrays, regardless of the `--warnings` flag

### AC: validate-json-ignores-warnings-flag

Given a project with warnings
When `lazyspec validate --json` is run without `--warnings`
Then the `warnings` array is still present and populated, since JSON mode always includes both severity levels

### AC: validate-warnings-no-exit-code

Given a project with warnings but no errors or parse errors
When `lazyspec validate` is run
Then the exit code is 0, because warnings alone do not constitute failure
