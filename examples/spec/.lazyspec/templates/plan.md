---
title: "{title}"
type: plan
status: draft
author: "{author}"
date: {date}
tags: []
related: []
---
<!-- Target: under 100 lines. If longer, split into multiple plans per vertical slice. -->

## Changes

Numbered task breakdown. Each task should be self-contained enough for a zero-context subagent.

### Task 1: [descriptive name]

**Contracts addressed:** [which spec contracts this implements]

**Files:**
- Create/Modify: `exact/path/to/file`
- Test: `tests/exact/path/to/test`

**What to implement:**
[Complete description of the work]

**How to verify:**
[Test commands and expected output]

## Test Plan

Each test must map to at least one spec AC. Tag tests with the AC(s) they cover (e.g. AC1, AC3). If an AC can't be tested automatically, describe the manual verification.

## Notes

Collect discoveries, surprises, and decision rationale here. Don't scatter notes inline through the tasks.
