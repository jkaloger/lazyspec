---
title: Populate type lifecycle from bound List status set
type: iteration
status: complete
author: unknown
date: 2026-07-05
tags: []
related:
- implements: STORY-198
- blocks: ITERATION-270
- blocks: ITERATION-275
---

Objective: at sync, populate the type effective lifecycle states from the bound List ClickUp status set, with no local edges or gating.

Refs: RFC-056 Design section Status handling; ticket type empty-lifecycle posture (empty states and edges).

Satisfies: STORY-198 AC3.

Tasks:
1. Fetch the bound List status definitions via ClickupClient.
2. Populate the type effective lifecycle states from that status set at sync time.
3. No local edges or gating; mirror the ticket empty-lifecycle posture (ClickUp enforces its own transition rules).

Out of scope: no local transition edges or gating; no status mapping table; write path and relations excluded.

Principles: match the ticket empty-lifecycle posture; lifecycle derived at sync, never hardcoded.

AC:
- Given a bound List, then the type effective lifecycle states equal the List status set captured at sync time.
- Given that lifecycle, then no local edges or gating exist (same posture as ticket empty lifecycle).
