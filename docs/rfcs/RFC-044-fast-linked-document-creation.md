---
title: Fast linked-document creation
type: rfc
status: superseded
author: jkaloger
date: 2026-06-18
tags: []
related:
- related-to: RFC-042
- related-to: RFC-043
---

## Problem

Creating a doc linked to another is multi-step: `create`, recall/look up target id, `link`. TUI has no single motion to relate a new doc to an existing one. Relating a story to an RFC is slow and breaks flow.

## Intent

One motion to spawn a document and link it. TUI telescope-style picker: choose type, choose relationship, fuzzy-find target, create + link atomically. CLI gets a `--link` flag so the same is one command.

## Sketch

Depends on:
- Relationship types as config (declared `[[relationships]]`) — picker lists only valid relationships for the chosen source/target pair.
- Fuzzy matcher — picker target search.

CLI:

```
@draft lazyspec create story "Title" --link implements RFC-042
```

Creates the story, links `implements`→RFC-042 in one call. Target accepts partial id/title via fuzzy resolution.

TUI:
- Keybind opens picker overlay. Pick relationship type (from config). Fuzzy-find target doc. New doc created + linked on confirm.
- Reuse external-editor suspend pattern (`src/tui/views/keys.rs`) where relevant.

## Stories

- CLI `create --link <rel> <target>` (atomic create+link, fuzzy target resolution).
- TUI telescope picker overlay (relationship + fuzzy target).
- Validate chosen relationship against `[[relationships]]` rules at create time.

## Open questions

- Picker order: type → relationship → target, or target-first?
- Reuse TUI search overlay component vs new picker?
- Link an existing doc (not just freshly created) through the same picker?

