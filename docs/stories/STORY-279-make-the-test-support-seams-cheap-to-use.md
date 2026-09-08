---
title: Make the test-support seams cheap to use
type: story
status: draft
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: RFC-069
---

As a maintainer writing a test that needs a store, a type or a mock git client, I want the test-support seams to cost one line rather than three, so that the next batch does not pay the toll this one did.

## Acceptance criteria

- Given `TypeDef::test_fixture`, when it sets `dir`, then it uses the type's own `plural` — `docs/{plural}` — rather than `docs/{name}`. Nearly every one of the ~30 call sites STORY-277 introduced overrides `dir` immediately, because the fixture contradicts its own `plural` field. Pre-existing callers depend on the singular form, so check each before changing the default.
- Given a test that reads `MockGitRefClient`'s call log, when it asserts on the calls, then it does so in one line. `call_log()` returns the `Rc<RefCell<_>>`, which forces a `let log = …; let calls = log.borrow();` dance at ~20 sites; a `fn calls(&self) -> Vec<String>` satisfies STORY-277 AC1 without it.
- Given the `rfc_store(.., days)` family of test helpers, when a test needs a document of a known age, then the age comes from an injected clock rather than `Utc::now() - Duration::days(n)`. The convention's "Deterministic: no timestamps" desideratum says so, and `days_since(doc.date)` is what makes a fixed date unusable today.

## Notes

Filed out of the RFC-069 chunk's comprehensive review (STORY-276, STORY-277). All three are ergonomics, not correctness: nothing here can produce a wrong answer, and the suite is not flaky today — `num_days()` truncation absorbs the wall-clock epsilon.

The first two are residue from STORY-277's own tidying: it made `calls` private and replaced the literal fixtures as asked, and in both cases the seam it landed on is one step short of the one that would have deleted the boilerplate outright. Worth doing before the next `[[types]]` key or mock method charges the same toll again.

The third is inherited rather than introduced, but STORY-277 added three new `show` tests to the pattern, so "pre-existing" is thinner than it was.
