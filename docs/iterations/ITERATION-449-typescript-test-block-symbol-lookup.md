---
title: TypeScript test block symbol lookup
type: iteration
status: draft
author: Jack Kaloger
date: 2026-10-05
tags: []
related:
- implements: STORY-298
---

## Changes

All in `src/engine/symbols.rs`. Test-first.

- T1: failing tests, TS fixtures. Cover each STORY-298 AC: `it`, `test`, `describe`, nested, `.skip`/`.only`, template literal, dup title, existing decl unchanged.
- T2: TS matcher. Add `call_expression` to TS `match_node_types`. Existing `name`/`type` field path untouched.
- T3: `call_title(node, source)` helper. Callee = `identifier` in {`it`,`test`,`describe`} or `member_expression` with object in set, property in {`skip`,`only`}. First arg = `string` (strip quotes) or `template_string` with no `template_substitution` child. Else `None`.
- T4: `find_symbol_node`. If node is `call_expression`, compare `call_title` to symbol. Else existing logic.
- T5: return range. Whole call. Prefer enclosing `expression_statement` (include trailing `;`).
- T6: traversal. Pre-order document order, so first match wins on dup. Recurse into call args so nested `it` in `describe` callback found.
- T7: Rust/other langs unaffected. Run full symbols suite.
- T8: quoted form. Regex group 2: `("(?:[^"\\]|\\.)*"|[^@\s]+)`. Groups 1-4 indices unchanged. `Ref.quoted: bool`; strip quotes, unescape `\"`. Test-first.
- T9: `Ref::symbol_text` re-quotes + escapes. `pin` target + new ref use it (round-trip keeps quotes).
- T10: audit consumers. validation (`REF_RE` group 1 only), cascade (group 0 only): no change, add tests. TUI: `contains("@ref ")` gate only, no change.
- T11: e2e test. Body `@ref x.test.ts#"does a thing"` -> parse -> `resolve_ref` -> `it` block.
- T12: docs. README, SPEC-005, skills grep.

## Test Plan

- AC1: `it("a")` and `test("a")` return full statement text.
- AC2: `describe("s")` returns whole block.
- AC3: nested `it` inside `describe` found.
- AC4: `it.skip`, `it.only`, `test.skip`, `test.only`, `describe.skip`, `describe.only` each resolve.
- AC5: `` it(`a`) `` matches; `` it(`a${x}`) `` does not.
- AC6: existing function/class/const lookups pass unchanged.
- AC7: two `it("dup")` -> first returned.
- AC9: `#"a b"` -> symbol `a b`. `\"` unescaped. `#"a b"@{blob:ab}`, `#"a b"@ab12` parse suffix. Unquoted unchanged.
- AC9: unterminated `#"a b` -> bare symbol `"a`, no panic.
- AC9: pin quoted ref -> `#"a b"@{blob:..}`; re-pin stable.
- AC9: e2e parse -> resolve -> `it` block.
- `cargo clippy -- -D warnings`.

## Notes

- `describe > it` path syntax out of scope.
- `.each` forms out of scope.
- Non-matching callee (`foo("title")`) must not match.
