---
title: Catch a pin that matches no file
type: story
status: accepted
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- implements: RFC-068
- blocks: STORY-271
---

As a document author, I want `validate` to warn when a `governs` glob matches nothing, so that a pin left behind by a refactor is visible instead of silently governing nothing.

## Acceptance criteria

- Given a document whose `governs` glob matches zero files under root, when I run `validate`, then a `governs-no-match` warning names the document and the glob.
- Given the same case, when I run `validate --json`, then the finding carries `rule: governs-no-match`, `path`, `glob` and `message`.
- Given a glob that matches at least one file, when I run `validate`, then no `governs-no-match` finding is emitted for it.
- Given a document with several globs where one matches nothing, when I run `validate`, then exactly one finding is emitted, for that glob.
- Given the TUI validation panel, when this finding exists, then it appears there with the same message.

## Notes

Warning severity, fixed. The `renamed` and `suggested_glob` fields are empty here; a later story fills them from git when `reviewed` is set.

The web has no validation surface, so the rendering AC covers the TUI alone. See STORY-266 Notes.
