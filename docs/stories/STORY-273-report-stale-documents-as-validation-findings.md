---
title: Report stale documents as validation findings
type: story
status: draft
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: RFC-069
---

As a maintainer keeping documentation honest, I want `validate` to report every stale document as a finding, so that documentation rot is a list I can work down in CI rather than something I only notice when a document misleads me.

## Acceptance criteria

- Given documents whose bands are `fresh`, `aging` and `stale`, when I run `validate --json`, then exactly the `stale` one produces a finding with `rule: "stale"` and its computed staleness as fields.
- Given `[staleness] finding = "error"`, when I run `validate`, then the finding is an error and the exit code is non-zero; given `finding = "warning"`, then it is a warning and the exit code is zero.
- Given `[staleness]` with no `finding` key, when I run `validate`, then the finding is a warning (the default).
- Given `[staleness] finding` omitted by setting it off, when I run `validate`, then no `stale` findings are emitted and no staleness computation runs.
- Given a stale document, when I open the TUI validation panel or the web validation view, then the finding appears there with its message, carried by the existing object finding shape with no per-surface special casing.
- Given a project with no stale documents, when I run `validate --json`, then no `stale` finding appears.

## Notes

Adds `ValidationIssue::Stale { path, staleness }` and the `[staleness].finding` severity knob, consuming `compute` from the skeleton slice. Serialises through the object finding shape landed by STORY-266, so the CLI, TUI validation panel and web validation view carry it without further change.
