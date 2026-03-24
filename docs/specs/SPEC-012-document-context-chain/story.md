---
title: "Document Context Chain"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: []
related: []
---

## Acceptance Criteria

### AC: backward-chain-walks-implements

Given a document with an `implements` relation pointing to a parent, which itself implements a grandparent
When `resolve_chain` is called with the leaf document's shorthand ID
Then the returned `chain` vector contains `[grandparent, parent, leaf]` in order from root to target

### AC: chain-terminates-at-root

Given a document with no `implements` relation in its `related` field
When `resolve_chain` is called with that document's ID
Then the `chain` vector contains only the single document and `target_index` is 0

### AC: target-index-marks-requested-document

Given a chain of three documents where the middle document was requested
When `resolve_chain` returns
Then `target_index` equals 1, corresponding to the position of the requested document in the chain

### AC: forward-context-collects-implementors

Given a document that is implemented by two other documents via `implements` relations
When `resolve_chain` is called with that document's ID
Then the `forward` vector contains both implementing documents

### AC: forward-context-empty-when-none

Given a document that no other document implements
When `resolve_chain` is called
Then the `forward` vector is empty

### AC: related-records-from-all-chain-members

Given the root document has a `related-to` link to document A and the leaf has a `related-to` link to document B
When `resolve_chain` is called on the leaf
Then the `related` vector contains both A and B

### AC: related-records-deduplicated

Given two chain members both have `related-to` links pointing to the same document
When `resolve_chain` collects related records
Then that document appears exactly once in the `related` vector

### AC: related-excludes-chain-members

Given a chain member has a `related-to` link pointing to another document that is already in the chain
When related records are collected
Then the linked document is not included in the `related` vector

### AC: related-includes-reverse-links

Given an external document has a `related-to` link pointing at a chain member (but the chain member does not link back)
When `resolve_chain` collects related records
Then the external document appears in the `related` vector via the reverse link

### AC: not-found-error

Given an ID that does not match any document in the store
When `resolve_chain` is called
Then it returns an error with the message `"document not found: {id}"`

### AC: ambiguous-id-error

Given a shorthand ID that matches multiple documents
When `resolve_chain` is called
Then it returns an error listing all matching document paths and advising the user to specify the full path

### AC: human-output-mini-cards

Given a chain of two documents
When `run_human` renders the output
Then each document appears as a bordered card showing title, uppercased shorthand ID, lowercase doc type, and status in brackets, with a vertical connector between cards

### AC: human-output-you-are-here-marker

Given a chain where the target document is not the root
When `run_human` renders the output
Then only the target document's mini-card displays the `<- you are here` marker

### AC: json-output-structure

Given any document
When `run_json` is called
Then the output is a JSON object with a `chain` array and a `related` array, where each element contains the document's frontmatter fields plus children and parent information

### AC: human-output-related-section-omitted-when-empty

Given no documents in the chain have `related-to` links
When `run_human` renders the output
Then no `--- related ---` separator or related entries appear in the output
