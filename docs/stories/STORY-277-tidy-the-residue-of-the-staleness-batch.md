---
title: Tidy the residue of the staleness batch
type: story
status: draft
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- related-to: RFC-069
---

As a maintainer reading the staleness code after the fact, I want the small inconsistencies the batch left behind cleaned up, so that the next person to touch it does not inherit three answers to the same question.

## Acceptance criteria

- Given `MockGitRefClient`, when a test reads its call log, then it goes through `call_log()` and the `calls` field is private. Roughly 40 pre-existing sites across `git_ref_store.rs`, `fetch.rs`, `store.rs`, `pin.rs`, `store_dispatch.rs` and `git_ref.rs` read the field directly.
- Given a `stale` finding on an age-driven document, when it renders, then the message does not claim `0 files` — the drift clause belongs to drift-driven documents.
- Given a test that needs a `TypeDef`, when it constructs one, then it uses `TypeDef::test_fixture` rather than naming every field. Twenty literal sites across fifteen files paid a mechanical edit for one new key; the next `[[types]]` key will charge the same toll.
- Given `run <id>` on `show`, when the human output is asserted, then the assertion goes through `show::run` rather than the private `staleness_line` helper. STORY-272 AC6 says "when I run `show <id>`" and nothing tests that.
- Given `Days`, when it is defined, then it derives only what is used — `PartialOrd`/`Ord` are speculative, `band_by_age` compares `.0`.
- Given the two `diff_stat` tests in `tests/integration/git_ref_test.rs`, when they set up commits, then they share one `commit_all` helper instead of duplicating the closure.
- Given the `validate` test at `src/cli/validate.rs:196`, when the suite runs, then it prints nothing to stdout or stderr.
- Given `cli_fix_config_test.rs:101`, when it dates a fixture, then it uses a fixed date with `finding = "off"` as the other two sites do, not `Utc::now()`.
- Given `default_aging` and `default_stale` in `src/engine/config.rs`, when nothing outside the module calls them, then they are private.
- Given `StalenessRequest`, when it is built per selection change, then it carries the thresholds and the selected type's driver rather than a clone of the whole `Config`.

## Notes

Filed out of the RFC-069 batch's comprehensive review (STORY-272 through STORY-275, ITERATION-421 through ITERATION-429). None of these is blocking and none changes behaviour; they are the residue that only showed at batch scale.

Deliberately excluded: `engine::ops::update::run` has no production caller and just gained a `git` parameter it forwards as `config: None`. That is a pre-existing dead surface, wider by one argument — its own question, not this story's.
