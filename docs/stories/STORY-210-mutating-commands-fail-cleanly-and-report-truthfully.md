---
title: Mutating commands fail cleanly and report truthfully
type: story
status: accepted
author: agent
date: 2026-07-16
tags: []
related:
- related-to: AUDIT-018
---

## Value

As a lazyspec user (human or agent), mutating commands either do exactly what they report or fail with a clear error — never panic, never silently drop data.

## Acceptance Criteria

- AC1: Given a doc whose frontmatter has a bare `related:` (YAML null), when I `lazyspec link A rel B`, then the link is written (value coerced to a sequence) — no panic. (AUDIT-018 C2)
- AC2: Given a git-ref-stored doc, when I `update` a key not yet present in its frontmatter, then the key is inserted; values with YAML-significant characters are escaped/quoted. Frontmatter round-trips through serde_yaml as `set_provenance` does. (AUDIT-018 C3)
- AC3: Given `RFC-1` and `RFC-12` both exist, when I reference `RFC-1/child`, then resolution prefers the exact parent id and errors on genuine ambiguity, mirroring `resolve_unqualified`. (AUDIT-018 C6)
- AC4: `create --json` output carries the real assigned document id, not `""`. (AUDIT-018 F5)

