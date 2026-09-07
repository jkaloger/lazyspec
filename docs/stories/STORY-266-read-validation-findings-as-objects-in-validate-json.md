---
title: Read validation findings as objects in validate --json
type: story
status: draft
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- implements: RFC-068
- related-to: STORY-264
- blocks: STORY-267
- blocks: STORY-268
---

As an agent consuming `validate --json`, I want every finding to be an object with a `rule` slug and a `message`, so that I can select findings by rule and read repair data as fields instead of parsing a sentence.

## Acceptance criteria

- Given any validation finding, when `validate --json` reports it, then `warnings` and `errors` contain objects, each with `rule` and `message`, plus the variant's own fields.
- Given the `message` field, when compared with the string `validate --json` emitted before this change, then the text is identical.
- Given human-readable `validate` output, when nothing else changes, then its wording is unchanged.
- Given the TUI validation panel and the web validation view, when a finding renders, then they display `message` and nothing else about them changes.
- Given the `/lazy` and `/execute` skill prose, when this lands, then any instruction that string-parses a finding reads `rule` or a field instead.

## Notes

Breaking change for `validate --json` consumers, accepted in RFC-068 §Risks. Must land before any `governs-*` rule because those findings carry fields a string cannot hold. STORY-264 asks the same shape for `UnsatisfiedEdge`; this story is the general form and satisfies it once `UnsatisfiedEdge` serialises its fields.
