---
title: Edit an edge's fields from the settings panel
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-260
- blocks: ITERATION-388
---

## Objective

`name`, `from`, `to`, `via`, `traversal` and `required` are editable on a drilled edge row: `name` and the two type selectors through text entry, `via` and the two optionals through the enum cycler with an explicit unset position.

## Satisfies

STORY-260 AC2, less `require_to_status` -- a field the story names and the code does not have (see Context). AC1 landed in ITERATION-386; AC5 lands in ITERATION-388, AC4 in ITERATION-389, AC6 in ITERATION-390, AC3 in ITERATION-391.

## Context

- Story + ACs: STORY-260
- The six keys and their meanings: ADR-030 §Decision; RFC-067 §"Interface sketch"
- `required` absent means "legal, may walk, absence is not a finding": RFC-067 §Design. That is why unset has to be reachable from the panel and not merely representable in the file
- Touch:
  - `src/tui/state/app.rs:39-49` `SettingsValue` -- `Severity(Severity)` is non-optional and `required` is `Option<Severity>`; `traversal` is `Option<Traversal>` with no carrier at all
  - `src/tui/state/app.rs:176-182` `severity_from_variant` -- a free function that existed for the rule severity field. If ITERATION-382 left it behind as dead code, reuse it; if it took it, reintroduce it rather than inlining a second parser
  - `src/tui/state/app.rs:1160-1250` `settings_write` -- replace ITERATION-386's no-op arm
  - `src/tui/state/app.rs:1404-1412` `settings_cycle_enum` and `:1418-1476` `settings_set_enum_variant` -- the shared cycle/pick write. Its doc comment still describes the rule-shape special case ITERATION-382 deleted; correct it (convention §Governance)
  - `src/tui/state/app.rs:1330-1372` `settings_confirm_edit` -- the `Text` / `Nullable` / `List` commits, and `settings_edit_error` as the refusal channel
  - `src/tui/views/panels.rs:2101` `settings_fields` -- the `3 =>` arm's editors change from `ReadOnly`
  - `src/tui/state/forms.rs:429-446` `FieldEditor` -- `EnumCycle` carries `variants: &'static [&'static str]`
- **The field the story names and the code does not have.** AC2 lists `require_to_status`. It is not on `EdgeDef` (`src/engine/config.rs:55-65`): it was added in `e475491`, wired into `create` in `7e3ab28`, and both were reverted in `40b91f3` when ADR-033 abandoned status-conditioned gating outright -- "the edge table therefore has one policy, not two" (RFC-067 §Design), and ADR-030's 2026-08-31 amendment says the same. Amend the story to drop the field. Do not add it back to satisfy an AC written before the revert.
- **`traversal` must already exist.** `EdgeDef.traversal` arrives in ITERATION-372 (STORY-257). Until it lands there is no field to edit, which is the blocking edge.
- **The panel has no spelling for an optional enum.** `settings_cycle_enum` indexes a `&'static` variant list and `settings_set_enum_variant` no-ops on an unrecognised variant, so an `Option` has nowhere to sit. Put an explicit unset entry first in both variant lists and map it to `None` in the write, rather than composing `Nullable` with `EnumCycle`. `RelationshipDef.traversal` is the same `Option<Traversal>` and has never been editable in the panel, so this is a new affordance and there is no precedent to copy.
- **`from` and `to` get the interim editor here.** Both are `TypeSelector`, so `FieldEditor::List` -- the comma-separated editor `types[].agents` already uses (`panels.rs:2205`, commit at `app.rs:1348-1358`) -- makes them editable with no new machinery. ITERATION-391 replaces it for both with the picker AC3 asks for. Do not read the comma editor as AC3 satisfied: it neither offers `"*"` nor adds and removes members individually.
- **The hole the interim editor opens, and where it has to be closed.** `List`'s commit filters empty strings (`app.rs:1352-1356`), so clearing `to` yields `Types(vec![])` -- an empty target set. Nothing at load rejects it: the check at `config.rs:1327-1336` iterates `names()` and an empty list iterates nothing. It is also not a selector a human would write. Refuse it at commit through `settings_edit_error` rather than mapping it to `Any`, because `Any` is a different claim and the user did not make it. Record that the loader is missing this check and that the panel is compensating; ITERATION-391 must carry the same refusal.
- `via` is best a cycler over the declared relationship names plus `"*"`, since strict load already rejects an unknown relationship (`config.rs:1337-1345`) and a cycler over the legal set stops the user reaching that error. But `EnumCycle`'s variant list is `&'static` and this one is buffer-derived. Either widen the carrier or keep `via` a text field and say which -- widening it touches every `EnumCycle` site, so this is a real cost, not a footnote.

## Tasks

1. Test-first through the App state API: `settings_write` on each `EdgeKey` lands in `settings_buffer.edges[0]` and `settings_focused_raw` reads it back in the TOML spelling. Assert on the model, not on a rendered line.
2. Add the `EdgeKey` write arms and the `SettingsValue` carriers the optional fields need.
3. Variant lists for `traversal` and `required` with unset first, wired through `settings_set_enum_variant` beside the numbering / store / reserved arms, and its stale rule-shape doc comment corrected.
4. Test-first: cycling `required` from `error` through `warning` to unset and back sets `edges[0].required` to `Some(Error)`, `Some(Warning)`, `None`, `Some(Error)` -- and that the unset write actually removes the key rather than writing a default, which `to_toml`'s `skip_serializing_if` on `EdgeDef.required` (`config.rs:63-64`) is what makes visible.
5. Resolve the `via` carrier question from Context and implement whichever answer, with the reason in a comment.
6. `from` / `to` as `List` with the empty-set refusal from Context, and a test for the refusal that asserts the buffer is untouched and `settings_dirty` stays false.
7. Test that every edit sets `settings_dirty`, and that each field's `FieldEditor` matches the commit path that writes it -- a mismatch is a silent no-op by design (`settings_write`'s doc comment, `app.rs:1156-1159`).

## Out of scope

- `require_to_status` -- withdrawn by ADR-033, not deferred.
- Getting any of this to disk (AC5) -> ITERATION-388. Until then a save silently drops every edge edit, because `write_config_in_place` renders from the buffer through thirteen writers and none of them touches `edges`. That is a live footgun for the duration of one slice and it is chosen, not overlooked: the writer is being built for the migration in ITERATION-377 and duplicating it here would give the project two edge writers.
- Rejecting an edit the loader would refuse (AC4) -> ITERATION-389. This slice lets the buffer hold an unknown type name.
- Individually adding and removing target types, and `"*"` as an offered choice (AC3) -> ITERATION-391.
- Seeding and deleting rows (AC6) -> ITERATION-390.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 6: reuse `List` and `EnumCycle` rather than inventing an editor per field; the one place indirection is warranted is the `EnumCycle` carrier, and only if `via` demands it. TUI dictums: key handling produces state transitions, so every editor commit is a buffer write and nothing else.

## Verification

Drill into an edge in the TUI, retype `to`, cycle `required` to unset, press `w`: the save reports success and `git diff .lazyspec.toml` is empty. That gap is the next slice, and confirming it by hand is the point of running this.
