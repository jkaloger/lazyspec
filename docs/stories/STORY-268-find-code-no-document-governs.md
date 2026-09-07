---
title: Find code no document governs
type: story
status: in-progress
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- implements: RFC-068
---

As a maintainer seeding pins, I want `validate` to list files in scope that no document governs, so that the unpinned modules are a finding I can work down in CI rather than a gap nobody sees.

## Acceptance criteria

- Given `[governs] scope = ["src/**"]` and `unowned = "warning"`, when I run `validate`, then one `governs-unowned` warning is emitted per file under scope that no document's glob matches.
- Given `unowned = "error"`, when unowned files exist, then the findings are errors and `validate` exits non-zero.
- Given no `unowned` key in config, when I run `validate`, then no `governs-unowned` findings are emitted, whatever `scope` says.
- Given `scope` narrowed to one module, when I run `validate`, then files outside that module are not reported.
- Given `validate --json`, when an unowned file is reported, then the finding carries `rule: governs-unowned`, `file` and `message`, so `jq 'select(.rule=="governs-unowned") | .file'` lists the files.
- Given this repo's own config, when the story lands, then `scope` and `unowned` are set to values that pass `validate`.

## Notes

Default off, per RFC-068 §Configuration: an unpinned repo turning this on gets one finding per file. Dogfooding requirement in the last criterion is what grows pin density here.
