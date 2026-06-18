---
title: Config dependency auto-scaffolding
type: story
status: draft
author: jkaloger
date: 2026-06-19
tags: []
related:
- implements: RFC-023
---

## Context

In RFC-023 the Settings screen edits configuration through an inline atomic dirty buffer (slice 3), where cycling an enum field such as a type's `numbering` or `store` is a self-contained edit and whole-config validation runs only at save time. Several enum values, however, introduce a dependency on a configuration section that may not yet exist: `numbering = "sqids"` requires a `[numbering.sqids]` section with a non-empty `salt`, `numbering = "reserved"` requires `[numbering.reserved]`, and `store = "github-issues"` requires a `[github]` section. Rather than letting the user cycle to such a value and only discover the missing section when save-time validation rejects it, this slice adds an auto-scaffolding side-effect on top of slice 3's enum cycling: the moment an enum edit creates a section dependency, the dirty buffer inserts the dependent section pre-populated with the same defaults the parser would supply, flags any required-but-empty field (such as the sqids `salt`), and offers to move focus there. This is guidance only: slice 3's atomic save and whole-config validation are unchanged and still reject an empty required field, so the buffer cannot be saved into an invalid state.

## Acceptance Criteria

### AC1: Cycling to sqids scaffolds the numbering.sqids section

**Given** a type whose `numbering` is being cycled in the dirty buffer and the buffer contains no `[numbering.sqids]` section
**When** the value is cycled to `sqids`
**Then** the buffer gains a `[numbering.sqids]` section with `salt` empty and `min_length = 3`

### AC2: An empty required sqids salt is flagged

**Given** a `[numbering.sqids]` section has just been auto-scaffolded with an empty `salt`
**When** the buffer is displayed after the cycle
**Then** the `salt` field is marked as required-but-empty (visually distinguished as needing a value before save)

### AC3: The user is offered a jump to the flagged field

**Given** a section was auto-scaffolded containing a required-but-empty field
**When** the auto-scaffold completes
**Then** the screen offers to move focus to that field (e.g. the empty sqids `salt`) and accepting the offer places edit focus on it

### AC4: Cycling to reserved scaffolds the numbering.reserved section with defaults

**Given** a type whose `numbering` is being cycled in the dirty buffer and the buffer contains no `[numbering.reserved]` section
**When** the value is cycled to `reserved`
**Then** the buffer gains a `[numbering.reserved]` section with `remote = "origin"`, `format = "incremental"`, and `max_retries = 5`

### AC5: Cycling store to github-issues scaffolds the github section

**Given** a type whose `store` is being cycled in the dirty buffer and the buffer contains no `[github]` section
**When** the value is cycled to `github-issues`
**Then** the buffer gains a `[github]` section populated with its parser defaults

### AC6: Scaffolding is skipped when the dependent section already exists

**Given** the buffer already contains the section a cycled enum value depends on (e.g. `[numbering.sqids]` with a user-set `salt`)
**When** the enum value is cycled to the dependent value
**Then** no new section is inserted and the existing section's field values are left unchanged

### AC7: Save-time validation still rejects an unfilled required field

**Given** a `[numbering.sqids]` section was auto-scaffolded and its `salt` was left empty
**When** the user attempts to save the buffer
**Then** the save is rejected by whole-config validation with the existing non-empty-salt error and the buffer remains dirty

## Scope

### In Scope
- Detecting, at the moment an enum field is cycled in the dirty buffer, that the new value introduces a dependency on a config section not present in the buffer.
- Auto-inserting the dependent section into the dirty buffer with parser-matching defaults: `[numbering.sqids]` (`salt` empty, `min_length = 3`), `[numbering.reserved]` (`remote = "origin"`, `format = "incremental"`, `max_retries = 5`), and `[github]` (its parser defaults).
- Marking required-but-empty fields produced by scaffolding (notably the sqids `salt`) and offering to jump edit focus to the first such field.
- Leaving an already-present dependent section and its field values untouched when the dependency is re-asserted.

### Out of Scope
- The base enum-cycle editor and the atomic save plus whole-config validation themselves (owned by slice 3); this slice only adds the auto-insert side-effect on top of them.
- Collection add/delete editing (owned by slice 5).
- The read-only settings view (owned by slice 1).
- Settings reload machinery (owned by slice 2).

