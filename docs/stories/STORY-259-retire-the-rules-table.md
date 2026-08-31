---
title: Retire the rules table
type: story
status: accepted
author: Jack Kaloger
date: 2026-08-29
tags: []
related:
- implements: RFC-067
---
As a DAG designer, I want exactly one place to read the document DAG, so that a config cannot declare it twice and disagree with itself.

STORY-254 deliberately left `[[rules]]` working so the edge table could land incrementally. This closes that window.

## Acceptance criteria

- Given a config containing `[[rules]]`, when it loads, then load fails naming `fix --config` as the remedy, matching the ADR-011 strict-load pattern.
- Given a config containing only `[[edges]]`, when it loads, then it loads cleanly.
- Given the codebase after this story, when searched, then `ValidationRule` and both its variants are gone rather than deprecated.
- Given `init`, when it scaffolds a new project, then it writes `[[edges]]` and no `[[rules]]`.
- Given the README and JSON schema, when this story lands, then neither documents `[[rules]]` as current.

## Notes

Depends on STORY-258: users need a working escape route before the old shape becomes a hard error.

Deletion, not deprecation. Per RFC-042's precedent (ADR-011), lazyspec does not carry silent fallbacks in the load path.
