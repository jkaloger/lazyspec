---
title: Find the documents that govern a file
type: story
status: draft
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- implements: RFC-068
- blocks: STORY-267
- blocks: STORY-268
- blocks: STORY-269
- blocks: STORY-270
---

As an agent about to change a source file, I want `why <path>` to list the documents that declare that file in `governs`, so that I read the spec that applies instead of grepping and guessing.

## Acceptance criteria

- Given a document with `governs: [src/engine/context/**]`, when I run `why src/engine/context/resolve.rs --json`, then the output is a list containing that document's id, type, title, status, `reviewed` and the glob that matched.
- Given two documents whose globs both match a path, when I run `why <path>`, then both appear, each with its own matching glob.
- Given a path no glob matches, when I run `why <path> --json`, then the output is an empty list and the exit code is zero.
- Given `[governs] root = "../app"`, when globs are matched, then they resolve relative to that root, not the docs repo.
- Given a document with `governs` and `reviewed` set, when I run `show <id>` or `show <id> --json`, then both fields are printed.
- Given a document of any type with no `governs` key, when the store loads, then it parses with an empty list and no finding.
- Given a `governs` entry that is not a valid glob, when the store loads, then the load reports which document and which entry failed.

## Notes

Walking skeleton for RFC-068. Parse `governs` and `reviewed` on `DocMeta`, load `[governs]`, compile with `globset` at store load, add `governing(store, path)` in the engine and a `why` verb in the CLI. No validation rules, no TUI or web changes; those are later slices. `reviewed` is parsed but not judged; RFC-069 owns that.
