---
title: Show the staleness band where documents are inspected
type: story
status: in-progress
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: RFC-069
reviewed: 1a7db94929dbfe5c9b1a0520e12cb9927ac6803d
---

As a reader browsing documents in the TUI or web view, I want the selected document's detail surface to show its staleness band, so that I can judge whether to trust what I am reading without dropping to the CLI.

## Acceptance criteria

- Given a document selected in the TUI, when its staleness has been computed, then its detail surface shows a band badge reading `fresh`, `aging` or `stale`, with the driver and the drift or age fact alongside.
- Given the same document opened in the web view, when its detail is rendered, then it shows the same band and facts.
- Given I move the cursor down a list of documents, when each selection changes, then the render loop never blocks on a git subprocess and the TUI stays responsive.
- Given a selection whose staleness has not yet been computed, when the detail surface renders, then it shows a placeholder in the badge's place.
- Given I move the selection on before a computation lands, when that result arrives, then it is dropped and the previously selected document's band is never shown against the current one.
- Given a list of documents in either surface, when rows are rendered, then no row carries a staleness band and no computation is issued per row.

## Notes

Computed in a background worker on selection, the same pattern search uses after BUG-011, for the same reason: a git subprocess on the event loop stalls every cursor move. Detail surfaces only; list rows and list filters are named non-goals of RFC-069.
