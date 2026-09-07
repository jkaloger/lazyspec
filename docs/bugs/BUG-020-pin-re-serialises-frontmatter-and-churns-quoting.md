---
title: pin re-serialises frontmatter and churns quoting
type: bug
status: reported
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- related-to: RFC-068
---

Found by the end-of-batch review on the RFC-068 governs batch (`cafb665..eb13d8b`).

ITERATION-418 made `pin` write `reviewed` through `document::rewrite_frontmatter`, which re-serialises the entire frontmatter mapping through `serde_yaml`. Verified live: `title: "Thing"` becomes `title: Thing`, and `  - src/old/**` becomes `- src/old/**`.

`pin` is a verb users run routinely, so it will churn quoting and indentation across every document it touches — noise in diffs that has nothing to do with the pin.

This is the cost of the instruction ITERATION-418 gave, not a slip in carrying it out: the same normalisation already applies to `tag`, `link` and TUI edits, and this repo's own documents are already canonical, so nothing is visibly broken today. It needs a decision rather than an automatic fix.

Options: accept it as repo-wide behaviour and say so in the README; or give `reviewed` a targeted line-level write that leaves the rest of the frontmatter bytes alone.
