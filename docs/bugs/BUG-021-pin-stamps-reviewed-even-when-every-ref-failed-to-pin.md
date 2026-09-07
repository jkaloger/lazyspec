---
title: pin stamps reviewed even when every ref failed to pin
type: bug
status: reported
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- related-to: RFC-068
- related-to: RFC-069
---

Found by the end-of-batch review on the RFC-068 governs batch (`cafb665..eb13d8b`).

`pin` writes the `reviewed` sha even when every `@ref` in the document failed to pin. Verified live: 0 pinned / 2 errors, sha written, exit 0.

The document then claims it was checked against that commit while its own pins are broken — the stamp asserts more than the run actually established.

No acceptance criterion in STORY-270 or ITERATION-418 governs this case, so the current behaviour is not a violation of anything written down. It needs a decision.

Options: don't stamp when nothing pinned; stamp but exit non-zero; or keep it and document that `reviewed` records "when the document was last checked", not "the pins are good". RFC-069 is about to build staleness judgements on this field, so it should be settled before that lands.
